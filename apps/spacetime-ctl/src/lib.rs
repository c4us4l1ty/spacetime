//! CLI supervisor for Spacetime Blocks 1-2.

use spacetime_bench::{baseline_toml, resource_gate, ResourceGateStatus};
use spacetime_cortex::{Cortex, ScriptedCortex};
use spacetime_exec::{Runner, ScriptedRunner};
use spacetime_ledger::{Ledger, LedgerState};
use spacetime_types::{
    sha256_hex, DecisionKind, Event, GoalStatus, PolicyDecision, Result, SpacetimeError, Verdict,
    VerifierSource,
};
use spacetime_verifier::{
    differential_score, false_pass_report_checked, mutation_score_bps, population_report_checked,
    validate_differential_limit, validate_operator_count, validate_reruns,
    verifier_source_priority, Candidate, CrucibleVerifier, Fixture, FixtureVerifier,
    PanelResolution, Verifier, VerifierPanel,
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
  crucible panel --task <file>
  crucible adversarial --dir <dir>
  crucible population --operators <n> --runs <n>
  crucible fixture-population --runs <n>
  verify --out <file>
  machine
  machine gate [--measurement <file>]
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
        "machine" => cmd_machine(&args[1..]),
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
        "panel" => cmd_crucible_panel(&args[1..]),
        "adversarial" => cmd_crucible_adversarial(&args[1..]),
        "population" => cmd_crucible_population(&args[1..]),
        "fixture-population" => cmd_crucible_fixture_population(&args[1..]),
        _ => Err(SpacetimeError::new(format!(
            "unknown crucible subcommand: {subcommand}"
        ))),
    }
}

fn tasks_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/tasks")
}

fn adversarial_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/adversarial")
}

