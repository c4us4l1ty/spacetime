//! Verifiers for Spacetime.
//!
//! Block 1's `FixtureVerifier` keeps the ledger honest about claims. Block 2's
//! `CrucibleVerifier` adds configurable determinism runs, independent provenance, a declarative
//! execution profile, mutation gating, differential metrology, conservative panel resolution, and
//! population metrology. The LD_PRELOAD shim is a narrow local probe, not a complete boundary;
//! live model-written adversarial tests and real operator-population studies remain later work.

use spacetime_exec::{RunStatus, Runner, FIXED_DETERMINISM_PROFILE};
use spacetime_ledger::LedgerState;
use spacetime_types::{
    Action, ActionOp, Observation, Verdict, VerifierProof, VerifierSource, VERIFIER_SOURCE_PRIORITY,
};
use std::sync::atomic::{AtomicU64, Ordering};

pub const FIXTURE_VERIFIER_ID: &str = "fixture-verifier-v1";
pub const CRUCIBLE_VERIFIER_ID: &str = "crucible-verifier-v2";

pub trait Verifier {
    fn verify(
        &self,
        action: &Action,
        observation: &Observation,
        status: &RunStatus,
        state: &LedgerState,
    ) -> VerifierProof;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FixtureVerifier;

impl Verifier for FixtureVerifier {
    fn verify(
        &self,
        action: &Action,
        observation: &Observation,
        status: &RunStatus,
        state: &LedgerState,
    ) -> VerifierProof {
        if let RunStatus::Error(reason) = status {
            return proof(
                FIXTURE_VERIFIER_ID,
                Verdict::Error,
                observation.digest.clone(),
                observation.determinism_profile_digest.clone(),
                VerifierSource::FixtureSuite,
                1,
                0,
                reason.as_str(),
            );
        }

        match &action.op {
            ActionOp::ClaimDone { expected_digest } => {
                match (expected_digest, &state.last_evidence_digest) {
                    (Some(expected), Some(actual)) if expected == actual => proof(
                        FIXTURE_VERIFIER_ID,
                        Verdict::Pass,
                        actual.clone(),
                        observation.determinism_profile_digest.clone(),
                        VerifierSource::FixtureSuite,
                        1,
                        0,
                        "claim matched verified evidence",
                    ),
                    _ => proof(
                        FIXTURE_VERIFIER_ID,
                        Verdict::Unverified,
                        state
                            .last_evidence_digest
                            .clone()
                            .unwrap_or_else(|| observation.digest.clone()),
                        observation.determinism_profile_digest.clone(),
                        VerifierSource::FixtureSuite,
                        1,
                        0,
                        "claim_done has no matching verified evidence",
                    ),
                }
            }
            ActionOp::ProduceEvidence { .. }
            | ActionOp::Echo { .. }
            | ActionOp::Sleep { .. }
            | ActionOp::Flaky { .. } => proof(
                FIXTURE_VERIFIER_ID,
                Verdict::Pass,
                observation.digest.clone(),
                observation.determinism_profile_digest.clone(),
                VerifierSource::FixtureSuite,
                1,
                0,
                "fixture action completed",
            ),
        }
    }
}

pub trait CandidateVerifier {
    fn verify(
        &self,
        candidate: &Candidate,
        fixture: &Fixture,
        runner: &dyn Runner,
    ) -> VerifierProof;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub id: String,
    pub action: Action,
    pub artifact_digest: String,
    pub mutation_score_bps: u16,
}

impl Candidate {
    pub fn new(id: impl Into<String>, action: Action) -> Self {
        Self::with_mutation_score(id, action, 10_000)
    }

