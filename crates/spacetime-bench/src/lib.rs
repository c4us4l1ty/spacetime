//! Machine baseline and explicit resource gate for the First Law.
//!
//! Host facts are cheap and unmeasured; the resource gate stays unmeasured until a real
//! target-machine measurement file is supplied. No RAM, CPU, boot-time, or latency claim is made.

use spacetime_types::{sha256_hex, Result, SpacetimeError};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;

pub const TARGET_PROFILE: &str = "4gb_potato";
pub const TARGET_MEMORY_BUDGET_KB: u64 = 4 * 1024 * 1024;
pub const BOOT_BUDGET_MILLIS: u64 = 1_000;
pub const TAB_SWITCH_BUDGET_MILLIS: u64 = 1_000;
pub const CPU_BUDGET_PERCENT: u64 = 100;

pub const MEASUREMENT_FILE_LIMIT_BYTES: u64 = 64 * 1024;
pub const MEASUREMENT_LINE_LIMIT: usize = 64;
pub const MEASUREMENT_LINE_LIMIT_BYTES: usize = 512;
pub const MEASUREMENT_VALUE_LIMIT_BYTES: usize = 256;
pub const MEASUREMENT_HOST_LIMIT_BYTES: usize = 128;

const MEASUREMENT_KEYS: [&str; 7] = [
    "schema_version",
    "target_profile",
    "measurement_host",
    "peak_rss_kb",
    "boot_ms",
    "tab_switch_ms",
    "cpu_percent",
];

const NON_TARGET_HOST_MARKERS: [&str; 7] = [
    "github",
    "actions",
    "runner",
    "azure",
    "synthetic",
    "ci_host",
    "self_test",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceGateStatus {
    Unmeasured,
    Pass,
    Fail,
}

impl ResourceGateStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceGateStatus::Unmeasured => "unmeasured",
            ResourceGateStatus::Pass => "pass",
            ResourceGateStatus::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceGate {
    pub status: ResourceGateStatus,
    pub target_profile: String,
    pub memory_budget_kb: u64,
    pub measurements: BTreeMap<String, u64>,
    pub reasons: Vec<String>,
}

pub fn resource_gate(measurement_path: Option<&Path>) -> Result<ResourceGate> {
    let mut gate = ResourceGate {
        status: ResourceGateStatus::Unmeasured,
        target_profile: TARGET_PROFILE.to_string(),
        memory_budget_kb: TARGET_MEMORY_BUDGET_KB,
        measurements: BTreeMap::new(),
        reasons: vec!["no measurement file supplied; 4 GB potato machine unmeasured".to_string()],
    };
    let Some(path) = measurement_path else {
        return Ok(gate);
    };
    let text = fs::read_to_string(path)?;
    let mut metadata: BTreeMap<String, String> = BTreeMap::new();
    let mut measurements: BTreeMap<String, u64> = BTreeMap::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| SpacetimeError::new(format!("invalid measurement line: {line}")))?;
        let key = key.trim().to_string();
        let value = value.trim();
        if key == "schema_version" || key == "target_profile" || key == "measurement_host" {
            metadata.insert(key, value.to_string());
        } else {
            measurements.insert(key, value.parse::<u64>()?);
        }
    }
    gate.measurements = measurements;
    let mut fail_reasons = Vec::new();
    let mut unmeasured_reasons = Vec::new();
    if metadata.get("schema_version").map(String::as_str) != Some("1") {
        unmeasured_reasons.push("schema_version missing or unsupported".to_string());
    }
    match metadata.get("target_profile").map(String::as_str) {
        Some(profile) if profile == TARGET_PROFILE => {}
        Some(other) => fail_reasons.push(format!("wrong target_profile: {other}")),
        None => unmeasured_reasons.push("target_profile missing".to_string()),
    }
    let host = metadata
        .get("measurement_host")
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if host.is_empty()
        || NON_TARGET_HOST_MARKERS
            .iter()
            .any(|marker| host.contains(marker))
    {
        unmeasured_reasons.push("measurement_host missing or non-target host".to_string());
    }
    let required = ["peak_rss_kb", "boot_ms", "tab_switch_ms"];
    for key in required {
        if !gate.measurements.contains_key(key) {
            unmeasured_reasons.push(format!("missing measured value: {key}"));
        }
    }
    if let Some(&rss) = gate.measurements.get("peak_rss_kb") {
        if rss > TARGET_MEMORY_BUDGET_KB {
            fail_reasons.push(format!("peak_rss_kb {rss} exceeds 4 GB budget"));
        }
    }
    if let Some(&boot) = gate.measurements.get("boot_ms") {
        if boot > BOOT_BUDGET_MILLIS {
            fail_reasons.push(format!("boot_ms {boot} exceeds 1 s budget"));
        }
    }
    if let Some(&tab) = gate.measurements.get("tab_switch_ms") {
        if tab > TAB_SWITCH_BUDGET_MILLIS {
            fail_reasons.push(format!("tab_switch_ms {tab} exceeds 1 s budget"));
        }
    }
    if let Some(&cpu) = gate.measurements.get("cpu_percent") {
        if cpu > CPU_BUDGET_PERCENT {
            fail_reasons.push(format!("cpu_percent {cpu} exceeds 100"));
        }
    }
    gate.status = if !fail_reasons.is_empty() {
        ResourceGateStatus::Fail
    } else if !unmeasured_reasons.is_empty() {
        ResourceGateStatus::Unmeasured
    } else {
        ResourceGateStatus::Pass
    };
    gate.reasons = fail_reasons.into_iter().chain(unmeasured_reasons).collect();
    Ok(gate)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceGateIntake {
    pub file_supplied: bool,
    pub status: ResourceGateStatus,
    pub digest: String,
    pub measurements: BTreeMap<String, u64>,
    pub reasons: Vec<String>,
}

