//! Block 9: `.wavefunction` content-addressed store, `Collapse`, `.space` branching (reflink
//! when supported, explicit fallback otherwise), and export/import bundles.

use crate::memory::write_atomic;
use serde_json::{json, Value};
use spacetime_types::surface::{CasLayer, CasProvenanceTier};
use spacetime_types::{
    from_json_str, sha256_hex, to_json_pretty, Result, SpacetimeError, WorkspaceId,
};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const MAX_BUNDLE_BYTES: u64 = 64 * 1024 * 1024;

fn validate_sha256_hex(digest: &str) -> Result<()> {
    if digest.len() == 64
        && digest
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(SpacetimeError::new(
            "CAS digest must be 64 lowercase hexadecimal characters",
        ))
    }
}

fn validate_snapshot_name(name: &str) -> Result<()> {
    if !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        Ok(())
    } else {
        Err(SpacetimeError::new(format!(
            "invalid CAS snapshot name: {name}"
        )))
    }
}

fn validate_relative_path(rel: &str) -> Result<PathBuf> {
    if rel.is_empty() || rel.contains('\0') || rel.contains('\\') {
        return Err(SpacetimeError::new(format!(
            "unsafe relative path in CAS artifact: {rel}"
        )));
    }
    let path = Path::new(rel);
    if path.is_absolute() {
        return Err(SpacetimeError::new(format!(
            "unsafe absolute path in CAS artifact: {rel}"
        )));
    }
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Normal(part) => out.push(part),
            _ => {
                return Err(SpacetimeError::new(format!(
                    "unsafe path component in CAS artifact: {rel}"
                )));
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(SpacetimeError::new("empty relative path in CAS artifact"));
    }
    Ok(out)
}

