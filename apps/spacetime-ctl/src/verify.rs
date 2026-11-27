//! End-to-end verification for Blocks 1-2.
//!
//! This command never claims a measured resource gate. It proves the offline functional
//! pipeline and labels the 4 GB potato-machine resource budget as unmeasured.

use crate::airlock::{airlock_copy_file, ClipboardAirlock};
use crate::cas::{branch_space, export_space_bundle, import_space_bundle, WavefunctionCas};
use crate::console::{cmd_console, console_report, default_ledger_path};
use crate::distill::{DistillationStore, OperatorSpec};
use crate::econbench::econbench_report;
use crate::memory::SpaceSupervisor;
use crate::security::{run_redteam_suite, HostVault};
use crate::{
    adversarial_report, cmd_crucible_adversarial, cmd_crucible_differential,
    cmd_crucible_false_pass, cmd_crucible_fixture_population, cmd_crucible_population,
    cmd_crucible_verify, cmd_machine_gate, crucible_inputs, fixture_population_report,
    panel_resolution_for_task, run_task, state_path,
};
use serde_json::Value;
use spacetime_bench::econbench::{verify_task_case_with_candidate, CandidateVariant};
use spacetime_bench::economy::{MockEconomy, TransferOutcome};
use spacetime_bench::{
    baseline_profile, baseline_toml, evaluate_provenance_gate, profile_by_id, resource_gate,
    resource_gate_intake, target_profiles, ProfileRole, ProvenanceGateStatus, ResourceGateIntake,
    ResourceGateStatus, BASELINE_TARGET_PROFILE,
};
use spacetime_cortex::adapters::CortexPromptEnvelope;
use spacetime_cortex::population::{
    evaluate_abc_harness, CandidateEvaluation, CandidateProposal, HarnessTaskCase,
    PopulationCycler, PopulationSelection,
};
use spacetime_exec::ark::{probe_net_none_denial, select_domain, ArkBroker, ArkCellSpec, ArkDomain};
use spacetime_exec::autopsy::{close_and_tombstone, snapshot_dir_manifest};
use spacetime_exec::egress::EgressBroker;
use spacetime_exec::governor::{GovernorPolicy, GovernorSample, ResourceGovernor};
use spacetime_exec::leak_audit::run_leak_audit;
use spacetime_exec::surface::{default_mixed_workspace_descriptors, SurfaceBroker};
use spacetime_exec::wasi::{ConstrainedWasiWorker, WasiInstruction, WasiRuntime, WasiWorkerSpec};
use spacetime_exec::{RunStatus, Runner, ScriptedRunner, FIXED_DETERMINISM_PROFILE};
use spacetime_ledger::Ledger;
use spacetime_types::supervisor::{FactKind, OperatorHealth};
use spacetime_types::surface::{CasProvenanceTier, GovernorAction, SemanticField, SemanticView};
use spacetime_types::{
    sha256_hex, Action, ActionOp, CapabilityScope, EgressClass, Event, EventSeq, GoalId,
    GoalStatus, LivenessState, PreviewMode, Result, SpaceId, SpacetimeError, SurfaceId,
    SurfaceKind, Taint, TrustTier, Verdict, VerifierSource, WorkspaceBinding, WorkspaceId,
    SCHEMA_VERSION, VERIFIER_SOURCE_PRIORITY,
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
    gates.push(capture("econbench_offline", check_econbench_offline));
    gates.push(capture(
        "mock_economy_invariants",
        check_mock_economy_invariants,
    ));
    gates.push(capture("static_console_report", || {
        check_static_console_report(&root)
    }));
    gates.push(capture("econbench_planted_bugs_and_bounties", || {
        check_econbench_planted_bugs_and_bounties()
    }));
    gates.push(capture("provenance_gate_contract", || {
        check_provenance_gate_contract(&root)
    }));
    gates.push(capture("population_cycler_and_abc_harness", || {
        check_population_cycler_and_abc_harness()
    }));
    gates.push(capture("space_supervisor_bm25_memory", || {
        check_space_supervisor_bm25_memory(&root)
    }));
    gates.push(capture("ring3_policy_vault_redteam", || {
        check_ring3_policy_vault_redteam()
    }));
    gates.push(capture("tier1_wasi_tier2_ark_net_none", || {
        check_tier1_wasi_tier2_ark_net_none(&root)
    }));
    gates.push(capture("resource_governor_and_surface_broker", || {
        check_resource_governor_and_surface_broker()
    }));
    gates.push(capture("quarantine_autopsy_and_leak_audit", || {
        check_quarantine_autopsy_and_leak_audit(&root)
    }));
    gates.push(capture("wavefunction_cas_branch_export", || {
        check_wavefunction_cas_branch_export(&root)
    }));
    gates.push(capture("egress_airlock_and_distillation", || {
        check_egress_airlock_and_distillation(&root)
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
        toml.contains("host_facts_only_performance_claims_unmeasured"),
        "machine baseline does not mark performance claims unmeasured",
    )?;
    ensure(
        toml.contains("host_profile = \"unspecified\""),
        "machine baseline inferred a target profile from host facts",
    )?;
    ensure(
        toml.contains("release_baseline_profile = \"4gb_potato\""),
        "machine baseline lost the 4 GB release gate",
    )?;
    ensure(
        toml.contains("profile_count = 3"),
        "machine baseline lost the explicit profile registry",
    )?;
    ensure(
        toml.contains("resource_law = \"holy_grail\""),
        "machine baseline is missing the First Law",
    )?;
    ensure(
        !toml.contains("boot_ms"),
        "machine baseline made an unapproved boot-time claim",
    )?;
    Ok("resource_gate=unmeasured_target_4gb host_facts_only profiles=3".to_string())
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
    Ok(
        "bounded=none/unreadable/malformed/duplicate/ci/synthetic/incomplete/wrong-profile/oversize shape_pass_only"
            .to_string(),
    )
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
    ensure(
        profiles.len() == 3,
        "target profile registry lost a profile",
    )?;
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

fn json_u64(value: &Value, key: &str) -> Result<u64> {
    value
        .get(key)
        .and_then(|field| field.as_u64())
        .ok_or_else(|| SpacetimeError::new(format!("missing numeric field {key}")))
}

fn json_f64(value: &Value, key: &str) -> Result<f64> {
    value
        .get(key)
        .and_then(|field| field.as_f64())
        .ok_or_else(|| SpacetimeError::new(format!("missing floating field {key}")))
}

fn json_str<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(|field| field.as_str())
        .ok_or_else(|| SpacetimeError::new(format!("missing string field {key}")))
}

fn check_mock_economy_invariants() -> Result<String> {
    let mut economy =
        MockEconomy::with_accounts([("alice".to_string(), 100), ("bounty-board".to_string(), 10)])?;
    ensure(
        economy.transfer("payout-001", "alice", "bounty-board", 25)? == TransferOutcome::Applied,
        "first mock-economy payout was not applied",
    )?;
    ensure(
        economy.transfer("payout-001", "alice", "bounty-board", 25)? == TransferOutcome::Duplicate,
        "replayed mock-economy payout was not idempotent",
    )?;
    ensure(
        economy
            .transfer("overdraft", "alice", "bounty-board", 76)
            .is_err(),
        "mock economy accepted an overdraft",
    )?;
    ensure(
        economy.balance("alice") == Some(75)
            && economy.balance("bounty-board") == Some(35)
            && economy.events().len() == 1,
        "failed or duplicate mock-economy payout changed wallet state",
    )?;
    ensure(
        MockEconomy::verify_events(economy.events())?,
        "mock-economy transfer hash chain did not verify",
    )?;
    Ok("atomic=true overdraft_rejected=true idempotent=true hash_chain=verified".to_string())
}

fn check_econbench_offline() -> Result<String> {
    let report = econbench_report("A", "scripted-fixture", 1000)?;
    ensure(
        json_u64(&report, "fixtures")? == 20,
        "EconBench seed lost offline fixtures",
    )?;
    ensure(
        json_u64(&report, "completed")? == 20,
        "an offline EconBench fixture did not complete",
    )?;
    ensure(
        json_u64(&report, "claims_surviving_reverification")? == 20,
        "an EconBench claim did not survive independent re-verification",
    )?;
    ensure(
        json_f64(&report, "false_pass_rate")? <= 0.01,
        "EconBench false-PASS rate exceeded the gate",
    )?;
    ensure(
        json_str(&report, "network")? == "none",
        "EconBench prototype attempted network access",
    )?;
    ensure(
        json_str(&report, "model_adapter")? == "offline_scripted",
        "EconBench prototype called a real model",
    )?;
    ensure(
        json_str(&report, "resource_gate")? == "unmeasured_target_4gb",
        "EconBench prototype claimed a measured resource gate",
    )?;
    ensure(
        json_str(&report, "performance_claims")? == "unmeasured",
        "EconBench prototype claimed measured performance",
    )?;
    ensure(
        report
            .get("arm_efficacy_claim")
            .and_then(|field| field.as_bool())
            == Some(false),
        "EconBench prototype claimed arm efficacy",
    )?;
    Ok(format!(
        "fixtures=20 completed=20 claims=20 false_pass_rate={:.4} network=none",
        json_f64(&report, "false_pass_rate")?
    ))
}

fn check_static_console_report(root: &TempRoot) -> Result<String> {
    let ledger_path = default_ledger_path();
    let report = console_report(&ledger_path)?;
    ensure(
        json_str(&report, "resource_gate")? == "unmeasured_target_4gb",
        "console report claimed a measured resource gate",
    )?;
    ensure(
        json_str(&report, "performance_claims")? == "unmeasured",
        "console report claimed measured performance",
    )?;
    let ledger = report
        .get("ledger")
        .ok_or_else(|| SpacetimeError::new("console report has no ledger"))?;
    ensure(
        json_u64(ledger, "event_count")? >= 3,
        "console ledger has too few events",
    )?;
    let snapshots = report
        .get("snapshot_strip")
        .and_then(|field| field.as_array())
        .ok_or_else(|| SpacetimeError::new("console report has no snapshot strip"))?;
    ensure(
        snapshots.len() == 3,
        "console snapshot strip lost its ledger frames",
    )?;
    let workspaces = report
        .get("workspaces")
        .and_then(|field| field.as_array())
        .ok_or_else(|| SpacetimeError::new("console report has no mixed-workspace tiles"))?;
    ensure(
        workspaces.len() == 2,
        "console report lost a mixed-workspace tile",
    )?;
    let html = include_str!("../../spacetime-console/index.html");
    let css = include_str!("../../spacetime-console/styles.css");
    let js = include_str!("../../spacetime-console/console.js");
    ensure(
        html.contains("Content-Security-Policy"),
        "static console lost its content security policy",
    )?;
    ensure(
        !html.contains("http://") && !html.contains("https://"),
        "static console references a remote URL",
    )?;
    ensure(
        !js.contains("innerHTML") && !js.contains("eval("),
        "static console uses unsafe DOM or code evaluation",
    )?;
    ensure(
        js.contains("fetch(\"./report.json\")"),
        "static console does not load its offline report",
    )?;
    ensure(
        css.contains("snapshot-frame"),
        "static console lost snapshot styling",
    )?;
    let out = root.join("console-report.json");
    let out_str = out
        .to_str()
        .ok_or_else(|| SpacetimeError::new("console report path is not UTF-8"))?;
    let ledger_str = ledger_path
        .to_str()
        .ok_or_else(|| SpacetimeError::new("ledger path is not UTF-8"))?;
    cmd_console(&[
        "--ledger".to_string(),
        ledger_str.to_string(),
        "--out".to_string(),
        out_str.to_string(),
    ])?;
    let written: Value = serde_json::from_str(&fs::read_to_string(&out)?)?;
    ensure(
        json_str(&written, "resource_gate")? == "unmeasured_target_4gb",
        "exported console report claimed a measured resource gate",
    )?;
    Ok("static_html=offline ledger_verified=true snapshot_frames=3 workspaces=2".to_string())
}

fn check_econbench_planted_bugs_and_bounties() -> Result<String> {
    let cases = [
        "wallet_overdraft",
        "wallet_exact_balance",
        "wallet_idempotent_replay",
        "wallet_id_reuse",
        "wallet_overflow_atomicity",
        "wallet_invalid_requests",
        "wallet_tamper_detection",
        "transform_bit_reverse",
        "transform_byte_swap",
        "transform_rotate_left",
        "transform_rotate_right",
        "transform_saturating_add",
        "transform_checked_mul",
        "transform_parity",
        "protocol_empty_transaction",
        "protocol_two_records",
        "protocol_abort_discards",
        "protocol_out_of_order",
        "protocol_duplicate_sequence",
        "protocol_write_after_commit",
    ];
    for case_id in cases {
        let passing = verify_task_case_with_candidate(case_id, CandidateVariant::Passing)?;
        ensure(passing.checks > 0, "passing case had zero checks")?;
        ensure(
            verify_task_case_with_candidate(case_id, CandidateVariant::PlantedBug).is_err(),
            format!("planted bug escaped verification on {case_id}"),
        )?;
    }

    let bounties: Value =
        serde_json::from_str(include_str!("../../../bench/economy/bounties.json"))?;
    let list = bounties
        .get("bounties")
        .and_then(|v| v.as_array())
        .ok_or_else(|| SpacetimeError::new("bounties.json missing bounties array"))?;
    ensure(list.len() == 20, "bounties.json must list 20 bounties")?;
    let total = json_u64(&bounties, "fake_credits_total")?;
    let sum: u64 = list
        .iter()
        .filter_map(|item| item.get("reward_fake_credits").and_then(|v| v.as_u64()))
        .sum();
    ensure(
        total == sum && total == 60,
        "bounties.json fake_credits_total does not match bounty rewards",
    )?;
    Ok("passing=20 planted_bugs_caught=20 bounties=20 fake_credits_total=60".to_string())
}

fn check_provenance_gate_contract(root: &TempRoot) -> Result<String> {
    let meas_text = "schema_version=1\ntarget_profile=4gb_potato\nmeasurement_host=potato-target-lab\npeak_rss_kb=3000000\nboot_ms=900\ntab_switch_ms=900\ncpu_percent=80\n";
    let meas_path = root.join("prov-check.measurement");
    fs::write(&meas_path, meas_text)?;
    let meas_sha = sha256_hex(meas_text.as_bytes());
    let facts_sha = sha256_hex(b"host-facts-dmi");

    let unmeasured = evaluate_provenance_gate(Some(&meas_path), None);
    ensure(
        unmeasured.status == ProvenanceGateStatus::Unmeasured,
        "missing provenance promoted gate unexpectedly",
    )?;

    let bad_prov = root.join("prov-bad.kv");
    let bad_prov_text = format!(
        "schema_version=1\n\
         target_profile=4gb_potato\n\
         measurement_host=github_actions\n\
         measurement_sha256={meas_sha}\n\
         host_facts_sha256={facts_sha}\n\
         hardware_model=CI\n\
         installed_ram_mb=4096\n\
         capture_utc=2026-11-27T10:00:00Z\n\
         operator=alice\n\
         reviewer=bob\n\
         workload_protocol=v1\n"
    );
    fs::write(&bad_prov, bad_prov_text)?;
    let rejected = evaluate_provenance_gate(Some(&meas_path), Some(&bad_prov));
    ensure(
        rejected.status == ProvenanceGateStatus::Rejected,
        "CI provenance was not rejected",
    )?;

    let ok_prov = root.join("prov-ok.kv");
    let ok_prov_text = format!(
        "schema_version=1\n\
         target_profile=4gb_potato\n\
         measurement_host=potato-target-lab\n\
         measurement_sha256={meas_sha}\n\
         host_facts_sha256={facts_sha}\n\
         hardware_model=Celeron-N3060\n\
         installed_ram_mb=4096\n\
         capture_utc=2026-11-27T10:00:00Z\n\
         operator=alice\n\
         reviewer=bob\n\
         workload_protocol=mixed-workspace-v1\n"
    );
    fs::write(&ok_prov, ok_prov_text)?;
    let ok = evaluate_provenance_gate(Some(&meas_path), Some(&ok_prov));
    ensure(
        ok.status == ProvenanceGateStatus::VerifiedTarget4Gb,
        "valid provenance attestation failed verification",
    )?;
    Ok("unmeasured_default=true ci_rejected=true valid_attested_verified=true".to_string())
}

fn check_population_cycler_and_abc_harness() -> Result<String> {
    let cycler = PopulationCycler::new(8, 8_000)?;
    let binding = WorkspaceBinding {
        workspace_id: WorkspaceId::new("ws-pop"),
        surface_id: SurfaceId::new("surf-pop"),
        surface_kind: SurfaceKind::Framebuffer,
        preview_mode: PreviewMode::SnapshotOnAction,
        liveness: LivenessState::Running,
    };
    let proposals: Vec<CandidateProposal> = (0..8)
        .map(|i| {
            let action = Action::new(
                EventSeq(i),
                SpaceId::new("space-pop"),
                GoalId::new("goal-pop"),
                binding.clone(),
                ActionOp::Echo {
                    text: format!("cand-{i}"),
                },
                100,
                Some("fixture".to_string()),
            )
            .expect("action");
            CandidateProposal {
                candidate_id: format!("cand-{i:02}"),
                strategy: "population".to_string(),
                actions: vec![action],
                token_cost: 100 + i * 5,
                speculative_draft: false,
            }
        })
        .collect();

    let (rejected, _) = cycler.cycle(&proposals, |p| {
        Ok(CandidateEvaluation {
            candidate_id: p.candidate_id.clone(),
            verdict: Verdict::Fail,
            deterministic: true,
            mutation_score_bps: 10_000,
            token_cost: p.token_cost,
            reason: "failed check".to_string(),
        })
    })?;
    ensure(
        matches!(rejected, PopulationSelection::RejectedAll { .. }),
        "population cycler committed an unverified batch",
    )?;

    let (selected, _) = cycler.cycle(&proposals, |p| {
        Ok(CandidateEvaluation {
            candidate_id: p.candidate_id.clone(),
            verdict: if p.candidate_id == "cand-02" || p.candidate_id == "cand-05" {
                Verdict::Pass
            } else {
                Verdict::Fail
            },
            deterministic: true,
            mutation_score_bps: 9_000,
            token_cost: p.token_cost,
            reason: "verified".to_string(),
        })
    })?;
    ensure(
        matches!(
            selected,
            PopulationSelection::Selected { ref winner, .. } if winner.candidate_id == "cand-02"
        ),
        "population cycler did not pick lowest-cost verified candidate",
    )?;

    let harness = evaluate_abc_harness(&[HarnessTaskCase {
        task_id: "t1".to_string(),
        arm_a_single_passes: false,
        arm_b_verified_retry_passes: true,
        population_candidates_pass_mask: vec![
            false, false, true, false, false, false, false, false,
        ],
        candidate_tokens: 100,
    }]);
    ensure(
        harness.len() == 3 && harness.iter().all(|s| s.performance_claims == "unmeasured"),
        "A/B/C harness lost arms or performance_claims=unmeasured label",
    )?;
    Ok("population=8 rejected_all_when_0_pass=true performance_claims=unmeasured".to_string())
}

fn check_space_supervisor_bm25_memory(root: &TempRoot) -> Result<String> {
    let dir = root.join("space-sup");
    {
        let mut sup = SpaceSupervisor::open_or_init(&dir)?;
        sup.add_goal("g1", None, "Verify upload workflow", 5000, "oracle", 4)?;
        sup.push_steering("st1", "keep mock endpoint only", 2)?;
        sup.record_fact(
            "f1",
            "upload_title",
            FactKind::Positive,
            "Title field accepts unicode safely",
            "ledger:0",
            5,
        )?;
        sup.queue_blocker(
            "b1",
            "g1",
            "publish",
            "mock-channel",
            "requires human approval",
            Taint::UntrustedToolOutput,
        )?;
    }
    let mut reopened = SpaceSupervisor::open_or_init(&dir)?;
    let rebuilt_docs = reopened.drop_and_rebuild_index();
    ensure(
        rebuilt_docs == 3,
        "BM25 rebuild did not index all live .space records",
    )?;
    let hits = reopened.retrieve_memory("unicode upload title", 2);
    ensure(
        hits.first().map(|h| h.doc_id.as_str()) == Some("fact:f1"),
        "BM25 search did not retrieve matching fact",
    )?;
    reopened.resolve_blocker("b1", true)?;
    ensure(
        reopened.pending_blockers().is_empty(),
        "resolved blocker stayed pending",
    )?;
    Ok("durable_resume=true bm25_rebuild=3 blocker_resolved=true".to_string())
}

fn check_ring3_policy_vault_redteam() -> Result<String> {
    let report = run_redteam_suite()?;
    ensure(
        report.total == 10 && report.blocked == 10 && report.leaked == 0,
        "red-team security suite allowed a leak",
    )?;
    let env = CortexPromptEnvelope::build(
        0,
        "g1",
        "Upload",
        vec![],
        vec![],
        vec![SemanticView {
            workspace_id: WorkspaceId::new("ws-upload"),
            surface_id: SurfaceId::new("s-upload"),
            fields: vec![SemanticField {
                name: "upload_password".to_string(),
                field_type: "password".to_string(),
                value: "RAW_SECRET_ABC".to_string(),
                is_secret: true,
            }],
            contains_secret_fields: true,
            raw_screenshot_allowed_for_cortex: true,
        }],
    )?;
    ensure(
        !env.semantic_views[0].raw_screenshot_allowed_for_cortex
            && env.semantic_views[0].fields[0].value == "[MASKED]",
        "CortexPromptEnvelope failed to mask password or block screenshot",
    )?;
    Ok(format!(
        "redteam_total={} redteam_blocked={} redteam_leaked=0",
        report.total, report.blocked
    ))
}

fn check_tier1_wasi_tier2_ark_net_none(root: &TempRoot) -> Result<String> {
    let spec = WasiWorkerSpec::strict_default("wasi-gate");
    let worker = ConstrainedWasiWorker;
    let outcome = worker.execute(
        &spec,
        &[
            WasiInstruction::PushU64(6),
            WasiInstruction::PushU64(7),
            WasiInstruction::MulU64,
        ],
        b"",
    )?;
    ensure(outcome.result_u64 == 42, "WASI math transform mismatch")?;
    ensure(
        worker
            .execute(&spec, &[WasiInstruction::OpenSocket], b"")
            .is_err(),
        "WASI worker allowed socket instruction",
    )?;

    ensure(
        select_domain(true) == ArkDomain::ArkQuarantine && ArkDomain::ArkWork.shares_kernel(),
        "Ark domain classification violated isolation rules",
    )?;
    let cred = ArkCellSpec::derive_channel_credential("ws-work-1", ArkDomain::ArkWork, 1);
    let cell = ArkCellSpec {
        workspace_id: "ws-work-1".to_string(),
        domain: ArkDomain::ArkWork,
        channel_credential: cred.clone(),
        memory_limit_kb: 256 * 1024,
        cpu_quota_percent: 50,
        pids_max: 32,
        ro_base_dir: root.join("ro"),
        upper_dir: root.join("up"),
        egress_class: EgressClass::None,
    };
    let mut broker = ArkBroker::new();
    broker.register_cell(&cell)?;
    ensure(
        broker.authenticate(&cred, "ws-spoof").is_err(),
        "ArkBroker accepted forged workspace_id",
    )?;

    let probe = probe_net_none_denial()?;
    ensure(
        !probe.host_listener_received_connection && probe.shim_connect_errno == 13,
        "net=none listener probe failed",
    )?;
    Ok("wasi_confined=true ark_quarantine=true net_none_listener_denied=true".to_string())
}

fn check_resource_governor_and_surface_broker() -> Result<String> {
    let governor = ResourceGovernor::new(GovernorPolicy::baseline_4gb_potato());
    let mut broker = SurfaceBroker::new();
    for desc in default_mixed_workspace_descriptors() {
        broker.register_workspace(desc)?;
    }
    let eval = governor.evaluate_workspace(
        broker.reader().descriptor("ws-ai-upload").expect("ai tab"),
        &GovernorSample {
            total_rss_kb: 900_000,
            active_heavy_rendering: 1,
            running_cells: 2,
        },
        true,
    );
    ensure(
        eval.chosen_preview_mode == PreviewMode::SnapshotOnAction
            && eval.action == GovernorAction::Allow,
        "ResourceGovernor did not permit SnapshotOnAction",
    )?;
    broker.apply_supervisor_field_update(
        "ws-ai-upload",
        SemanticField {
            name: "title".to_string(),
            field_type: "text".to_string(),
            value: "Demo Title".to_string(),
            is_secret: false,
        },
        "set title",
        "hash-a",
    )?;
    broker.apply_supervisor_field_update(
        "ws-ai-upload",
        SemanticField {
            name: "token".to_string(),
            field_type: "password".to_string(),
            value: "SECRET_DO_NOT_EXFIL".to_string(),
            is_secret: true,
        },
        "masked token",
        "hash-b",
    )?;
    let semantic = broker.cortex_semantic_view("ws-ai-upload")?;
    ensure(
        semantic
            .fields
            .iter()
            .any(|f| f.name == "token" && f.value == "[MASKED]"),
        "SurfaceBroker exposed raw secret to Cortex",
    )?;
    let frame = broker
        .switch_active_tab("ws-ai-upload")?
        .expect("RAM snapshot frame");
    ensure(
        frame.rgba.len() == frame.width * frame.height * 4
            && broker.reader().disk_writes_performed() == 0
            && broker.reader().rendering_count() == 1,
        "SurfaceBroker violated RAM-only or single-rendering invariant",
    )?;
    broker.close_workspace("ws-ai-upload")?;
    Ok("governor_enforced=true ram_only_snapshots=true cortex_secrets_masked=true".to_string())
}

fn check_quarantine_autopsy_and_leak_audit(root: &TempRoot) -> Result<String> {
    let base = root.join("q-base");
    let upper = root.join("q-upper");
    fs::create_dir_all(&base)?;
    fs::create_dir_all(&upper)?;
    fs::write(base.join("orig.txt"), b"v1")?;
    let base_manifest = snapshot_dir_manifest(&base)?;
    fs::write(upper.join("dropped.bin"), b"payload")?;
    fs::write(upper.join(".wh.orig.txt"), b"")?;

    let (autopsy, tombstone) = close_and_tombstone(
        "ws-q",
        TrustTier::QuarantineMicroVm,
        &base_manifest,
        &upper,
        vec!["127.0.0.1:8080".to_string()],
        64_000,
        0,
    )?;
    ensure(
        tombstone.upperdir_removed && !upper.exists() && autopsy.whiteouts.len() == 1,
        "quarantine autopsy did not capture whiteouts or delete upperdir",
    )?;

    let leak = run_leak_audit(Some(root.path.as_path()));
    ensure(
        !leak.zero_traces_claimed && leak.no_spacetime_thumbnails_on_disk,
        "leak audit claimed zero traces or found thumbnails on disk",
    )?;
    Ok("autopsy_tombstone=true upperdir_removed=true zero_traces_claimed=false".to_string())
}

fn check_wavefunction_cas_branch_export(root: &TempRoot) -> Result<String> {
    let cas = WavefunctionCas::open_or_init(&root.join("cas"))?;
    let local = cas.put_layer(
        "tab-scratch",
        b"local-only",
        CasProvenanceTier::TabLocal,
        Some(WorkspaceId::new("ws-a")),
        None,
        None,
    )?;
    ensure(
        cas.can_mount_layer(&local.digest, "ws-a")?
            && !cas.can_mount_layer(&local.digest, "ws-b")?,
        "TabLocal CAS layer leaked to another workspace",
    )?;

    let sp_a = root.join("cas-space-a");
    fs::create_dir_all(&sp_a)?;
    fs::write(sp_a.join("state.json"), b"{\"ok\":true}")?;
    let collapsed = cas.collapse_space(&sp_a, "snap-0")?;
    ensure(collapsed.object_count == 1, "Collapse missed file")?;

    let sp_b = root.join("cas-space-b");
    let branched = branch_space(&sp_a, &sp_b)?;
    ensure(branched.files_copied == 1, "branch_space missed file")?;

    let bundle = root.join("export.bundle.json");
    export_space_bundle(&cas, &sp_a, "snap-0", &bundle)?;
    let imported = root.join("cas-imported");
    ensure(
        import_space_bundle(&bundle, &imported)? == 1
            && fs::read(imported.join("state.json"))? == b"{\"ok\":true}",
        "bundle export/import verification failed",
    )?;
    Ok("cas_isolation=true collapse=1 branch=1 bundle_roundtrip=verified".to_string())
}

fn check_egress_airlock_and_distillation(root: &TempRoot) -> Result<String> {
    let mut egress = EgressBroker::new();
    let cred = "cred-gate-direct-0001";
    egress.register_cell(
        cred,
        "ws-net",
        EgressClass::Direct,
        None,
        Some("tunnel://dns-pin:53".to_string()),
    )?;
    ensure(
        !egress
            .evaluate_connect(cred, Some("ws-net"), "127.0.0.1", 8080)
            .allowed
            && !egress
                .evaluate_connect(cred, Some("ws-net"), "169.254.169.254", 80)
                .allowed
            && egress.validate_guest_resolver(cred, "/etc/resolv.conf").is_err(),
        "EgressBroker failed to deny localhost/metadata/host-resolv.conf",
    )?;

    let mut vault = HostVault::new();
    vault.plant_canary("can", "CANARY_AIRLOCK_GATE")?;
    let mut clip = ClipboardAirlock::new();
    ensure(
        clip.stage_copy("ws-a", Taint::TrustedUser, "CANARY_AIRLOCK_GATE", &mut vault)
            .is_err(),
        "ClipboardAirlock allowed canary leak",
    )?;
    clip.stage_copy("ws-a", Taint::TrustedUser, "safe snippet", &mut vault)?;
    ensure(
        clip.paste_once("ws-b")?.payload == "safe snippet" && clip.paste_once("ws-c").is_err(),
        "ClipboardAirlock did not enforce one-shot consumption",
    )?;

    let src = root.join("air-src");
    let dst = root.join("air-dst");
    fs::create_dir_all(&src)?;
    fs::write(src.join("report.txt"), b"ok")?;
    let receipt = airlock_copy_file(
        "ws-a",
        &src,
        Path::new("report.txt"),
        "ws-b",
        &dst,
        &mut vault,
    )?;
    ensure(receipt.size_bytes == 2, "FileAirlock failed copy")?;

    let mut distill = DistillationStore::open_or_init(&root.join("distill"))?;
    distill.register_operator(OperatorSpec {
        id: "op-1",
        precondition: "pre",
        op_name: "transform",
        postcondition: "post",
        verifier_id: "verifier-1",
        mutation_score_bps: 9_200,
        differential_inputs: 512,
        current_turn: 1,
        provenance: "ledger:1",
    })?;
    ensure(
        distill.audit_operator_drift("op-1", 2, false, 6_000)? == OperatorHealth::Suspect
            && distill.audit_operator_drift("op-1", 3, false, 6_000)? == OperatorHealth::Dead,
        "operator drift quarantine did not transition Suspect -> Dead",
    )?;
    let skill = distill.distill_skill("sk-1", "test_sk_1", 4, "trace:4", || Ok(()))?;
    ensure(skill.enabled, "distilled green skill was not enabled")?;
    Ok("egress_guarded=true airlock_one_shot=true drift_quarantine=true".to_string())
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
    out.push_str("econbench_fixtures = 20\n");
    out.push_str("static_console = \"offline_prototype\"\n");
    out.push_str("block3_done = false\n");
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
