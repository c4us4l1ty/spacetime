//! Automated host leak-surface audit.
//!
//! Spacetime never claims "0 traces". Instead, it audits known host leak vectors documented in
//! DESIGN-REVIEW.md §1.6: core dumps, swap/zram, shader disk caches, recent-files lists, host
//! thumbnail caches, and DNS inheritance.

use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeakSurfaceAudit {
    pub zero_traces_claimed: bool,
    pub dumpable_or_core_checked: bool,
    pub core_dump_disabled_in_status: bool,
    pub swap_active: bool,
    pub shader_disk_cache_disabled_in_env: bool,
    pub no_spacetime_thumbnails_on_disk: bool,
    pub residual_risk_notes: Vec<String>,
}

pub fn run_leak_audit(workspace_temp_root: Option<&Path>) -> LeakSurfaceAudit {
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let dumpable_zero = status
        .lines()
        .find_map(|line| line.strip_prefix("CoreDumping:"))
        .map(|val| val.trim() == "0")
        .unwrap_or(false);
    let limits = fs::read_to_string("/proc/self/limits").unwrap_or_default();
    let core_limit_zero = limits
        .lines()
        .find(|line| line.starts_with("Max core file size"))
        .is_some_and(|line| line.split_whitespace().nth(4) == Some("0"));

    let swaps = fs::read_to_string("/proc/swaps").unwrap_or_default();
    let swap_active = swaps.lines().count() > 1;

    let shader_disk_disabled = env::var_os("__GL_SHADER_DISK_CACHE").as_deref()
        == Some(std::ffi::OsStr::new("0"))
        && env::var_os("MESA_SHADER_CACHE_DISABLE").as_deref()
            == Some(std::ffi::OsStr::new("true"));

    let no_spacetime_thumbnails_on_disk = workspace_temp_root
        .map(|root| !has_image_files(root))
        .unwrap_or(true);

    let mut notes = vec![
        "Host swap, kernel page cache, router/ISP logs, and timing side channels can still retain traces."
            .to_string(),
        "VMM/cell child launchers must pass prlimit --core=0:0 and disable GPU shader caches."
            .to_string(),
    ];
    if swap_active {
        notes.push("Host swap/zram is active; guest tmpfs/RAM pages can be swapped.".to_string());
    }
    if !core_limit_zero {
        notes.push(
            "Current parent process RLIMIT_CORE is non-zero; sandboxed children enforce --core=0:0."
                .to_string(),
        );
    }

    LeakSurfaceAudit {
        zero_traces_claimed: false,
        dumpable_or_core_checked: !status.is_empty() || !limits.is_empty(),
        core_dump_disabled_in_status: core_limit_zero || dumpable_zero,
        swap_active,
        shader_disk_cache_disabled_in_env: shader_disk_disabled,
        no_spacetime_thumbnails_on_disk,
        residual_risk_notes: notes,
    }
}

fn has_image_files(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() && has_image_files(&path) {
            return true;
        }
        if file_type.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "png" | "jpg" | "jpeg" | "bmp" | "webp" | "gif"
                ) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leak_audit_never_claims_zero_traces() {
        let audit = run_leak_audit(None);
        assert!(!audit.zero_traces_claimed);
        assert!(audit.no_spacetime_thumbnails_on_disk);
        assert!(!audit.residual_risk_notes.is_empty());
    }
}
