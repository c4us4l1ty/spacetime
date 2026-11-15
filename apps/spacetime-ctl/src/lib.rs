//! CLI supervisor for Spacetime Blocks 1-2.

use spacetime_cortex::{Cortex, ScriptedCortex};
use spacetime_exec::{Runner, ScriptedRunner};
use spacetime_ledger::{Ledger, LedgerState};
use spacetime_types::{
    sha256_hex, DecisionKind, Event, GoalStatus, PolicyDecision, Result, SpacetimeError,
};
use spacetime_verifier::{
    differential_score, false_pass_report, mutation_score_bps, verifier_source_priority, Candidate,
    CrucibleVerifier, Fixture, FixtureVerifier, Verifier,
};
use std::fs;
use std::path::{Path, PathBuf};

mod verify;

const USAGE: &str = r#"spacetime-ctl — Block 1-2 CLI

Commands:
  run --task <file> --cortex scripted --ledger <file>
  ledger verify <file>
  replay <file>
  crucible verify --task <file>
  crucible false-pass --task <file> --runs <n>
  crucible differential --limit <n>
  crucible sources
  verify --out <file>
  machine
  digest <text>
  help
"#;

pub fn run(args: &[String]) -> Result<()> {
    let command = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| SpacetimeError::new(format!("missing command\n{USAGE}")))?;

    match command {
        "run" => cmd_run(&args[1..]),
        "ledger" => {
            let subcommand = args
                .get(1)
                .map(String::as_str)
                .ok_or_else(|| SpacetimeError::new("missing ledger subcommand"))?;
            match subcommand {
                "verify" => {
                    let path = args
                        .get(2)
                        .map(String::as_str)
                        .ok_or_else(|| SpacetimeError::new("missing ledger path"))?;
                    cmd_verify(path)
                }
                _ => Err(SpacetimeError::new(format!(
                    "unknown ledger subcommand: {subcommand}"
                ))),
            }
        }
        "replay" => {
            let path = args
                .get(1)
                .map(String::as_str)
                .ok_or_else(|| SpacetimeError::new("missing ledger path"))?;
            cmd_replay(path)
        }
        "crucible" => cmd_crucible(&args[1..]),
        "verify" => verify::cmd_verify_all(&args[1..]),
        "machine" => cmd_machine(),
        "digest" => {
            let text = args
                .get(1)
                .map(String::as_str)
                .ok_or_else(|| SpacetimeError::new("missing digest text"))?;
            cmd_digest(text)
        }
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            Ok(())
        }
        _ => Err(SpacetimeError::new(format!(
            "unknown command: {command}\n{USAGE}"
        ))),
    }
}

fn cmd_crucible(args: &[String]) -> Result<()> {
    let subcommand = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| SpacetimeError::new("missing crucible subcommand"))?;
    match subcommand {
        "verify" => cmd_crucible_verify(&args[1..]),
        "false-pass" => cmd_crucible_false_pass(&args[1..]),
        "differential" => cmd_crucible_differential(&args[1..]),
        "sources" => cmd_crucible_sources(),
        _ => Err(SpacetimeError::new(format!(
            "unknown crucible subcommand: {subcommand}"
        ))),
    }
}

pub(crate) fn crucible_inputs(task_path: &Path) -> Result<(Candidate, Fixture)> {
    let cortex = ScriptedCortex::from_file(task_path)?;
    let action = cortex.first_action()?;
    let expected_digest = cortex
        .expected_digest()
        .ok_or_else(|| SpacetimeError::new("task has no expected_digest"))?;
    let source = cortex.verifier_source()?;
    let expected_verdict = cortex.expected_verdict()?;
    let candidate = Candidate::new("scripted-candidate", action);
    let fixture = Fixture {
        id: "scripted-fixture".to_string(),
        source,
        expected_digest,
        expected_verdict,
        determinism_runs: 2,
        mutation_threshold_bps: 8_000,
    };
    Ok((candidate, fixture))
}

fn cmd_crucible_verify(args: &[String]) -> Result<()> {
    let task = flag(args, "--task")?;
    let (candidate, fixture) = crucible_inputs(Path::new(task.as_str()))?;
    let runner = ScriptedRunner::new()?;
    let proof = CrucibleVerifier.verify_candidate(&candidate, &fixture, &runner);
    println!(
        "verdict={} verifier_id={}",
        proof.verdict.as_str(),
        proof.verifier_id
    );
    println!(
        "source={:?} source_priority={} expected_verdict={}",
        proof.source,
        proof.source_priority,
        fixture.expected_verdict.as_str()
    );
    println!(
        "determinism_runs={} mutation_score_bps={} evidence_digest={}",
        proof.determinism_runs, proof.mutation_score_bps, proof.evidence_digest
    );
    println!(
        "execution_profile_digest={}",
        proof.execution_profile_digest
    );
    if proof.verdict != spacetime_types::Verdict::Pass {
        return Err(SpacetimeError::new(format!(
            "crucible did not pass: {:?}",
            proof.verdict
        )));
    }
    Ok(())
}