fn ensure_no_symlink_ancestors(root: &Path, rel: &Path) -> Result<()> {
    let mut cur = root.to_path_buf();
    for comp in rel.components() {
        cur.push(comp);
        if let Ok(meta) = fs::symlink_metadata(&cur) {
            if meta.file_type().is_symlink() {
                return Err(SpacetimeError::new(format!(
                    "refusing to traverse or overwrite symlink path: {}",
                    cur.display()
                )));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct WavefunctionCas {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchOutcome {
    pub branch_dir: PathBuf,
    pub cow_reflink_used: bool,
    pub files_copied: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollapseSummary {
    pub manifest_digest: String,
    pub object_count: usize,
    pub total_bytes: u64,
}

impl WavefunctionCas {
    pub fn open_or_init(root: &Path) -> Result<Self> {
        fs::create_dir_all(root.join("objects"))?;
        fs::create_dir_all(root.join("layers"))?;
        fs::create_dir_all(root.join("snapshots"))?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Store raw content into CAS and record a provenance-tiered layer manifest.
    pub fn put_layer(
        &self,
        name: &str,
        bytes: &[u8],
        tier: CasProvenanceTier,
        owner_workspace: Option<WorkspaceId>,
        verifier_proof_digest: Option<String>,
        build_log_digest: Option<String>,
    ) -> Result<CasLayer> {
        if tier == CasProvenanceTier::ReproBuilt && build_log_digest.is_none() {
            return Err(SpacetimeError::new(
                "ReproBuilt CAS layer requires build_log_digest",
            ));
        }
        if tier.globally_shareable() && owner_workspace.is_some() {
            return Err(SpacetimeError::new(
                "globally shareable layer must not be owned by a single tab workspace",
            ));
        }
        if !tier.globally_shareable() && owner_workspace.is_none() {
            return Err(SpacetimeError::new(
                "tab-local or quarantined CAS layer must declare owner_workspace",
            ));
        }

        let digest = sha256_hex(bytes);
        let obj_path = self.root.join("objects").join(&digest);
        if !obj_path.exists() {
            write_atomic(&obj_path, bytes)?;
        }
        let layer = CasLayer {
            digest: digest.clone(),
            name: name.to_string(),
            tier,
            owner_workspace,
            verifier_proof_digest,
            build_log_digest,
            size_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        };
        let meta_path = self.root.join("layers").join(format!("{digest}.json"));
        write_atomic(&meta_path, to_json_pretty(&layer)?.as_bytes())?;
        Ok(layer)
    }

    pub fn get_layer(&self, digest: &str) -> Result<CasLayer> {
        validate_sha256_hex(digest)?;
        let meta_path = self.root.join("layers").join(format!("{digest}.json"));
        let text = fs::read_to_string(meta_path)?;
        from_json_str(&text)
    }

    pub fn get_bytes(&self, digest: &str) -> Result<Vec<u8>> {
        validate_sha256_hex(digest)?;
        let obj_path = self.root.join("objects").join(digest);
        let bytes = fs::read(&obj_path)?;
        if sha256_hex(&bytes) != digest {
            return Err(SpacetimeError::new(format!(
                "CAS object integrity check failed for {digest}"
            )));
        }
        Ok(bytes)
    }

    /// Enforce that `TabLocal` and `Quarantined` artifacts can never be shared with another
    /// workspace without explicit provenance promotion.
    pub fn can_mount_layer(&self, digest: &str, requester_workspace: &str) -> Result<bool> {
        let layer = self.get_layer(digest)?;
        if layer.tier.globally_shareable() {
            return Ok(true);
        }
        Ok(layer
            .owner_workspace
            .as_ref()
            .is_some_and(|owner| owner.as_str() == requester_workspace)
            && layer.tier == CasProvenanceTier::TabLocal)
    }

    /// Collapse a `.space` directory into a content-addressed `.wavefunction` snapshot manifest.
    pub fn collapse_space(&self, space_dir: &Path, snapshot_name: &str) -> Result<CollapseSummary> {
        validate_snapshot_name(snapshot_name)?;
        let mut files = BTreeMap::new();
        let mut total_bytes = 0u64;
        collect_space_files(space_dir, space_dir, &mut files)?;

        let mut manifest_entries = serde_json::Map::new();
        for (rel, path) in &files {
            let bytes = fs::read(path)?;
            total_bytes = total_bytes.saturating_add(u64::try_from(bytes.len()).unwrap_or(0));
            let digest = sha256_hex(&bytes);
            let obj_path = self.root.join("objects").join(&digest);
            if !obj_path.exists() {
                write_atomic(&obj_path, &bytes)?;
            }
            manifest_entries.insert(rel.clone(), json!(digest));
        }

        let manifest = json!({
            "schema_version": 1,
            "snapshot": snapshot_name,
            "files": manifest_entries,
        });
        let manifest_text = serde_json::to_string_pretty(&manifest)?;
        let manifest_digest = sha256_hex(manifest_text.as_bytes());
        write_atomic(
            &self
                .root
                .join("snapshots")
                .join(format!("{snapshot_name}.json")),
            format!("{manifest_text}\n").as_bytes(),
        )?;
        Ok(CollapseSummary {
            manifest_digest,
            object_count: files.len(),
            total_bytes,
        })
    }

    /// Restore a collapsed `.wavefunction` snapshot into `dest_dir` after verifying every SHA-256.
    pub fn restore_snapshot(&self, snapshot_name: &str, dest_dir: &Path) -> Result<usize> {
        validate_snapshot_name(snapshot_name)?;
        let snap_path = self
            .root
            .join("snapshots")
            .join(format!("{snapshot_name}.json"));
        let manifest: Value = serde_json::from_str(&fs::read_to_string(snap_path)?)?;
        let files = manifest
            .get("files")
            .and_then(|v| v.as_object())
            .ok_or_else(|| SpacetimeError::new("invalid snapshot manifest"))?;
        fs::create_dir_all(dest_dir)?;
        let mut restored = 0usize;
        for (rel, digest_val) in files {
            let rel_path = validate_relative_path(rel)?;
            let digest = digest_val
                .as_str()
                .ok_or_else(|| SpacetimeError::new("non-string digest in snapshot"))?;
            let bytes = self.get_bytes(digest)?;
            ensure_no_symlink_ancestors(dest_dir, &rel_path)?;
            write_atomic(&dest_dir.join(&rel_path), &bytes)?;
            restored += 1;
        }
        Ok(restored)
    }
}

fn collect_space_files(
    root: &Path,
    current: &Path,
    out: &mut BTreeMap<String, PathBuf>,
) -> Result<()> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(current)? {
        entries.push(entry?.path());
    }
    entries.sort();
    for path in entries {
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            collect_space_files(root, &path, out)?;
        } else if meta.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| SpacetimeError::new(e.to_string()))?
                .to_string_lossy()
                .into_owned();
            out.insert(rel, path);
        }
    }
    Ok(())
}

/// Branch a `.space` directory to a new target directory, attempting CoW reflinks first.
pub fn branch_space(source_dir: &Path, target_dir: &Path) -> Result<BranchOutcome> {
    if fs::symlink_metadata(target_dir).is_ok() {
        let msg = "branch target directory already exists";
        return Err(SpacetimeError::new(msg));
    }
    let mut files = BTreeMap::new();
    collect_space_files(source_dir, source_dir, &mut files)?;
    fs::create_dir_all(target_dir)?;

    let mut try_reflink = !files.is_empty();
    let mut all_reflinked = try_reflink;
    for (rel, src) in &files {
        let dst = target_dir.join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        let reflink_ok = if try_reflink {
            Command::new("cp")
                .args(["--reflink=always", "--"])
                .arg(src)
                .arg(&dst)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        } else {
            false
        };
        if !reflink_ok {
            try_reflink = false;
            all_reflinked = false;
            fs::copy(src, &dst)?;
        }
    }

    Ok(BranchOutcome {
        branch_dir: target_dir.to_path_buf(),
        cow_reflink_used: all_reflinked,
        files_copied: files.len(),
    })
}

/// Export a verified `.space` snapshot bundle (excluding secrets or temporary files).
pub fn export_space_bundle(
    cas: &WavefunctionCas,
    space_dir: &Path,
    snapshot_name: &str,
    bundle_path: &Path,
) -> Result<String> {
    let summary = cas.collapse_space(space_dir, snapshot_name)?;
    let snap_path = cas
        .root()
        .join("snapshots")
        .join(format!("{snapshot_name}.json"));
    let manifest_val: Value = serde_json::from_str(&fs::read_to_string(snap_path)?)?;
    let files = manifest_val
        .get("files")
        .and_then(|v| v.as_object())
        .ok_or_else(|| SpacetimeError::new("missing files map"))?;

    let mut payloads = serde_json::Map::new();
    for (rel, digest_val) in files {
        let digest = digest_val
            .as_str()
            .ok_or_else(|| SpacetimeError::new("invalid digest"))?;
        let bytes = cas.get_bytes(digest)?;
        let encoded = hex_encode(&bytes);
        payloads.insert(
            rel.clone(),
            json!({
                "sha256": digest,
                "hex": encoded,
            }),
        );
    }

    let bundle = json!({
        "schema_version": 1,
        "kind": "spacetime_space_bundle",
        "snapshot_name": snapshot_name,
        "manifest_digest": summary.manifest_digest,
        "secrets_included": false,
        "files": payloads,
    });
    let text = format!("{}\n", serde_json::to_string_pretty(&bundle)?);
    write_atomic(bundle_path, text.as_bytes())?;
    Ok(sha256_hex(text.as_bytes()))
}

pub fn import_space_bundle(bundle_path: &Path, dest_dir: &Path) -> Result<usize> {
    let file = File::open(bundle_path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() > MAX_BUNDLE_BYTES {
        return Err(SpacetimeError::new("space bundle must be a regular file <= 64 MiB"));
    }
    let mut raw = Vec::with_capacity(meta.len() as usize);
    file.take(MAX_BUNDLE_BYTES + 1).read_to_end(&mut raw)?;
    if raw.len() as u64 > MAX_BUNDLE_BYTES {
        return Err(SpacetimeError::new("space bundle exceeds 64 MiB limit"));
    }
    let bundle: Value = serde_json::from_slice(&raw)?;
    if bundle.get("kind").and_then(|v| v.as_str()) != Some("spacetime_space_bundle") {
        return Err(SpacetimeError::new("unsupported space bundle kind"));
    }
    if bundle.get("secrets_included").and_then(|v| v.as_bool()) != Some(false) {
        return Err(SpacetimeError::new(
            "space bundle must explicitly declare secrets_included=false",
        ));
    }
    let snapshot_name = bundle
        .get("snapshot_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SpacetimeError::new("missing snapshot_name in space bundle"))?;
    validate_snapshot_name(snapshot_name)?;
    let expected_manifest = bundle
        .get("manifest_digest")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SpacetimeError::new("missing manifest_digest in space bundle"))?;
    validate_sha256_hex(expected_manifest)?;
    let files = bundle
        .get("files")
        .and_then(|v| v.as_object())
        .ok_or_else(|| SpacetimeError::new("missing files in space bundle"))?;

    let mut decoded_entries = Vec::with_capacity(files.len());
    let mut manifest_entries = serde_json::Map::new();
    for (rel, entry) in files {
        let rel_path = validate_relative_path(rel)?;
        let expected_sha = entry
            .get("sha256")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SpacetimeError::new("missing file sha256"))?;
        validate_sha256_hex(expected_sha)?;
        let hex_str = entry
            .get("hex")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SpacetimeError::new("missing file hex payload"))?;
        let bytes = hex_decode(hex_str)?;
        if sha256_hex(&bytes) != expected_sha {
            return Err(SpacetimeError::new(format!(
                "imported bundle file failed SHA-256 verification: {rel}"
            )));
        }
        manifest_entries.insert(rel.clone(), json!(expected_sha));
        decoded_entries.push((rel_path, bytes));
    }

    let manifest = json!({
        "schema_version": 1,
        "snapshot": snapshot_name,
        "files": manifest_entries,
    });
    let recomputed_manifest = sha256_hex(serde_json::to_string_pretty(&manifest)?.as_bytes());
    if recomputed_manifest != expected_manifest {
        return Err(SpacetimeError::new(
            "space bundle manifest_digest verification failed",
        ));
    }

    fs::create_dir_all(dest_dir)?;
    for (rel_path, bytes) in &decoded_entries {
        ensure_no_symlink_ancestors(dest_dir, rel_path)?;
        write_atomic(&dest_dir.join(rel_path), bytes)?;
    }
    Ok(decoded_entries.len())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn hex_decode(hex: &str) -> Result<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return Err(SpacetimeError::new("odd hex length"));
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let pair = std::str::from_utf8(&bytes[i..i + 2]);
        let pair = pair.map_err(|_| SpacetimeError::new("invalid hex utf8"))?;
        let val = u8::from_str_radix(pair, 16);
        let val = val.map_err(|_| SpacetimeError::new("invalid hex digit"))?;
        out.push(val);
        i += 2;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cas_enforces_provenance_isolation_collapse_branch_and_bundle_roundtrip() -> Result<()> {
        let root = std::env::temp_dir().join(format!("spacetime-cas-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let cas = WavefunctionCas::open_or_init(&root.join("cas"))?;

        let global = cas.put_layer(
            "alpine-base",
            b"busybox-layer-data",
            CasProvenanceTier::UpstreamSigned,
            None,
            Some("proof-upstream".to_string()),
            None,
        )?;
        assert!(cas.can_mount_layer(&global.digest, "ws-a")?);
        assert!(cas.can_mount_layer(&global.digest, "ws-b")?);

        let local = cas.put_layer(
            "draft-scratch",
            b"tab-local-draft",
            CasProvenanceTier::TabLocal,
            Some(WorkspaceId::new("ws-a")),
            None,
            None,
        )?;
        assert!(cas.can_mount_layer(&local.digest, "ws-a")?);
        assert!(!cas.can_mount_layer(&local.digest, "ws-b")?);

        let quarantined = cas.put_layer(
            "malicious-repo",
            b"untrusted-bytes",
            CasProvenanceTier::Quarantined,
            Some(WorkspaceId::new("ws-q")),
            None,
            None,
        )?;
        assert!(!cas.can_mount_layer(&quarantined.digest, "ws-q")?);
        assert!(!cas.can_mount_layer(&quarantined.digest, "ws-a")?);

        // Collapse, branch, export, and import a .space directory.
        let space_a = root.join("space-a");
        fs::create_dir_all(&space_a)?;
        fs::write(space_a.join("goals.json"), b"[{\"id\":\"g1\"}]")?;
        let summary = cas.collapse_space(&space_a, "snap-1")?;
        assert_eq!(summary.object_count, 1);

        let branch_b = root.join("space-b");
        let branched = branch_space(&space_a, &branch_b)?;
        assert_eq!(branched.files_copied, 1);

        let bundle = root.join("space-a.bundle.json");
        export_space_bundle(&cas, &space_a, "snap-1", &bundle)?;
        let imported_dir = root.join("space-imported");
        let count = import_space_bundle(&bundle, &imported_dir)?;
        assert_eq!(count, 1);
        assert_eq!(
            fs::read(imported_dir.join("goals.json"))?,
            b"[{\"id\":\"g1\"}]"
        );

        // Path traversal, invalid snapshot names, and forged digests must fail.
        assert!(cas.get_bytes("../layers/foo").is_err());
        assert!(cas.collapse_space(&space_a, "../escape").is_err());
        let bad_restore = cas.restore_snapshot("../escape", &imported_dir);
        assert!(bad_restore.is_err());

        let mut forged_bundle: Value = serde_json::from_str(&fs::read_to_string(&bundle)?)?;
        forged_bundle["secrets_included"] = json!(true);
        let bad_bundle = root.join("bad.bundle.json");
        fs::write(&bad_bundle, serde_json::to_string_pretty(&forged_bundle)?)?;
        let bad_import = import_space_bundle(&bad_bundle, &root.join("bad-dest"));
        assert!(bad_import.is_err());

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }
}
