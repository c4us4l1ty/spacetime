//! End-to-end gate for the Block 3 offline task pack.
//!
//! The shipped EconBench seed declares a `case_id` and runs an in-process verifier. The task pack
//! in `bench/taskpack/` is the seeded-workspace half that was missing: twenty candidates, each
//! carrying one planted bug, graded by a hidden independent oracle that never imports them.
//!
//!  * seven **planted-invariant** wallet tasks, graded by an in-process model written inside the
//!    oracle, with no reference implementation available to the candidate;
//!  * seven **transmutation** tasks, graded by replaying a 2,498-row differential corpus whose
//!    expected values were computed with Python's arbitrary-precision integers;
//!  * six **protocol-distillation** tasks, graded by replaying captured traces against recordings
//!    taken from the service itself, so the candidate cannot define what the trace should do.
//!
//! These tests are the CI gate. They
//!
//!  1. re-assemble every candidate from the pinned ABI, so a seeded file cannot ship a private copy
//!     of the contract;
//!  2. require both reference solutions to pass every check;
//!  3. require every seeded candidate to fail the expected check or corpus function, and nothing
//!     else;
//!  4. require that the seeds between them exercise every oracle check, every corpus function, and
//!     every recorded trace, and that no trace is attributed to two seeds.
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
    Protocol,
}

