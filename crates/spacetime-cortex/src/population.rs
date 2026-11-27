//! Block 4 population-per-turn cycler and A/B/C offline harness.
//!
//! Instead of trusting a single unverified proposal, the cycler evaluates a bounded population
//! (`8..=12` candidates by default) under deterministic verification gates. Only a verified
//! candidate may be committed; if zero pass, all proposals are rejected.

use spacetime_types::{Action, Result, SpacetimeError, Verdict};

pub const DEFAULT_POPULATION_SIZE: usize = 10;
pub const MIN_POPULATION_SIZE: usize = 8;
pub const MAX_POPULATION_SIZE: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateProposal {
    pub candidate_id: String,
    pub strategy: String,
    pub actions: Vec<Action>,
    pub token_cost: u64,
    pub speculative_draft: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateEvaluation {
    pub candidate_id: String,
    pub verdict: Verdict,
    pub deterministic: bool,
    pub mutation_score_bps: u16,
    pub token_cost: u64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopulationSelection {
    Selected {
        winner: CandidateProposal,
        evaluation: CandidateEvaluation,
        candidates_tested: usize,
        candidates_passed: usize,
    },
    RejectedAll {
        candidates_tested: usize,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct PopulationCycler {
    target_population: usize,
    mutation_threshold_bps: u16,
}

impl PopulationCycler {
    pub fn new(target_population: usize, mutation_threshold_bps: u16) -> Result<Self> {
        if !(MIN_POPULATION_SIZE..=MAX_POPULATION_SIZE).contains(&target_population) {
            return Err(SpacetimeError::new(format!(
                "population size must be in {MIN_POPULATION_SIZE}..={MAX_POPULATION_SIZE}"
            )));
        }
        Ok(Self {
            target_population,
            mutation_threshold_bps,
        })
    }

    pub fn target_population(&self) -> usize {
        self.target_population
    }

    /// Evaluate candidates via the caller-provided deterministic verifier oracle and select the
    /// highest-scoring passing candidate (lowest token cost breaks ties).
    pub fn cycle<F>(
        &self,
        proposals: &[CandidateProposal],
        mut evaluator: F,
    ) -> Result<(PopulationSelection, Vec<CandidateEvaluation>)>
    where
        F: FnMut(&CandidateProposal) -> Result<CandidateEvaluation>,
    {
        if proposals.is_empty() {
            return Ok((
                PopulationSelection::RejectedAll {
                    candidates_tested: 0,
                    reason: "zero proposals supplied to population cycler".to_string(),
                },
                Vec::new(),
            ));
        }

        let bounded = &proposals[..proposals.len().min(self.target_population)];
        let mut evaluations = Vec::with_capacity(bounded.len());
        let mut passing: Vec<(&CandidateProposal, CandidateEvaluation)> = Vec::new();

        for proposal in bounded {
            let eval = evaluator(proposal)?;
            let passes = eval.verdict == Verdict::Pass
                && eval.deterministic
                && eval.mutation_score_bps >= self.mutation_threshold_bps;
            if passes {
                passing.push((proposal, eval.clone()));
            }
            evaluations.push(eval);
        }

        if passing.is_empty() {
            return Ok((
                PopulationSelection::RejectedAll {
                    candidates_tested: bounded.len(),
                    reason: "zero candidates satisfied deterministic verifier + mutation threshold"
                        .to_string(),
                },
                evaluations,
            ));
        }

        passing.sort_by(|(a_prop, a_eval), (b_prop, b_eval)| {
            b_eval
                .mutation_score_bps
                .cmp(&a_eval.mutation_score_bps)
                .then_with(|| a_eval.token_cost.cmp(&b_eval.token_cost))
                .then_with(|| a_prop.candidate_id.cmp(&b_prop.candidate_id))
        });

        let (winner, winner_eval) = passing.remove(0);
        let candidates_passed = evaluations
            .iter()
            .filter(|e| {
                e.verdict == Verdict::Pass
                    && e.deterministic
                    && e.mutation_score_bps >= self.mutation_threshold_bps
            })
            .count();

        Ok((
            PopulationSelection::Selected {
                winner: winner.clone(),
                evaluation: winner_eval,
                candidates_tested: bounded.len(),
                candidates_passed,
            },
            evaluations,
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessArm {
    ArmABaselineSingle,
    ArmBMemoryAndVerifier,
    ArmCPopulationPerTurn,
}

impl HarnessArm {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ArmABaselineSingle => "arm_a_baseline_single",
            Self::ArmBMemoryAndVerifier => "arm_b_memory_verifier",
            Self::ArmCPopulationPerTurn => "arm_c_population_per_turn",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessTaskCase {
    pub task_id: String,
    pub arm_a_single_passes: bool,
    pub arm_b_verified_retry_passes: bool,
    pub population_candidates_pass_mask: Vec<bool>,
    pub candidate_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessArmSummary {
    pub arm: HarnessArm,
    pub tasks_total: usize,
    pub tasks_passed: usize,
    pub unverified_claim_rate_bps: u16,
    pub total_tokens: u64,
    pub performance_claims: &'static str,
}

/// Deterministic offline A/B/C harness evaluator.
/// Reports `performance_claims = "unmeasured"` for real external LLMs because only controlled
/// synthetic / offline task suites run inside the sandboxed repository.
pub fn evaluate_abc_harness(cases: &[HarnessTaskCase]) -> Vec<HarnessArmSummary> {
    let total = cases.len().max(1);
    let mut summaries = Vec::with_capacity(3);

    for arm in [
        HarnessArm::ArmABaselineSingle,
        HarnessArm::ArmBMemoryAndVerifier,
        HarnessArm::ArmCPopulationPerTurn,
    ] {
        let mut passed = 0usize;
        let mut unverified_claims = 0usize;
        let mut tokens = 0u64;

        for case in cases {
            match arm {
                HarnessArm::ArmABaselineSingle => {
                    tokens = tokens.saturating_add(case.candidate_tokens);
                    if case.arm_a_single_passes {
                        passed += 1;
                    } else {
                        // Single unverified agent claims done without verifier catch.
                        unverified_claims += 1;
                    }
                }
                HarnessArm::ArmBMemoryAndVerifier => {
                    tokens = tokens.saturating_add(case.candidate_tokens.saturating_mul(2));
                    if case.arm_a_single_passes || case.arm_b_verified_retry_passes {
                        passed += 1;
                    }
                }
                HarnessArm::ArmCPopulationPerTurn => {
                    let pop_n = case.population_candidates_pass_mask.len().max(1) as u64;
                    tokens = tokens.saturating_add(case.candidate_tokens.saturating_mul(pop_n));
                    if case.population_candidates_pass_mask.iter().any(|p| *p) {
                        passed += 1;
                    }
                }
            }
        }

        let unverified_claim_rate_bps =
            u16::try_from((unverified_claims * 10_000) / total).unwrap_or(10_000);

        summaries.push(HarnessArmSummary {
            arm,
            tasks_total: cases.len(),
            tasks_passed: passed,
            unverified_claim_rate_bps,
            total_tokens: tokens,
            performance_claims: "unmeasured",
        });
    }

    summaries
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetime_types::{
        ActionOp, EventSeq, GoalId, LivenessState, PreviewMode, SpaceId, SurfaceId, SurfaceKind,
        WorkspaceBinding, WorkspaceId,
    };

    fn dummy_proposal(id: &str, token_cost: u64) -> CandidateProposal {
        let action = Action::new(
            EventSeq(0),
            SpaceId::new("space-1"),
            GoalId::new("goal-1"),
            WorkspaceBinding {
                workspace_id: WorkspaceId::new("ws-1"),
                surface_id: SurfaceId::new("s-1"),
                surface_kind: SurfaceKind::Framebuffer,
                preview_mode: PreviewMode::SnapshotOnAction,
                liveness: LivenessState::Running,
            },
            ActionOp::Echo {
                text: id.to_string(),
            },
            100,
            Some("fixture".to_string()),
        )
        .expect("action");
        CandidateProposal {
            candidate_id: id.to_string(),
            strategy: "test".to_string(),
            actions: vec![action],
            token_cost,
            speculative_draft: false,
        }
    }

    #[test]
    fn population_cycler_enforces_8_to_12_bounds_and_rejects_unverified_batches() -> Result<()> {
        assert!(PopulationCycler::new(7, 8_000).is_err());
        assert!(PopulationCycler::new(13, 8_000).is_err());
        let cycler = PopulationCycler::new(10, 8_000)?;

        let proposals: Vec<_> = (0..10)
            .map(|i| dummy_proposal(&format!("cand-{i:02}"), 100 + i * 10))
            .collect();

        // All fail -> RejectedAll
        let (rejected, evals) = cycler.cycle(&proposals, |p| {
            Ok(CandidateEvaluation {
                candidate_id: p.candidate_id.clone(),
                verdict: Verdict::Fail,
                deterministic: true,
                mutation_score_bps: 10_000,
                token_cost: p.token_cost,
                reason: "failed invariant".to_string(),
            })
        })?;
        assert!(matches!(rejected, PopulationSelection::RejectedAll { .. }));
        assert_eq!(evals.len(), 10);

        // Candidates 3 and 7 pass; cand-03 has lower token cost and higher score
        let (selected, _) = cycler.cycle(&proposals, |p| {
            let pass = p.candidate_id == "cand-03" || p.candidate_id == "cand-07";
            Ok(CandidateEvaluation {
                candidate_id: p.candidate_id.clone(),
                verdict: if pass { Verdict::Pass } else { Verdict::Fail },
                deterministic: true,
                mutation_score_bps: if p.candidate_id == "cand-03" {
                    9_500
                } else {
                    8_500
                },
                token_cost: p.token_cost,
                reason: "checked".to_string(),
            })
        })?;
        match selected {
            PopulationSelection::Selected {
                winner,
                candidates_tested,
                candidates_passed,
                ..
            } => {
                assert_eq!(winner.candidate_id, "cand-03");
                assert_eq!(candidates_tested, 10);
                assert_eq!(candidates_passed, 2);
            }
            PopulationSelection::RejectedAll { .. } => panic!("expected winner"),
        }
        Ok(())
    }

    #[test]
    fn abc_harness_tracks_unverified_claims_and_keeps_performance_claims_unmeasured() {
        let cases = vec![
            HarnessTaskCase {
                task_id: "t1".to_string(),
                arm_a_single_passes: false,
                arm_b_verified_retry_passes: true,
                population_candidates_pass_mask: vec![
                    false, false, true, false, false, false, false, false,
                ],
                candidate_tokens: 100,
            },
            HarnessTaskCase {
                task_id: "t2".to_string(),
                arm_a_single_passes: true,
                arm_b_verified_retry_passes: true,
                population_candidates_pass_mask: vec![
                    true, false, false, false, false, false, false, false,
                ],
                candidate_tokens: 100,
            },
        ];
        let summary = evaluate_abc_harness(&cases);
        assert_eq!(summary[0].tasks_passed, 1);
        assert_eq!(summary[0].unverified_claim_rate_bps, 5_000);
        assert_eq!(summary[1].unverified_claim_rate_bps, 0);
        assert_eq!(summary[2].tasks_passed, 2);
        assert_eq!(summary[2].performance_claims, "unmeasured");
    }
}
