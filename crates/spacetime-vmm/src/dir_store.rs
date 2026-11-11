//! Directory-backed [`ObjectBackend`] for the global content-addressable store.
//!
//! Layout matches the design doc:
//!
//! ```text
//! /spacetime/store/sha256/ab/cd/abcdef...   (64-hex name, sharded 2/2)
//! ```
//!
//! The hash function is SHA-256 today (see [`spacetime_core::hash`]); swapping in
//! BLAKE3 later only changes the shard directory name, not the layout.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use spacetime_core::cas::ObjectBackend;
use spacetime_core::Hash;

/// Store rooted at a directory on the host.
#[derive(Debug, Clone)]
pub struct DirBackend {
    root: PathBuf,
}

impl DirBackend {
    /// Open (creating if needed) a store at `root`.
    pub fn open(root: impl AsRef<Path>) -> io::Result<DirBackend> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("sha256"))?;
        Ok(DirBackend { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Path an object with `hash` lives at.
    pub fn path_for(&self, hash: Hash) -> PathBuf {
        let hex = hash.to_hex();
        self.root
            .join("sha256")
            .join(&hex[0..2])
            .join(&hex[2..4])
            .join(&hex)
    }

    /// Number of objects on disk.
    pub fn object_files(&self) -> io::Result<u64> {
        fn walk(dir: &Path, count: &mut u64) -> io::Result<()> {
            if !dir.exists() {
                return Ok(());
            }
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, count)?;
                } else {
                    *count += 1;
                }
            }
            Ok(())
        }
        let mut count = 0;
        walk(&self.root.join("sha256"), &mut count)?;
        Ok(count)
    }
}

impl ObjectBackend for DirBackend {
    fn put(&mut self, hash: Hash, data: &[u8]) -> Result<(), String> {
        let path = self.path_for(hash);
        if path.exists() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        // Write to a temp file then rename: a torn write must never leave a
        // half-object under its final name.
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, data).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn get(&self, hash: Hash) -> Option<Vec<u8>> {
        fs::read(self.path_for(hash)).ok()
    }

    fn remove(&mut self, hash: Hash) -> Result<(), String> {
        let path = self.path_for(hash);
        if path.exists() {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn total_bytes(&self) -> u64 {
        fn walk(dir: &Path, total: &mut u64) {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        walk(&path, total);
                    } else if let Ok(meta) = entry.metadata() {
                        *total += meta.len();
                    }
                }
            }
        }
        let mut total = 0;
        walk(&self.root.join("sha256"), &mut total);
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetime_core::CasStore;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "spacetime-test-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn put_get_release_on_disk() {
        let dir = temp_dir("cas");
        let backend = DirBackend::open(&dir).unwrap();
        let mut store = CasStore::new(backend);
        let insert = store.insert(b"torch-cu121").unwrap();
        assert!(!insert.deduplicated);
        assert_eq!(store.get(insert.hash).unwrap(), b"torch-cu121".to_vec());
        assert_eq!(store.object_count(), 1);
        store.insert(b"torch-cu121").unwrap();
        assert_eq!(store.stored_bytes(), b"torch-cu121".len() as u64);
        assert_eq!(store.saved_bytes(), b"torch-cu121".len() as u64);
        let removed = store.release(insert.hash).unwrap();
        assert!(!removed);
        let freed = store.release(insert.hash).unwrap();
        assert!(freed);
        assert_eq!(store.stored_bytes(), 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn paths_are_sharded() {
        let dir = temp_dir("shard");
        let backend = DirBackend::open(&dir).unwrap();
        let hash = spacetime_core::sha256(b"x");
        let path = backend.path_for(hash);
        let hex = hash.to_hex();
        assert!(path.ends_with(format!("sha256/{}/{}/{}", &hex[0..2], &hex[2..4], hex)));
        fs::remove_dir_all(&dir).ok();
    }
}