/// Every seeded candidate: family, source file, the oracle signal it must produce, and why.
const SEEDED: [(&str, Family, &str, &str, &str); 20] = [
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
    (
        "15_protocol_double_begin",
        Family::Protocol,
        "protocol.rs",
        "traces:begin_after_abort",
        "`begin` is accepted without checking that a transaction already ended",
    ),
    (
        "16_protocol_commit_leaves_open",
        Family::Protocol,
        "protocol.rs",
        "traces:empty_transaction,long_value,read_twice,sequence_zero_is_first,\
three_records,two_records",
        "`commit` never clears `begun`, so every committed state is wrong",
    ),
    (
        "17_protocol_abort_keeps_records",
        Family::Protocol,
        "protocol.rs",
        "traces:abort_discards",
        "`abort` discards the transaction but keeps its records",
    ),
    (
        "18_protocol_sequence_unchecked",
        Family::Protocol,
        "protocol.rs",
        "traces:duplicate_sequence,gap_sequence,sequence_gap_after_two",
        "`put` appends whatever index it is handed",
    ),
    (
        "19_protocol_read_early",
        Family::Protocol,
        "protocol.rs",
        "traces:read_before_commit",
        "`read` is accepted as soon as a transaction begins",
    ),
    (
        "20_protocol_abort_without_transaction",
        Family::Protocol,
        "protocol.rs",
        "traces:abort_before_begin",
        "`abort` is accepted outside a transaction",
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
        Family::Protocol => "oracle/protocol_oracle.rs",
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
    /// How many `<kind> <name> ...` verdict lines the grader actually emitted, per kind.
    ///
    /// The candidate runs in the same process as the oracle and shares its stdout, so a candidate
    /// that prints a line shaped like a verdict can satisfy a naive per-line check without earning
    /// it. Counting lines lets a caller require exactly the number the oracle owes.
    verdict_lines: usize,
    summary_lines: usize,
    wallet: BTreeMap<String, bool>,
    transform: BTreeMap<String, (u64, u64)>,
    /// `trace <name> pass|fail ...` from the protocol oracle.
    traces: BTreeMap<String, bool>,
    stdout: String,
}

fn parse_grader_run(passed: bool, stdout: String) -> GraderRun {
    let mut verdict_lines = 0;
    let mut summary_lines = 0;
    let mut wallet = BTreeMap::new();
    let mut transform = BTreeMap::new();
    let mut traces = BTreeMap::new();
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split(' ').collect();
        if line.starts_with("oracle failed_") || line.starts_with("grader failed_") {
            summary_lines += 1;
        }
        match fields.as_slice() {
            ["check", name, verdict, ..] => {
                verdict_lines += 1;
                wallet.insert((*name).to_string(), verdict.starts_with("pass"));
            }
            // `grader failed_functions=1 failed=...` also starts with a bare word; only a
            // `function <name> rows=... failures=...` line has a name without an `=`.
            ["function", name, rows, failures, ..] if !name.contains('=') => {
                verdict_lines += 1;
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
            ["trace", name, verdict, ..] => {
                verdict_lines += 1;
                traces.insert((*name).to_string(), verdict.starts_with("pass"));
            }
            _ => {}
        }
    }
    GraderRun {
        passed,
        verdict_lines,
        summary_lines,
        wallet,
        transform,
        traces,
        stdout,
    }
}

fn run_grader(binary: &Path, family: Family) -> Result<GraderRun, String> {
    let mut command = Command::new(binary);
    match family {
        Family::Transform => {
            command.arg(taskpack_dir().join("corpus/transmutation.tsv"));
        }
        Family::Protocol => {
            command.arg(taskpack_dir().join("corpus/protocol_traces.tsv"));
        }
        Family::Wallet => {}
    }
    let output = command
        .output()
        .map_err(|error| format!("cannot run the grader: {error}"))?;
    let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if stdout.trim().is_empty() {
        // A grader that exits before printing anything explains itself on stderr.
        stdout.push_str("[stderr] ");
        stdout.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    Ok(parse_grader_run(output.status.success(), stdout))
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
    // The candidate shares the oracle's stdout, so the run must contain exactly the lines the
    // oracle itself owes: one per check and one summary. Anything else was written by the
    // candidate and could have satisfied a verdict check it never earned.
    assert_eq!(
        wallet_run.verdict_lines,
        WALLET_CHECKS.len(),
        "the wallet grader emitted {} verdict lines for {} checks:\n{}",
        wallet_run.verdict_lines,
        WALLET_CHECKS.len(),
        wallet_run.stdout
    );
    assert_eq!(
        wallet_run.summary_lines, 1,
        "the wallet grader must emit exactly one summary line:\n{}",
        wallet_run.stdout
    );

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
    assert_eq!(
        transform_run.verdict_lines,
        TRANSFORM_FUNCTIONS.len(),
        "the transform grader emitted {} verdict lines for {} functions:\n{}",
        transform_run.verdict_lines,
        TRANSFORM_FUNCTIONS.len(),
        transform_run.stdout
    );
    assert_eq!(
        transform_run.summary_lines, 1,
        "the transform grader must emit exactly one summary line:\n{}",
        transform_run.stdout
    );
    for (function, (rows, failures)) in &transform_run.transform {
        assert!(*rows > 0, "{function} had no corpus rows");
        assert_eq!(
            *failures, 0,
            "the transform reference failed {failures} rows of {function}:\n{}",
            transform_run.stdout
        );
    }

    let protocol = build_grader(
        &taskpack_dir().join("reference/protocol_ref.rs"),
        Family::Protocol,
        "reference-protocol",
    )
    .expect("the protocol reference must assemble and compile");
    let protocol_run =
        run_grader(&protocol, Family::Protocol).expect("the protocol grader must run");
    assert!(
        protocol_run.passed,
        "the protocol reference disagreed with the recording:\n{}",
        protocol_run.stdout
    );
    assert!(
        !protocol_run.traces.is_empty(),
        "the protocol recording contained no traces"
    );
    assert_eq!(
        protocol_run.verdict_lines,
        protocol_run.traces.len(),
        "the protocol grader emitted {} verdict lines for {} traces:\n{}",
        protocol_run.verdict_lines,
        protocol_run.traces.len(),
        protocol_run.stdout
    );
    assert_eq!(
        protocol_run.summary_lines, 1,
        "the protocol grader must emit exactly one summary line:\n{}",
        protocol_run.stdout
    );
    for (trace, passed) in &protocol_run.traces {
        assert!(
            *passed,
            "the protocol reference failed {trace}:\n{}",
            protocol_run.stdout
        );
    }
    assert!(protocol_run
        .stdout
        .contains("oracle failed_traces=0 failed="));
}

#[test]
fn every_seeded_bug_is_caught_by_its_own_signal() {
    if !rustc_available() {
        eprintln!("skipping: no rustc on PATH (CI runs bench/taskpack/run.sh matrix)");
        return;
    }

    // Each transmutation seed owns one corpus function, so summing the row count of every seed's
    // target counts the whole corpus exactly once.
    let mut covered_rows = 0u64;
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
                covered_rows += rows;
                assert!(
                    run.stdout
                        .contains(&format!("grader failed_functions=1 failed={target}")),
                    "unexpected grader summary for seed {seed}:\n{}",
                    run.stdout
                );
            }
            "traces" => {
                let mut broken: Vec<&str> = run
                    .traces
                    .iter()
                    .filter(|(_, passed)| !**passed)
                    .map(|(name, _)| name.as_str())
                    .collect();
                broken.sort_unstable();
                let mut expected: Vec<&str> = target.split(',').collect();
                expected.sort_unstable();
                assert_eq!(
                    broken, expected,
                    "seed {seed} ({why}) should fail exactly its pinned traces:\n{}",
                    run.stdout
                );
                let recorded = format!(
                    "oracle failed_traces={} failed={}",
                    expected.len(),
                    expected.join(",")
                );
                assert!(
                    run.stdout.contains(&recorded),
                    "unexpected protocol oracle summary for seed {seed}:\n{}",
                    run.stdout
                );
            }
            other => panic!("unknown expectation kind: {other}"),
        }
    }
    assert!(
        covered_rows >= 2_000,
        "the differential corpus shrank to {covered_rows} rows"
    );
}

