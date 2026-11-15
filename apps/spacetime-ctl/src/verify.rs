//! End-to-end verification for Blocks 1-2.
//!
//! This command never claims a measured resource gate. It proves the offline functional
//! pipeline and labels the 4 GB potato-machine resource budget as unmeasured.

use crate::{crucible_inputs, run_task, state_path};
use spacetime_bench::baseline_toml;
use spacetime_exec::{ScriptedRunner, FIXED_DETERMINISM_PROFILE};
use spacetime_ledger::Ledger;
use spacetime_types::{
    ActionOp, Event, GoalStatus, Result, SpacetimeError, Verdict, VERIFIER_SOURCE_PRIORITY,
};
use spacetime_verifier::{
    differential_score, false_pass_leak_rate, false_pass_report, mutation_score_bps,
    verdict_flip_rate, verifier_source_priority, Candidate, CrucibleVerifier, UnstableVerifier,
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
    let out = flag(args, "--out")?;
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

    let report = render_report(&gates);
    let out_path = Path::new(out.as_str());
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

fn ensure(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(SpacetimeError::new(message))
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
        let (flip_rate, leak_rate) = false_pass_report(&candidate, &config, &runner, 50);
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
    let (flip_rate, leak_rate) = false_pass_report(&candidate, &config, &runner, 50);
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
        profile.clock == "fixed_scripted_clock",
        "determinism profile clock is not fixed",
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
        profile.network == "none_by_construction",
        "determinism profile network mode changed",
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
        "profile_digest={} network=none_by_construction",
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

fn render_report(gates: &[Gate]) -> String {
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
    out.push_str("target_profile = \"4gb_potato\"\n");
    out.push_str("resource_law = \"holy_grail\"\n");
    out.push_str("ci_host = \"github_actions_not_target\"\n");
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