/// Bounded measurement intake for `verify --measurement`.
///
/// A file that passes the evaluator's shape and budget arithmetic is only a shape pass. Physical
/// provenance must be recorded out of band, so this function never establishes a measured gate.
pub fn resource_gate_intake(measurement_path: Option<&Path>) -> ResourceGateIntake {
    let mut intake = ResourceGateIntake {
        file_supplied: measurement_path.is_some(),
        status: ResourceGateStatus::Unmeasured,
        digest: "none".to_string(),
        measurements: BTreeMap::new(),
        reasons: vec!["no measurement file supplied; 4 GB potato machine unmeasured".to_string()],
    };
    let Some(path) = measurement_path else {
        return intake;
    };
    let text = match read_bounded_measurement(path) {
        Ok(text) => text,
        Err(reason) => {
            intake.reasons = vec![reason];
            return intake;
        }
    };
    intake.digest = sha256_hex(text.as_bytes());
    if let Err(reason) = validate_measurement_shape(&text) {
        intake.reasons = vec![reason];
        return intake;
    }
    match resource_gate(Some(path)) {
        Ok(gate) => {
            intake.measurements = gate.measurements;
            if gate.status == ResourceGateStatus::Pass {
                intake.status = ResourceGateStatus::Pass;
                let reason = "shape passed budgets; physical provenance out of band; no measured gate";
                intake.reasons = vec![reason.to_string()];
            } else {
                let mut reasons = gate.reasons;
                reasons.push(
                    "measurement file does not establish a measured resource gate".to_string(),
                );
                intake.reasons = reasons;
            }
        }
        Err(_) => {
            intake.reasons = vec!["measurement evaluator rejected the file".to_string()];
        }
    }
    intake
}

fn read_bounded_measurement(path: &Path) -> std::result::Result<String, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return Err("measurement file is unreadable".to_string()),
    };
    if !metadata.is_file() {
        return Err("measurement path is not a regular file".to_string());
    }
    if metadata.len() > MEASUREMENT_FILE_LIMIT_BYTES {
        return Err("measurement file exceeds 64 KiB bound".to_string());
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Err("measurement file is unreadable".to_string()),
    };
    if bytes.len() as u64 > MEASUREMENT_FILE_LIMIT_BYTES {
        return Err("measurement file exceeds 64 KiB bound".to_string());
    }
    String::from_utf8(bytes).map_err(|_| "measurement file is not valid UTF-8".to_string())
}

