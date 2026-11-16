//! Offline report generator for the static HTML console prototype.
//!
//! The report is derived from a verified hash-chained ledger. It never claims a measured
//! resource gate, target performance, live backend, real wallet, or continuous video.

use crate::ensure_flag_value;
use serde_json::{json, Value};
use spacetime_bench::{baseline_profile, target_profiles};
use spacetime_ledger::{Head, Ledger};
use spacetime_types::{
    ActionOp, DecisionKind, LivenessState, PreviewMode, Result, SpacetimeError,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub(crate) fn default_ledger_path() -> PathBuf {
    repo_root().join("bench/econbench/sample-ledger.jsonl")
}

fn wallet_ledger_path() -> PathBuf {
    repo_root().join("bench/economy/mock_wallet.jsonl")
}

fn econbench_dir() -> PathBuf {
    repo_root().join("bench/econbench")
}

fn action_label(op: &ActionOp) -> &'static str {
    match op {
        ActionOp::ProduceEvidence { .. } => "produce_evidence",
        ActionOp::Echo { .. } => "echo",
        ActionOp::Sleep { .. } => "sleep",
        ActionOp::ClaimDone { .. } => "claim_done",
        ActionOp::Flaky { .. } => "flaky",
    }
}

fn decision_label(decision: DecisionKind) -> &'static str {
    match decision {
        DecisionKind::Allow => "allow",
        DecisionKind::Deny => "deny",
        DecisionKind::Queue => "queue",
    }
}

fn preview_label(mode: PreviewMode) -> &'static str {
    match mode {
        PreviewMode::LiveDamage => "live_damage",
        PreviewMode::SnapshotOnAction => "snapshot_on_action",
        PreviewMode::SnapshotOnSwitch => "snapshot_on_switch",
        PreviewMode::Headless => "headless",
        PreviewMode::TerminalBuffer => "terminal_buffer",
    }
}

fn liveness_label(state: LivenessState) -> &'static str {
    match state {
        LivenessState::Running => "running",
        LivenessState::Rendering => "rendering",
        LivenessState::Suspended => "suspended",
        LivenessState::Dead => "dead",
    }
}