fn cmd_crucible_false_pass(args: &[String]) -> Result<()> {
    let task = flag(args, "--task")?;
    let runs = flag(args, "--runs")?.parse::<u32>()?;
    let (candidate, fixture) = crucible_inputs(Path::new(task.as_str()))?;
    let runner = ScriptedRunner::new()?;
    let (rate, leak_rate) = false_pass_report(&candidate, &fixture, &runner, runs);
    println!("false_pass_rate={rate:.4} false_pass_leaks={leak_rate:.4} runs={runs}");
    if rate > 0.01 || leak_rate > 0.01 {
        return Err(SpacetimeError::new(format!(
            "false-PASS rate {rate:.4} or leak rate {leak_rate:.4} exceeds the 1% kill gate"
        )));
    }
    Ok(())
}

fn cmd_crucible_sources() -> Result<()> {
    for source in verifier_source_priority() {
        println!("priority={} source={:?}", source.priority(), source);
    }
    Ok(())
}

fn cmd_crucible_differential(args: &[String]) -> Result<()> {
    let limit = flag(args, "--limit")?.parse::<u64>()?;
    let (matched, inputs) = differential_score(limit);
    let score = mutation_score_bps(limit);
    println!("differential_matches={matched} inputs={inputs} mutation_score_bps={score}");
    if matched != inputs || score < 8_000 {
        return Err(SpacetimeError::new("differential or mutation gate failed"));
    }
    Ok(())
}

fn cmd_run(args: &[String]) -> Result<()> {
    let task = flag(args, "--task")?;
    let cortex = flag(args, "--cortex")?;
    let ledger = flag(args, "--ledger")?;
    if cortex != "scripted" {
        return Err(SpacetimeError::new(format!("unsupported cortex: {cortex}")));
    }
    run_task(Path::new(task.as_str()), Path::new(ledger.as_str()))?;
    println!("ok");
    Ok(())
}

fn cmd_verify(path: &str) -> Result<()> {
    let events = Ledger::verify(Path::new(path))?;
    let count = events.len();
    println!("ok {count} events");
    Ok(())
}

fn cmd_replay(path: &str) -> Result<()> {
    let state = Ledger::replay(Path::new(path))?;
    let json = state.to_json_string()?;
    print!("{json}");
    Ok(())
}

fn cmd_machine() -> Result<()> {
    let baseline = spacetime_bench::baseline_toml()?;
    print!("{baseline}");
    Ok(())
}

fn cmd_digest(text: &str) -> Result<()> {
    let digest = sha256_hex(text.as_bytes());
    println!("{digest}");
    Ok(())
}

fn flag(args: &[String], name: &str) -> Result<String> {
    let Some(index) = args.iter().position(|arg| arg.as_str() == name) else {
        return Err(SpacetimeError::new(format!("missing flag {name}")));
    };
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| SpacetimeError::new(format!("missing value for {name}")))
}

pub fn run_task(task_path: &Path, ledger_path: &Path) -> Result<()> {
    let mut cortex = ScriptedCortex::from_file(task_path)?;
    let mut ledger = Ledger::create(ledger_path)?;
    let runner = ScriptedRunner::new()?;
    let verifier = FixtureVerifier;
    let mut state = LedgerState::new();

    while let Some(action) = cortex.next_action(&state)? {
        let decision = PolicyDecision {
            decision: DecisionKind::Allow,
            reason: "blocks 1-2 placeholder; no external effects".to_string(),
        };
        let result = runner.run(&action);
        let proof = verifier.verify(&action, &result.observation, &result.status, &state);
        let event = Event::new(
            ledger.next_seq(),
            ledger.prev_hash(),
            action,
            result.observation,
            proof,
            decision,
        )?;
        ledger.append(&event)?;
        state.apply(&event);

        if state.goal_status == GoalStatus::Verified {
            break;
        }
    }

    fs::write(state_path(ledger_path), state.to_json_string()?)?;
    let status = state.goal_status;
    let events = state.event_count;
    let ledger = ledger_path.display();
    println!("status={status:?} events={events} ledger={ledger}");
    if status != GoalStatus::Verified {
        return Err(SpacetimeError::new(format!(
            "goal not verified: {status:?}"
        )));
    }
    Ok(())
}

