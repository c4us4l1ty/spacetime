//! End-to-end verification for Blocks 1-2.
//!
//! This command never claims a measured resource gate. It proves the offline functional
//! pipeline and labels the 4 GB potato-machine resource budget as unmeasured.

use crate::{
    adversarial_report, cmd_crucible_adversarial, cmd_crucible_differential,
    cmd_crucible_false_pass, cmd_crucible_fixture_population, cmd_crucible_population,
    cmd_crucible_verify, cmd_machine_gate, crucible_inputs, fixture_population_report,
    panel_resolution_for_task, run_task, state_path,
};
use spacetime_bench::{
    baseline_profile, baseline_toml, profile_by_id, resource_gate, resource_gate_intake,
    target_profiles, ProfileRole, ResourceGateIntake, ResourceGateStatus, BASELINE_TARGET_PROFILE,
};
use spacetime_exec::{RunStatus, Runner, ScriptedRunner, FIXED_DETERMINISM_PROFILE};
use spacetime_ledger::Ledger;
use spacetime_types::{
    ActionOp, CapabilityScope, Event, GoalStatus, Result, SpacetimeError, Taint, Verdict,
    VerifierSource, SCHEMA_VERSION, VERIFIER_SOURCE_PRIORITY,
};
use spacetime_verifier::{
    differential_score, false_pass_leak_rate, false_pass_report_checked, mutation_score_bps,
    population_report_checked, validate_differential_limit, verdict_flip_rate,
    verifier_source_priority, Candidate, CrucibleVerifier, UnstableVerifier,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static VERIFY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Gate {
    name: &'static str,
    ok: bool,
    detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VerifyArgs {
    out: String,
    measurement: Option<String>,
}

impl Gate {
    fn pass(name: &'static str, detail: String) -> Self {
        Self {
            name,
            ok: true,
            detail,
        }
    }

    fn fail(name: &'static str, detail: String) -> Self {
        Self {
            name,
            ok: false,
            detail,
        }
    }
}

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(label: &str) -> Result<Self> {
        let id = VERIFY_COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("{label}-{pid}-{id}"));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn cmd_verify_all(args: &[String]) -> Result<()> {
    let parsed = parse_verify_args(args)?;
    let measurement_path = parsed.measurement.as_deref().map(Path::new);
    let intake = resource_gate_intake(measurement_path);
    let root = TempRoot::new("spacetime-verify")?;
    let mut gates = Vec::new();

    gates.push(capture("block1_happy", || check_happy(&root)));
    gates.push(capture("block1_false_claim", || check_false_claim(&root)));
    gates.push(capture("block1_timeout", || check_timeout(&root)));
    gates.push(capture("block1_tamper", || check_tamper(&root)));
    gates.push(capture("block1_truncation", || check_truncation(&root)));
    gates.push(capture("block1_non_canonical", || {
        check_non_canonical(&root)
    }));
    gates.push(capture("block1_incomplete_goal", || {
        check_incomplete(&root)
    }));
    gates.push(capture("crucible_deterministic", || {
        check_crucible_deterministic(&root)
    }));
    gates.push(capture("crucible_flaky_family", || {
        check_flaky_family(&root)
    }));
    gates.push(capture("crucible_false_pass", || check_false_pass(&root)));
    gates.push(capture("crucible_unstable_operator", || {
        check_unstable_operator()
    }));
    gates.push(capture("crucible_trivial_mutation_gate", || {
        check_trivial_mutation_gate(&root)
    }));
    gates.push(capture("crucible_differential_mutation", || {
        check_differential_mutation()
    }));
    gates.push(capture("verifier_source_priority", || {
        check_verifier_source_priority()
    }));
    gates.push(capture("determinism_profile", || {
        check_determinism_profile(&root)
    }));
    gates.push(capture("machine_baseline", check_machine_baseline));
    gates.push(capture("determinism_controls", check_determinism_controls));
    gates.push(capture(
        "capability_enforcement",
        check_capability_enforcement,
    ));
    gates.push(capture("cli_input_validation", || {
        check_cli_input_validation(&root)
    }));
    gates.push(capture("verifier_panel", check_verifier_panel));
    gates.push(capture("adversarial_corpus", check_adversarial_corpus));
    gates.push(capture("operator_population", check_operator_population));
    gates.push(capture("fixture_population", check_fixture_population));
    gates.push(capture("measurement_intake", || {
        check_measurement_intake(&root)
    }));
    gates.push(capture("resource_gate_unmeasured", || {
        check_resource_gate(&root)
    }));
    gates.push(capture("target_profile_registry", || {
        check_target_profile_registry(&root)
    }));

    let report = render_report(&gates, &intake);
    let out_path = Path::new(parsed.out.as_str());
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(out_path, &report)?;
    print!("{report}");

    let failed = gates.iter().filter(|gate| !gate.ok).count();
    if failed > 0 {
        return Err(SpacetimeError::new(format!(
            "{failed} end-to-end verification gates failed"
        )));
    }
    Ok(())
}

fn capture<F>(name: &'static str, check: F) -> Gate
where
    F: FnOnce() -> Result<String>,
{
    match check() {
        Ok(detail) => Gate::pass(name, detail),
        Err(error) => Gate::fail(name, error.to_string()),
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bench/tasks")
        .join(name)
}

fn adversarial_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/adversarial")
}

fn ensure(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(SpacetimeError::new(message))
    }
}

fn parse_verify_args(args: &[String]) -> Result<VerifyArgs> {
    let mut out = None;
    let mut measurement = None;
    let mut index = 0;
    while index < args.len() {
        let name = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| SpacetimeError::new(format!("missing value for {name}")))?;
        ensure(!value.starts_with('-'), format!("missing value for {name}"))?;
        ensure(
            !value.is_empty() && value.len() <= 512,
            format!("invalid value for {name}"),
        )?;
        ensure(
            !value.chars().any(|character| character.is_control()),
            format!("invalid value for {name}"),
        )?;
        match name {
            "--out" => {
                ensure(out.is_none(), "duplicate --out flag")?;
                out = Some(value.clone());
            }
            "--measurement" => {
                ensure(measurement.is_none(), "duplicate --measurement flag")?;
                measurement = Some(value.clone());
            }
            _ => return Err(SpacetimeError::new(format!("unknown verify flag: {name}"))),
        }
        index += 2;
    }
    Ok(VerifyArgs {
        out: out.ok_or_else(|| SpacetimeError::new("missing flag --out"))?,
        measurement,
    })
}

fn check_happy(root: &TempRoot) -> Result<String> {
    let ledger = root.join("happy.jsonl");
    run_task(&fixture("block1_hello.task"), &ledger)?;
    let events = Ledger::verify(&ledger)?;
    let live = fs::read_to_string(state_path(&ledger))?;
    let replayed = Ledger::replay(&ledger)?;
    ensure(
        live == replayed.to_json_string()?,
        "live state and replay state differ",
    )?;
    ensure(
        replayed.goal_status == GoalStatus::Verified,
        "happy path did not reach Verified",
    )?;
    ensure(events.len() == 2, "happy path did not record two events")?;
    let expected_profile = FIXED_DETERMINISM_PROFILE.digest();

    for event in &events {
        let proof = &event.verifier_proof;
        if proof.verdict == Verdict::Pass {
            ensure(
                !proof.evidence_digest.is_empty(),
                "PASS proof has no evidence digest",
            )?;
            ensure(
                !proof.verifier_id.is_empty(),
                "PASS proof has no verifier id",
            )?;
            ensure(
                proof.source_priority == proof.source.priority() && proof.source_priority > 0,
                "PASS proof has invalid verifier provenance",
            )?;
            ensure(
                proof.determinism_runs >= 1,
                "PASS proof has no determinism run count",
            )?;
            ensure(
                proof.execution_profile_digest.as_str() == expected_profile.as_str(),
                "PASS proof has unexpected execution profile",
            )?;
            ensure(
                event.observation.determinism_profile_digest.as_str()
                    == proof.execution_profile_digest.as_str(),
                "observation and proof execution profiles differ",
            )?;
        }
    }

    Ok(format!(
        "events={} status=verified replay=byte_identical pass_proofs=provenance_complete",
        events.len()
    ))
}

fn check_false_claim(root: &TempRoot) -> Result<String> {
    let ledger = root.join("false.jsonl");
    let result = run_task(&fixture("block1_false_claim.task"), &ledger);
    ensure(result.is_err(), "false claim unexpectedly exited zero")?;
    let events = Ledger::verify(&ledger)?;
    let state = fs::read_to_string(state_path(&ledger))?;
    let replayed = Ledger::replay(&ledger)?;
    ensure(
        replayed.goal_status == GoalStatus::Unverified,
        "false claim did not replay as Unverified",
    )?;
    ensure(
        state.contains("\"goal_status\":\"unverified\""),
        "false claim state is missing Unverified",
    )?;
    ensure(
        events.iter().any(|event| {
            matches!(&event.action.op, ActionOp::ClaimDone { .. })
                && event.verifier_proof.verdict == Verdict::Unverified
        }),
        "false claim did not produce an Unverified proof",
    )?;
    Ok("status=unverified command_exit=nonzero ledger=valid".to_string())
}

fn check_timeout(root: &TempRoot) -> Result<String> {
    let ledger = root.join("timeout.jsonl");
    let result = run_task(&fixture("block1_timeout.task"), &ledger);
    ensure(result.is_err(), "timeout unexpectedly exited zero")?;
    let events = Ledger::verify(&ledger)?;
    let state = fs::read_to_string(state_path(&ledger))?;
    let replayed = Ledger::replay(&ledger)?;
    ensure(
        replayed.goal_status == GoalStatus::Failed,
        "timeout did not replay as Failed",
    )?;
    ensure(
        state.contains("\"goal_status\":\"failed\""),
        "timeout state is missing Failed",
    )?;
    ensure(
        events
            .iter()
            .any(|event| event.verifier_proof.verdict == Verdict::Error),
        "timeout did not produce an Error proof",
    )?;
    Ok("status=failed command_exit=nonzero ledger=valid".to_string())
}

fn check_tamper(root: &TempRoot) -> Result<String> {
    let ledger = root.join("tamper.jsonl");
    run_task(&fixture("block1_hello.task"), &ledger)?;
    let mut bytes = fs::read(&ledger)?;
    ensure(bytes.len() > 10, "ledger too short for tamper test")?;
    bytes[10] ^= 1;
    fs::write(&ledger, bytes)?;
    ensure(
        Ledger::verify(&ledger).is_err(),
        "byte-flip tamper was not detected",
    )?;
    Ok("byte_flip=rejected".to_string())
}

fn check_truncation(root: &TempRoot) -> Result<String> {
    let ledger = root.join("truncate.jsonl");
    run_task(&fixture("block1_hello.task"), &ledger)?;
    let text = fs::read_to_string(&ledger)?;
    let mut lines: Vec<&str> = text.lines().collect();
    ensure(!lines.is_empty(), "ledger had no lines to truncate")?;
    lines.pop();
    fs::write(&ledger, lines.join("\n"))?;
    ensure(
        Ledger::verify(&ledger).is_err(),
        "truncation was not detected",
    )?;
    Ok("truncation=rejected".to_string())
}

fn check_non_canonical(root: &TempRoot) -> Result<String> {
    let ledger = root.join("canonical.jsonl");
    run_task(&fixture("block1_hello.task"), &ledger)?;
    let text = fs::read_to_string(&ledger)?;
    let first_line = text
        .lines()
        .next()
        .ok_or_else(|| SpacetimeError::new("ledger had no first line"))?;
    let event = Event::from_json_str(first_line)?;
    let canonical = event.to_json_string()?;
    ensure(
        first_line == canonical.as_str(),
        "stored ledger line was not canonical",
    )?;
    ensure(
        canonical.starts_with('{'),
        "canonical event did not start with an object",
    )?;
    let non_canonical = canonical.replacen('{', "{ ", 1);
    ensure(
        non_canonical != canonical,
        "failed to construct a non-canonical line",
    )?;
    let _ = Event::from_json_str(&non_canonical)?;
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    lines[0] = non_canonical;
    fs::write(&ledger, lines.join("\n") + "\n")?;
    ensure(
        Ledger::verify(&ledger).is_err(),
        "non-canonical ledger line was not detected",
    )?;
    Ok("non_canonical=rejected".to_string())
}

fn check_incomplete(root: &TempRoot) -> Result<String> {
    let task = root.join("incomplete.task");
    let ledger = root.join("incomplete.jsonl");
    fs::write(
        &task,
        "schema_version=1\naction produce_evidence evidence=x\n",
    )?;
    let result = run_task(&task, &ledger);
    ensure(result.is_err(), "incomplete goal unexpectedly exited zero")?;
    let state = fs::read_to_string(state_path(&ledger))?;
    ensure(
        state.contains("\"goal_status\":\"in_progress\""),
        "incomplete goal was not InProgress",
    )?;
    let events = Ledger::verify(&ledger)?;
    ensure(!events.is_empty(), "incomplete goal wrote no events")?;
    Ok("status=in_progress command_exit=nonzero ledger=valid".to_string())
}

fn check_crucible_deterministic(_root: &TempRoot) -> Result<String> {
    let path = fixture("block1_hello.task");
    let (candidate, config) = crucible_inputs(&path)?;
    let runner = ScriptedRunner::new()?;
    let proof = CrucibleVerifier.verify_candidate(&candidate, &config, &runner);
    let expected_profile = FIXED_DETERMINISM_PROFILE.digest();
    ensure(
        proof.verdict == Verdict::Pass,
        "deterministic fixture failed",
    )?;
    ensure(
        config.expected_verdict == Verdict::Pass,
        "deterministic fixture has wrong expected verdict",
    )?;
    ensure(
        config.determinism_runs >= 2 && proof.determinism_runs == config.determinism_runs,
        "determinism run count is invalid",
    )?;
    ensure(
        proof.source_priority == config.source.priority(),
        "Crucible proof source priority is invalid",
    )?;
    ensure(
        proof.mutation_score_bps >= config.mutation_threshold_bps,
        "Crucible proof mutation score is below threshold",
    )?;
    ensure(
        !proof.evidence_digest.is_empty(),
        "Crucible PASS has no evidence digest",
    )?;
    ensure(
        proof.execution_profile_digest.as_str() == expected_profile.as_str(),
        "Crucible PASS has unexpected execution profile",
    )?;
    Ok(format!(
        "verdict=pass determinism_runs={} mutation_score_bps={}",
        proof.determinism_runs, proof.mutation_score_bps
    ))
}

fn check_flaky_family(_root: &TempRoot) -> Result<String> {
    let fixtures = [
        "block2_flaky.task",
        "block2_flaky_clock.task",
        "block2_flaky_rng.task",
        "block2_flaky_scheduler.task",
    ];
    for name in fixtures {
        let path = fixture(name);
        let (candidate, config) = crucible_inputs(&path)?;
        ensure(
            config.expected_verdict == Verdict::Error,
            format!("{name} has wrong expected verdict"),
        )?;
        let runner = ScriptedRunner::new()?;
        let proof = CrucibleVerifier.verify_candidate(&candidate, &config, &runner);
        ensure(
            proof.verdict == Verdict::Error,
            format!("{name} was promoted instead of classified ERROR"),
        )?;
        let (flip_rate, leak_rate) = false_pass_report_checked(&candidate, &config, &runner, 50)?;
        ensure(
            flip_rate <= 0.01 && leak_rate <= 0.01,
            format!("{name} leaked false-PASS verdicts"),
        )?;
    }
    Ok("family=4 verdict=error flip_rate=0 false_pass_leaks=0".to_string())
}

fn check_false_pass(_root: &TempRoot) -> Result<String> {
    let path = fixture("block1_hello.task");
    let (candidate, config) = crucible_inputs(&path)?;
    let runner = ScriptedRunner::new()?;
    let (flip_rate, leak_rate) = false_pass_report_checked(&candidate, &config, &runner, 50)?;
    ensure(
        flip_rate <= 0.01 && leak_rate <= 0.01,
        "deterministic candidate exceeded the 1% false-PASS gate",
    )?;
    Ok(format!(
        "runs=50 flip_rate={flip_rate:.4} false_pass_leaks={leak_rate:.4}"
    ))
}

fn check_unstable_operator() -> Result<String> {
    let unstable = UnstableVerifier::default();
    let verdicts: Vec<Verdict> = (0..50).map(|_| unstable.next_verdict()).collect();
    let flip_rate = verdict_flip_rate(&verdicts);
    let leak_rate = false_pass_leak_rate(&verdicts, Verdict::Pass);
    ensure(
        flip_rate > 0.01 && leak_rate > 0.01,
        "unstable operator was not detected",
    )?;
    Ok(format!(
        "runs=50 flip_rate={flip_rate:.4} false_pass_leaks={leak_rate:.4}"
    ))
}

fn check_trivial_mutation_gate(_root: &TempRoot) -> Result<String> {
    let path = fixture("block2_trivial.task");
    let (candidate, config) = crucible_inputs(&path)?;
    ensure(
        config.expected_verdict == Verdict::Fail,
        "trivial fixture has wrong expected verdict",
    )?;
    let trivial = Candidate::with_mutation_score("trivial-candidate", candidate.action.clone(), 0);
    let runner = ScriptedRunner::new()?;
    let proof = CrucibleVerifier.verify_candidate(&trivial, &config, &runner);
    ensure(
        proof.verdict == Verdict::Fail,
        "trivial adversarial candidate was promoted",
    )?;
    Ok("verdict=fail mutation_score_bps=0 promotion=blocked".to_string())
}

fn check_differential_mutation() -> Result<String> {
    let limit = 1_000_000;
    validate_differential_limit(limit)?;
    let (matched, inputs) = differential_score(limit);
    let score = mutation_score_bps(limit);
    ensure(
        matched == inputs && score >= 8_000,
        "differential equality or mutation gate failed",
    )?;
    Ok(format!(
        "inputs={inputs} matched={matched} mutation_score_bps={score}"
    ))
}

fn check_verifier_source_priority() -> Result<String> {
    let sources = verifier_source_priority();
    ensure(
        sources.len() == VERIFIER_SOURCE_PRIORITY.len(),
        "verifier source priority list has wrong length",
    )?;
    for (index, source) in sources.iter().enumerate() {
        ensure(
            *source == VERIFIER_SOURCE_PRIORITY[index],
            "verifier source priority order changed",
        )?;
        ensure(
            source.priority() == (index + 1) as u8,
            "verifier source priority value changed",
        )?;
    }
    Ok("sources=5 priority=fixture_suite_to_mutation".to_string())
}

fn check_determinism_profile(_root: &TempRoot) -> Result<String> {
    let profile = FIXED_DETERMINISM_PROFILE;
    ensure(
        profile.clock == "fixed_clock_gettime_shim",
        "determinism profile clock is not shim-fixed",
    )?;
    ensure(
        profile.rng_seed == 0,
        "determinism profile RNG seed changed",
    )?;
    ensure(profile.locale == "C", "determinism profile locale changed")?;
    ensure(
        profile.environment == "cleared",
        "determinism profile environment is not cleared",
    )?;
    ensure(
        profile.network == "ld_preload_socket_denied",
        "determinism profile network mode changed",
    )?;
    ensure(
        profile.shim == "ld_preload_not_complete_boundary",
        "determinism profile overstates the LD_PRELOAD shim",
    )?;
    ensure(
        profile.capability == "local_fixture_allow_list",
        "determinism profile capability mode changed",
    )?;
    let path = fixture("block1_hello.task");
    let (candidate, _) = crucible_inputs(&path)?;
    let runner = ScriptedRunner::new()?;
    let result = spacetime_exec::Runner::run(&runner, &candidate.action);
    let expected = profile.digest();
    ensure(
        result.observation.determinism_profile_digest.as_str() == expected.as_str(),
        "observation did not record the fixed determinism profile",
    )?;
    Ok(format!(
        "profile_digest={} network=ld_preload_socket_denied shim=not_complete_boundary",
        expected
    ))
}

fn check_machine_baseline() -> Result<String> {
    let toml = baseline_toml()?;
    ensure(
        toml.contains("baseline_only_performance_claims_unmeasured"),
        "machine baseline does not mark performance claims unmeasured",
    )?;
    ensure(
        toml.contains("resource_law = \"holy_grail\""),
        "machine baseline is missing the First Law",
    )?;
    ensure(
        !toml.contains("boot_ms"),
        "machine baseline made an unapproved boot-time claim",
    )?;
    Ok("resource_gate=unmeasured_target_4gb host_facts_only".to_string())
}

fn check_determinism_controls() -> Result<String> {
    let runner = ScriptedRunner::new()?;
    let probe = runner.probe_controls()?;
    let checks = [
        probe.clock_fixed,
        probe.rng_seed_fixed,
        probe.locale_c,
        probe.temp_root_runner_owned,
        probe.environment_cleared,
        probe.network_none_by_construction,
        probe.stdin_null,
        probe.shim_clock_fixed,
        probe.shim_rng_fixed,
        probe.shim_socket_denied,
        probe.shim_connect_denied,
        probe.shim_locale_c,
    ];
    ensure(
        !checks.contains(&false),
        "a deterministic control probe flag was false",
    )?;
    let expected = FIXED_DETERMINISM_PROFILE.digest();
    ensure(
        probe.profile_digest.as_str() == expected.as_str(),
        "control probe recorded the wrong profile digest",
    )?;
    Ok(
        "clock=fixed rng=fixed locale=C env=cleared stdin=null shim=clock/rng/socket/connect network=ld_preload_denied"
            .to_string(),
    )
}

fn check_capability_enforcement() -> Result<String> {
    let path = fixture("block1_hello.task");
    let (candidate, _) = crucible_inputs(&path)?;
    let runner = ScriptedRunner::new()?;
    let valid = runner.run(&candidate.action);
    ensure(
        valid.status == RunStatus::Completed,
        "valid local fixture action was denied",
    )?;

    let mut no_scope = candidate.action.clone();
    no_scope.capability.scope = CapabilityScope::None;
    let mut no_budget = candidate.action.clone();
    no_budget.capability.limit_millis = 0;
    let mut over_budget = candidate.action.clone();
    over_budget.capability.limit_millis = candidate.action.budget_millis + 1;
    let mut bad_digest = candidate.action.clone();
    bad_digest.args_digest = "tampered".to_string();
    let mut untrusted = candidate.action.clone();
    untrusted.taint = Taint::UntrustedWeb;
    let mut no_verifier = candidate.action.clone();
    no_verifier.expected_verifier = None;
    let mut bad_schema = candidate.action.clone();
    bad_schema.schema_version = SCHEMA_VERSION + 1;

    let cases: Vec<(&'static str, spacetime_types::Action)> = vec![
        ("none_scope", no_scope),
        ("no_budget", no_budget),
        ("over_budget", over_budget),
        ("bad_args_digest", bad_digest),
        ("untrusted_taint", untrusted),
        ("no_verifier", no_verifier),
        ("bad_schema", bad_schema),
    ];
    for (label, action) in cases {
        let result = runner.run(&action);
        match result.status {
            RunStatus::Error(reason) => ensure(
                reason.contains("capability denied"),
                format!("{label} returned an unrelated error"),
            )?,
            RunStatus::Completed => {
                return Err(SpacetimeError::new(format!("{label} was executed")));
            }
        }
    }
    Ok(format!("denied_cases={} valid_action=completed", 7))
}

fn check_fixture_population() -> Result<String> {
    let report = fixture_population_report(50)?;
    ensure(
        report.fixtures >= 20,
        "fixture population is smaller than the 20-task minimum",
    )?;
    ensure(
        report.skipped_without_oracle == 2,
        "unexpected number of fixtures without a declared oracle",
    )?;
    ensure(
        report.runs_per_fixture == 50 && report.verifier_runs == 1_300,
        "fixture population rerun dimensions changed",
    )?;
    ensure(
        report.planned_runner_executions > 0,
        "fixture population planned no runner executions",
    )?;
    ensure(
        report.mismatches == 0,
        "fixture population disagreed with its declared oracle",
    )?;
    ensure(
        report.max_flip_rate <= 0.01 && report.max_leak_rate <= 0.01,
        "fixture population exceeded the 1% false-PASS kill gate",
    )?;
    ensure(
        report.real_fixture_population && !report.real_operator_study,
        "fixture population was mislabeled as a real operator study",
    )?;
    Ok(format!(
        "fixtures={} verifier_runs={} max_flip_rate={:.4} max_leak_rate={:.4} real_operator_study=false",
        report.fixtures, report.verifier_runs, report.max_flip_rate, report.max_leak_rate
    ))
}

fn check_verifier_panel() -> Result<String> {
    let path = fixture("block1_hello.task");
    let resolution = panel_resolution_for_task(&path)?;
    ensure(
        resolution.verdict == Verdict::Pass,
        "verifier panel did not reach PASS",
    )?;
    ensure(
        resolution.sources.len() == VERIFIER_SOURCE_PRIORITY.len(),
        "verifier panel is missing sources",
    )?;
    for (index, source) in resolution.sources.iter().enumerate() {
        ensure(
            *source == VERIFIER_SOURCE_PRIORITY[index],
            "verifier panel sources are not priority ordered",
        )?;
    }
    ensure(
        resolution.selected_source == Some(VerifierSource::FixtureSuite),
        "verifier panel did not select the highest-priority source",
    )?;
    ensure(
        resolution.selected_priority == 1 && resolution.conflicts == 0,
        "verifier panel recorded a conflict or wrong priority",
    )?;
    ensure(
        !resolution.evidence_digest.is_empty(),
        "verifier panel PASS has no evidence digest",
    )?;
    Ok(format!(
        "sources={} verdict=pass conflicts={} selected_priority={}",
        resolution.sources.len(),
        resolution.conflicts,
        resolution.selected_priority
    ))
}

fn check_adversarial_corpus() -> Result<String> {
    let report = adversarial_report(&adversarial_dir())?;
    ensure(report.fixtures >= 20, "adversarial corpus is too small")?;
    ensure(
        report.mismatches == 0,
        "an adversarial fixture had an unexpected verdict",
    )?;
    ensure(
        report.expected_pass > 0 && report.expected_fail > 0 && report.expected_error > 0,
        "adversarial corpus lacks PASS, FAIL, and ERROR coverage",
    )?;
    ensure(
        report.expected_unverified == 0,
        "adversarial corpus has an UNVERIFIED expectation",
    )?;
    Ok(format!(
        "fixtures={} mismatches={} pass={} fail={} error={}",
        report.fixtures,
        report.mismatches,
        report.expected_pass,
        report.expected_fail,
        report.expected_error
    ))
}

fn check_operator_population() -> Result<String> {
    let report = population_report_checked(10, 50)?;
    ensure(
        report.operators == 10 && report.runs_per_operator == 50,
        "operator population dimensions changed",
    )?;
    ensure(
        report.total_runs == 500,
        "operator population did not run 500 verdicts",
    )?;
    ensure(
        report.unstable > 0 && report.accepted > 0,
        "population did not exercise stable and unstable operators",
    )?;
    ensure(
        report.promoted_false_pass_leaks == 0,
        "a promoted synthetic operator had a false-PASS leak",
    )?;
    ensure(
        report.synthetic && !report.real_operator_study,
        "population study was mislabeled as a real operator study",
    )?;
    Ok(format!(
        "operators={} runs={} unstable={} promoted_false_pass_leaks=0 real_operator_study=false",
        report.operators, report.runs_per_operator, report.unstable
    ))
}

fn ensure_rejected(result: Result<()>, label: &str) -> Result<()> {
    ensure(result.is_err(), format!("{label} unexpectedly succeeded"))
}

fn check_cli_input_validation(root: &TempRoot) -> Result<String> {
    let hello = fixture("block1_hello.task").display().to_string();
    ensure_rejected(
        cmd_crucible_false_pass(&[
            "--task".to_string(),
            hello.clone(),
            "--runs".to_string(),
            "0".to_string(),
        ]),
        "zero-rerun false-PASS",
    )?;
    ensure_rejected(
        cmd_crucible_differential(&["--limit".to_string(), "1".to_string()]),
        "tiny differential limit",
    )?;
    ensure_rejected(
        cmd_crucible_population(&[
            "--operators".to_string(),
            "10".to_string(),
            "--runs".to_string(),
            "0".to_string(),
        ]),
        "zero-rerun population",
    )?;
    ensure_rejected(
        cmd_crucible_fixture_population(&["--runs".to_string(), "0".to_string()]),
        "zero-rerun fixture population",
    )?;
    ensure_rejected(
        cmd_crucible_adversarial(&["--dir".to_string()]),
        "missing adversarial dir value",
    )?;
    ensure_rejected(
        cmd_machine_gate(&["--measurement".to_string()]),
        "missing measurement value",
    )?;
    ensure_rejected(
        parse_verify_args(&[
            "--out".to_string(),
            "a".to_string(),
            "--out".to_string(),
            "b".to_string(),
        ])
        .map(|_| ()),
        "duplicate verify --out",
    )?;
    ensure_rejected(
        parse_verify_args(&[
            "--out".to_string(),
            "a".to_string(),
            "--measurement".to_string(),
            "m1".to_string(),
            "--measurement".to_string(),
            "m2".to_string(),
        ])
        .map(|_| ()),
        "duplicate verify --measurement",
    )?;
    ensure_rejected(
        parse_verify_args(&[
            "--out".to_string(),
            "a".to_string(),
            "--bogus".to_string(),
            "b".to_string(),
        ])
        .map(|_| ()),
        "unknown verify flag",
    )?;

    let mismatch = root.join("expected-verdict-mismatch.task");
    fs::write(
        &mismatch,
        "schema_version=1\nverifier_source=fixture_suite\nexpected_verdict=fail\nexpected_digest=sha256:stable\ndeterminism_runs=2\nmutation_threshold_bps=8000\ncandidate_mutation_score_bps=10000\nspace_id=space-mismatch\ngoal_id=goal-mismatch\nsurface_kind=terminal\npreview_mode=headless\nliveness=running\naction echo text=stable\n",
    )?;
    ensure_rejected(
        cmd_crucible_verify(&["--task".to_string(), mismatch.display().to_string()]),
        "expected-verdict mismatch",
    )?;
    ensure(
        cmd_crucible_verify(&["--task".to_string(), hello]).is_ok(),
        "valid crucible verify was rejected",
    )?;
    Ok("rejected_cases=10 valid_verify=accepted".to_string())
}

fn check_measurement_intake(root: &TempRoot) -> Result<String> {
    let none = resource_gate_intake(None);
    ensure(
        none.status == ResourceGateStatus::Unmeasured && !none.file_supplied,
        "missing measurement file was not recorded as unmeasured",
    )?;
    let none_report = render_report(&[], &none);
    ensure(
        none_report.contains("measurement_intake = \"none\"")
            && none_report.contains("resource_gate = \"unmeasured_target_4gb\"")
            && none_report.contains("performance_claims = \"unmeasured\""),
        "default report lost its unmeasured measurement fields",
    )?;

    let cases: [(&str, &str, bool); 8] = [
        ("missing-file.measurement", "", false),
        ("malformed.measurement", "not-key-value\n", true),
        (
            "bad-number.measurement",
            "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=potato-target-lab\npeak_rss_kb=oops\n",
            true,
        ),
        (
            "duplicate.measurement",
            "schema_version=1\nschema_version=1\n",
            true,
        ),
        (
            "ci-host.measurement",
            "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=github_actions\npeak_rss_kb=3000000\nboot_ms=900\ntab_switch_ms=900\n",
            true,
        ),
        (
            "intake-synthetic-host.measurement",
            "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=synthetic-target-lab\npeak_rss_kb=3000000\nboot_ms=900\ntab_switch_ms=900\n",
            true,
        ),
        (
            "incomplete.measurement",
            "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=potato-target-lab\npeak_rss_kb=3000000\n",
            true,
        ),
        (
            "wrong-profile.measurement",
            "schema_version=1\ntarget_profile=github_actions\nmeasurement_host=potato-target-lab\npeak_rss_kb=3000000\nboot_ms=900\ntab_switch_ms=900\n",
            true,
        ),
    ];
    for (name, content, write) in cases {
        let path = root.join(name);
        if write {
            fs::write(&path, content)?;
        }
        let intake = resource_gate_intake(Some(path.as_path()));
        ensure(
            intake.file_supplied && intake.status == ResourceGateStatus::Unmeasured,
            format!("{name} did not stay unmeasured"),
        )?;
        ensure(
            !intake.reasons.is_empty(),
            format!("{name} lost its measurement reasons"),
        )?;
        let report = render_report(&[], &intake);
        ensure(
            report.contains("resource_gate = \"unmeasured_target_4gb\"")
                && report.contains("performance_claims = \"unmeasured\""),
            format!("{name} produced a measured report claim"),
        )?;
    }

    let missing = root.join("missing-file.measurement");
    let missing_intake = resource_gate_intake(Some(missing.as_path()));
    ensure(
        missing_intake.digest == "unavailable"
            && missing_intake
                .reasons
                .iter()
                .any(|reason| reason.contains("unreadable")),
        "unreadable measurement file lost its unavailable digest or reason",
    )?;
    let missing_report = render_report(&[], &missing_intake);
    ensure(
        missing_report.contains("measurement_digest = \"unavailable\"")
            && missing_report.contains("measurement_evaluator = \"unmeasured\""),
        "unreadable measurement file produced a measured digest or evaluator",
    )?;

    let oversize = root.join("oversize.measurement");
    fs::write(&oversize, "a".repeat(70_000))?;
    let big = resource_gate_intake(Some(oversize.as_path()));
    ensure(
        big.status == ResourceGateStatus::Unmeasured
            && big
                .reasons
                .iter()
                .any(|reason| reason.contains("64 KiB bound")),
        "oversize measurement file was not bounded",
    )?;

    let shape = root.join("shape-only.measurement");
    fs::write(
        &shape,
        "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=potato-target-lab\npeak_rss_kb=3000000\nboot_ms=900\ntab_switch_ms=900\ncpu_percent=80\n",
    )?;
    let shape_intake = resource_gate_intake(Some(shape.as_path()));
    ensure(
        shape_intake.status == ResourceGateStatus::Pass,
        "within-budget shape evaluator self-test did not pass",
    )?;
    let shape_report = render_report(&[], &shape_intake);
    ensure(
        shape_report.contains("measurement_evaluator = \"pass_shape_only\"")
            && shape_report.contains("resource_gate = \"unmeasured_target_4gb\"")
            && shape_report.contains("performance_claims = \"unmeasured\"")
            && shape_report.contains("target_profile = \"4gb_potato\"")
            && shape_report.contains("baseline_target_profile = \"4gb_potato\"")
            && shape_report.contains("target_profile_registry = 3")
            && !shape_report.contains("resource_gate = \"measured"),
        "shape pass was promoted to a measured resource gate",
    )?;
    Ok("bounded=none/unreadable/malformed/duplicate/ci/synthetic/incomplete/wrong-profile/oversize shape_pass_only".to_string())
}

fn check_resource_gate(root: &TempRoot) -> Result<String> {
    let unmeasured = resource_gate(None)?;
    ensure(
        unmeasured.status == ResourceGateStatus::Unmeasured,
        "default resource gate is not unmeasured",
    )?;
    ensure(
        unmeasured
            .reasons
            .iter()
            .any(|reason| reason.contains("no measurement")),
        "default resource gate lost its unmeasured reason",
    )?;

    let over = root.join("over-budget.measurement");
    fs::write(
        &over,
        "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=potato-target-over-budget\npeak_rss_kb=99999999\nboot_ms=900\ntab_switch_ms=900\n",
    )?;
    let over_gate = resource_gate(Some(over.as_path()))?;
    ensure(
        over_gate.status == ResourceGateStatus::Fail,
        "over-budget synthetic measurement did not fail the gate",
    )?;

    let within = root.join("within-budget.measurement");
    fs::write(
        &within,
        "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=potato-target-within-budget\npeak_rss_kb=3000000\nboot_ms=900\ntab_switch_ms=900\ncpu_percent=80\n",
    )?;
    let within_gate = resource_gate(Some(within.as_path()))?;
    ensure(
        within_gate.status == ResourceGateStatus::Pass,
        "within-budget target-lab measurement did not pass the evaluator self-test",
    )?;

    let synthetic = root.join("synthetic-host.measurement");
    fs::write(
        &synthetic,
        "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=synthetic-target-lab\npeak_rss_kb=3000000\nboot_ms=900\ntab_switch_ms=900\ncpu_percent=80\n",
    )?;
    let synthetic_gate = resource_gate(Some(synthetic.as_path()))?;
    ensure(
        synthetic_gate.status == ResourceGateStatus::Unmeasured,
        "synthetic host label was treated as a real target measurement",
    )?;
    ensure(
        synthetic_gate
            .reasons
            .iter()
            .any(|reason| reason.contains("non-target host")),
        "synthetic host rejection lost its reason",
    )?;
    Ok("evaluator=unmeasured/fail/pass/non_target_host real_target_4gb=unmeasured".to_string())
}

fn check_target_profile_registry(root: &TempRoot) -> Result<String> {
    let profiles = target_profiles();
    ensure(profiles.len() == 3, "target profile registry lost a profile")?;
    let baseline = baseline_profile();
    ensure(
        baseline.id == BASELINE_TARGET_PROFILE && baseline.role == ProfileRole::BaselineGate,
        "release baseline profile is not the 4 GB potato gate",
    )?;
    ensure(
        profiles
            .iter()
            .filter(|profile| profile.role == ProfileRole::BaselineGate)
            .count()
            == 1,
        "target profile registry must have exactly one baseline gate",
    )?;
    ensure(
        profiles
            .iter()
            .all(|profile| profile.memory_budget_kb >= baseline.memory_budget_kb),
        "an optional profile lowered the release baseline",
    )?;
    let mut ids: Vec<&str> = profiles.iter().map(|profile| profile.id).collect();
    ids.sort_unstable();
    ids.dedup();
    ensure(
        ids.len() == profiles.len(),
        "target profile registry contains duplicate ids",
    )?;
    let public = profile_by_id("8gb_reference")
        .ok_or_else(|| SpacetimeError::new("8 GB public profile is not registered"))?;
    let path = root.join("public-profile.measurement");
    fs::write(
        &path,
        "schema_version=1\ntarget_profile=8gb_reference\nmeasurement_host=public-target-lab\npeak_rss_kb=6000000\nboot_ms=900\ntab_switch_ms=900\ncpu_percent=80\n",
    )?;
    let gate = resource_gate(Some(path.as_path()))?;
    ensure(
        gate.status == ResourceGateStatus::Pass
            && gate.target_profile == public.id
            && gate.baseline_profile == BASELINE_TARGET_PROFILE
            && gate.profile_role == ProfileRole::OptionalProfile
            && gate.memory_budget_kb == public.memory_budget_kb,
        "8 GB profile shape did not use its own budget while preserving the baseline",
    )?;
    let intake = resource_gate_intake(Some(path.as_path()));
    let report = render_report(&[], &intake);
    ensure(
        report.contains("target_profile = \"8gb_reference\"")
            && report.contains("baseline_target_profile = \"4gb_potato\"")
            && report.contains("resource_gate = \"unmeasured_target_4gb\"")
            && report.contains("profile_role = \"optional_profile\"")
            && !report.contains("resource_gate = \"measured"),
        "8 GB shape pass promoted the release gate or hid the baseline",
    )?;
    Ok(format!(
        "profiles={} baseline={} optional=8gb_reference,16gb_developer release_gate=unmeasured",
        profiles.len(),
        baseline.id
    ))
}

fn render_report(gates: &[Gate], intake: &ResourceGateIntake) -> String {
    let failed = gates.iter().filter(|gate| !gate.ok).count();
    let status = if failed == 0 { "pass" } else { "fail" };
    let mut out = String::new();
    out.push_str("schema_version = 1\n");
    out.push_str("report = \"end_to_end_verification\"\n");
    out.push_str(&format!("status = \"{status}\"\n"));
    out.push_str(&format!("gates = {}\n", gates.len()));
    out.push_str(&format!("failed = {failed}\n"));
    out.push_str("resource_gate = \"unmeasured_target_4gb\"\n");
    out.push_str("performance_claims = \"unmeasured\"\n");
    let intake_label = if intake.file_supplied {
        "evaluated"
    } else {
        "none"
    };
    let evaluator_label = if !intake.file_supplied {
        "none"
    } else if intake.status == ResourceGateStatus::Pass {
        "pass_shape_only"
    } else {
        "unmeasured"
    };
    out.push_str(&format!("measurement_intake = \"{intake_label}\"\n"));
    out.push_str(&format!("measurement_evaluator = \"{evaluator_label}\"\n"));
    out.push_str(&format!("measurement_digest = {}\n", quote(&intake.digest)));
    let reasons: Vec<String> = intake
        .reasons
        .iter()
        .take(16)
        .map(|reason| quote(reason))
        .collect();
    out.push_str(&format!("measurement_reasons = [{}]\n", reasons.join(", ")));
    out.push_str(&format!(
        "target_profile = {}\n",
        quote(&intake.target_profile)
    ));
    out.push_str(&format!(
        "baseline_target_profile = {}\n",
        quote(&intake.baseline_profile)
    ));
    out.push_str(&format!(
        "target_profile_registry = {}\n",
        target_profiles().len()
    ));
    out.push_str(&format!(
        "evaluated_memory_budget_kb = {}\n",
        intake.memory_budget_kb
    ));
    out.push_str(&format!(
        "profile_role = {}\n",
        quote(intake.profile_role.as_str())
    ));
    out.push_str("resource_law = \"holy_grail\"\n");
    out.push_str("ci_host = \"github_actions_not_target\"\n");
    out.push_str(&format!(
        "panel_sources = {}\n",
        VERIFIER_SOURCE_PRIORITY.len()
    ));
    out.push_str("adversarial_fixtures = 20\n");
    out.push_str("determinism_shim = \"ld_preload_not_complete_boundary\"\n");
    out.push_str("capability_allow_list = \"local_fixture_only\"\n");
    out.push_str("fixture_population_fixtures = 26\n");
    out.push_str("fixture_population_runs = 50\n");
    out.push_str("fixture_population_verifier_runs = 1300\n");
    out.push_str("synthetic_operator_population = true\n");
    out.push_str("real_operator_study = \"unmeasured\"\n");
    out.push_str("resource_gate_evaluator = \"unmeasured_fail_pass_self_test\"\n");
    out.push_str("cli_input_validation = \"bounded\"\n");
    out.push_str("resource_gate_host_validation = true\n");
    for gate in gates {
        let gate_status = if gate.ok { "pass" } else { "fail" };
        out.push_str("[[gate]]\n");
        out.push_str(&format!("name = {}\n", quote(gate.name)));
        out.push_str(&format!("status = \"{gate_status}\"\n"));
        out.push_str(&format!("detail = {}\n", quote(&gate.detail)));
    }
    out
}

fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control.is_control() => {
                out.push_str(&format!("\\u{:04X}", u32::from(control)));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}