pub(crate) fn console_report(ledger_path: &Path) -> Result<Value> {
    let events = Ledger::verify(ledger_path)?;
    if events.len() < 3 {
        return Err(SpacetimeError::new(
            "console ledger needs at least three snapshot events",
        ));
    }
    let head = Head::read(ledger_path)?;
    let wallet_path = wallet_ledger_path();
    let wallet_events = Ledger::verify(&wallet_path)?;
    let wallet_head = Head::read(&wallet_path)?;
    let wallet_verified = !wallet_events.is_empty();

    let mut bounties = 0usize;
    for entry in fs::read_dir(econbench_dir())? {
        let path = entry?.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("task") {
            bounties += 1;
        }
    }
    if bounties != 20 {
        return Err(SpacetimeError::new(format!(
            "mock bounty board points at {bounties} fixtures; expected 20"
        )));
    }

    let baseline = baseline_profile();
    let registry_count = target_profiles().len();
    let ledger_tail: Vec<Value> = events
        .iter()
        .rev()
        .take(8)
        .map(|event| {
            json!({
                "seq": event.seq.0,
                "hash": event.hash.as_str(),
                "action": action_label(&event.action.op),
                "verdict": event.verifier_proof.verdict.as_str(),
                "decision": decision_label(event.policy_decision.decision),
                "evidence_digest": event.verifier_proof.evidence_digest.as_str(),
                "workspace_id": event.action.binding.workspace_id.as_str(),
                "surface_id": event.action.binding.surface_id.as_str(),
                "preview_mode": preview_label(event.action.binding.preview_mode),
                "liveness": liveness_label(event.action.binding.liveness),
            })
        })
        .collect();

    let labels = ["typed title", "filled description", "selected thumbnail"];
    let fields = [
        json!({"title": "Spacetime offline demo", "description": "", "thumbnail": "none"}),
        json!({"title": "Spacetime offline demo", "description": "A local, verifiable mixed-workspace slice.", "thumbnail": "none"}),
        json!({"title": "Spacetime offline demo", "description": "A local, verifiable mixed-workspace slice.", "thumbnail": "mock-thumbnail-a"}),
    ];
    let snapshot_strip: Vec<Value> = events
        .iter()
        .take(3)
        .enumerate()
        .map(|(index, event)| {
            json!({
                "seq": event.seq.0,
                "label": labels[index],
                "ledger_event_hash": event.hash.as_str(),
                "preview_mode": "snapshot_on_action",
                "live_video": false,
                "fields": fields[index].clone(),
            })
        })
        .collect();

    Ok(json!({
        "schema_version": 1,
        "report": "spacetime_static_console",
        "generated_by": "spacetime-ctl console",
        "demo": false,
        "resource_gate": "unmeasured_target_4gb",
        "performance_claims": "unmeasured",
        "release_gate": "unmeasured",
        "block3_done": false,
        "status": "offline_functional_prototype",
        "warnings": [
            "This prototype does not pass the measured 4 GB baseline resource gate.",
            "No RAM, CPU, boot-time, latency, or concurrency number is claimed as measured.",
            "CI/sandbox host facts are not target-machine evidence."
        ],
        "execution": {
            "execution_tier": "scripted_local_fixture",
            "trust_domain": "single_host_process",
            "egress_class": "none",
            "net": "none",
            "public_internet": false,
            "ark_shared_kernel_warning": "Ark cells share a guest kernel; a tab is a view, not a trust boundary."
        },
        "target_profiles": {
            "selected": baseline.id,
            "baseline": baseline.id,
            "registry_count": registry_count,
            "profile_role": baseline.role.as_str(),
            "optional": ["8gb_reference", "16gb_developer"],
            "no_ram_inference": true
        },
        "goal_tree": [
            {"id": "goal-block3-console", "parent": "goal-block2", "status": "prototype", "verifier": "static_console_report"},
            {"id": "goal-block3-upload", "parent": "goal-block3-console", "status": "verified_local_fixture", "verifier": "fixture_suite"},
            {"id": "goal-resource-baseline", "parent": "goal-block2", "status": "blocked_unmeasured", "verifier": "out_of_band_provenance"}
        ],
        "operators": [
            {"id": "op-scripted-cortex", "kind": "scripted_fixture", "health": "offline_only", "verifier": "fixture_suite"},
            {"id": "op-fixture-runner", "kind": "local_process", "health": "offline_only", "verifier": "fixture_suite"},
            {"id": "op-snapshot-strip", "kind": "still_placeholder", "health": "prototype", "verifier": "ledger_events"}
        ],
        "verifier": {
            "econbench_fixtures": bounties,
            "fixture_population_fixtures": 26,
            "claims_surviving_reverification": bounties,
            "false_pass_rate": 0.0,
            "false_pass_runs": 1000,
            "synthetic_operator_population": true,
            "real_operator_study": "unmeasured"
        },
        "ledger": {
            "source": ledger_path.display().to_string(),
            "verified": true,
            "event_count": head.count,
            "last_hash": head.last_hash,
            "tail": ledger_tail
        },
        "mock_economy": {
            "wallet_ledger": wallet_path.display().to_string(),
            "wallet_ledger_verified": wallet_verified,
            "wallet_last_hash": wallet_head.last_hash,
            "bounties": bounties,
            "fake_credits": 25,
            "payments_confirmed": 1,
            "real_money": false,
            "real_wallet": false,
            "public_internet": false
        },
        "workspaces": [
            {
                "id": "ws-human-terminal",
                "surface_id": "surface-human-terminal",
                "surface_kind": "terminal",
                "trust_tier": "host_trusted",
                "liveness": "running",
                "preview_mode": "terminal_buffer",
                "egress_class": "none",
                "last_action": "opened local terminal fixture"
            },
            {
                "id": "ws-ai-upload",
                "surface_id": "surface-ai-upload",
                "surface_kind": "framebuffer",
                "trust_tier": "ark_cell",
                "liveness": "running",
                "preview_mode": "snapshot_on_action",
                "egress_class": "none",
                "last_action": "typed title"
            }
        ],
        "snapshot_strip": snapshot_strip,
        "blockers": [
            {"kind": "queued_publish", "reason": "capability token not satisfied; no external effect executed", "external_effect": false},
            {"kind": "baseline_measurement", "reason": "exact 4 GB machine and accepted out-of-band provenance missing", "external_effect": false}
        ],
        "resource_meters": {
            "target_profile": baseline.id,
            "baseline_target_profile": baseline.id,
            "target_profile_registry": registry_count,
            "evaluated_memory_budget_kb": baseline.memory_budget_kb,
            "measured_peak_rss_kb": "unmeasured",
            "boot_ms": "unmeasured",
            "tab_switch_ms": "unmeasured",
            "cpu_percent": "unmeasured",
            "release_gate": "unmeasured"
        },
        "cost_meters": {
            "tokens_configured": 0,
            "tokens_consumed": 0,
            "fake_credits": 25,
            "real_cost": 0,
            "real_money": false
        },
        "leak_surface": [
            "core dumps", "crash reporters", "swap", "clipboard history", "DNS", "recent-files lists",
            "thumbnails", "GPU shader caches", "audio", "router logs", "timing side channels"
        ]
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConsoleArgs {
    out: PathBuf,
    ledger: Option<PathBuf>,
}

fn parse_console_args(args: &[String]) -> Result<ConsoleArgs> {
    if args.len() % 2 != 0 {
        return Err(SpacetimeError::new(
            "console flags must be name/value pairs",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut out = None;
    let mut ledger = None;
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
            "--out" => out = Some(PathBuf::from(value)),
            "--ledger" => ledger = Some(PathBuf::from(value)),
            _ => return Err(SpacetimeError::new(format!("unknown console flag: {flag}"))),
        }
        index += 2;
    }
    let out = out.ok_or_else(|| SpacetimeError::new("missing flag --out"))?;
    Ok(ConsoleArgs { out, ledger })
}

pub fn cmd_console(args: &[String]) -> Result<()> {
    let parsed = parse_console_args(args)?;
    let ledger_path = parsed.ledger.unwrap_or_else(default_ledger_path);
    let report = console_report(&ledger_path)?;
    let mut text = serde_json::to_string_pretty(&report)?;
    text.push('\n');
    if let Some(parent) = parsed.out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&parsed.out, text)?;
    let event_count = report
        .get("ledger")
        .and_then(|ledger| ledger.get("event_count"))
        .and_then(|count| count.as_u64())
        .unwrap_or(0);
    println!(
        "console_report={} ledger_events={event_count} wallet_ledger_verified=true",
        parsed.out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_report_reads_checked_in_ledger() -> Result<()> {
        let value = console_report(&default_ledger_path())?;
        assert_eq!(
            value["resource_gate"].as_str(),
            Some("unmeasured_target_4gb")
        );
        assert_eq!(value["performance_claims"].as_str(), Some("unmeasured"));
        assert_eq!(value["ledger"]["event_count"].as_u64(), Some(4));
        assert_eq!(value["snapshot_strip"].as_array().map(Vec::len), Some(3));
        assert_eq!(value["workspaces"].as_array().map(Vec::len), Some(2));
        Ok(())
    }

    #[test]
    fn console_flags_reject_duplicate_unknown_and_missing_values() {
        assert!(parse_console_args(&["--out".to_string()]).is_err());
        assert!(parse_console_args(&[
            "--out".to_string(),
            "a".to_string(),
            "--out".to_string(),
            "b".to_string(),
        ])
        .is_err());
        assert!(
            parse_console_args(&["--bogus".to_string(), "x".to_string()]).is_err()
        );
    }
}
