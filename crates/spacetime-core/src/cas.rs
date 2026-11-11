//! Content-addressed object store.
//!
//! Shared dependencies are hashed once and stored once; every microVM mounts the
//! store read-only, so ten tabs using the same 5 GB PyTorch/Torch stack occupy
//! one copy on disk and (with virtio-fs DAX) one copy in the page cache.
//!
//! This module implements the store's policy — dedup, refcounts, verified reads
//! and garbage collection — on top of any [`ObjectBackend`]. The default backend
//! is in-memory so tests need no filesystem; `spacetime-vmm` supplies a
//! directory-backed one (`DirBackend`) for real use.

use std::collections::BTreeMap;

use crate::hash::{sha256, Hash};

/// Pluggable byte storage.
pub trait ObjectBackend {
    fn put(&mut self, hash: Hash, data: &[u8]) -> Result<(), String>;
    fn get(&self, hash: Hash) -> Option<Vec<u8>>;
    fn remove(&mut self, hash: Hash) -> Result<(), String>;
    fn total_bytes(&self) -> u64;
}

/// In-memory backend (tests, engine daemon).
#[derive(Debug, Default)]
pub struct MemBackend {
    objects: BTreeMap<Hash, Vec<u8>>,
}

impl ObjectBackend for MemBackend {
    fn put(&mut self, hash: Hash, data: &[u8]) -> Result<(), String> {
        self.objects.insert(hash, data.to_vec());
        Ok(())
    }
    fn get(&self, hash: Hash) -> Option<Vec<u8>> {
        self.objects.get(&hash).cloned()
    }
    fn remove(&mut self, hash: Hash) -> Result<(), String> {
        self.objects.remove(&hash);
        Ok(())
    }
    fn total_bytes(&self) -> u64 {
        self.objects.values().map(|v| v.len() as u64).sum()
    }
}

/// Result of storing an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Insert {
    pub hash: Hash,
    pub bytes: u64,
    /// True when the object already existed (nothing new written).
    pub deduplicated: bool,
}

/// Refcounted content-addressable store.
#[derive(Debug)]
pub struct CasStore<B: ObjectBackend = MemBackend> {
    backend: B,
    refs: BTreeMap<Hash, u32>,
    sizes: BTreeMap<Hash, u64>,
    /// Bytes that would have been written without dedup.
    naive_bytes: u64,
}

impl Default for CasStore<MemBackend> {
    fn default() -> Self {
        CasStore::new(MemBackend::default())
    }
}

impl<B: ObjectBackend> CasStore<B> {
    pub fn new(backend: B) -> Self {
        CasStore {
            backend,
            refs: BTreeMap::new(),
            sizes: BTreeMap::new(),
            naive_bytes: 0,
        }
    }

    /// Store `data`, returning its address and whether it was a duplicate.
    pub fn insert(&mut self, data: &[u8]) -> Result<Insert, String> {
        let hash = sha256(data);
        self.naive_bytes += data.len() as u64;
        if let Some(count) = self.refs.get_mut(&hash) {
            *count += 1;
            return Ok(Insert {
                hash,
                bytes: data.len() as u64,
                deduplicated: true,
            });
        }
        self.backend.put(hash, data)?;
        self.refs.insert(hash, 1);
        self.sizes.insert(hash, data.len() as u64);
        Ok(Insert {
            hash,
            bytes: data.len() as u64,
            deduplicated: false,
        })
    }

    /// Read and verify an object.
    pub fn get(&self, hash: Hash) -> Result<Vec<u8>, String> {
        let data = self
            .backend
            .get(hash)
            .ok_or_else(|| format!("object {hash} not in store"))?;
        if sha256(&data) != hash {
            return Err(format!("object {hash} failed integrity check"));
        }
        Ok(data)
    }

    /// Drop one reference; frees the object when the last reference goes.
    pub fn release(&mut self, hash: Hash) -> Result<bool, String> {
        match self.refs.get_mut(&hash) {
            Some(count) if *count > 1 => {
                *count -= 1;
                Ok(false)
            }
            Some(_) => {
                self.refs.remove(&hash);
                self.sizes.remove(&hash);
                self.backend.remove(hash)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn refcount(&self, hash: Hash) -> u32 {
        self.refs.get(&hash).copied().unwrap_or(0)
    }

    pub fn contains(&self, hash: Hash) -> bool {
        self.refs.contains_key(&hash)
    }

    pub fn object_count(&self) -> usize { self.refs.len() }

    /// Bytes physically stored.
    pub fn stored_bytes(&self) -> u64 { self.backend.total_bytes() }

    /// Bytes saved by deduplication.
    pub fn saved_bytes(&self) -> u64 { self.naive_bytes.saturating_sub(self.stored_bytes()) }

    /// Remove objects with no references (e.g. after a tab closes).
    pub fn sweep(&mut self) -> Vec<Hash> {
        let dead: Vec<Hash> = self
            .refs
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(hash, _)| *hash)
            .collect();
        for hash in &dead {
            self.refs.remove(hash);
            self.sizes.remove(hash);
            let _ = self.backend.remove(*hash);
        }
        dead
    }

    /// JSON summary for the "Global Store" sidebar item.
    pub fn to_json(&self) -> crate::json::Value {
        crate::json::Value::obj()
            .with("objects", self.object_count())
            .with("stored_bytes", self.stored_bytes())
            .with("saved_bytes", self.saved_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicates_deduplicate() {
        let mut store = CasStore::default();
        let first = store.insert(b"torch-2.5.1").unwrap();
        let second = store.insert(b"torch-2.5.1").unwrap();
        assert(!first.deduplicated);        assert(second.deduplicated);        assert_eq(first.hash, second.hash);        assert_eq(store.object_count(), 1);        assert_eq(store.stored_bytes(), 10);        assert_eq(store.saved_bytes(), 10);    }

    #[test]
    fn reads_are_verified() {
        let mut store = CasStore::default();
        let insert = store.insert(b"payload").unwrap();
        assert_eq(store.get(insert.hash).unwrap(), b"payload".to_vec());        assert(store.get(sha256(b"other")).is_err());    }

    #[test]
    fn release_frees_only_on_last_ref() {
        let mut store = CasStore::default();
        let a = store.insert(b"shared lib").unwrap();
        store.insert(b"shared lib").unwrap();
        assert(!store.release(a.hash).unwrap());        assert_eq(store.refcount(a.hash), 1);        assert(store.release(a.hash).unwrap());        assert(!store.contains(a.hash));        assert_eq(store.stored_bytes(), 0);    }

    #[test]
    fn sweep_removes_unreferenced() {
        let mut store = CasStore::default();
        let a = store.insert(b"x").unwrap();
        let b = store.insert(b"y").unwrap();
        store.release(a.hash).unwrap();
        let swept = store.sweep();
        assert(swept.contains(&a.hash));        assert(!swept.contains(&b.hash));        assert_eq(store.object_count(), 1);    }
}
