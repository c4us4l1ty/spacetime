//! Block 10: Explicit one-shot clipboard & file airlock.
//!
//! Workspaces never share an ambient clipboard or filesystem mount. Transfers across workspace
//! boundaries happen only through explicit, size-bounded, secret/canary-checked one-shot tickets.

use crate::security::HostVault;
use spacetime_types::{sha256_hex, Result, SpacetimeError, Taint};
use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_CLIPBOARD_BYTES: usize = 64 * 1024;
const MAX_AIRLOCK_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardTicket {
    pub ticket_id: String,
    pub source_workspace: String,
    pub source_taint: Taint,
    pub payload: String,
    pub payload_sha256: String,
    pub consumed: bool,
}

#[derive(Debug, Default)]
pub struct ClipboardAirlock {
    pending: Option<ClipboardTicket>,
    transfer_log: Vec<String>,
    next_id: u64,
}

impl ClipboardAirlock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stage_copy(
        &mut self,
        source_workspace: &str,
        source_taint: Taint,
        payload: &str,
        vault: &mut HostVault,
    ) -> Result<&ClipboardTicket> {
        if payload.len() > MAX_CLIPBOARD_BYTES {
            return Err(SpacetimeError::new(
                "clipboard airlock payload exceeds 64 KiB cap",
            ));
        }
        if payload.contains("@vault:") {
            return Err(SpacetimeError::new(
                "clipboard airlock refused @vault: symbolic secret handle",
            ));
        }
        vault.inspect_for_canary_or_secret(&format!("clipboard:{source_workspace}"), payload)?;
        self.next_id = self.next_id.saturating_add(1);
        let ticket = ClipboardTicket {
            ticket_id: format!("clip-{:06}", self.next_id),
            source_workspace: source_workspace.to_string(),
            source_taint,
            payload: payload.to_string(),
            payload_sha256: sha256_hex(payload.as_bytes()),
            consumed: false,
        };
        self.pending = Some(ticket);
        Ok(self.pending.as_ref().expect("staged ticket"))
    }

    /// Consume the one-shot clipboard ticket into `dest_workspace`. A second read without a fresh
    /// explicit copy fails because there is no ambient shared clipboard.
    pub fn paste_once(&mut self, dest_workspace: &str) -> Result<ClipboardTicket> {
        let mut ticket = self
            .pending
            .take()
            .ok_or_else(|| SpacetimeError::new("no explicit clipboard airlock grant pending"))?;
        ticket.consumed = true;
        self.transfer_log.push(format!(
            "{}: {} -> {} ({})",
            ticket.ticket_id, ticket.source_workspace, dest_workspace, ticket.payload_sha256
        ));
        Ok(ticket)
    }

    pub fn transfer_log(&self) -> &[String] {
        &self.transfer_log
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAirlockReceipt {
    pub source_workspace: String,
    pub dest_workspace: String,
    pub relative_name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub dest_path: PathBuf,
}

/// Transfer a single approved file across a workspace boundary without exposing directory trees or
/// following symlinks outside `source_root`.
pub fn airlock_copy_file(
    source_workspace: &str,
    source_root: &Path,
    relative_path: &Path,
    dest_workspace: &str,
    dest_root: &Path,
    vault: &mut HostVault,
) -> Result<FileAirlockReceipt> {
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(SpacetimeError::new(
            "file airlock rejects absolute or parent-traversal paths",
        ));
    }
    let file_name = relative_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| SpacetimeError::new("invalid airlock relative path"))?;
    if file_name.starts_with(".wh.") {
        return Err(SpacetimeError::new(
            "file airlock refuses OverlayFS whiteout markers",
        ));
    }

    let canonical_root = fs::canonicalize(source_root)?;
    let source_path = source_root.join(relative_path);
    let link_meta = fs::symlink_metadata(&source_path)?;
    if link_meta.file_type().is_symlink() {
        return Err(SpacetimeError::new(
            "file airlock refuses symlinks across workspace boundaries",
        ));
    }
    let canonical_src = fs::canonicalize(&source_path)?;
    if !canonical_src.starts_with(&canonical_root) || !canonical_src.is_file() {
        return Err(SpacetimeError::new(
            "file airlock source escaped workspace root or is not a regular file",
        ));
    }
    let meta = fs::metadata(&canonical_src)?;
    if meta.len() > MAX_AIRLOCK_FILE_BYTES {
        return Err(SpacetimeError::new(
            "file airlock payload exceeds 4 MiB safety cap",
        ));
    }
    let bytes = fs::read(&canonical_src)?;
    if let Ok(text) = std::str::from_utf8(&bytes) {
        vault.inspect_for_canary_or_secret(
            &format!("file_airlock:{source_workspace}:{file_name}"),
            text,
        )?;
    }
    let digest = sha256_hex(&bytes);
    fs::create_dir_all(dest_root)?;
    let dest_path = dest_root.join(file_name);
    fs::write(&dest_path, &bytes)?;

    Ok(FileAirlockReceipt {
        source_workspace: source_workspace.to_string(),
        dest_workspace: dest_workspace.to_string(),
        relative_name: file_name.to_string(),
        size_bytes: meta.len(),
        sha256: digest,
        dest_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airlock_enforces_one_shot_clipboard_canary_check_and_symlink_denial() -> Result<()> {
        let mut vault = HostVault::new();
        vault.plant_canary("can-1", "SECRET_CANARY_AIRLOCK")?;

        let mut clip = ClipboardAirlock::new();
        // Ambient paste before copy fails.
        assert!(clip.paste_once("ws-dest").is_err());
        // Canary or @vault: leaks fail.
        let bad_canary = clip.stage_copy(
            "ws-src",
            Taint::TrustedUser,
            "SECRET_CANARY_AIRLOCK",
            &mut vault,
        );
        assert!(bad_canary.is_err());
        let bad_vault = clip.stage_copy("ws-src", Taint::TrustedUser, "@vault:api_key", &mut vault);
        assert!(bad_vault.is_err());

        // Valid one-shot copy succeeds once and cannot be re-read ambiently.
        clip.stage_copy("ws-src", Taint::TrustedUser, "hello world", &mut vault)?;
        let ticket = clip.paste_once("ws-dest")?;
        assert_eq!(ticket.payload, "hello world");
        assert!(clip.paste_once("ws-other").is_err());

        // File airlock copies regular files and rejects symlinks / traversal.
        let root =
            std::env::temp_dir().join(format!("spacetime-airlock-test-{}", std::process::id()));
        let src_dir = root.join("src");
        let dst_dir = root.join("dst");
        fs::create_dir_all(&src_dir)?;
        fs::write(src_dir.join("draft.txt"), b"clean artifact")?;
        let receipt = airlock_copy_file(
            "ws-src",
            &src_dir,
            Path::new("draft.txt"),
            "ws-dest",
            &dst_dir,
            &mut vault,
        )?;
        assert_eq!(receipt.size_bytes, 14);
        assert!(airlock_copy_file(
            "ws-src",
            &src_dir,
            Path::new("../escape.txt"),
            "ws-dest",
            &dst_dir,
            &mut vault,
        )
        .is_err());
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }
}
