//! Machine baseline for the First Law.
//!
//! This reports cheap host facts only. It makes no RAM, CPU, boot-time, or latency claims.

use spacetime_types::Result;
use std::fs;
use std::process::Command;

pub fn baseline_toml() -> Result<String> {
    let meminfo = read("/proc/meminfo");
    let cpuinfo = read("/proc/cpuinfo");
    let swaps = read("/proc/swaps");
    let filesystems = read("/proc/filesystems");

    let kernel = uname().unwrap_or_else(|| "unmeasured".to_string());
    let cpu_model = value_after_colon(&cpuinfo, "model name")
        .unwrap_or_else(|| "unmeasured".to_string());
    let memory_total_kb = value_after_colon(&meminfo, "MemTotal")
        .and_then(|value| value.split_whitespace().next().map(str::to_string))
        .unwrap_or_else(|| "unmeasured".to_string());

    let swap_present = swaps.lines().count() > 1;
    let zram_detected =
        swaps.contains("zram") || fs::metadata("/sys/block/zram0").is_ok();
    let kvm_available = fs::metadata("/dev/kvm").is_ok();

    let mut cow = Vec::new();
    if filesystems.contains("btrfs") {
        cow.push("btrfs");
    }
    if filesystems.contains("xfs") {
        cow.push("xfs");
    }
    let cow_filesystems = if cow.is_empty() {
        "none".to_string()
    } else {
        cow.join(",")
    };

    Ok(format!(
        r#"schema_version = 1
measurement_status = "baseline_only_performance_claims_unmeasured"
target_profile = "4gb_potato"
resource_law = "holy_grail"
notes = "Host facts only. No RAM, CPU, boot, or latency claims are valid until measured on this machine."

[host]
os = "{}"
kernel = "{}"
arch = "{}"
cpu_model = "{}"
memory_total_kb = "{}"
kvm_available = {}
swap_present = {}
zram_detected = {}
cow_filesystems = "{}"
"#,
        quote(std::env::consts::OS),
        quote(&kernel),
        quote(std::env::consts::ARCH),
        quote(&cpu_model),
        quote(&memory_total_kb),
        kvm_available,
        swap_present,
        zram_detected,
        quote(&cow_filesystems),
    ))
}

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn uname() -> Option<String> {
    let output = Command::new("uname").args(["-s", "-r"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn value_after_colon(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (left, right) = line.split_once(':')?;
        if left.trim() == key {
            Some(right.trim().to_string())
        } else {
            None
        }
    })
}

fn quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_keeps_performance_claims_unmeasured() -> Result<()> {
        let toml = baseline_toml()?;
        assert!(toml.contains("resource_law = \"holy_grail\""));
        assert!(toml.contains("baseline_only_performance_claims_unmeasured"));
        assert!(!toml.contains("boot_ms"));
        Ok(())
    }
}
