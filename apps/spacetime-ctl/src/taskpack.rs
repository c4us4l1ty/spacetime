//! Block 3 offline task pack: seeded candidate workspaces, graded by a hidden oracle.
//!
//! The shipped `bench/econbench/*.task` files declare an oracle and run an in-process verifier.
//! This module adds the part those fixtures could not: real, compiled candidate workspaces that
//! contain a planted bug, plus a hidden independent oracle that grades them through a pinned ABI.
//!
//! This command is a *grader*, not a sandbox: it shells out to `rustc` on pinned local files only.
//! It never accepts a caller-supplied command, path, or model, and it makes no resource,
//! performance, or model-efficacy claim. Grade results are offline functional evidence for the
//! Block 3 task pack; they do not measure the 4 GB target and do not set `block3_done = true`.

use crate::ensure_flag_value;
use serde_json::{json, Value};
use spacetime_types::{Result, SpacetimeError};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Every seeded candidate and the oracle signal its single planted bug must produce.
///
/// Kept in one place so this module, `run.sh`, and the Rust test harness cannot drift apart.
pub(crate) const SEEDED: [(&str, &str, &str); 20] = [
    (
        "01_wallet_overdraft",
        "wallet.rs",
        "check:overdraft_is_rejected",
    ),
    (
        "02_wallet_exact_balance",
        "wallet.rs",
        "check:exact_balance_is_allowed",
    ),
    (
        "03_wallet_idempotent_replay",
        "wallet.rs",
        "check:duplicate_retry_is_idempotent",
    ),
    (
        "04_wallet_id_reuse",
        "wallet.rs",
        "check:transfer_id_reuse_is_rejected",
    ),
    (
        "05_wallet_overflow_atomicity",
        "wallet.rs",
        "check:overflow_is_atomic",
    ),
    (
        "06_wallet_invalid_requests",
        "wallet.rs",
        "check:invalid_requests_are_rejected",
    ),
    (
        "07_wallet_tamper_detection",
        "wallet.rs",
        "check:tamper_is_detected",
    ),
    (
        "08_transform_bit_reverse",
        "func.rs",
        "function:bit_reverse",
    ),
    ("09_transform_byte_swap", "func.rs", "function:byte_swap"),
    (
        "10_transform_rotate_left",
        "func.rs",
        "function:rotate_left",
    ),
    (
        "11_transform_rotate_right",
        "func.rs",
        "function:rotate_right",
    ),
    (
        "12_transform_saturating_add",
        "func.rs",
        "function:saturating_add",
    ),
    (
        "13_transform_checked_mul",
        "func.rs",
        "function:checked_mul",
    ),
    ("14_transform_parity", "func.rs", "function:parity"),
    ("15_protocol_double_begin", "protocol.rs", "traces:begin_after_abort"),
    (
        "16_protocol_commit_leaves_open",
        "protocol.rs",
        "traces:empty_transaction,long_value,read_twice,\
sequence_zero_is_first,three_records,two_records",
    ),
    ("17_protocol_abort_keeps_records", "protocol.rs", "traces:abort_discards"),
    (
        "18_protocol_sequence_unchecked",
        "protocol.rs",
        "traces:duplicate_sequence,gap_sequence,sequence_gap_after_two",
    ),
    ("19_protocol_read_early", "protocol.rs", "traces:read_before_commit"),
    (
        "20_protocol_abort_without_transaction",
        "protocol.rs",
        "traces:abort_before_begin",
    ),
];

/// The one graded wallet check with no dedicated seed. `Wallet::new` runs at the top of every
/// seed and of the reference, so a planted constructor bug is still caught, but the pack carries
/// one seed per planted bug rather than one per check.
pub(crate) const UNSEEDED_WALLET_CHECK: &str = "constructor_rejections";

pub(crate) const WALLET_CHECKS: [&str; 8] = [
    "constructor_rejections",
    "exact_balance_is_allowed",
    "overdraft_is_rejected",
    "duplicate_retry_is_idempotent",
    "transfer_id_reuse_is_rejected",
    "overflow_is_atomic",
    "invalid_requests_are_rejected",
    "tamper_is_detected",
];