fn validate_measurement_shape(text: &str) -> std::result::Result<(), String> {
    let mut seen = BTreeSet::new();
    let mut lines = 0usize;
    for raw_line in text.lines() {
        lines += 1;
        if lines > MEASUREMENT_LINE_LIMIT {
            return Err("measurement file exceeds line bound".to_string());
        }
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.len() > MEASUREMENT_LINE_LIMIT_BYTES {
            return Err("measurement line exceeds bound".to_string());
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| "measurement line is not key=value".to_string())?;
        let key = key.trim();
        let value = value.trim();
        if !MEASUREMENT_KEYS.contains(&key) {
            return Err("unsupported measurement key".to_string());
        }
        if !seen.insert(key) {
            return Err("duplicate measurement key".to_string());
        }
        if value.is_empty() || value.len() > MEASUREMENT_VALUE_LIMIT_BYTES {
            return Err("measurement value is empty or too long".to_string());
        }
        if !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
        }) {
            return Err("measurement value contains unsupported characters".to_string());
        }
        if key == "measurement_host" && value.len() > MEASUREMENT_HOST_LIMIT_BYTES {
            return Err("measurement host exceeds bound".to_string());
        }
        if matches!(
            key,
            "peak_rss_kb" | "boot_ms" | "tab_switch_ms" | "cpu_percent"
        ) {
            value
                .parse::<u64>()
                .map_err(|_| "measurement value is not a bounded unsigned integer".to_string())?;
        }
    }
    Ok(())
}