pub(crate) fn crucible_inputs(task_path: &Path) -> Result<(Candidate, Fixture)> {
    let cortex = ScriptedCortex::from_file(task_path)?;
    let action = cortex.first_action()?;
    let expected_digest = cortex
        .expected_digest()
        .ok_or_else(|| SpacetimeError::new("task has no expected_digest"))?;
    let source = cortex.verifier_source()?;
    let expected_verdict = cortex.expected_verdict()?;
    let determinism_runs = cortex.determinism_runs()?;
    let mutation_threshold_bps = cortex.mutation_threshold_bps()?;
    let candidate_mutation_score_bps = cortex.candidate_mutation_score_bps()?;
    let candidate =
        Candidate::with_mutation_score("scripted-candidate", action, candidate_mutation_score_bps);
    let fixture = Fixture {
        id: "scripted-fixture".to_string(),
        source,
        expected_digest,
        expected_verdict,
        determinism_runs,
        mutation_threshold_bps,
    };
    Ok((candidate, fixture))
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FixturePopulationReport {
    pub fixtures: usize,
    pub skipped_without_oracle: usize,
    pub runs_per_fixture: u32,
    pub verifier_runs: u64,
    pub planned_runner_executions: u64,
    pub mismatches: usize,
    pub stable: usize,
    pub unstable: usize,
    pub max_flip_rate: f64,
    pub max_leak_rate: f64,
    pub real_fixture_population: bool,
    pub real_operator_study: bool,
}

pub(crate) fn fixture_population_report(runs: u32) -> Result<FixturePopulationReport> {
    validate_reruns(runs)?;
    let mut paths = Vec::new();
    for dir in [tasks_dir(), adversarial_dir()] {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("task") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    let mut report = FixturePopulationReport {
        runs_per_fixture: runs,
        real_fixture_population: true,
        real_operator_study: false,
        ..Default::default()
    };
    for path in paths {
        let cortex = ScriptedCortex::from_file(&path)?;
        if cortex.expected_digest().is_none() {
            report.skipped_without_oracle += 1;
            continue;
        }
        let (candidate, fixture) = crucible_inputs(&path)?;
        let runner = ScriptedRunner::new()?;
        let (flip_rate, leak_rate) =
            false_pass_report_checked(&candidate, &fixture, &runner, runs)?;
        let proof = CrucibleVerifier.verify_candidate(&candidate, &fixture, &runner);
        report.fixtures += 1;
        report.verifier_runs += u64::from(runs);
        if candidate.mutation_score_bps >= fixture.mutation_threshold_bps {
            report.planned_runner_executions +=
                u64::from(runs) * u64::from(fixture.determinism_runs);
        }
        if flip_rate > report.max_flip_rate {
            report.max_flip_rate = flip_rate;
        }
        if leak_rate > report.max_leak_rate {
            report.max_leak_rate = leak_rate;
        }
        if flip_rate > 0.0 || leak_rate > 0.0 {
            report.unstable += 1;
        } else {
            report.stable += 1;
        }
        if proof.verdict != fixture.expected_verdict {
            report.mismatches += 1;
        }
    }
    Ok(report)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AdversarialReport {
    pub fixtures: usize,
    pub mismatches: usize,
    pub expected_pass: usize,
    pub expected_fail: usize,
    pub expected_error: usize,
    pub expected_unverified: usize,
}

pub(crate) fn adversarial_report(dir: &Path) -> Result<AdversarialReport> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("task") {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.len() < 20 {
        return Err(SpacetimeError::new(format!(
            "adversarial corpus has {} fixtures; expected at least 20",
            paths.len()
        )));
    }
    let mut report = AdversarialReport {
        fixtures: paths.len(),
        ..Default::default()
    };
    for path in &paths {
        let (candidate, fixture) = crucible_inputs(path)?;
        if fixture.source != VerifierSource::AdversarialModel {
            return Err(SpacetimeError::new(format!(
                "{} is not adversarial_model",
                path.display()
            )));
        }
        let runner = ScriptedRunner::new()?;
        let proof = CrucibleVerifier.verify_candidate(&candidate, &fixture, &runner);
        if proof.source != VerifierSource::AdversarialModel
            || proof.source_priority != fixture.source.priority()
        {
            return Err(SpacetimeError::new(format!(
                "{} lost adversarial verifier provenance",
                path.display()
            )));
        }
        if proof.determinism_runs < 2 {
            return Err(SpacetimeError::new(format!(
                "{} did not run determinism twice",
                path.display()
            )));
        }
        if proof.verdict == Verdict::Pass && proof.evidence_digest.is_empty() {
            return Err(SpacetimeError::new(format!(
                "{} has a PASS without evidence",
                path.display()
            )));
        }
        if proof.verdict != fixture.expected_verdict {
            report.mismatches += 1;
        }
        match fixture.expected_verdict {
            Verdict::Pass => report.expected_pass += 1,
            Verdict::Fail => report.expected_fail += 1,
            Verdict::Error => report.expected_error += 1,
            Verdict::Unverified => report.expected_unverified += 1,
        }
    }
    Ok(report)
}

pub(crate) fn panel_resolution_for_task(task_path: &Path) -> Result<PanelResolution> {
    let (candidate, fixture) = crucible_inputs(task_path)?;
    let runner = ScriptedRunner::new()?;
    let mut panel = VerifierPanel::new();

    let mut suite_fixture = fixture.clone();
    suite_fixture.source = VerifierSource::FixtureSuite;
    let suite_proof = CrucibleVerifier.verify_candidate(&candidate, &suite_fixture, &runner);
    panel = panel.push(
        VerifierSource::FixtureSuite,
        suite_proof.verdict,
        suite_proof.evidence_digest.clone(),
        suite_proof.mutation_score_bps,
    );

    let observed = runner.run(&candidate.action);
    let spec_verdict = if observed.observation.digest.as_str() == fixture.expected_digest.as_str() {
        Verdict::Pass
    } else {
        Verdict::Fail
    };
    panel = panel.push(
        VerifierSource::SpecOracle,
        spec_verdict,
        observed.observation.digest.clone(),
        candidate.mutation_score_bps,
    );

    let (matched, inputs) = differential_score(1_000_000);
    let score = mutation_score_bps(1_000_000);
    let differential_verdict = if matched == inputs && score >= fixture.mutation_threshold_bps {
        Verdict::Pass
    } else {
        Verdict::Fail
    };
    panel = panel.push(
        VerifierSource::DifferentialReference,
        differential_verdict,
        format!("{matched}/{inputs}"),
        score,
    );

    let trivial = Candidate::with_mutation_score(
        "adversarial-trivial-candidate",
        candidate.action.clone(),
        0,
    );
    let mut adversarial_fixture = fixture.clone();
    adversarial_fixture.source = VerifierSource::AdversarialModel;
    adversarial_fixture.expected_verdict = Verdict::Fail;
    let adversarial_proof =
        CrucibleVerifier.verify_candidate(&trivial, &adversarial_fixture, &runner);
    let adversarial_strength = if adversarial_proof.verdict == Verdict::Fail {
        10_000
    } else {
        0
    };
    let model_verdict = if adversarial_strength == 10_000 {
        Verdict::Pass
    } else {
        Verdict::Fail
    };
    panel = panel.push(
        VerifierSource::AdversarialModel,
        model_verdict,
        adversarial_proof.evidence_digest.clone(),
        adversarial_strength,
    );

    let mutation_verdict = if candidate.mutation_score_bps >= fixture.mutation_threshold_bps {
        Verdict::Pass
    } else {
        Verdict::Fail
    };
    panel = panel.push(
        VerifierSource::Mutation,
        mutation_verdict,
        candidate.artifact_digest.clone(),
        candidate.mutation_score_bps,
    );

    Ok(panel.resolve())
}

fn cmd_crucible_panel(args: &[String]) -> Result<()> {
    let task = flag(args, "--task")?;
    let resolution = panel_resolution_for_task(Path::new(task.as_str()))?;
    println!(
        "panel_verdict={} selected_source={:?} selected_priority={} sources={} conflicts={}",
        resolution.verdict.as_str(),
        resolution.selected_source,
        resolution.selected_priority,
        resolution.sources.len(),
        resolution.conflicts,
    );
    println!(
        "panel_evidence_digest={} panel_mutation_score_bps={}",
        resolution.evidence_digest, resolution.mutation_score_bps
    );
    println!("panel_reason={}", resolution.reason);
    if resolution.verdict != Verdict::Pass {
        return Err(SpacetimeError::new(format!(
            "verifier panel did not pass: {:?}",
            resolution.verdict
        )));
    }
    Ok(())
}

fn cmd_crucible_adversarial(args: &[String]) -> Result<()> {
    let dir = optional_flag(args, "--dir")?
        .map(PathBuf::from)
        .unwrap_or_else(adversarial_dir);
    let report = adversarial_report(&dir)?;
    println!(
        "adversarial_fixtures={} mismatches={} expected_pass={} expected_fail={} expected_error={} expected_unverified={}",
        report.fixtures,
        report.mismatches,
        report.expected_pass,
        report.expected_fail,
        report.expected_error,
        report.expected_unverified,
    );
    if report.mismatches > 0 {
        return Err(SpacetimeError::new(format!(
            "{} adversarial fixtures disagreed with their expected verdict",
            report.mismatches
        )));
    }
    Ok(())
}

fn cmd_crucible_population(args: &[String]) -> Result<()> {
    let operators = flag(args, "--operators")?.parse::<usize>()?;
    let runs = flag(args, "--runs")?.parse::<u32>()?;
    validate_operator_count(operators)?;
    validate_reruns(runs)?;
    let report = population_report_checked(operators, runs)?;
    println!(
        "operators={} runs_per_operator={} total_runs={} accepted={} unstable={} max_flip_rate={:.4} promoted_false_pass_leaks={} synthetic={} real_operator_study={}",
        report.operators,
        report.runs_per_operator,
        report.total_runs,
        report.accepted,
        report.unstable,
        report.max_flip_rate,
        report.promoted_false_pass_leaks,
        report.synthetic,
        report.real_operator_study,
    );
    if report.unstable == 0 || report.accepted == 0 {
        return Err(SpacetimeError::new(
            "population study did not exercise stable and unstable operators",
        ));
    }
    if report.promoted_false_pass_leaks > 0 {
        return Err(SpacetimeError::new(
            "promoted synthetic operator had a false-PASS leak",
        ));
    }
    Ok(())
}

fn cmd_crucible_fixture_population(args: &[String]) -> Result<()> {
    let runs = flag(args, "--runs")?.parse::<u32>()?;
    validate_reruns(runs)?;
    let report = fixture_population_report(runs)?;
    println!(
        "fixtures={} skipped_without_oracle={} runs_per_fixture={} verifier_runs={} planned_runner_executions={} stable={} unstable={} max_flip_rate={:.4} max_leak_rate={:.4} mismatches={} real_fixture_population={} real_operator_study={}",
        report.fixtures,
        report.skipped_without_oracle,
        report.runs_per_fixture,
        report.verifier_runs,
        report.planned_runner_executions,
        report.stable,
        report.unstable,
        report.max_flip_rate,
        report.max_leak_rate,
        report.mismatches,
        report.real_fixture_population,
        report.real_operator_study,
    );
    if report.fixtures < 20 {
        return Err(SpacetimeError::new(
            "fixture population is smaller than the 20-task minimum",
        ));
    }
    if report.mismatches > 0 {
        return Err(SpacetimeError::new(format!(
            "{} fixture population verdicts disagreed with their oracle",
            report.mismatches
        )));
    }
    if report.max_flip_rate > 0.01 || report.max_leak_rate > 0.01 {
        return Err(SpacetimeError::new(
            "fixture population exceeded the 1% false-PASS kill gate",
        ));
    }
    Ok(())
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
    if proof.verdict != Verdict::Pass {
        return Err(SpacetimeError::new(format!(
            "crucible did not pass: {:?}",
            proof.verdict
        )));
    }
    if proof.verdict != fixture.expected_verdict {
        return Err(SpacetimeError::new(format!(
            "crucible verdict {:?} did not match expected {:?}",
            proof.verdict, fixture.expected_verdict
        )));
    }
    Ok(())
}

fn cmd_crucible_false_pass(args: &[String]) -> Result<()> {
    let task = flag(args, "--task")?;
    let runs = flag(args, "--runs")?.parse::<u32>()?;
    validate_reruns(runs)?;
    let (candidate, fixture) = crucible_inputs(Path::new(task.as_str()))?;
    let runner = ScriptedRunner::new()?;
    let (rate, leak_rate) = false_pass_report_checked(&candidate, &fixture, &runner, runs)?;
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
    validate_differential_limit(limit)?;
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

fn cmd_machine(args: &[String]) -> Result<()> {
    if args.first().map(String::as_str) == Some("gate") {
        return cmd_machine_gate(&args[1..]);
    }
    let baseline = baseline_toml()?;
    print!("{baseline}");
    Ok(())
}

fn cmd_machine_gate(args: &[String]) -> Result<()> {
    let measurement = optional_flag(args, "--measurement")?;
    let path = measurement.as_deref().map(Path::new);
    let gate = resource_gate(path)?;
    println!(
        "resource_gate={} target_profile={} memory_budget_kb={} measurement_count={}",
        gate.status.as_str(),
        gate.target_profile,
        gate.memory_budget_kb,
        gate.measurements.len(),
    );
    for reason in &gate.reasons {
        println!("reason={reason}");
    }
    if gate.status == ResourceGateStatus::Fail {
        return Err(SpacetimeError::new("resource budget gate failed"));
    }
    Ok(())
}

fn cmd_digest(text: &str) -> Result<()> {
    let digest = sha256_hex(text.as_bytes());
    println!("{digest}");
    Ok(())
}

fn optional_flag(args: &[String], name: &str) -> Result<Option<String>> {
    match args.iter().position(|arg| arg.as_str() == name) {
        Some(index) => args
            .get(index + 1)
            .cloned()
            .map(Some)
            .ok_or_else(|| SpacetimeError::new(format!("missing value for {name}"))),
        None => Ok(None),
    }
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

    #[test]
    fn adversarial_corpus_has_twenty_spec_only_fixtures() -> Result<()> {
        let report = adversarial_report(&adversarial_dir())?;
        assert_eq!(report.fixtures, 20);
        assert_eq!(report.mismatches, 0);
        assert!(report.expected_pass > 0);
        assert!(report.expected_fail > 0);
        assert!(report.expected_error > 0);
        assert_eq!(report.expected_unverified, 0);
        Ok(())
    }

    #[test]
    fn fixture_population_covers_real_fixtures() -> Result<()> {
        let report = fixture_population_report(50)?;
        assert_eq!(report.fixtures, 26);
        assert_eq!(report.skipped_without_oracle, 2);
        assert_eq!(report.runs_per_fixture, 50);
        assert_eq!(report.verifier_runs, 1_300);
        assert!(report.planned_runner_executions > 0);
        assert_eq!(report.mismatches, 0);
        assert_eq!(report.stable, 26);
        assert_eq!(report.unstable, 0);
        assert!(report.max_flip_rate <= 0.01);
        assert!(report.max_leak_rate <= 0.01);
        assert!(report.real_fixture_population);
        assert!(!report.real_operator_study);
        Ok(())
    }

    #[test]
    fn cli_input_validation_rejects_unsafe_ranges() -> Result<()> {
        assert!(validate_reruns(0).is_err());
        assert!(validate_reruns(49).is_err());
        assert!(validate_reruns(10_001).is_err());
        assert!(validate_reruns(50).is_ok());
        assert!(validate_operator_count(4).is_err());
        assert!(validate_operator_count(10_001).is_err());
        assert!(validate_differential_limit(1).is_err());
        assert!(validate_differential_limit(10_000_001).is_err());
        Ok(())
    }

    #[test]
    fn optional_flag_rejects_missing_value() -> Result<()> {
        assert!(optional_flag(&["--dir".to_string()], "--dir").is_err());
        assert_eq!(optional_flag(&[], "--dir")?, None);
        assert_eq!(
            optional_flag(&["--dir".to_string(), "x".to_string()], "--dir")?,
            Some("x".to_string())
        );
        Ok(())
    }

    #[test]
    fn cli_commands_reject_invalid_inputs() -> Result<()> {
        let hello = fixture("block1_hello.task").display().to_string();
        assert!(cmd_crucible_false_pass(&[
            "--task".to_string(),
            hello.clone(),
            "--runs".to_string(),
            "0".to_string(),
        ])
        .is_err());
        assert!(cmd_crucible_differential(&["--limit".to_string(), "1".to_string()]).is_err());
        assert!(cmd_crucible_population(&[
            "--operators".to_string(),
            "10".to_string(),
            "--runs".to_string(),
            "0".to_string(),
        ])
        .is_err());
        assert!(cmd_crucible_fixture_population(&["--runs".to_string(), "0".to_string()]).is_err());
        assert!(cmd_crucible_adversarial(&["--dir".to_string()]).is_err());
        assert!(cmd_machine_gate(&["--measurement".to_string()]).is_err());
        assert!(cmd_crucible_verify(&["--task".to_string(), hello]).is_ok());
        Ok(())
    }

    #[test]
    fn crucible_verify_rejects_expected_verdict_mismatch() -> Result<()> {
        let paths = TempPaths::new("mismatch")?;
        let task = paths.dir.join("mismatch.task");
        fs::write(
            &task,
            "schema_version=1\nverifier_source=fixture_suite\nexpected_verdict=fail\nexpected_digest=sha256:stable\ndeterminism_runs=2\nmutation_threshold_bps=8000\ncandidate_mutation_score_bps=10000\nspace_id=space-mismatch\ngoal_id=goal-mismatch\nsurface_kind=terminal\npreview_mode=headless\nliveness=running\naction echo text=stable\n",
        )?;
        assert!(cmd_crucible_verify(&["--task".to_string(), task.display().to_string()]).is_err());
        Ok(())
    }

    #[test]
    fn verifier_panel_resolves_five_sources() -> Result<()> {
        let resolution = panel_resolution_for_task(&fixture("block1_hello.task"))?;
        assert_eq!(resolution.verdict, Verdict::Pass);
        assert_eq!(resolution.sources.len(), 5);
        assert_eq!(resolution.conflicts, 0);
        assert_eq!(
            resolution.selected_source,
            Some(VerifierSource::FixtureSuite)
        );
        Ok(())
    }
}
