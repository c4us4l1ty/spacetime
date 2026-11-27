//! Machine baseline, explicit target profiles, and resource gate for the First Law.
//!
//! The 4 GB potato profile is the release baseline, not the identity of every machine. Public
//! builds may record additional registered profiles, but a larger profile never replaces the
//! baseline gate. Host facts and measurement-file shapes are unmeasured; physical provenance is
//! out of band. No RAM, CPU, boot-time, or latency claim is made.

pub mod econbench;
pub mod economy;

use spacetime_types::{sha256_hex, Result, SpacetimeError};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;

pub const BASELINE_TARGET_PROFILE: &str = "4gb_potato";
pub const TARGET_PROFILE: &str = BASELINE_TARGET_PROFILE;
pub const TARGET_MEMORY_BUDGET_KB: u64 = 4 * 1024 * 1024;
pub const BOOT_BUDGET_MILLIS: u64 = 1_000;
pub const TAB_SWITCH_BUDGET_MILLIS: u64 = 1_000;
pub const CPU_BUDGET_PERCENT: u64 = 100;
pub const PROFILE_REGISTRY_TSV: &str = include_str!("../../../bench/targets/profiles.tsv");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileRole {
    BaselineGate,
    OptionalProfile,
}

impl ProfileRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProfileRole::BaselineGate => "baseline_gate",
            ProfileRole::OptionalProfile => "optional_profile",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetProfile<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub memory_budget_kb: u64,
    pub boot_budget_ms: u64,
    pub tab_switch_budget_ms: u64,
    pub cpu_budget_percent: u64,
    pub role: ProfileRole,
}

const FALLBACK_BASELINE_PROFILE: TargetProfile<'static> = TargetProfile {
    id: BASELINE_TARGET_PROFILE,
    label: "4 GB",
    memory_budget_kb: TARGET_MEMORY_BUDGET_KB,
    boot_budget_ms: BOOT_BUDGET_MILLIS,
    tab_switch_budget_ms: TAB_SWITCH_BUDGET_MILLIS,
    cpu_budget_percent: CPU_BUDGET_PERCENT,
    role: ProfileRole::BaselineGate,
};

pub fn target_profiles() -> Vec<TargetProfile<'static>> {
    PROFILE_REGISTRY_TSV
        .lines()
        .filter_map(parse_profile_line)
        .collect()
}

fn parse_profile_line(line: &'static str) -> Option<TargetProfile<'static>> {
    let line = line.strip_suffix('\u{000d}').unwrap_or(line);
    if line.trim().is_empty() || line.starts_with('#') {
        return None;
    }
    let mut parts = line.split('\t');
    let id = parts.next()?.trim();
    let label = parts.next()?.trim();
    let memory_budget_kb = parts.next()?.trim().parse().ok()?;
    let boot_budget_ms = parts.next()?.trim().parse().ok()?;
    let tab_switch_budget_ms = parts.next()?.trim().parse().ok()?;
    let cpu_budget_percent = parts.next()?.trim().parse().ok()?;
    let role = match parts.next()?.trim() {
        "baseline_gate" => ProfileRole::BaselineGate,
        "optional_profile" => ProfileRole::OptionalProfile,
        _ => return None,
    };
    if parts.next().is_some() {
        return None;
    }
    Some(TargetProfile {
        id,
        label,
        memory_budget_kb,
        boot_budget_ms,
        tab_switch_budget_ms,
        cpu_budget_percent,
        role,
    })
}

pub fn profile_by_id(id: &str) -> Option<TargetProfile<'static>> {
    target_profiles()
        .into_iter()
        .find(|profile| profile.id == id)
}

pub fn baseline_profile() -> TargetProfile<'static> {
    profile_by_id(BASELINE_TARGET_PROFILE).unwrap_or(FALLBACK_BASELINE_PROFILE)
}

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
    pub baseline_profile: String,
    pub profile_role: ProfileRole,
    pub memory_budget_kb: u64,
    pub measurements: BTreeMap<String, u64>,
    pub reasons: Vec<String>,
}