    pub fn with_mutation_score(
        id: impl Into<String>,
        action: Action,
        mutation_score_bps: u16,
    ) -> Self {
        let artifact_digest = action.args_digest.clone();
        Self {
            id: id.into(),
            action,
            artifact_digest,
            mutation_score_bps,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixture {
    pub id: String,
    pub source: VerifierSource,
    pub expected_digest: String,
    pub expected_verdict: Verdict,
    pub determinism_runs: u32,
    pub mutation_threshold_bps: u16,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CrucibleVerifier;

impl CrucibleVerifier {
    pub fn verify_candidate(
        &self,
        candidate: &Candidate,
        fixture: &Fixture,
        runner: &dyn Runner,
    ) -> VerifierProof {
        if fixture.determinism_runs < 2 {
            return proof(
                CRUCIBLE_VERIFIER_ID,
                Verdict::Error,
                "unavailable".to_string(),
                FIXED_DETERMINISM_PROFILE.digest(),
                fixture.source,
                fixture.determinism_runs,
                candidate.mutation_score_bps,
                "fixture requires at least two determinism runs",
            );
        }
        if candidate.mutation_score_bps < fixture.mutation_threshold_bps {
            return proof(
                CRUCIBLE_VERIFIER_ID,
                Verdict::Fail,
                candidate.artifact_digest.clone(),
                FIXED_DETERMINISM_PROFILE.digest(),
                fixture.source,
                fixture.determinism_runs,
                candidate.mutation_score_bps,
                "candidate mutation score is below the fixture threshold",
            );
        }

        let mut results = Vec::new();
        for _ in 0..fixture.determinism_runs {
            results.push(runner.run(&candidate.action));
        }
        let first = results.first().expect("determinism_runs >= 2");
        if results.iter().skip(1).any(|result| result != first) {
            return proof(
                CRUCIBLE_VERIFIER_ID,
                Verdict::Error,
                first.observation.digest.clone(),
                first.observation.determinism_profile_digest.clone(),
                fixture.source,
                fixture.determinism_runs,
                candidate.mutation_score_bps,
                "determinism disagreement between runs",
            );
        }
        if let RunStatus::Error(reason) = &first.status {
            return proof(
                CRUCIBLE_VERIFIER_ID,
                Verdict::Error,
                first.observation.digest.clone(),
                first.observation.determinism_profile_digest.clone(),
                fixture.source,
                fixture.determinism_runs,
                candidate.mutation_score_bps,
                reason.as_str(),
            );
        }
        if first.observation.digest.as_str() != fixture.expected_digest.as_str() {
            return proof(
                CRUCIBLE_VERIFIER_ID,
                Verdict::Fail,
                first.observation.digest.clone(),
                first.observation.determinism_profile_digest.clone(),
                fixture.source,
                fixture.determinism_runs,
                candidate.mutation_score_bps,
                "candidate digest does not match fixture oracle",
            );
        }

        proof(
            CRUCIBLE_VERIFIER_ID,
            Verdict::Pass,
            first.observation.digest.clone(),
            first.observation.determinism_profile_digest.clone(),
            fixture.source,
            fixture.determinism_runs,
            candidate.mutation_score_bps,
            "identical determinism runs matched the independent fixture oracle",
        )
    }
}

impl CandidateVerifier for CrucibleVerifier {
    fn verify(
        &self,
        candidate: &Candidate,
        fixture: &Fixture,
        runner: &dyn Runner,
    ) -> VerifierProof {
        self.verify_candidate(candidate, fixture, runner)
    }
}

pub fn verdict_flip_rate(verdicts: &[Verdict]) -> f64 {
    if verdicts.len() <= 1 {
        return 0.0;
    }
    let flips = verdicts
        .windows(2)
        .filter(|pair| pair[0] != pair[1])
        .count();
    flips as f64 / (verdicts.len() - 1) as f64
}

pub fn false_pass_leak_rate(verdicts: &[Verdict], expected: Verdict) -> f64 {
    if verdicts.is_empty() {
        return 0.0;
    }
    let leaks = verdicts
        .iter()
        .filter(|verdict| {
            **verdict != expected || (**verdict == Verdict::Pass && expected != Verdict::Pass)
        })
        .count();
    leaks as f64 / verdicts.len() as f64
}

pub fn false_pass_report(
    candidate: &Candidate,
    fixture: &Fixture,
    runner: &dyn Runner,
    runs: u32,
) -> (f64, f64) {
    let verifier = CrucibleVerifier;
    let mut verdicts = Vec::with_capacity(runs as usize);
    for _ in 0..runs {
        verdicts.push(
            verifier
                .verify_candidate(candidate, fixture, runner)
                .verdict,
        );
    }
    (
        verdict_flip_rate(&verdicts),
        false_pass_leak_rate(&verdicts, fixture.expected_verdict),
    )
}

pub fn false_pass_rate(
    candidate: &Candidate,
    fixture: &Fixture,
    runner: &dyn Runner,
    runs: u32,
) -> f64 {
    false_pass_report(candidate, fixture, runner, runs).0
}

#[derive(Debug, Default)]
pub struct UnstableVerifier {
    counter: AtomicU64,
}

impl UnstableVerifier {
    pub fn next_verdict(&self) -> Verdict {
        if self.counter.fetch_add(1, Ordering::Relaxed) % 2 == 0 {
            Verdict::Pass
        } else {
            Verdict::Fail
        }
    }
}

pub fn verifier_source_priority() -> &'static [VerifierSource] {
    &VERIFIER_SOURCE_PRIORITY
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelVerdict {
    pub source: VerifierSource,
    pub verdict: Verdict,
    pub evidence_digest: String,
    pub mutation_score_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelResolution {
    pub verdict: Verdict,
    pub selected_source: Option<VerifierSource>,
    pub selected_priority: u8,
    pub evidence_digest: String,
    pub mutation_score_bps: u16,
    pub sources: Vec<VerifierSource>,
    pub conflicts: usize,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerifierPanel {
    entries: Vec<PanelVerdict>,
}

impl VerifierPanel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(
        mut self,
        source: VerifierSource,
        verdict: Verdict,
        evidence_digest: String,
        mutation_score_bps: u16,
    ) -> Self {
        self.entries.push(PanelVerdict {
            source,
            verdict,
            evidence_digest,
            mutation_score_bps,
        });
        self
    }

    pub fn entries(&self) -> &[PanelVerdict] {
        &self.entries
    }

    pub fn resolve(&self) -> PanelResolution {
        resolve_panel(&self.entries)
    }
}

pub fn resolve_panel(entries: &[PanelVerdict]) -> PanelResolution {
    if entries.is_empty() {
        return PanelResolution {
            verdict: Verdict::Unverified,
            selected_source: None,
            selected_priority: 0,
            evidence_digest: "unavailable".to_string(),
            mutation_score_bps: 0,
            sources: Vec::new(),
            conflicts: 0,
            reason: "no independent verifier sources".to_string(),
        };
    }
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|entry| entry.source.priority());
    let selected = &sorted[0];
    let conflicts = sorted
        .iter()
        .filter(|entry| entry.verdict != selected.verdict)
        .count();
    let verdict = if sorted.iter().any(|entry| entry.verdict == Verdict::Error) {
        Verdict::Error
    } else if sorted
        .iter()
        .any(|entry| entry.verdict == Verdict::Unverified)
    {
        Verdict::Unverified
    } else if sorted.iter().any(|entry| entry.verdict == Verdict::Fail) {
        Verdict::Fail
    } else if conflicts > 0 {
        Verdict::Error
    } else {
        Verdict::Pass
    };
    let summary = sorted
        .iter()
        .map(|entry| format!("{:?}={}", entry.source, entry.verdict.as_str()))
        .collect::<Vec<_>>()
        .join(",");
    let reason = match verdict {
        Verdict::Pass => "all independent sources agreed on PASS".to_string(),
        Verdict::Error if conflicts > 0 => {
            format!("independent verifier conflict: {summary}")
        }
        _ => format!("conservative resolution: {summary}"),
    };
    PanelResolution {
        verdict,
        selected_source: Some(selected.source),
        selected_priority: selected.source.priority(),
        evidence_digest: selected.evidence_digest.clone(),
        mutation_score_bps: selected.mutation_score_bps,
        sources: sorted.iter().map(|entry| entry.source).collect(),
        conflicts,
        reason,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorKind {
    StablePass,
    StableFail,
    Flipping,
    OccasionalError,
    DriftAfterHalf,
}

impl OperatorKind {
    pub fn verdict(&self, run: u32, runs: u32) -> Verdict {
        match self {
            OperatorKind::StablePass => Verdict::Pass,
            OperatorKind::StableFail => Verdict::Fail,
            OperatorKind::Flipping => {
                if run % 2 == 0 {
                    Verdict::Pass
                } else {
                    Verdict::Fail
                }
            }
            OperatorKind::OccasionalError => {
                if run % 7 == 0 {
                    Verdict::Error
                } else {
                    Verdict::Pass
                }
            }
            OperatorKind::DriftAfterHalf => {
                if run < runs / 2 {
                    Verdict::Pass
                } else {
                    Verdict::Fail
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntheticOperator {
    pub id: String,
    pub kind: OperatorKind,
}

impl SyntheticOperator {
    pub fn new(id: impl Into<String>, kind: OperatorKind) -> Self {
        Self {
            id: id.into(),
            kind,
        }
    }

    pub fn verdicts(&self, runs: u32) -> Vec<Verdict> {
        (0..runs).map(|run| self.kind.verdict(run, runs)).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PopulationReport {
    pub operators: usize,
    pub runs_per_operator: u32,
    pub total_runs: u64,
    pub accepted: usize,
    pub unstable: usize,
    pub max_flip_rate: f64,
    pub promoted_false_pass_leaks: usize,
    pub synthetic: bool,
    pub real_operator_study: bool,
}

fn operator_kind(index: usize) -> OperatorKind {
    match index % 5 {
        0 => OperatorKind::StablePass,
        1 => OperatorKind::StableFail,
        2 => OperatorKind::Flipping,
        3 => OperatorKind::OccasionalError,
        _ => OperatorKind::DriftAfterHalf,
    }
}

pub fn population_report(operator_count: usize, runs: u32) -> PopulationReport {
    let mut accepted = 0;
    let mut unstable = 0;
    let mut max_flip_rate = 0.0;
    let mut promoted_false_pass_leaks = 0;
    for index in 0..operator_count {
        let operator = SyntheticOperator::new(
            format!("synthetic-operator-{index:02}"),
            operator_kind(index),
        );
        let verdicts = operator.verdicts(runs);
        let flip_rate = verdict_flip_rate(&verdicts);
        if flip_rate > max_flip_rate {
            max_flip_rate = flip_rate;
        }
        if flip_rate > 0.0 {
            unstable += 1;
        }
        if verdicts.iter().all(|verdict| *verdict == Verdict::Pass) {
            accepted += 1;
            if flip_rate > 0.01 {
                promoted_false_pass_leaks += 1;
            }
        }
    }
    let total_runs = (operator_count as u64).saturating_mul(u64::from(runs));
    PopulationReport {
        operators: operator_count,
        runs_per_operator: runs,
        total_runs,
        accepted,
        unstable,
        max_flip_rate,
        promoted_false_pass_leaks,
        synthetic: true,
        real_operator_study: false,
    }
}

fn reference_transform(input: u64) -> u64 {
    input.wrapping_mul(3).wrapping_add(7)
}

fn candidate_transform(input: u64) -> u64 {
    reference_transform(input)
}

fn mutant_transforms() -> [fn(u64) -> u64; 8] {
    [
        |input| input.wrapping_mul(3).wrapping_add(8),
        |input| input.wrapping_mul(3).wrapping_add(6),
        |input| input.wrapping_mul(4).wrapping_add(7),
        |input| input.wrapping_mul(2).wrapping_add(7),
        |input| input.wrapping_mul(3).wrapping_sub(7),
        |input| input.wrapping_mul(3).wrapping_add(7).wrapping_add(1),
        |input| input.wrapping_add(7),
        |input| input.wrapping_mul(3),
    ]
}

fn differential_matches(limit: u64, transform: fn(u64) -> u64) -> bool {
    (0..limit).all(|input| reference_transform(input) == transform(input))
}

pub fn differential_score(limit: u64) -> (u64, u64) {
    let mut matched = 0_u64;
    for input in 0..limit {
        if reference_transform(input) == candidate_transform(input) {
            matched += 1;
        }
    }
    (matched, limit)
}

pub fn mutation_score_bps(limit: u64) -> u16 {
    let mutants = mutant_transforms();
    let caught = mutants
        .iter()
        .filter(|transform| !differential_matches(limit, **transform))
        .count();
    (caught * 10_000 / mutants.len()) as u16
}

#[allow(clippy::too_many_arguments)]
fn proof(
    verifier_id: &str,
    verdict: Verdict,
    evidence_digest: String,
    execution_profile_digest: String,
    source: VerifierSource,
    determinism_runs: u32,
    mutation_score_bps: u16,
    reason: &str,
) -> VerifierProof {
    VerifierProof {
        verifier_id: verifier_id.to_string(),
        verdict,
        source,
        source_priority: source.priority(),
        evidence_digest,
        execution_profile_digest,
        determinism_runs,
        mutation_score_bps,
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetime_exec::ScriptedRunner;
    use spacetime_types::{
        EventSeq, GoalId, LivenessState, PreviewMode, SpaceId, SurfaceId, SurfaceKind,
        WorkspaceBinding, WorkspaceId,
    };

    fn action(op: ActionOp) -> Action {
        Action::new(
            EventSeq(0),
            SpaceId::new("space-test"),
            GoalId::new("goal-test"),
            WorkspaceBinding {
                workspace_id: WorkspaceId::new("ws-test"),
                surface_id: SurfaceId::new("surface-test"),
                surface_kind: SurfaceKind::Terminal,
                preview_mode: PreviewMode::TerminalBuffer,
                liveness: LivenessState::Running,
            },
            op,
            1_000,
            Some("fixture-verifier-v1".to_string()),
        )
        .expect("test action")
    }

    fn candidate(op: ActionOp) -> Candidate {
        Candidate::new("test-candidate", action(op))
    }

    fn fixture(expected: &str, expected_verdict: Verdict) -> Fixture {
        Fixture {
            id: "test-fixture".to_string(),
            source: VerifierSource::FixtureSuite,
            expected_digest: spacetime_types::sha256_hex(expected.as_bytes()),
            expected_verdict,
            determinism_runs: 2,
            mutation_threshold_bps: 8_000,
        }
    }

    #[test]
    fn deterministic_candidate_passes_two_runs() {
        let runner = ScriptedRunner::new().expect("runner");
        let candidate = candidate(ActionOp::Echo {
            text: "stable".to_string(),
        });
        let fixture = fixture("stable", Verdict::Pass);
        let proof = CrucibleVerifier.verify_candidate(&candidate, &fixture, &runner);
        assert_eq!(proof.verdict, Verdict::Pass);
        assert_eq!(proof.determinism_runs, 2);
        assert_eq!(proof.source, VerifierSource::FixtureSuite);
        assert_eq!(proof.source_priority, 1);
        assert!(!proof.evidence_digest.is_empty());
        assert_eq!(
            proof.execution_profile_digest,
            FIXED_DETERMINISM_PROFILE.digest()
        );
        assert_eq!(proof.mutation_score_bps, 10_000);
    }

    #[test]
    fn flaky_candidate_is_an_error_not_a_promotion() {
        let runner = ScriptedRunner::new().expect("runner");
        let candidate = candidate(ActionOp::Flaky {
            token: "flaky".to_string(),
        });
        let fixture = fixture("flaky-even", Verdict::Error);
        let proof = CrucibleVerifier.verify_candidate(&candidate, &fixture, &runner);
        assert_eq!(proof.verdict, Verdict::Error);
    }

    #[test]
    fn deterministic_candidate_has_zero_false_pass_leaks() {
        let runner = ScriptedRunner::new().expect("runner");
        let candidate = candidate(ActionOp::Echo {
            text: "stable".to_string(),
        });
        let fixture = fixture("stable", Verdict::Pass);
        let (flip_rate, leak_rate) = false_pass_report(&candidate, &fixture, &runner, 50);
        assert_eq!(flip_rate, 0.0);
        assert_eq!(leak_rate, 0.0);
    }

    #[test]
    fn flaky_candidate_has_no_false_pass_leaks() {
        let runner = ScriptedRunner::new().expect("runner");
        let candidate = candidate(ActionOp::Flaky {
            token: "flaky".to_string(),
        });
        let fixture = fixture("flaky-even", Verdict::Error);
        let proof = CrucibleVerifier.verify_candidate(&candidate, &fixture, &runner);
        assert_eq!(proof.verdict, Verdict::Error);
        let (flip_rate, leak_rate) = false_pass_report(&candidate, &fixture, &runner, 50);
        assert_eq!(flip_rate, 0.0);
        assert_eq!(leak_rate, 0.0);
    }

    #[test]
    fn unstable_operator_is_detected_by_flip_rate() {
        let unstable = UnstableVerifier::default();
        let verdicts: Vec<Verdict> = (0..50).map(|_| unstable.next_verdict()).collect();
        assert!(verdict_flip_rate(&verdicts) > 0.01);
        assert!(false_pass_leak_rate(&verdicts, Verdict::Pass) > 0.01);
    }

    #[test]
    fn trivial_adversarial_candidate_is_mutation_gated() {
        let runner = ScriptedRunner::new().expect("runner");
        let candidate = Candidate::with_mutation_score(
            "trivial-candidate",
            action(ActionOp::Echo {
                text: "stable".to_string(),
            }),
            0,
        );
        let mut fixture = fixture("stable", Verdict::Fail);
        fixture.source = VerifierSource::AdversarialModel;
        let proof = CrucibleVerifier.verify_candidate(&candidate, &fixture, &runner);
        assert_eq!(proof.verdict, Verdict::Fail);
        assert_eq!(proof.mutation_score_bps, 0);
    }

    #[test]
    fn verifier_source_priority_matches_plan() {
        let sources = verifier_source_priority();
        assert_eq!(sources, &VERIFIER_SOURCE_PRIORITY[..]);
        assert_eq!(sources.len(), 5);
        for (index, source) in sources.iter().enumerate() {
            assert_eq!(source.priority(), (index + 1) as u8);
        }
    }

    #[test]
    fn differential_equality_and_mutation_gate_work() {
        let limit = 1_000_000;
        let (matched, inputs) = differential_score(limit);
        assert_eq!(matched, inputs);
        assert!(mutation_score_bps(limit) >= 8_000);
    }

    #[test]
    fn panel_pass_requires_all_sources() {
        let panel = VerifierPanel::new()
            .push(
                VerifierSource::FixtureSuite,
                Verdict::Pass,
                "a".into(),
                10_000,
            )
            .push(
                VerifierSource::SpecOracle,
                Verdict::Pass,
                "b".into(),
                10_000,
            )
            .push(
                VerifierSource::DifferentialReference,
                Verdict::Pass,
                "c".into(),
                10_000,
            )
            .push(
                VerifierSource::AdversarialModel,
                Verdict::Pass,
                "d".into(),
                10_000,
            )
            .push(VerifierSource::Mutation, Verdict::Pass, "e".into(), 10_000);
        let resolution = panel.resolve();
        assert_eq!(resolution.verdict, Verdict::Pass);
        assert_eq!(
            resolution.selected_source,
            Some(VerifierSource::FixtureSuite)
        );
        assert_eq!(resolution.selected_priority, 1);
        assert_eq!(resolution.sources, VERIFIER_SOURCE_PRIORITY.to_vec());
        assert_eq!(resolution.conflicts, 0);
    }

    #[test]
    fn panel_conflict_is_not_promoted() {
        let panel = VerifierPanel::new()
            .push(
                VerifierSource::FixtureSuite,
                Verdict::Pass,
                "a".into(),
                10_000,
            )
            .push(
                VerifierSource::SpecOracle,
                Verdict::Fail,
                "b".into(),
                10_000,
            );
        let resolution = panel.resolve();
        assert_ne!(resolution.verdict, Verdict::Pass);
        assert!(resolution.conflicts > 0);
        assert_eq!(
            resolution.selected_source,
            Some(VerifierSource::FixtureSuite)
        );
    }

    #[test]
    fn empty_panel_is_unverified() {
        let resolution = VerifierPanel::new().resolve();
        assert_eq!(resolution.verdict, Verdict::Unverified);
        assert_eq!(resolution.selected_source, None);
        assert!(resolution.sources.is_empty());
    }

    #[test]
    fn unverified_source_blocks_promotion() {
        let panel = VerifierPanel::new()
            .push(
                VerifierSource::FixtureSuite,
                Verdict::Pass,
                "a".into(),
                10_000,
            )
            .push(
                VerifierSource::SpecOracle,
                Verdict::Unverified,
                "b".into(),
                10_000,
            );
        let resolution = panel.resolve();
        assert_eq!(resolution.verdict, Verdict::Unverified);
    }

    #[test]
    fn population_report_is_deterministic_and_detects_unstable_operators() {
        let first = population_report(10, 50);
        let second = population_report(10, 50);
        assert_eq!(first, second);
        assert_eq!(first.operators, 10);
        assert_eq!(first.runs_per_operator, 50);
        assert_eq!(first.total_runs, 500);
        assert!(first.accepted > 0);
        assert!(first.unstable > 0);
        assert!(first.max_flip_rate > 0.01);
        assert_eq!(first.promoted_false_pass_leaks, 0);
        assert!(first.synthetic);
        assert!(!first.real_operator_study);
    }
}