#[test]
fn seeds_cover_every_oracle_signal() {
    let mut wallet_covered: BTreeSet<&str> = BTreeSet::new();
    let mut transform_covered: BTreeSet<&str> = BTreeSet::new();
    let mut trace_covered: BTreeSet<&str> = BTreeSet::new();
    let mut protocol_seeds = 0usize;
    for (_, family, _, expectation, _) in SEEDED {
        let (kind, target) = expectation.split_once(':').expect("kind:target");
        match family {
            Family::Wallet => {
                assert_eq!(kind, "check", "wallet seeds are graded by a named check");
                wallet_covered.insert(target);
            }
            Family::Transform => {
                assert_eq!(
                    kind, "function",
                    "transmutation seeds are graded per function"
                );
                transform_covered.insert(target);
            }
            Family::Protocol => {
                assert_eq!(
                    kind, "traces",
                    "protocol seeds are graded by a pinned trace set"
                );
                protocol_seeds += 1;
                for trace in target.split(',') {
                    // A trace claimed by two seeds would make "this seed failed because of its
                    // own bug" unprovable, so the insert must always be new.
                    assert!(
                        trace_covered.insert(trace),
                        "trace {trace} is pinned to more than one protocol seed"
                    );
                }
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
        WALLET_CHECKS.len() - 1 + TRANSFORM_FUNCTIONS.len() + protocol_seeds,
        SEEDED.len()
    );
    // The recording must carry traces that no seed pins, or a candidate could pass every pinned
    // trace by rejecting legal protocol behaviour outright.
    let recorded = std::fs::read_to_string(taskpack_dir().join("corpus/protocol_traces.tsv"))
        .expect("the protocol recording must be checked in");
    let names: BTreeSet<&str> = recorded
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| line.split('\t').next().unwrap_or_default())
        .collect();
    assert!(
        names.len() > trace_covered.len(),
        "the recording has no unpinned decoy traces"
    );
    for trace in &trace_covered {
        assert!(
            names.contains(trace),
            "a seed pins {trace}, which the recording does not contain"
        );
    }
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
            Family::Protocol => {
                assert_eq!(
                    text.matches("pub mod protocol {").count(),
                    1,
                    "{seed} must define exactly one protocol module"
                );
                assert!(
                    text.contains("pub fn replay("),
                    "{seed} is missing the pinned replay function"
                );
            }
        }
    }
    for reference in [
        "reference/wallet_ref.rs",
        "reference/transform_ref.rs",
        "reference/protocol_ref.rs",
    ] {
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
    let protocol = std::fs::read_to_string(taskpack_dir().join("oracle/protocol_oracle.rs"))
        .expect("protocol oracle must be readable");
    for forbidden in ["include!", "seeded/", "reference/"] {
        assert!(
            !protocol.contains(forbidden),
            "the hidden protocol oracle must not reference a candidate via {forbidden}"
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

#[test]
fn protocol_recording_is_well_formed() {
    let path = taskpack_dir().join("corpus/protocol_traces.tsv");
    let text = std::fs::read_to_string(&path).expect("the protocol recording must be checked in");
    let mut ops: BTreeMap<&str, usize> = BTreeMap::new();
    let mut outcomes: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(
            fields.len(),
            3,
            "row {} must have exactly three columns",
            index + 1
        );
        let name = fields[0];
        *ops.entry(name).or_insert(0) += usize::from(fields[1] != "result");
        if fields[1] == "result" {
            *outcomes.entry(name).or_insert(0) += 1;
            let payload = fields[2].trim();
            assert!(
                payload.starts_with("state ") || payload.starts_with("error "),
                "trace {name} has an unreadable outcome"
            );
            if let Some(error) = payload.strip_prefix("error ") {
                assert!(
                    error.contains("op=") && error.contains("error="),
                    "trace {name} records an error without an op index and a name"
                );
            } else {
                for field in ["begun=", "committed=", "aborted=", "reads=", "records="] {
                    assert!(
                        payload.contains(field),
                        "trace {name} state is missing {field}"
                    );
                }
            }
        }
    }
    assert!(
        !outcomes.is_empty(),
        "the protocol recording contains no traces"
    );
    for (name, count) in &outcomes {
        assert_eq!(*count, 1, "trace {name} records more than one outcome");
        assert!(
            ops.contains_key(name),
            "trace {name} records an outcome but no operations"
        );
    }
    for name in ops.keys() {
        assert!(
            outcomes.contains_key(name),
            "trace {name} has operations but no recorded outcome"
        );
    }
}

/// The grading shell must not route its verdict checks through a pipeline.
///
/// `run.sh` runs under `set -euo pipefail`. `printf ... | grep -qx` looks harmless, but `grep -q`
/// exits as soon as it matches, the writer then dies of `SIGPIPE` (141), and `pipefail` promotes
/// that to the command's status — so a *successful* match is reported as a miss. The oracle output
/// is small enough today that the write fits the pipe buffer and the flaw stays latent, which is
/// exactly why it needs a regression test rather than an incident.
#[test]
fn grading_shell_does_not_pipe_grep_into_pipefail() {
    let script = std::fs::read_to_string(taskpack_dir().join("run.sh"))
        .expect("the grading script must be readable");
    for (index, line) in script.lines().enumerate() {
        // The comment above the helper quotes the forbidden form to explain why it is forbidden,
        // so only executable lines can violate it.
        let code = line.trim_start();
        if code.starts_with('#') {
            continue;
        }
        let verdict_check = line.contains("grep -qx");
        let piped = line.contains("| grep");
        assert!(
            !(verdict_check && piped),
            "run.sh:{} feeds a verdict check through a pipeline; under `set -o pipefail` a \
             successful `grep -q` can be reported as a miss when the writer is killed by SIGPIPE: \
             {line}",
            index + 1
        );
    }
    // The helper that replaced the pipelines must actually be defined and used.
    assert!(
        script.contains("matched() {"),
        "run.sh no longer defines the SIGPIPE-safe `matched` helper"
    );
}

/// A grader's stderr must survive into the graded output.
///
/// `output="$(...)"` captures stdout only. An oracle that rejects a malformed recording exits
/// before printing any verdict, so without folding stderr in, "the recording is malformed" and
/// "the candidate failed the wrong check" produce the same empty string.
#[test]
fn grading_shell_folds_grader_stderr_into_the_capture() {
    let script = std::fs::read_to_string(taskpack_dir().join("run.sh"))
        .expect("the grading script must be readable");
    // Both halves must appear in the same `run_candidate` body: a redirect with no fold would
    // capture the diagnostics and then throw them away, which is the defect this guards.
    let body = script
        .split("run_candidate()")
        .nth(1)
        .expect("run.sh must still define run_candidate");
    assert!(
        body.contains("2>\"$diagnostics\""),
        "run_candidate no longer redirects grader stderr to a file"
    );
    assert!(
        body.contains("\"[stderr] \""),
        "run_candidate no longer folds stderr into the captured output, so a silent grader looks \
         like a wrong answer"
    );
}

/// A candidate shares the grader's stdout, so it can print text shaped like a verdict.
///
/// `run.sh` decides each pin with `grep -qx`, which accepts *any* matching line. A candidate that
/// cannot fail the pinned traces can still print `trace <name> fail ...` lines for them and a
/// summary naming exactly that set, and satisfy every pin without earning one. The oracle emits
/// exactly one `trace` line per recorded trace and exactly one summary, so surplus lines are
/// candidate impersonation and the whole run must be rejected rather than graded.
#[test]
fn grading_shell_detects_a_forged_protocol_verdict() {
    let script = std::fs::read_to_string(taskpack_dir().join("run.sh"))
        .expect("the grading script must be readable");
    assert!(
        script.contains("forged its verdict"),
        "run.sh no longer rejects a protocol run that wrote its own verdict lines"
    );
    assert!(
        script.contains("grep -c '^trace '"),
        "run.sh no longer counts trace verdict lines against the recording"
    );
    assert!(
        script.contains("grep -c '^oracle failed_traces='"),
        "run.sh no longer requires exactly one oracle summary line"
    );
}