pub(crate) const TRANSFORM_FUNCTIONS: [&str; 7] = [
    "bit_reverse",
    "byte_swap",
    "rotate_left",
    "rotate_right",
    "saturating_add",
    "checked_mul",
    "parity",
];

fn family_of(source: &str) -> &'static str {
    match source {
        "func.rs" => "transform",
        "protocol.rs" => "protocol",
        _ => "wallet",
    }
}

fn taskpack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/taskpack")
}

fn work_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/taskpack-ctl")
}

fn rustc_available() -> bool {
    Command::new("rustc")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn abi_path(family: &str) -> PathBuf {
    taskpack_dir().join(match family {
        "transform" => "abi/transform_abi.rs",
        "protocol" => "abi/protocol_abi.rs",
        _ => "abi/wallet_abi.rs",
    })
}

fn oracle_path(family: &str) -> PathBuf {
    taskpack_dir().join(match family {
        "transform" => "oracle/transform_oracle.rs",
        "protocol" => "oracle/protocol_oracle.rs",
        _ => "oracle/wallet_oracle.rs",
    })
}

/// Assemble `ABI + candidate + oracle` for one family and compile it with plain `rustc`.
fn build_grader(candidate: &Path, family: &str, label: &str) -> Result<PathBuf> {
    let dir = work_dir();
    std::fs::create_dir_all(&dir)?;
    let assembled = dir.join(format!("{label}.assembled.rs"));
    let status = Command::new("bash")
        .arg(taskpack_dir().join("assemble.sh"))
        .arg(candidate)
        .arg(&assembled)
        .status()
        .map_err(|error| SpacetimeError::new(format!("cannot run assemble.sh: {error}")))?;
    if !status.success() {
        return Err(SpacetimeError::new(format!(
            "assemble.sh rejected the candidate {label}"
        )));
    }
    if !abi_path(family).exists() {
        return Err(SpacetimeError::new(format!(
            "missing pinned ABI for the {family} family"
        )));
    }
    let oracle = std::fs::read_to_string(oracle_path(family))?;
    let mut source = std::fs::read_to_string(&assembled)?;
    source.push('\n');
    source.push_str(&oracle);
    let crate_path = dir.join(format!("{label}.grader.rs"));
    std::fs::write(&crate_path, source)?;
    let binary = dir.join(format!("{label}.grader"));
    let output = Command::new("rustc")
        .args([
            "--edition",
            "2021",
            "--crate-name",
            "taskpack_grader",
            "-D",
            "warnings",
            "-o",
        ])
        .arg(&binary)
        .arg(&crate_path)
        .output()
        .map_err(|error| SpacetimeError::new(format!("cannot run rustc: {error}")))?;
    if !output.status.success() {
        return Err(SpacetimeError::new(format!(
            "the oracle failed to compile against {label}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(binary)
}

struct GraderRun {
    passed: bool,
    results: Vec<(String, bool)>,
    transform: Vec<(String, (u64, u64))>,
    /// `trace <name> fail|pass ...` lines from the protocol oracle, in recording order.
    traces: Vec<(String, bool)>,
    stdout: String,
}

fn run_grader(binary: &Path, family: &str) -> Result<GraderRun> {
    let mut command = Command::new(binary);
    match family {
        "transform" => {
            command.arg(taskpack_dir().join("corpus/transmutation.tsv"));
        }
        "protocol" => {
            command.arg(taskpack_dir().join("corpus/protocol_traces.tsv"));
        }
        _ => {}
    }
    let output = command
        .output()
        .map_err(|error| SpacetimeError::new(format!("cannot run the oracle: {error}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let mut results = Vec::new();
    let mut transform = Vec::new();
    let mut traces = Vec::new();
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split(' ').collect();
        match fields.as_slice() {
            ["check", name, verdict, ..] => {
                results.push(((*name).to_string(), verdict.starts_with("pass")));
            }
            ["function", name, rows, failures, ..] if !name.contains('=') => {
                let _: &str = name;
                let rows = rows
                    .strip_prefix("rows=")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
                let failures = failures
                    .strip_prefix("failures=")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
                transform.push(((*name).to_string(), (rows, failures)));
            }
            ["trace", name, verdict, ..] => {
                traces.push(((*name).to_string(), verdict.starts_with("pass")));
            }
            _ => {}
        }
    }
    Ok(GraderRun {
        passed: output.status.success(),
        results,
        transform,
        traces,
        stdout,
    })
}

/// Grade the whole task pack. Returns a machine-readable report; every gate is explicit.
pub fn taskpack_report() -> Result<Value> {
    if !rustc_available() {
        return Err(SpacetimeError::new(
            "taskpack grading needs rustc; CI provides it (bench/taskpack/run.sh matrix)",
        ));
    }

    // 1. Both reference solutions must pass every check.
    let reference_wallet = build_grader(
        &taskpack_dir().join("reference/wallet_ref.rs"),
        "wallet",
        "reference-wallet",
    )?;
    let wallet_run = run_grader(&reference_wallet, "wallet")?;
    if !wallet_run.passed || wallet_run.results.len() != WALLET_CHECKS.len() {
        return Err(SpacetimeError::new(format!(
            "the wallet reference solution failed the hidden oracle:\n{}",
            wallet_run.stdout
        )));
    }
    let reference_transform = build_grader(
        &taskpack_dir().join("reference/transform_ref.rs"),
        "transform",
        "reference-transform",
    )?;
    let transform_run = run_grader(&reference_transform, "transform")?;
    if !transform_run.passed || transform_run.transform.len() != TRANSFORM_FUNCTIONS.len() {
        return Err(SpacetimeError::new(format!(
            "the transform reference solution disagreed with the corpus:\n{}",
            transform_run.stdout
        )));
    }

    let reference_protocol = build_grader(
        &taskpack_dir().join("reference/protocol_ref.rs"),
        "protocol",
        "reference-protocol",
    )?;
    let protocol_run = run_grader(&reference_protocol, "protocol")?;
    if !protocol_run.passed || protocol_run.traces.iter().any(|(_, passed)| !passed) {
        return Err(SpacetimeError::new(format!(
            "the protocol reference solution disagreed with the recording:\n{}",
            protocol_run.stdout
        )));
    }

    // 2. Every seeded candidate must fail exactly the signal its planted bug explains.
    let mut seeded = Vec::new();
    let mut caught = 0usize;
    for (seed, source, expectation) in SEEDED {
        let family = family_of(source);
        let candidate = taskpack_dir().join(format!("seeded/{seed}/{source}"));
        if !candidate.is_file() {
            return Err(SpacetimeError::new(format!(
                "missing seeded candidate: {}",
                candidate.display()
            )));
        }
        let binary = build_grader(&candidate, family, seed)?;
        let run = run_grader(&binary, family)?;
        let (kind, target) = expectation
            .split_once(':')
            .ok_or_else(|| SpacetimeError::new("malformed seed expectation"))?;
        let observed = match kind {
            "check" => {
                let failed: Vec<&str> = run
                    .results
                    .iter()
                    .filter(|(_, passed)| !passed)
                    .map(|(name, _)| name.as_str())
                    .collect();
                failed.join(",")
            }
            "function" => run
                .transform
                .iter()
                .filter(|(_, (_, failures))| *failures > 0)
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(","),
            "traces" => {
                let mut failed: Vec<&str> = run
                    .traces
                    .iter()
                    .filter(|(_, passed)| !passed)
                    .map(|(name, _)| name.as_str())
                    .collect();
                failed.sort_unstable();
                failed.join(",")
            }
            other => {
                return Err(SpacetimeError::new(format!(
                    "unknown expectation kind: {other}"
                )))
            }
        };
        let exactly_expected = !run.passed && observed == target;
        if exactly_expected {
            caught += 1;
        }
        seeded.push(json!({
            "seed": seed,
            "family": family,
            "expected_signal": expectation,
            "observed": observed,
            "response_digest": spacetime_types::sha256_hex(run.stdout.as_bytes()),
            "caught": exactly_expected,
        }));
    }
    if caught != SEEDED.len() {
        return Err(SpacetimeError::new(format!(
            "only {caught} of {} planted bugs were caught by their expected signal",
            SEEDED.len()
        )));
    }

    // 3. The seeds must exercise every oracle check and every corpus function between them.
    // One seed per planted bug: `Wallet::new` is exercised by every seed and by the reference,
    // so the constructor is graded without carrying a task of its own.
    for check in WALLET_CHECKS {
        if check == UNSEEDED_WALLET_CHECK {
            continue;
        }
        if !SEEDED
            .iter()
            .any(|(_, _, expectation)| *expectation == format!("check:{check}"))
        {
            return Err(SpacetimeError::new(format!(
                "no seeded wallet task exercises {check}"
            )));
        }
    }
    for function in TRANSFORM_FUNCTIONS {
        if !SEEDED
            .iter()
            .any(|(_, _, expectation)| *expectation == format!("function:{function}"))
        {
            return Err(SpacetimeError::new(format!(
                "no seeded transmutation task exercises {function}"
            )));
        }
    }

    // A trace may be pinned to at most one seed: two seeds claiming the same trace would make
    // "this seed failed because of its own bug" unprovable. A trace may also be pinned to none —
    // those are the decoys, which no planted bug touches and every correct candidate must still
    // accept. A pin that names a trace nobody recorded is a typo and is rejected.
    let mut pinned: std::collections::BTreeMap<&str, &str> = std::collections::BTreeMap::new();
    for (seed, _, expectation) in SEEDED {
        let Some(list) = expectation.strip_prefix("traces:") else {
            continue;
        };
        for trace in list.split(',') {
            if pinned.insert(trace, seed).is_some() {
                return Err(SpacetimeError::new(format!(
                    "trace {trace} is pinned to more than one seed"
                )));
            }
        }
    }
    for trace in pinned.keys() {
        if !protocol_run.traces.iter().any(|(name, _)| name == trace) {
            return Err(SpacetimeError::new(format!(
                "seed pins trace {trace}, which the recording does not contain"
            )));
        }
    }
    // Without unpinned decoys a candidate could pass every pinned trace by rejecting legal
    // protocol behaviour outright, so the recording must always carry some.
    let decoys = protocol_run
        .traces
        .iter()
        .filter(|(name, _)| !pinned.contains_key(name.as_str()))
        .count();
    if decoys == 0 {
        return Err(SpacetimeError::new(
            "the protocol recording has no unpinned decoy traces",
        ));
    }

    let corpus_rows: u64 = transform_run.transform.iter().map(|(_, rows)| rows.0).sum();
    Ok(json!({
        "schema_version": 1,
        "report": "block3_offline_task_pack",
        "candidates": SEEDED.len(),
        "planted_invariant_tasks": SEEDED.iter().filter(|(_, s, _)| *s == "wallet.rs").count(),
        "transmutation_tasks": SEEDED.iter().filter(|(_, s, _)| *s == "func.rs").count(),
        "wallet_checks": WALLET_CHECKS.len(),
        "wallet_checks_without_a_seed": [UNSEEDED_WALLET_CHECK],
        "corpus_functions": TRANSFORM_FUNCTIONS.len(),
        "corpus_rows": corpus_rows,
        "protocol_tasks": SEEDED.iter().filter(|(_, s, _)| *s == "protocol.rs").count(),
        "protocol_traces": protocol_run.traces.len(),
        "seeded_bugs_caught": caught,
        "reference_checks_passed": wallet_run.results.len()
            + transform_run.transform.len()
            + protocol_run.traces.len(),
        "build": {"toolchain": "rustc_edition_2021", "dependencies": "none", "network": "none"},
        "reference_wallet_digest": spacetime_types::sha256_hex(wallet_run.stdout.as_bytes()),
        "reference_transform_digest": spacetime_types::sha256_hex(transform_run.stdout.as_bytes()),
        "reference_protocol_digest": spacetime_types::sha256_hex(protocol_run.stdout.as_bytes()),
        "seeded": seeded,
        "resource_gate": "unmeasured_target_4gb",
        "performance_claims": "unmeasured",
        "block3_done": false,
        "note": "offline functional evidence for seeded candidate workspaces, \
        not a measured resource gate",
    }))
}

/// `spacetime-ctl bench taskpack --out <file>`
pub fn cmd_taskpack(args: &[String]) -> Result<()> {
    let flags = crate::parse_exact_flags(args, &["--out"])?;
    let out = &flags["--out"];
    ensure_flag_value("--out", out)?;
    let report = taskpack_report()?;
    let mut text = serde_json::to_string_pretty(&report)?;
    text.push('\n');
    let out_path = PathBuf::from(out);
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&out_path, text)?;
    let candidates = report
        .get("candidates")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let caught = report
        .get("seeded_bugs_caught")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    println!(
        "taskpack_report={} candidates={candidates} \
seeded_bugs_caught={caught} block3_done=false",
        out_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_signals_cover_every_oracle_surface() {
        let mut wallet = std::collections::BTreeSet::new();
        let mut transform = std::collections::BTreeSet::new();
        let mut protocol = std::collections::BTreeSet::new();
        for (_, source, expectation) in SEEDED {
            let (kind, target) = expectation.split_once(':').expect("kind:target");
            match kind {
                "check" => {
                    wallet.insert(target);
                }
                "function" => {
                    transform.insert(target);
                }
                "traces" => {
                    for trace in target.split(',') {
                        protocol.insert(trace);
                    }
                }
                other => panic!("unknown expectation kind {other}"),
            }
            let expected_family = match source {
                "func.rs" => "function",
                "protocol.rs" => "traces",
                _ => "check",
            };
            assert_eq!(kind, expected_family, "seed family and signal disagree");
        }
        // The pack carries one seed per planted bug, so the constructor is graded on every run but
        // has no seed of its own; every other check must be pinned to exactly one seed.
        for check in WALLET_CHECKS {
            if check == UNSEEDED_WALLET_CHECK {
                continue;
            }
            assert!(wallet.contains(check), "unexercised wallet check: {check}");
        }
        for function in TRANSFORM_FUNCTIONS {
            assert!(
                transform.contains(function),
                "unexercised corpus function: {function}"
            );
        }
        assert_eq!(
            wallet.len() + 1,
            WALLET_CHECKS.len(),
            "two seeds share a check"
        );
        assert_eq!(
            transform.len(),
            TRANSFORM_FUNCTIONS.len(),
            "two seeds share a function"
        );
        // Protocol seeds pin exact failure sets; no trace may be claimed twice, because a shared
        // trace would make "this seed failed because of its own bug" unprovable.
        let claimed: usize = SEEDED
            .iter()
            .filter(|(_, source, _)| *source == "protocol.rs")
            .map(|(_, _, expectation)| expectation.matches(',').count() + 1)
            .sum();
        assert_eq!(protocol.len(), claimed, "two seeds share a trace");
    }

    #[test]
    fn seeded_sources_stay_minimal() {
        for (seed, source, _) in SEEDED {
            let path = taskpack_dir().join(format!("seeded/{seed}/{source}"));
            let text = std::fs::read_to_string(&path).expect("seeded candidate must be readable");
            assert!(
                !text.contains("CANDIDATE EVENT ABI"),
                "{seed} must not embed a private ABI copy"
            );
            assert!(!text.contains("include!"), "{seed} must not use include!");
        }
    }

    #[test]
    fn families_resolve_to_their_pinned_files() {
        assert_eq!(family_of("wallet.rs"), "wallet");
        assert_eq!(family_of("func.rs"), "transform");
        assert!(abi_path("wallet").exists());
        assert!(abi_path("transform").exists());
        assert!(oracle_path("wallet").exists());
        assert!(oracle_path("transform").exists());
        assert!(taskpack_dir().join("corpus/transmutation.tsv").exists());
    }
}
