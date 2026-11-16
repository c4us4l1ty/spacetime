//! Offline EconBench seed for Block 3.
//!
//! This is a functional prototype over scripted local fixtures. It makes no resource,
//! performance, model-efficacy, or real-economy claim. Arms A/B/C are harness metadata only.

use crate::{crucible_inputs, ensure_flag_value};
use serde_json::{json, Value};
use spacetime_bench::econbench::verify_task_case;
use spacetime_bench::{baseline_profile, target_profiles};
use spacetime_cortex::{Cortex, ScriptedCortex};
use spacetime_exec::{Runner, ScriptedRunner};
use spacetime_ledger::{Ledger, LedgerState};
use spacetime_types::{DecisionKind, Event, GoalStatus, PolicyDecision, Result, SpacetimeError};
use spacetime_verifier::{
    false_pass_report_checked, validate_reruns, CrucibleVerifier, FixtureVerifier, Verifier,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

const RERUNS: u32 = 50;
const MAX_TOKEN_BUDGET: u64 = 1_000_000;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn econbench_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/econbench")
}

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(label: &str) -> Result<Self> {
        let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct BenchArgs {
    arm: String,
    model: String,
    budget: u64,
    out: PathBuf,
}

fn parse_bench_run(args: &[String]) -> Result<BenchArgs> {
    if args.len() % 2 != 0 {
        return Err(SpacetimeError::new(
            "bench run flags must be name/value pairs",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut arm = None;
    let mut model = None;
    let mut budget = None;
    let mut out = None;
    let mut index = 0;
    while index < args.len() {
        let flag = &args[index];
        let value = args
            .get(index + 1)
            .ok_or_else(|| SpacetimeError::new(format!("missing value for {flag}")))?;
        ensure_flag_value(flag.as_str(), value.as_str())?;
        if !seen.insert(flag.clone()) {
            return Err(SpacetimeError::new(format!("duplicate {flag} flag")));
        }
        match flag.as_str() {
            "--arm" => arm = Some(value.clone()),
            "--model" => model = Some(value.clone()),
            "--budget" => budget = Some(value.clone()),
            "--out" => out = Some(PathBuf::from(value)),
            _ => {
                return Err(SpacetimeError::new(format!(
                    "unknown bench run flag: {flag}"
                )))
            }
        }
        index += 2;
    }
    let arm = arm.ok_or_else(|| SpacetimeError::new("missing flag --arm"))?;
    if !matches!(arm.as_str(), "A" | "B" | "C") {
        return Err(SpacetimeError::new("bench arm must be A, B, or C"));
    }
    let model = model.ok_or_else(|| SpacetimeError::new("missing flag --model"))?;
    let budget_raw = budget.ok_or_else(|| SpacetimeError::new("missing flag --budget"))?;
    let budget = budget_raw.parse::<u64>()?;
    if !(1..=MAX_TOKEN_BUDGET).contains(&budget) {
        return Err(SpacetimeError::new("token budget must be 1..=1000000"));
    }
    let out = out.ok_or_else(|| SpacetimeError::new("missing flag --out"))?;
    Ok(BenchArgs {
        arm,
        model,
        budget,
        out,
    })
}

pub fn cmd_bench(args: &[String]) -> Result<()> {
    let subcommand = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| SpacetimeError::new("missing bench subcommand"))?;
    match subcommand {
        "run" => cmd_bench_run(&args[1..]),
        _ => Err(SpacetimeError::new(format!(
            "unknown bench subcommand: {subcommand}"
        ))),
    }
}

fn cmd_bench_run(args: &[String]) -> Result<()> {
    let parsed = parse_bench_run(args)?;
    let report = econbench_report(&parsed.arm, &parsed.model, parsed.budget)?;
    let mut text = serde_json::to_string_pretty(&report)?;
    text.push('\n');
    if let Some(parent) = parsed.out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&parsed.out, text)?;
    let fixtures = report
        .get("fixtures")
        .and_then(|field| field.as_u64())
        .unwrap_or(0);
    let false_pass_rate = report
        .get("false_pass_rate")
        .and_then(|field| field.as_f64())
        .unwrap_or(1.0);
    println!(
        "report={} fixtures={fixtures} completed={fixtures} claims_surviving_reverification={fixtures} false_pass_rate={false_pass_rate:.4}",
        parsed.out.display()
    );
    Ok(())
}

pub(crate) fn econbench_report(arm: &str, model: &str, budget: u64) -> Result<Value> {
    validate_reruns(RERUNS)?;
    let (candidate_count, cycler_enabled, fork_graph) = match arm {
        "A" => (1, false, false),
        "B" => (8, false, false),
        "C" => (8, true, true),
        _ => return Err(SpacetimeError::new("bench arm must be A, B, or C")),
    };
    let mut paths = Vec::new();
    for entry in fs::read_dir(econbench_dir())? {
        let path = entry?.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("task") {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.len() != 20 {
        return Err(SpacetimeError::new(format!(
            "EconBench seed has {} fixtures; expected 20",
            paths.len()
        )));
    }
    let root = TempRoot::new("spacetime-econbench")?;
    let started = Instant::now();
    let mut tasks = Vec::new();
    let mut claims = 0usize;
    let mut max_flip_rate = 0.0f64;
    let mut max_leak_rate = 0.0f64;

    for (index, path) in paths.iter().enumerate() {
        let source_text = fs::read_to_string(path)?;
        let category =
            metadata_value(&source_text, "category").unwrap_or_else(|| "unknown".to_string());
        let verifier_source = metadata_value(&source_text, "verifier_source")
            .unwrap_or_else(|| "fixture_suite".to_string());
        let domain_case = metadata_value(&source_text, "case_id").ok_or_else(|| {
            SpacetimeError::new(format!("fixture {} is missing case_id", path.display()))
        })?;
        let domain_verification = verify_task_case(&domain_case)?;
        let declared_source = match domain_verification.verifier_id {
            "independent-differential-v1" => "differential_reference",
            "mock-economy-invariant-v1" | "protocol-state-machine-v1" => "spec_oracle",
            _ => {
                return Err(SpacetimeError::new(format!(
                    "unregistered domain verifier for case {domain_case}"
                )))
            }
        };
        if verifier_source != declared_source {
            return Err(SpacetimeError::new(format!(
                "fixture {} declares verifier_source={verifier_source}, but case {domain_case} requires {declared_source}",
                path.display()
            )));
        }
        let (candidate, fixture) = crucible_inputs(path)?;
        let runner = ScriptedRunner::new()?;
        let (flip_rate, leak_rate) =
            false_pass_report_checked(&candidate, &fixture, &runner, RERUNS)?;
        let proof = CrucibleVerifier.verify_candidate(&candidate, &fixture, &runner);
        if proof.verdict != fixture.expected_verdict {
            return Err(SpacetimeError::new(format!(
                "fixture {} disagreed with its declared oracle",
                path.display()
            )));
        }
        let ledger_name = format!("fixture-{index:02}.jsonl");
        let ledger_path = root.join(ledger_name.as_str());
        let ledger_events = run_offline_task(path, &ledger_path)?;
        claims += 1;
        max_flip_rate = max_flip_rate.max(flip_rate);
        max_leak_rate = max_leak_rate.max(leak_rate);
        let id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unknown");
        tasks.push(json!({
            "id": id,
            "category": category,
            "verifier_source": verifier_source,
            "domain_case": domain_case,
            "domain_verifier": domain_verification.verifier_id,
            "domain_checks": domain_verification.checks,
            "domain_evidence_digest": domain_verification.evidence_digest,
            "expected_verdict": fixture.expected_verdict.as_str(),
            "status": proof.verdict.as_str(),
            "evidence_digest": fixture.expected_digest,
            "flip_rate": flip_rate,
            "false_pass_rate": leak_rate,
            "ledger_events": ledger_events,
        }));
    }

    let wall_clock_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let false_pass_runs = u64::try_from(tasks.len()).unwrap_or(0) * u64::from(RERUNS);
    let baseline = baseline_profile();
    let registry_count = target_profiles().len();
    Ok(json!({
        "schema_version": 1,
        "report": "econbench_offline",
        "arm": arm,
        "model": model,
        "budget_tokens": budget,
        "model_adapter": "offline_scripted",
        "model_calls": 0,
        "network": "none",
        "public_internet": false,
        "real_wallet": false,
        "real_account": false,
        "real_model": false,
        "arm_candidate_count": candidate_count,
        "arm_cycler_enabled": cycler_enabled,
        "arm_fork_graph": fork_graph,
        "arm_efficacy_claim": false,
        "resource_gate": "unmeasured_target_4gb",
        "performance_claims": "unmeasured",
        "release_gate": "unmeasured",
        "block3_done": false,
        "wall_clock_ms": wall_clock_ms,
        "wall_clock_scope": "ci_or_local_host_only_not_target_evidence",
        "fixtures": tasks.len(),
        "completed": tasks.len(),
        "interventions": 0,
        "tokens_configured": budget,
        "tokens_consumed": 0,
        "cost_fake_credits": 0,
        "claims_surviving_reverification": claims,
        "false_pass_rate": max_leak_rate,
        "max_flip_rate": max_flip_rate,
        "false_pass_runs": false_pass_runs,
        "target_profile": baseline.id,
        "baseline_target_profile": baseline.id,
        "target_profile_registry": registry_count,
        "profile_role": baseline.role.as_str(),
        "tasks": tasks,
    }))
}

fn metadata_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let (found, value) = line.split_once('=')?;
        if found.trim() == key {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

fn run_offline_task(task_path: &Path, ledger_path: &Path) -> Result<usize> {
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
    if state.goal_status != GoalStatus::Verified {
        return Err(SpacetimeError::new(format!(
            "offline fixture did not verify: {}",
            task_path.display()
        )));
    }
    Ok(Ledger::verify(ledger_path)?.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bench_run_flags_are_bounded() {
        let valid = vec![
            "--arm".to_string(),
            "A".to_string(),
            "--model".to_string(),
            "scripted-fixture".to_string(),
            "--budget".to_string(),
            "1000".to_string(),
            "--out".to_string(),
            "target/econbench.json".to_string(),
        ];
        assert!(parse_bench_run(&valid).is_ok());
        assert!(parse_bench_run(&[
            "--arm".to_string(),
            "D".to_string(),
            "--model".to_string(),
            "scripted-fixture".to_string(),
            "--budget".to_string(),
            "1000".to_string(),
            "--out".to_string(),
            "target/econbench.json".to_string(),
        ])
        .is_err());
        assert!(parse_bench_run(&["--bogus".to_string(), "x".to_string()]).is_err());
    }

    #[test]
    fn econbench_report_runs_twenty_offline_fixtures() -> Result<()> {
        let value = econbench_report("A", "scripted-fixture", 1000)?;
        assert_eq!(value["fixtures"].as_u64(), Some(20));
        assert_eq!(value["completed"].as_u64(), Some(20));
        assert_eq!(value["claims_surviving_reverification"].as_u64(), Some(20));
        assert_eq!(value["false_pass_rate"].as_f64(), Some(0.0));
        let tasks = value["tasks"].as_array().expect("task report array");
        assert_eq!(tasks.len(), 20);
        assert!(tasks.iter().all(|task| {
            task["domain_case"].as_str().is_some()
                && task["domain_verifier"].as_str().is_some()
                && task["domain_checks"].as_u64().unwrap_or(0) > 0
                && task["domain_evidence_digest"]
                    .as_str()
                    .is_some_and(|digest| digest.len() == 64)
        }));
        let domain_cases: BTreeSet<_> = tasks
            .iter()
            .filter_map(|task| task["domain_case"].as_str())
            .collect();
        assert_eq!(
            domain_cases.len(),
            20,
            "domain cases must not be duplicated"
        );
        assert_eq!(value["network"].as_str(), Some("none"));
        assert_eq!(value["arm_efficacy_claim"].as_bool(), Some(false));
        assert_eq!(
            value["resource_gate"].as_str(),
            Some("unmeasured_target_4gb")
        );
        assert_eq!(value["performance_claims"].as_str(), Some("unmeasured"));
        Ok(())
    }
}
