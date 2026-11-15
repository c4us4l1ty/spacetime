//! Independent fixture verifier for Spacetime Block 1.
//!
//! A `claim_done` action cannot pass unless it names the digest of evidence already verified in
//! the ledger state. This is the smallest mechanical version of "a claim without a verifier is
//! unverified".

use spacetime_exec::RunStatus;
use spacetime_ledger::LedgerState;
use spacetime_types::{Action, ActionOp, Observation, Verdict, VerifierProof};

pub const FIXTURE_VERIFIER_ID: &str = "fixture-verifier-v1";

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
                Verdict::Error,
                observation.digest.clone(),
                reason.as_str(),
            );
        }

        match &action.op {
            ActionOp::ClaimDone { expected_digest } => {
                match (expected_digest, &state.last_evidence_digest) {
                    (Some(expected), Some(actual)) if expected == actual => proof(
                        Verdict::Pass,
                        actual.clone(),
                        "claim matched verified evidence",
                    ),
                    _ => proof(
                        Verdict::Unverified,
                        state
                            .last_evidence_digest
                            .clone()
                            .unwrap_or_else(|| observation.digest.clone()),
                        "claim_done has no matching verified evidence",
                    ),
                }
            }
            ActionOp::ProduceEvidence { .. }
            | ActionOp::Echo { .. }
            | ActionOp::Sleep { .. } => proof(
                Verdict::Pass,
                observation.digest.clone(),
                "fixture action completed",
            ),
        }
    }
}

fn proof(verdict: Verdict, evidence_digest: String, reason: &str) -> VerifierProof {
    VerifierProof {
        verifier_id: FIXTURE_VERIFIER_ID.to_string(),
        verdict,
        evidence_digest,
        determinism_runs: 1,
        mutation_score_bps: 0,
        reason: reason.to_string(),
    }
}
