//! End-to-end gate for the Block 3 offline task pack.
//!
//! The shipped EconBench seed declares a `case_id` and runs an in-process verifier. The task pack
//! in `bench/taskpack/` is the seeded-workspace half that was missing: fourteen candidates, each
//! carrying one planted bug, graded by a hidden independent oracle that never imports them.
//!
//!  * seven **planted-invariant** wallet tasks, graded by an in-process model written inside the
//!    oracle, with no reference implementation available to the candidate;
//!  * seven **transmutation** tasks, graded by replaying a 2,498-row differential corpus whose
//!    expected values were computed with Python's arbitrary-precision integers.
//!
//! These tests are the CI gate. They
//!
//!  1. re-assemble every candidate from the pinned ABI, so a seeded file cannot ship a private copy
//!     of the contract;
//!  2. require both reference solutions to pass every check;
//!  3. require every seeded candidate to fail the expected check or corpus function, and nothing
//!     else;
//!  4. require that the seeds between them exercise every oracle check and every corpus function.
//!
//! Compilation is `rustc --edition 2021 -D warnings` on a single self-contained file: no crate,
//! network, model, wallet provider, or service. Without `rustc` on `PATH` the tests skip locally
//! and the dedicated CI step runs `bench/taskpack/run.sh matrix` instead.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Wallet,
    Transform,
}

/// Every seeded candidate: family, source file, the oracle signal it must produce, and why.
const SEEDED: [(&str, Family, &str, &str, &str); 14] = [
    (
        "01_wallet_overdraft",
        Family::Wallet,
        "wallet.rs",
        "check:overdraft_is_rejected",
        "wrapping subtraction turns an overdraft into a huge balance",
    ),
    (
        "02_wallet_exact_balance",
        Family::Wallet,
        "wallet.rs",
        "check:exact_balance_is_allowed",
        "an inclusive balance check rejects spending the whole balance",
    ),
    (
        "03_wallet_idempotent_replay",
        Family::Wallet,
        "wallet.rs",
        "check:duplicate_retry_is_idempotent",
        "the idempotency guard is skipped, so a retry applies twice",
    ),
    (
        "04_wallet_id_reuse",
        Family::Wallet,
        "wallet.rs",
        "check:transfer_id_reuse_is_rejected",
        "a known transfer ID is reported as a duplicate whatever the arguments",
    ),
    (
        "05_wallet_overflow_atomicity",
        Family::Wallet,
        "wallet.rs",
        "check:overflow_is_atomic",
        "the source is debited before the destination overflow is checked",
    ),
    (
        "06_wallet_invalid_requests",
        Family::Wallet,
        "wallet.rs",
        "check:invalid_requests_are_rejected",
        "the zero-amount guard is unreachable",
    ),
    (
        "07_wallet_tamper_detection",
        Family::Wallet,
        "wallet.rs",
        "check:tamper_is_detected",
        "chain validation never recomputes the event hash",
    ),
    (
        "08_transform_bit_reverse",
        Family::Transform,
        "func.rs",
        "function:bit_reverse",
        "the reversal runs 63 steps instead of 64",
    ),
    (
        "09_transform_byte_swap",
        Family::Transform,
        "func.rs",
        "function:byte_swap",
        "the lowest byte is never copied",
    ),
    (
        "10_transform_rotate_left",
        Family::Transform,
        "func.rs",
        "function:rotate_left",
        "the rotation amount is reduced modulo 32 instead of 64",
    ),
    (
        "11_transform_rotate_right",
        Family::Transform,
        "func.rs",
        "function:rotate_right",
        "the source index adds where it should subtract",
    ),
    (
        "12_transform_saturating_add",
        Family::Transform,
        "func.rs",
        "function:saturating_add",
        "the addition wraps instead of clamping",
    ),
    (
        "13_transform_checked_mul",
        Family::Transform,
        "func.rs",
        "function:checked_mul",
        "overflow is narrowed away instead of reported",
    ),
    (
        "14_transform_parity",
        Family::Transform,
        "func.rs",
        "function:parity",
        "the parity predicate is inverted",
    ),
];

// The constructor is graded on every seed but has no planted-bug task of its own.
const UNSEEDED_WALLET_CHECK: &str = "constructor_rejections";

const WALLET_CHECKS: [&str; 8] = [
    "constructor_rejections",
    "exact_balance_is_allowed",
    "overdraft_is_rejected",
    "duplicate_retry_is_idempotent",
    "transfer_id_reuse_is_rejected",
    "overflow_is_atomic",
    "invalid_requests_are_rejected",
    "tamper_is_detected",
];

