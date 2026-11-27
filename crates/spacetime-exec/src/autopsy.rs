//! Quarantine upperdir diff autopsy and tombstone generation.
//!
//! Diffs an ephemeral upperdir against a recorded base manifest, records whiteouts, blocked
//! egress, and peak RSS into an `AutopsyReport`, writes a compact `TombstoneRecord`, and deletes
//! the ephemeral upperdir so untrusted artifacts are never retained.

use spacetime_types::surface::{AutopsyReport, TombstoneRecord};
use spacetime_types::{sha256_hex, Result, SpacetimeError, TrustTier, SCHEMA_VERSION};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub fn snapshot_dir_manifest(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut manifest = BTreeMap::new();
    if root.exists() {
        walk_manifest(root, root, &mut manifest)?;
    }
    Ok(manifest)
}

fn walk_manifest(
    root: &Path,
    current: &Path,
    manifest: &mut BTreeMap<String, String>,
) -> Result<()> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(current)? {
        entries.push(entry?.path());
    }
    entries.sort();
    for path in entries {
        let rel = path
            .strip_prefix(root)
            .map_err(|e| SpacetimeError::new(e.to_string()))?
            .to_string_lossy()
            .into_owned();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_dir() {
            manifest.insert(rel, "dir".to_string());
            walk_manifest(root, &path, manifest)?;
        } else if meta.file_type().is_file() {
            let bytes = fs::read(&path)?;
            manifest.insert(rel, sha256_hex(&bytes));
        } else {
            manifest.insert(rel, "special".to_string());
        }
    }
    Ok(())
}

pub fn build_autopsy_report(
    workspace_id: &str,
    trust_tier: TrustTier,
    base_manifest: &BTreeMap<String, String>,
    upper_dir: &Path,
    blocked_egress: Vec<String>,
    peak_rss_kb: u64,
    canary_reads: u64,
) -> Result<AutopsyReport> {
    let upper = snapshot_dir_manifest(upper_dir)?;
    let mut added_files = Vec::new();
    let mut modified_files = Vec::new();
    let mut deleted_files = Vec::new();
    let mut whiteouts = Vec::new();

    for (rel, digest) in &upper {
        let file_name = Path::new(rel)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if let Some(target) = file_name.strip_prefix(".wh.") {
            whiteouts.push(format!("{rel}->{target}"));
            continue;
        }
        match base_manifest.get(rel) {
            None => added_files.push(rel.clone()),
            Some(old) if old != digest => modified_files.push(rel.clone()),
            Some(_) => {}
        }
    }
    for rel in base_manifest.keys() {
        if !upper.contains_key(rel) {
            deleted_files.push(rel.clone());
        }
    }

    let summary_material = format!(
        "{workspace_id}|add={}|mod={}|del={}|wh={}|egress={}|canary={canary_reads}",
        added_files.join(","),
        modified_files.join(","),
        deleted_files.join(","),
        whiteouts.join(","),
        blocked_egress.join(","),
    );
    Ok(AutopsyReport {
        workspace_id: workspace_id.to_string(),
        trust_tier,
        added_files,
        modified_files,
        deleted_files,
        whiteouts,
        blocked_egress,
        peak_rss_kb,
        canary_reads,
        blast_radius_hash: sha256_hex(summary_material.as_bytes()),
    })
}

pub fn close_and_tombstone(
    workspace_id: &str,
    trust_tier: TrustTier,
    base_manifest: &BTreeMap<String, String>,
    upper_dir: &Path,
    blocked_egress: Vec<String>,
    peak_rss_kb: u64,
    canary_reads: u64,
) -> Result<(AutopsyReport, TombstoneRecord)> {
    let autopsy = build_autopsy_report(
        workspace_id,
        trust_tier,
        base_manifest,
        upper_dir,
        blocked_egress,
        peak_rss_kb,
        canary_reads,
    )?;
    if upper_dir.exists() {
        fs::remove_dir_all(upper_dir)?;
    }
    let summary = format!(
        "added={} modified={} deleted={} whiteouts={} blocked_egress={} canary_reads={} peak_rss_kb={}",
        autopsy.added_files.len(),
        autopsy.modified_files.len(),
        autopsy.deleted_files.len(),
        autopsy.whiteouts.len(),
        autopsy.blocked_egress.len(),
        autopsy.canary_reads,
        autopsy.peak_rss_kb,
    );
    let tombstone = TombstoneRecord {
        schema_version: SCHEMA_VERSION,
        workspace_id: workspace_id.to_string(),
        trust_tier,
        blast_radius_hash: autopsy.blast_radius_hash.clone(),
        summary,
        upperdir_removed: !upper_dir.exists(),
    };
    Ok((autopsy, tombstone))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autopsy_diffs_upperdir_detects_whiteouts_and_deletes_artifacts() -> Result<()> {
        let root =
            std::env::temp_dir().join(format!("spacetime-autopsy-test-{}", std::process::id()));
        let base_dir = root.join("base");
        let upper_dir = root.join("upper");
        fs::create_dir_all(&base_dir)?;
        fs::create_dir_all(&upper_dir)?;
        fs::write(base_dir.join("keep.txt"), b"v1")?;
        fs::write(base_dir.join("removed.txt"), b"gone")?;
        let base_manifest = snapshot_dir_manifest(&base_dir)?;

        fs::write(upper_dir.join("keep.txt"), b"v2-modified")?;
        fs::write(upper_dir.join("payload.bin"), b"dropped")?;
        fs::write(upper_dir.join(".wh.etc-crontab"), b"")?;

        let (autopsy, tombstone) = close_and_tombstone(
            "ws-quarantine-1",
            TrustTier::QuarantineMicroVm,
            &base_manifest,
            &upper_dir,
            vec!["185.0.0.1:4444".to_string()],
            12_345,
            1,
        )?;
        assert_eq!(autopsy.added_files, vec!["payload.bin".to_string()]);
        assert_eq!(autopsy.modified_files, vec!["keep.txt".to_string()]);
        assert_eq!(autopsy.deleted_files, vec!["removed.txt".to_string()]);
        let expected_wh = vec![".wh.etc-crontab->etc-crontab".to_string()];
        assert_eq!(autopsy.whiteouts, expected_wh);
        assert_eq!(autopsy.canary_reads, 1);
        assert!(tombstone.upperdir_removed);
        assert!(!upper_dir.exists());

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }
}