pub(crate) fn state_path(ledger_path: &Path) -> PathBuf {
    let mut state = ledger_path.as_os_str().to_os_string();
    state.push(".state");
    PathBuf::from(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempPaths {
        dir: PathBuf,
        ledger: PathBuf,
    }

    impl TempPaths {
        fn new(label: &str) -> Result<Self> {
            let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let pid = std::process::id();
            let dir = std::env::temp_dir().join(format!("spacetime-ctl-{label}-{pid}-{id}"));
            fs::create_dir_all(&dir)?;
            let ledger = dir.join("ledger.jsonl");
            Ok(Self { dir, ledger })
        }

        fn state(&self) -> PathBuf {
            state_path(&self.ledger)
        }
    }

    impl Drop for TempPaths {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../bench/tasks")
            .join(name)
    }

    #[test]
    fn happy_path_verifies_and_replays_identically() -> Result<()> {
        let paths = TempPaths::new("happy")?;
        run_task(&fixture("block1_hello.task"), &paths.ledger)?;
        Ledger::verify(&paths.ledger)?;
        let live = fs::read_to_string(paths.state())?;
        let replayed = Ledger::replay(&paths.ledger)?;
        assert_eq!(live, replayed.to_json_string()?);
        assert_eq!(replayed.goal_status, GoalStatus::Verified);
        Ok(())
    }

    #[test]
    fn false_claim_does_not_advance_goal() -> Result<()> {
        let paths = TempPaths::new("false")?;
        assert!(run_task(&fixture("block1_false_claim.task"), &paths.ledger).is_err());
        let state = Ledger::replay(&paths.ledger)?;
        assert_eq!(state.goal_status, GoalStatus::Unverified);
        Ok(())
    }

    #[test]
    fn timeout_is_a_failure_not_a_pass() -> Result<()> {
        let paths = TempPaths::new("timeout")?;
        assert!(run_task(&fixture("block1_timeout.task"), &paths.ledger).is_err());
        let state = Ledger::replay(&paths.ledger)?;
        assert_eq!(state.goal_status, GoalStatus::Failed);
        Ok(())
    }

    #[test]
    fn tamper_and_truncation_are_detected() -> Result<()> {
        let tampered = TempPaths::new("tamper")?;
        run_task(&fixture("block1_hello.task"), &tampered.ledger)?;
        let mut bytes = fs::read(&tampered.ledger)?;
        if bytes.len() > 10 {
            bytes[10] ^= 1;
        }
        fs::write(&tampered.ledger, bytes)?;
        assert!(Ledger::verify(&tampered.ledger).is_err());

        let truncated = TempPaths::new("truncate")?;
        run_task(&fixture("block1_hello.task"), &truncated.ledger)?;
        let text = fs::read_to_string(&truncated.ledger)?;
        let mut lines: Vec<&str> = text.lines().collect();
        let _ = lines.pop();
        fs::write(&truncated.ledger, lines.join("\n"))?;
        assert!(Ledger::verify(&truncated.ledger).is_err());
        Ok(())
    }

    #[test]
    fn incomplete_goal_is_not_reported_as_success() -> Result<()> {
        let paths = TempPaths::new("incomplete")?;
        let task = paths.dir.join("incomplete.task");
        fs::write(
            &task,
            "schema_version=1\naction produce_evidence evidence=x\n",
        )?;
        assert!(run_task(&task, &paths.ledger).is_err());
        let state = fs::read_to_string(paths.state())?;
        assert!(state.contains("\"goal_status\":\"in_progress\""));
        Ok(())
    }

    #[test]
    fn non_canonical_ledger_line_is_rejected() -> Result<()> {
        let paths = TempPaths::new("canonical")?;
        run_task(&fixture("block1_hello.task"), &paths.ledger)?;
        let text = fs::read_to_string(&paths.ledger)?;
        let first_line = text.lines().next().expect("first line");
        let event = spacetime_types::Event::from_json_str(first_line)?;
        let non_canonical = serde_json::to_string(&event)?;
        assert_ne!(non_canonical, first_line);
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        lines[0] = non_canonical;
        fs::write(&paths.ledger, lines.join("\n") + "\n")?;
        assert!(Ledger::verify(&paths.ledger).is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn state_path_preserves_non_utf8_bytes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let path = PathBuf::from(OsString::from_vec(vec![b'f', b'o', b'o', 0xff]));
        let state = state_path(&path);
        let mut expected = path.into_os_string();
        expected.push(".state");
        assert_eq!(state.as_os_str(), expected.as_os_str());
    }

    #[test]
    fn crucible_verify_passes_on_deterministic_fixture() -> Result<()> {
        let (candidate, fixture) = crucible_inputs(&fixture("block1_hello.task"))?;
        let runner = ScriptedRunner::new()?;
        let proof = CrucibleVerifier.verify_candidate(&candidate, &fixture, &runner);
        assert_eq!(proof.verdict, spacetime_types::Verdict::Pass);
        Ok(())
    }

    #[test]
    fn crucible_verify_rejects_flaky_fixture() -> Result<()> {
        let (candidate, fixture) = crucible_inputs(&fixture("block2_flaky.task"))?;
        let runner = ScriptedRunner::new()?;
        let proof = CrucibleVerifier.verify_candidate(&candidate, &fixture, &runner);
        assert_eq!(proof.verdict, spacetime_types::Verdict::Error);
        Ok(())
    }
}