const TRANSFORM_FUNCTIONS: [&str; 7] = [
    "bit_reverse",
    "byte_swap",
    "rotate_left",
    "rotate_right",
    "saturating_add",
    "checked_mul",
    "parity",
];

fn taskpack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/taskpack")
}

fn work_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/taskpack-ci")
}

fn rustc_available() -> bool {
    Command::new("rustc")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn oracle(family: Family) -> &'static str {
    match family {
        Family::Wallet => "oracle/wallet_oracle.rs",
        Family::Transform => "oracle/transform_oracle.rs",
    }
}

/// Assemble `ABI + candidate + oracle` for one family and compile it with plain `rustc`.
fn build_grader(candidate: &Path, family: Family, label: &str) -> Result<PathBuf, String> {
    let dir = work_dir();
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let assembled = dir.join(format!("{label}.assembled.rs"));
    let status = Command::new("bash")
        .arg(taskpack_dir().join("assemble.sh"))
        .arg(candidate)
        .arg(&assembled)
        .status()
        .map_err(|error| format!("cannot run assemble.sh: {error}"))?;
    if !status.success() {
        return Err(format!("assemble.sh rejected {label}"));
    }
    let mut source = std::fs::read_to_string(&assembled).map_err(|error| error.to_string())?;
    let oracle = std::fs::read_to_string(taskpack_dir().join(oracle(family)))
        .map_err(|error| error.to_string())?;
    source.push('\n');
    source.push_str(&oracle);
    let crate_path = dir.join(format!("{label}.grader.rs"));
    std::fs::write(&crate_path, source).map_err(|error| error.to_string())?;
    let binary = dir.join(format!("{label}.grader"));
    let output = Command::new("rustc")
        .args(["--edition", "2021", "-D", "warnings", "-o"])
        .arg(&binary)
        .arg(&crate_path)
        .output()
        .map_err(|error| format!("cannot run rustc: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "rustc rejected {label}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(binary)
}

struct GraderRun {
    passed: bool,
    wallet: BTreeMap<String, bool>,
    transform: BTreeMap<String, (u64, u64)>,
    stdout: String,
}

fn parse_grader_run(passed: bool, stdout: String) -> GraderRun {
    let mut wallet = BTreeMap::new();
    let mut transform = BTreeMap::new();
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split(' ').collect();
        match fields.as_slice() {
            ["check", name, verdict, ..] => {
                wallet.insert((*name).to_string(), verdict.starts_with("pass"));
            }
            // `grader failed_functions=1 failed=...` also starts with a bare word; only a
            // `function <name> rows=... failures=...` line has a name without an `=`.
            ["function", name, rows, failures, ..] if !name.contains('=') => {
                let rows = rows
                    .strip_prefix("rows=")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
                let failures = failures
                    .strip_prefix("failures=")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
                transform.insert((*name).to_string(), (rows, failures));
            }
            _ => {}
        }
    }
    GraderRun {
        passed,
        wallet,
        transform,
        stdout,
    }
}

fn run_grader(binary: &Path, family: Family) -> Result<GraderRun, String> {
    let mut command = Command::new(binary);
    if family == Family::Transform {
        command.arg(taskpack_dir().join("corpus/transmutation.tsv"));
    }
    let output = command
        .output()
        .map_err(|error| format!("cannot run the grader: {error}"))?;
    Ok(parse_grader_run(
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
    ))
}

#[test]
fn reference_solutions_pass_every_check() {
    if !rustc_available() {
        eprintln!("skipping: no rustc on PATH (CI runs bench/taskpack/run.sh matrix)");
        return;
    }

    let wallet = build_grader(
        &taskpack_dir().join("reference/wallet_ref.rs"),
        Family::Wallet,
        "reference-wallet",
    )
    .expect("the wallet reference must assemble and compile");
    let wallet_run = run_grader(&wallet, Family::Wallet).expect("the wallet oracle must run");
    assert!(
        wallet_run.passed,
        "the wallet reference failed:\n{}",
        wallet_run.stdout
    );
    assert_eq!(wallet_run.wallet.len(), WALLET_CHECKS.len());
    for check in WALLET_CHECKS {
        assert_eq!(
            wallet_run.wallet.get(check),
            Some(&true),
            "the wallet reference failed {check}:\n{}",
            wallet_run.stdout
        );
    }
    assert!(wallet_run.stdout.contains("oracle failed_checks=0 failed="));

    let transform = build_grader(
        &taskpack_dir().join("reference/transform_ref.rs"),
        Family::Transform,
        "reference-transform",
    )
    .expect("the transform reference must assemble and compile");
    let transform_run =
        run_grader(&transform, Family::Transform).expect("the transform grader must run");
    assert!(
        transform_run.passed,
        "the transform reference disagreed with the corpus:\n{}",
        transform_run.stdout
    );
    assert_eq!(transform_run.transform.len(), TRANSFORM_FUNCTIONS.len());
    for (function, (rows, failures)) in &transform_run.transform {
        assert!(*rows > 0, "{function} had no corpus rows");
        assert_eq!(
            *failures, 0,
            "the transform reference failed {failures} rows of {function}:\n{}",
            transform_run.stdout
        );
    }
}

#[test]
fn every_seeded_bug_is_caught_by_its_own_signal() {
    if !rustc_available() {
        eprintln!("skipping: no rustc on PATH (CI runs bench/taskpack/run.sh matrix)");
        return;
    }

    let mut corpus_rows = 0u64;
    for (seed, family, source, expectation, why) in SEEDED {
        let candidate = taskpack_dir().join(format!("seeded/{seed}/{source}"));
        assert!(
            candidate.is_file(),
            "missing seeded candidate: {candidate:?}"
        );
        let binary = build_grader(&candidate, family, seed)
            .unwrap_or_else(|error| panic!("seed {seed} must still compile: {error}"));
        let run = run_grader(&binary, family)
            .unwrap_or_else(|error| panic!("seed {seed} grader failed to run: {error}"));
        assert!(
            !run.passed,
            "seed {seed} passed its grader, but its planted bug ({why}) should be caught"
        );

        let (kind, target) = expectation
            .split_once(':')
            .expect("every expectation is kind:target");
        match kind {
            "check" => {
                let failed: Vec<&str> = run
                    .wallet
                    .iter()
                    .filter(|(_, passed)| !**passed)
                    .map(|(name, _)| name.as_str())
                    .collect();
                assert_eq!(
                    failed,
                    vec![target],
                    "seed {seed} ({why}) should fail only {target}:\n{}",
                    run.stdout
                );
                assert!(
                    run.stdout.contains("oracle failed_checks=1 "),
                    "seed {seed} should fail exactly one check:\n{}",
                    run.stdout
                );
            }
            "function" => {
                let broken: Vec<&str> = run
                    .transform
                    .iter()
                    .filter(|(_, (_, failures))| *failures > 0)
                    .map(|(name, _)| name.as_str())
                    .collect();
                assert_eq!(
                    broken,
                    vec![target],
                    "seed {seed} ({why}) should break only {target}:\n{}",
                    run.stdout
                );
                let (rows, failures) = run.transform[target];
                assert!(rows > 0, "seed {seed}: {target} had no corpus rows");
                assert!(failures > 0, "seed {seed}: {target} failed no rows");
                assert!(
                    run.stdout
                        .contains(&format!("grader failed_functions=1 failed={target}")),
                    "unexpected grader summary for seed {seed}:\n{}",
                    run.stdout
                );
                corpus_rows = corpus_rows.max(rows);
            }
            other => panic!("unknown expectation kind: {other}"),
        }
    }
    assert!(
        corpus_rows >= 2_000,
        "the differential corpus shrank to {corpus_rows} rows per function"
    );
}

#[test]
fn seeds_cover_every_oracle_signal() {
    let mut wallet_covered: BTreeSet<&str> = BTreeSet::new();
    let mut transform_covered: BTreeSet<&str> = BTreeSet::new();
    for (_, family, _, expectation, _) in SEEDED {
        let target = expectation.split_once(':').expect("kind:target").1;
        match family {
            Family::Wallet => {
                wallet_covered.insert(target);
            }
            Family::Transform => {
                transform_covered.insert(target);
            }
        }
    }
    // One seed per planted bug: the constructor is graded every run but carries no seed.
    for check in WALLET_CHECKS {
        if check == UNSEEDED_WALLET_CHECK {
            continue;
        }
        assert!(
            wallet_covered.contains(check),
            "no seeded wallet task exercises {check}, so that invariant is unproven"
        );
    }
    for function in TRANSFORM_FUNCTIONS {
        assert!(
            transform_covered.contains(function),
            "no seeded transmutation task exercises {function}"
        );
    }
    assert_eq!(wallet_covered.len() + 1, WALLET_CHECKS.len());
    assert_eq!(transform_covered.len(), TRANSFORM_FUNCTIONS.len());
    assert_eq!(
        WALLET_CHECKS.len() - 1 + TRANSFORM_FUNCTIONS.len(),
        SEEDED.len()
    );
}

#[test]
fn candidates_are_checked_in_without_a_private_abi_copy() {
    for (seed, family, source, _, _) in SEEDED {
        let path = taskpack_dir().join(format!("seeded/{seed}/{source}"));
        let text = std::fs::read_to_string(&path).expect("seeded candidate must be readable");
        assert!(
            !text.contains("CANDIDATE EVENT ABI"),
            "{seed} must not embed its own ABI copy; assemble.sh prepends the pinned one"
        );
        assert!(!text.contains("include!"), "{seed} must not use include!");
        match family {
            Family::Wallet => assert_eq!(
                text.matches("pub mod wallet {").count(),
                1,
                "{seed} must define exactly one wallet module"
            ),
            Family::Transform => {
                for function in TRANSFORM_FUNCTIONS {
                    assert!(
                        text.contains(&format!("pub fn {function}(")),
                        "{seed} is missing the pinned function {function}"
                    );
                }
            }
        }
    }
    for reference in ["reference/wallet_ref.rs", "reference/transform_ref.rs"] {
        let text = std::fs::read_to_string(taskpack_dir().join(reference)).expect("reference");
        assert!(!text.contains("CANDIDATE EVENT ABI"));
    }
}

#[test]
fn oracles_never_reference_a_candidate() {
    let wallet = std::fs::read_to_string(taskpack_dir().join("oracle/wallet_oracle.rs"))
        .expect("wallet oracle must be readable");
    assert!(
        wallet.contains("use crate::wallet::Wallet;"),
        "the wallet oracle must drive the candidate only through the pinned ABI path"
    );
    for forbidden in ["include!", "seeded/", "reference/"] {
        assert!(
            !wallet.contains(forbidden),
            "the hidden wallet oracle must not reference a candidate via {forbidden}"
        );
    }
    let transform = std::fs::read_to_string(taskpack_dir().join("oracle/transform_oracle.rs"))
        .expect("transform oracle must be readable");
    for forbidden in ["include!", "seeded/", "reference/"] {
        assert!(
            !transform.contains(forbidden),
            "the hidden transform oracle must not reference a candidate via {forbidden}"
        );
    }
    assert!(
        transform.contains("env::args"),
        "the transform oracle must take the corpus path as an argument, not embed it"
    );
}

#[test]
fn seeded_workspaces_are_bounded_and_offline() {
    // Keep the pack reviewable: a seed is a small single-file candidate, and the corpus must stay
    // small enough to live in the repository without becoming a data dump.
    for (seed, _, source, _, _) in SEEDED {
        let path = taskpack_dir().join(format!("seeded/{seed}/{source}"));
        let bytes = std::fs::metadata(&path)
            .unwrap_or_else(|error| panic!("cannot stat {seed}: {error}"))
            .len();
        assert!(
            bytes < 32 * 1024,
            "seed {seed} is {bytes} bytes; keep a candidate workspace small"
        );
    }
    let corpus = taskpack_dir().join("corpus/transmutation.tsv");
    let bytes = std::fs::metadata(&corpus).expect("corpus metadata").len();
    assert!(
        bytes < 1024 * 1024,
        "the differential corpus grew to {bytes} bytes"
    );
    for script in ["run.sh", "assemble.sh"] {
        let text = std::fs::read_to_string(taskpack_dir().join(script))
            .unwrap_or_else(|error| panic!("cannot read {script}: {error}"));
        for forbidden in ["curl", "wget", "git", "nc ", "ssh ", "http://", "https://"] {
            assert!(
                !text.contains(forbidden),
                "{script} must stay offline; it mentions {forbidden}"
            );
        }
    }
}

#[test]
fn differential_corpus_covers_every_function_and_stays_well_formed() {
    let text = std::fs::read_to_string(taskpack_dir().join("corpus/transmutation.tsv"))
        .expect("the differential corpus must be readable");
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 4, "corpus row {} is malformed", index + 1);
        assert!(
            TRANSFORM_FUNCTIONS.contains(&fields[0]),
            "corpus row {} names an unknown function",
            index + 1
        );
        fields[1].parse::<u64>().expect("operand a is a u64");
        fields[2].parse::<u64>().expect("operand b is a u64");
        if fields[3] != "overflow" {
            fields[3].parse::<u64>().expect("expected is a u64");
        } else {
            assert_eq!(
                fields[0], "checked_mul",
                "only checked_mul reports overflow"
            );
        }
        *counts.entry(fields[0]).or_insert(0) += 1;
    }
    for function in TRANSFORM_FUNCTIONS {
        let count = counts.get(function).copied().unwrap_or(0);
        assert!(count >= 100, "{function} has only {count} corpus rows");
    }
}
