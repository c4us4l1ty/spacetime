//! CLI supervisor for Spacetime Block 1.

use spacetime_cortex::{Cortex, ScriptedCortex};
use spacetime_exec::{Runner, ScriptedRunner};
use spacetime_ledger::{Ledger, LedgerState};
use spacetime_types::{
    sha256_hex, DecisionKind, Event, GoalStatus, PolicyDecision, Result, SpacetimeError,
};
use spacetime_verifier::{FixtureVerifier, Verifier};
use std::fs;
use std::path::{Path, PathBuf};

const USAGE: &str = r#"spacetime-ctl — Block 1 CLI

Commands:
  run --task <file> --cortex scripted --ledger <file>
  ledger verify <file>
  replay <file>
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

fn cmd_run(args: &[String]) -> Result<()> {
    let task = flag(args, "--task")?;
    let cortex = flag(args, "--cortex")?;
    let ledger = flag(args, "--ledger")?;
    if cortex != "scripted" {
        return Err(SpacetimeError::new(format!("unsupported cortex: {cortex}")));
    }
    run_task(Path::new(&task), Path::new(&ledger))?;
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
    for index in 0..args.len() {
        if args[index].as_str() == name {
            return args
                .get(index + 1)
                .cloned()
                .ok_or_else(|| SpacetimeError::new(format!("missing value for {name}")));
        }
    }
    Err(SpacetimeError::new(format!("missing flag {name}")))
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
            reason: "block1 placeholder; no external effects".to_string(),
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
    Ok(())
}

fn state_path(ledger_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.state", ledger_path.display()))
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
        run_task(&fixture("block1_false_claim.task"), &paths.ledger)?;
        let state = Ledger::replay(&paths.ledger)?;
        assert_eq!(state.goal_status, GoalStatus::Unverified);
        Ok(())
    }

    #[test]
    fn timeout_is_a_failure_not_a_pass() -> Result<()> {
        let paths = TempPaths::new("timeout")?;
        run_task(&fixture("block1_timeout.task"), &paths.ledger)?;
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
}