pub fn resource_gate(measurement_path: Option<&Path>) -> Result<ResourceGate> {
    let baseline = baseline_profile();
    let mut gate = ResourceGate {
        status: ResourceGateStatus::Unmeasured,
        target_profile: baseline.id.to_string(),
        baseline_profile: baseline.id.to_string(),
        profile_role: baseline.role,
        memory_budget_kb: baseline.memory_budget_kb,
        measurements: BTreeMap::new(),
        reasons: vec![
            "no measurement file supplied; baseline 4 GB potato machine unmeasured".to_string(),
        ],
    };
    let Some(path) = measurement_path else {
        return Ok(gate);
    };
    let text = fs::read_to_string(path)?;
    let mut metadata: BTreeMap<String, String> = BTreeMap::new();
    let mut measurements: BTreeMap<String, u64> = BTreeMap::new();
    let mut seen = BTreeSet::new();
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
        if !seen.insert(key.clone()) {
            return Err(SpacetimeError::new(format!(
                "duplicate measurement key: {key}"
            )));
        }
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
    let profile_id = metadata.get("target_profile").map(String::as_str);
    let selected_profile = profile_id.and_then(profile_by_id);
    let budget_profile = selected_profile.unwrap_or(baseline);
    match (profile_id, selected_profile) {
        (Some(_), Some(profile)) => {
            gate.target_profile = profile.id.to_string();
            gate.profile_role = profile.role;
            gate.memory_budget_kb = profile.memory_budget_kb;
        }
        (Some(other), None) => fail_reasons.push(format!("wrong target_profile: {other}")),
        (None, _) => unmeasured_reasons.push("target_profile missing".to_string()),
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
        if rss > budget_profile.memory_budget_kb {
            fail_reasons.push(format!(
                "peak_rss_kb {rss} exceeds {} budget",
                budget_profile.label
            ));
        }
    }
    if let Some(&boot) = gate.measurements.get("boot_ms") {
        if boot > budget_profile.boot_budget_ms {
            fail_reasons.push(format!(
                "boot_ms {boot} exceeds {} ms budget",
                budget_profile.boot_budget_ms
            ));
        }
    }
    if let Some(&tab) = gate.measurements.get("tab_switch_ms") {
        if tab > budget_profile.tab_switch_budget_ms {
            fail_reasons.push(format!(
                "tab_switch_ms {tab} exceeds {} ms budget",
                budget_profile.tab_switch_budget_ms
            ));
        }
    }
    if let Some(&cpu) = gate.measurements.get("cpu_percent") {
        if cpu > budget_profile.cpu_budget_percent {
            fail_reasons.push(format!(
                "cpu_percent {cpu} exceeds {} percent budget",
                budget_profile.cpu_budget_percent
            ));
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

/// Bounded evaluator for `machine gate --measurement`. It preserves budget-arithmetic
/// failures but never establishes physical provenance.
pub fn bounded_resource_gate(measurement_path: Option<&Path>) -> Result<ResourceGate> {
    let Some(path) = measurement_path else {
        return resource_gate(None);
    };
    let text = read_bounded_measurement(path).map_err(SpacetimeError::new)?;
    validate_measurement_shape(&text).map_err(SpacetimeError::new)?;
    resource_gate(Some(path))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceGateIntake {
    pub file_supplied: bool,
    pub status: ResourceGateStatus,
    pub digest: String,
    pub target_profile: String,
    pub baseline_profile: String,
    pub profile_role: ProfileRole,
    pub memory_budget_kb: u64,
    pub measurements: BTreeMap<String, u64>,
    pub reasons: Vec<String>,
}

/// Bounded measurement intake for `verify --measurement`.
///
/// A file that passes the evaluator's shape and budget arithmetic is only a shape pass. Physical
/// provenance must be recorded out of band, so this function never establishes a measured gate.
pub fn resource_gate_intake(measurement_path: Option<&Path>) -> ResourceGateIntake {
    let baseline = baseline_profile();
    let mut intake = ResourceGateIntake {
        file_supplied: measurement_path.is_some(),
        status: ResourceGateStatus::Unmeasured,
        digest: "none".to_string(),
        target_profile: baseline.id.to_string(),
        baseline_profile: baseline.id.to_string(),
        profile_role: baseline.role,
        memory_budget_kb: baseline.memory_budget_kb,
        measurements: BTreeMap::new(),
        reasons: vec![
            "no measurement file supplied; baseline 4 GB potato machine unmeasured".to_string(),
        ],
    };
    let Some(path) = measurement_path else {
        return intake;
    };
    let text = match read_bounded_measurement(path) {
        Ok(text) => text,
        Err(reason) => {
            intake.digest = "unavailable".to_string();
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
            intake.measurements = gate.measurements.clone();
            intake.target_profile = gate.target_profile.clone();
            intake.baseline_profile = gate.baseline_profile.clone();
            intake.profile_role = gate.profile_role;
            intake.memory_budget_kb = gate.memory_budget_kb;
            if gate.status == ResourceGateStatus::Pass {
                intake.status = ResourceGateStatus::Pass;
                let reason =
                    "shape passed budgets; physical provenance out of band; no measured gate";
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceGateStatus {
    Unmeasured,
    VerifiedTarget4Gb,
    VerifiedOptionalProfileBaselinePending,
    Rejected,
}

impl ProvenanceGateStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unmeasured => "unmeasured_target_4gb",
            Self::VerifiedTarget4Gb => "measured_target_4gb",
            Self::VerifiedOptionalProfileBaselinePending => "measured_optional_baseline_pending",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceRecord {
    pub target_profile: String,
    pub measurement_host: String,
    pub measurement_sha256: String,
    pub host_facts_sha256: String,
    pub hardware_model: String,
    pub installed_ram_mb: u64,
    pub capture_utc: String,
    pub operator: String,
    pub reviewer: String,
    pub workload_protocol: String,
    pub baseline_4gb_evidence_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceGateEvaluation {
    pub status: ProvenanceGateStatus,
    pub measurement_digest: String,
    pub provenance_digest: String,
    pub target_profile: String,
    pub reasons: Vec<String>,
}

fn is_valid_hex64(digest: &str) -> bool {
    digest.len() == 64
        && digest.bytes().all(|b| b.is_ascii_hexdigit())
        && !digest.bytes().all(|b| b == b'0')
}

fn has_disallowed_marker(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.is_empty()
        || lower.contains("<fill")
        || lower == "pending"
        || lower == "none"
        || lower == "unspecified"
        || lower == "unknown"
        || NON_TARGET_HOST_MARKERS.iter().any(|m| lower.contains(m))
}

/// Evaluate a measurement file paired with an out-of-band provenance attestation record.
/// Without both a passing measurement and a consistent, non-synthetic provenance bundle, the
/// baseline 4 GB gate remains unmeasured (`ProvenanceGateStatus::Unmeasured` / `Rejected`).
pub fn evaluate_provenance_gate(
    measurement_path: Option<&Path>,
    provenance_path: Option<&Path>,
) -> ProvenanceGateEvaluation {
    let intake = resource_gate_intake(measurement_path);
    let Some(prov_path) = provenance_path else {
        return ProvenanceGateEvaluation {
            status: ProvenanceGateStatus::Unmeasured,
            measurement_digest: intake.digest,
            provenance_digest: "none".to_string(),
            target_profile: intake.target_profile,
            reasons: vec![
                "no provenance bundle supplied; resource_gate stays unmeasured".to_string(),
            ],
        };
    };

    if intake.status != ResourceGateStatus::Pass || !intake.file_supplied {
        return ProvenanceGateEvaluation {
            status: ProvenanceGateStatus::Rejected,
            measurement_digest: intake.digest,
            provenance_digest: "unavailable".to_string(),
            target_profile: intake.target_profile,
            reasons: intake.reasons,
        };
    }

    let raw = match read_bounded_measurement(prov_path) {
        Ok(text) => text,
        Err(reason) => {
            return ProvenanceGateEvaluation {
                status: ProvenanceGateStatus::Rejected,
                measurement_digest: intake.digest,
                provenance_digest: "unavailable".to_string(),
                target_profile: intake.target_profile,
                reasons: vec![reason],
            };
        }
    };
    let provenance_digest = sha256_hex(raw.as_bytes());
    let mut kv = BTreeMap::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((k, v)) = trimmed.split_once('=') else {
            return ProvenanceGateEvaluation {
                status: ProvenanceGateStatus::Rejected,
                measurement_digest: intake.digest,
                provenance_digest,
                target_profile: intake.target_profile,
                reasons: vec!["provenance line is not key=value".to_string()],
            };
        };
        if kv.insert(k.trim().to_string(), v.trim().to_string()).is_some() {
            return ProvenanceGateEvaluation {
                status: ProvenanceGateStatus::Rejected,
                measurement_digest: intake.digest,
                provenance_digest,
                target_profile: intake.target_profile,
                reasons: vec!["duplicate provenance key".to_string()],
            };
        }
    }

    let mut reasons = Vec::new();
    if kv.get("schema_version").map(String::as_str) != Some("1") {
        reasons.push("provenance schema_version must equal 1".to_string());
    }
    let prov_profile = kv.get("target_profile").cloned().unwrap_or_default();
    if prov_profile != intake.target_profile {
        reasons.push("provenance target_profile does not match measurement file".to_string());
    }
    let meas_sha = kv.get("measurement_sha256").map(String::as_str).unwrap_or("");
    if !is_valid_hex64(meas_sha) || meas_sha != intake.digest {
        reasons.push("provenance measurement_sha256 does not match measurement digest".to_string());
    }
    let facts_sha = kv.get("host_facts_sha256").map(String::as_str).unwrap_or("");
    if !is_valid_hex64(facts_sha) {
        reasons.push("provenance host_facts_sha256 must be a valid non-zero sha256".to_string());
    }
    for required_field in [
        "measurement_host",
        "hardware_model",
        "capture_utc",
        "operator",
        "reviewer",
        "workload_protocol",
    ] {
        let val = kv.get(required_field).map(String::as_str).unwrap_or("");
        if has_disallowed_marker(val) {
            reasons.push(format!(
                "provenance field {required_field} is missing or contains placeholder/non-target marker"
            ));
        }
    }
    if kv
        .get("operator")
        .zip(kv.get("reviewer"))
        .is_some_and(|(op, rev)| op.eq_ignore_ascii_case(rev))
    {
        reasons.push("provenance operator and reviewer must be distinct".to_string());
    }
    match kv
        .get("installed_ram_mb")
        .and_then(|v| v.parse::<u64>().ok())
    {
        Some(mb) if mb >= 1024 => {}
        _ => {
            reasons.push("provenance installed_ram_mb must be an integer >= 1024".to_string());
        }
    }

    if !reasons.is_empty() {
        return ProvenanceGateEvaluation {
            status: ProvenanceGateStatus::Rejected,
            measurement_digest: intake.digest,
            provenance_digest,
            target_profile: intake.target_profile,
            reasons,
        };
    }

    let status = if intake.target_profile == BASELINE_TARGET_PROFILE
        || kv
            .get("baseline_4gb_evidence_sha256")
            .is_some_and(|h| is_valid_hex64(h))
    {
        ProvenanceGateStatus::VerifiedTarget4Gb
    } else {
        ProvenanceGateStatus::VerifiedOptionalProfileBaselinePending
    };

    ProvenanceGateEvaluation {
        status,
        measurement_digest: intake.digest,
        provenance_digest,
        target_profile: intake.target_profile,
        reasons,
    }
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
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
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
    let baseline = baseline_profile();
    let profile_count = target_profiles().len();

    Ok(format!(
        r#"schema_version = 1
measurement_status = "host_facts_only_performance_claims_unmeasured"
host_profile = "unspecified"
release_baseline_profile = "{}"
target_profile_registry = "bench/targets/profiles.tsv"
profile_count = {}
resource_law = "holy_grail"
notes = "Host facts only. No target profile is inferred from RAM size; no RAM, CPU, boot, or latency claim is valid until measured on a recorded machine."

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
        quote(baseline.id),
        profile_count,
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
        assert!(ci_gate
            .reasons
            .iter()
            .any(|r| r.contains("non-target host")));

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
        assert!(toml.contains("host_facts_only_performance_claims_unmeasured"));
        assert!(toml.contains("host_profile = \"unspecified\""));
        assert!(toml.contains("release_baseline_profile = \"4gb_potato\""));
        assert!(toml.contains("profile_count = 3"));
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
    fn resource_gate_rejects_duplicate_metadata_keys() -> Result<()> {
        let path = temp_measurement(
            "duplicate-host",
            "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=github_actions\nmeasurement_host=potato-target-lab\npeak_rss_kb=3000000\nboot_ms=900\ntab_switch_ms=900\n",
        )?;
        let result = resource_gate(Some(path.as_path()));
        let _ = fs::remove_file(&path);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn bounded_resource_gate_rejects_oversize_file() -> Result<()> {
        let path = temp_measurement("bounded-oversize", &"a".repeat(70_000))?;
        let result = bounded_resource_gate(Some(path.as_path()));
        let _ = fs::remove_file(&path);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn intake_marks_unreadable_digest_unavailable() {
        let missing = std::env::temp_dir().join(format!(
            "spacetime-missing-measurement-{}",
            std::process::id()
        ));
        let _ = fs::remove_file(&missing);
        let intake = resource_gate_intake(Some(missing.as_path()));
        assert!(intake.file_supplied);
        assert_eq!(intake.status, ResourceGateStatus::Unmeasured);
        assert_eq!(intake.digest, "unavailable");
        assert!(intake.reasons.iter().any(|r| r.contains("unreadable")));
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

    #[test]
    fn target_profile_registry_has_one_baseline_and_public_profiles() {
        let profiles = target_profiles();
        assert_eq!(profiles.len(), 3);
        let baseline = baseline_profile();
        assert_eq!(baseline.id, BASELINE_TARGET_PROFILE);
        assert_eq!(baseline.role, ProfileRole::BaselineGate);
        assert_eq!(
            profiles
                .iter()
                .filter(|profile| profile.role == ProfileRole::BaselineGate)
                .count(),
            1
        );
        assert!(profiles
            .iter()
            .all(|profile| profile.memory_budget_kb >= baseline.memory_budget_kb));
        assert!(profile_by_id("8gb_reference").is_some());
        assert!(profile_by_id("16gb_developer").is_some());
        let mut ids: Vec<&str> = profiles.iter().map(|profile| profile.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), profiles.len());
    }

    #[test]
    fn resource_gate_uses_selected_public_profile_budget() -> Result<()> {
        let path = temp_measurement(
            "public-profile",
            "schema_version=1\ntarget_profile=8gb_reference\nmeasurement_host=public-target-lab\npeak_rss_kb=6000000\nboot_ms=900\ntab_switch_ms=900\ncpu_percent=80\n",
        )?;
        let gate = resource_gate(Some(path.as_path()))?;
        let _ = fs::remove_file(&path);
        assert_eq!(gate.status, ResourceGateStatus::Pass);
        assert_eq!(gate.target_profile, "8gb_reference");
        assert_eq!(gate.baseline_profile, BASELINE_TARGET_PROFILE);
        assert_eq!(gate.profile_role, ProfileRole::OptionalProfile);
        assert_eq!(gate.memory_budget_kb, 8 * 1024 * 1024);
        Ok(())
    }

    #[test]
    fn provenance_gate_requires_matching_digest_and_attestation() -> Result<()> {
        let meas_content = "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=potato-target-lab\npeak_rss_kb=3000000\nboot_ms=900\ntab_switch_ms=900\ncpu_percent=80\n";
        let meas_path = temp_measurement("prov-meas", meas_content)?;
        let meas_sha = sha256_hex(meas_content.as_bytes());
        let facts_sha = sha256_hex(b"real-dmi-capture");

        // Missing provenance stays unmeasured.
        let unmeasured = evaluate_provenance_gate(Some(meas_path.as_path()), None);
        assert_eq!(unmeasured.status, ProvenanceGateStatus::Unmeasured);

        // Wrong measurement sha rejects.
        let bad_sha = "f".repeat(64);
        let bad_prov_text = format!(
            "schema_version=1\n\
             target_profile=4gb_potato\n\
             measurement_host=potato-target-lab\n\
             measurement_sha256={bad_sha}\n\
             host_facts_sha256={facts_sha}\n\
             hardware_model=Acer-Aspire-ES1\n\
             installed_ram_mb=4096\n\
             capture_utc=2026-11-27T10:00:00Z\n\
             operator=alice\n\
             reviewer=bob\n\
             workload_protocol=mixed-workspace-v1\n"
        );
        let bad_prov_path = temp_measurement("prov-bad", &bad_prov_text)?;
        let rejected =
            evaluate_provenance_gate(Some(meas_path.as_path()), Some(bad_prov_path.as_path()));
        let _ = fs::remove_file(&bad_prov_path);
        assert_eq!(rejected.status, ProvenanceGateStatus::Rejected);

        // Matching provenance verifies.
        let ok_prov_text = format!(
            "schema_version=1\n\
             target_profile=4gb_potato\n\
             measurement_host=potato-target-lab\n\
             measurement_sha256={meas_sha}\n\
             host_facts_sha256={facts_sha}\n\
             hardware_model=Acer-Aspire-ES1\n\
             installed_ram_mb=4096\n\
             capture_utc=2026-11-27T10:00:00Z\n\
             operator=alice\n\
             reviewer=bob\n\
             workload_protocol=mixed-workspace-v1\n"
        );
        let ok_prov_path = temp_measurement("prov-ok", &ok_prov_text)?;
        let verified =
            evaluate_provenance_gate(Some(meas_path.as_path()), Some(ok_prov_path.as_path()));
        let _ = fs::remove_file(&ok_prov_path);
        let _ = fs::remove_file(&meas_path);
        assert_eq!(verified.status, ProvenanceGateStatus::VerifiedTarget4Gb);
        assert_eq!(verified.status.as_str(), "measured_target_4gb");
        Ok(())
    }
}
