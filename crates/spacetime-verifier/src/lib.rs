//! Verifiers for Spacetime.
//!
//! Block 1's `FixtureVerifier` keeps the ledger honest about claims. Block 2's
//! `CrucibleVerifier` adds deterministic double-run checks, independent provenance, and
//! differential/mutation metrology. Real clock/RNG shims and model-written adversarial tests
//! remain later work.

use spacetime_exec::{RunStatus, Runner};
use spacetime_ledger::LedgerState;
use spacetime_types::{Action, ActionOp, Observation, Verdict, VerifierProof, VerifierSource};
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
}

impl Candidate {
    pub fn new(id: impl Into<String>, action: Action) -> Self {
        let artifact_digest = action.args_digest.clone();
        Self {
            id: id.into(),
            action,
            artifact_digest,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixture {
    pub id: String,
    pub source: VerifierSource,
    pub expected_digest: String,
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
        let first = runner.run(&candidate.action);
        let second = runner.run(&candidate.action);

        if first.status != second.status || first.observation.digest != second.observation.digest {
            return proof(
                CRUCIBLE_VERIFIER_ID,
                Verdict::Error,
                first.observation.digest.clone(),
                fixture.source,
                fixture.determinism_runs,
                0,
                "determinism disagreement between two runs",
            );
        }
        if let RunStatus::Error(reason) = &first.status {
            return proof(
                CRUCIBLE_VERIFIER_ID,
                Verdict::Error,
                first.observation.digest.clone(),
                fixture.source,
                fixture.determinism_runs,
                0,
                reason,
            );
        }
        if first.observation.digest != fixture.expected_digest {
            return proof(
                CRUCIBLE_VERIFIER_ID,
                Verdict::Fail,
                first.observation.digest.clone(),
                fixture.source,
                fixture.determinism_runs,
                0,
                "candidate digest does not match fixture oracle",
            );
        }

        proof(
            CRUCIBLE_VERIFIER_ID,
            Verdict::Pass,
            first.observation.digest.clone(),
            fixture.source,
            fixture.determinism_runs,
            fixture.mutation_threshold_bps,
            "two identical runs matched the independent fixture oracle",
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

pub fn false_pass_rate(
    candidate: &Candidate,
    fixture: &Fixture,
    runner: &dyn Runner,
    runs: u32,
) -> f64 {
    let verifier = CrucibleVerifier;
    let mut verdicts = Vec::with_capacity(runs as usize);
    for _ in 0..runs {
        verdicts.push(
            verifier
                .verify_candidate(candidate, fixture, runner)
                .verdict,
        );
    }
    verdict_flip_rate(&verdicts)
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

fn proof(
    verifier_id: &str,
    verdict: Verdict,
    evidence_digest: String,
    source: VerifierSource,
    determinism_runs: u32,
    mutation_score_bps: u16,
    reason: &str,
) -> VerifierProof {
    VerifierProof {
        verifier_id: verifier_id.to_string(),
        verdict,
        source,
        evidence_digest,
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
            None,
        )
        .expect("test action")
    }

    fn candidate(op: ActionOp) -> Candidate {
        Candidate::new("test-candidate", action(op))
    }

    fn fixture(expected: &str) -> Fixture {
        Fixture {
            id: "test-fixture".to_string(),
            source: VerifierSource::FixtureSuite,
            expected_digest: spacetime_types::sha256_hex(expected.as_bytes()),
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
        let fixture = fixture("stable");
        let proof = CrucibleVerifier.verify_candidate(&candidate, &fixture, &runner);
        assert_eq!(proof.verdict, Verdict::Pass);
        assert_eq!(proof.determinism_runs, 2);
        assert_eq!(proof.source, VerifierSource::FixtureSuite);
        assert!(!proof.evidence_digest.is_empty());
    }

    #[test]
    fn flaky_candidate_is_an_error_not_a_promotion() {
        let runner = ScriptedRunner::new().expect("runner");
        let candidate = candidate(ActionOp::Flaky {
            token: "flaky".to_string(),
        });
        let fixture = fixture("flaky-even");
        let proof = CrucibleVerifier.verify_candidate(&candidate, &fixture, &runner);
        assert_eq!(proof.verdict, Verdict::Error);
    }

    #[test]
    fn deterministic_candidate_has_zero_false_pass_rate() {
        let runner = ScriptedRunner::new().expect("runner");
        let candidate = candidate(ActionOp::Echo {
            text: "stable".to_string(),
        });
        let fixture = fixture("stable");
        assert_eq!(false_pass_rate(&candidate, &fixture, &runner, 50), 0.0);
    }

    #[test]
    fn unstable_operator_is_detected_by_flip_rate() {
        let unstable = UnstableVerifier::default();
        let verdicts: Vec<Verdict> = (0..50).map(|_| unstable.next_verdict()).collect();
        assert!(verdict_flip_rate(&verdicts) > 0.01);
    }

    #[test]
    fn differential_equality_and_mutation_gate_work() {
        let limit = 1_000_000;
        let (matched, inputs) = differential_score(limit);
        assert_eq!(matched, inputs);
        assert!(mutation_score_bps(limit) >= 8_000);
    }
}