pub fn baseline_toml() -> Result<String> {
    let meminfo = read("/proc/meminfo");
    let cpuinfo = read("/proc/cpuinfo");
    let swaps = read("/proc/swaps");
    let filesystems = read("/proc/filesystems");

    let kernel = uname().unwrap_or_else(|| "unmeasured".to_string());
    let cpu_model =
        value_after_colon(&cpuinfo, "model name").unwrap_or_else(|| "unmeasured".to_string());
    let memory_total_kb = value_after_colon(&meminfo, "MemTotal")
        .and_then(|value| value.split_whitespace().next().map(str::to_string))
        .unwrap_or_else(|| "unmeasured".to_string());

    let swap_present = swaps.lines().count() > 1;
    let zram_detected = swaps.contains("zram") || fs::metadata("/sys/block/zram0").is_ok();
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
    let mut out = String::with_capacity(value.len() + 2);
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control.is_control() => {
                let code = u32::from(control);
                let _ = write!(out, "\\u{code:04X}");
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intake_is_bounded_and_shape_pass_is_not_a_measurement() -> Result<()> {
        let none = resource_gate_intake(None);
        assert!(!none.file_supplied);
        assert_eq!(none.status, ResourceGateStatus::Unmeasured);
        assert_eq!(none.digest, "none");
        assert!(none.reasons.iter().any(|r| r.contains("no measurement")));

        let malformed = temp_measurement("intake-malformed", "not-key-value\n")?;
        let bad = resource_gate_intake(Some(malformed.as_path()));
        let _ = fs::remove_file(&malformed);
        assert!(bad.file_supplied);
        assert_eq!(bad.status, ResourceGateStatus::Unmeasured);
        assert!(bad.reasons.iter().any(|r| r.contains("not key=value")));

        let oversize = temp_measurement("intake-oversize", &"a".repeat(70_000))?;
        let big = resource_gate_intake(Some(oversize.as_path()));
        let _ = fs::remove_file(&oversize);
        assert_eq!(big.status, ResourceGateStatus::Unmeasured);
        assert!(big.reasons.iter().any(|r| r.contains("64 KiB bound")));

        let ci = temp_measurement(
            "intake-ci-host",
            "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=github_actions\npeak_rss_kb=3000000\nboot_ms=900\ntab_switch_ms=900\n",
        )?;
        let ci_gate = resource_gate_intake(Some(ci.as_path()));
        let _ = fs::remove_file(&ci);
        assert_eq!(ci_gate.status, ResourceGateStatus::Unmeasured);
        assert!(ci_gate.reasons.iter().any(|r| r.contains("non-target host")));

        let shape = temp_measurement(
            "intake-shape",
            "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=potato-target-lab\npeak_rss_kb=3000000\nboot_ms=900\ntab_switch_ms=900\ncpu_percent=80\n",
        )?;
        let pass = resource_gate_intake(Some(shape.as_path()));
        let _ = fs::remove_file(&shape);
        assert_eq!(pass.status, ResourceGateStatus::Pass);
        assert_ne!(pass.digest, "none");
        assert!(pass
            .reasons
            .iter()
            .any(|r| r.contains("provenance out of band")));
        Ok(())
    }

    #[test]
    fn baseline_keeps_performance_claims_unmeasured() -> Result<()> {
        let toml = baseline_toml()?;
        assert!(toml.contains("resource_law = \"holy_grail\""));
        assert!(toml.contains("baseline_only_performance_claims_unmeasured"));
        assert!(!toml.contains("boot_ms"));
        Ok(())
    }

    #[test]
    fn quote_escapes_toml_basic_strings() {
        assert_eq!(quote("a\\b\"c\n"), "a\\\\b\\\"c\\n");
        assert_eq!(quote("\u{1}"), "\\u0001");
    }

    fn temp_measurement(name: &str, content: &str) -> Result<std::path::PathBuf> {
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("spacetime-bench-{name}-{pid}"));
        fs::write(&path, content)?;
        Ok(path)
    }

    #[test]
    fn resource_gate_is_unmeasured_without_measurements() -> Result<()> {
        let gate = resource_gate(None)?;
        assert_eq!(gate.status, ResourceGateStatus::Unmeasured);
        assert_eq!(gate.target_profile, TARGET_PROFILE);
        assert!(gate.measurements.is_empty());
        assert!(gate
            .reasons
            .iter()
            .any(|reason| reason.contains("no measurement")));
        Ok(())
    }

    #[test]
    fn resource_gate_rejects_ci_host_and_wrong_profile() -> Result<()> {
        let path = temp_measurement(
            "ci-host",
            "schema_version=1\ntarget_profile=github_actions\nmeasurement_host=github_actions\npeak_rss_kb=100\nboot_ms=10\ntab_switch_ms=10\n",
        )?;
        let gate = resource_gate(Some(path.as_path()))?;
        let _ = fs::remove_file(&path);
        assert_eq!(gate.status, ResourceGateStatus::Fail);
        assert!(gate
            .reasons
            .iter()
            .any(|reason| reason.contains("wrong target_profile")));
        assert!(gate
            .reasons
            .iter()
            .any(|reason| reason.contains("non-target host")));
        Ok(())
    }

    #[test]
    fn resource_gate_rejects_synthetic_host_label() -> Result<()> {
        let path = temp_measurement(
            "synthetic-host",
            "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=synthetic-target-lab\npeak_rss_kb=3000000\nboot_ms=900\ntab_switch_ms=900\ncpu_percent=80\n",
        )?;
        let gate = resource_gate(Some(path.as_path()))?;
        let _ = fs::remove_file(&path);
        assert_eq!(gate.status, ResourceGateStatus::Unmeasured);
        assert!(gate
            .reasons
            .iter()
            .any(|reason| reason.contains("non-target host")));
        Ok(())
    }

    #[test]
    fn resource_gate_passes_only_with_explicit_target_measurements() -> Result<()> {
        let path = temp_measurement(
            "within-budget",
            "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=potato-target-lab\npeak_rss_kb=3000000\nboot_ms=900\ntab_switch_ms=900\ncpu_percent=80\n",
        )?;
        let gate = resource_gate(Some(path.as_path()))?;
        let _ = fs::remove_file(&path);
        assert_eq!(gate.status, ResourceGateStatus::Pass);
        assert!(gate.reasons.is_empty());
        Ok(())
    }

    #[test]
    fn resource_gate_fails_over_budget() -> Result<()> {
        let path = temp_measurement(
            "over-budget",
            "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=potato-target-lab\npeak_rss_kb=99999999\nboot_ms=900\ntab_switch_ms=900\n",
        )?;
        let gate = resource_gate(Some(path.as_path()))?;
        let _ = fs::remove_file(&path);
        assert_eq!(gate.status, ResourceGateStatus::Fail);
        assert!(gate
            .reasons
            .iter()
            .any(|reason| reason.contains("4 GB budget")));
        Ok(())
    }
}
