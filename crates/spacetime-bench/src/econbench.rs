//! Offline, domain-level verifiers for the Block 3 EconBench seed.
//!
//! Cases are deterministic and self-contained. They exercise real wallet invariants, independent
//! reference/candidate transformations, and local protocol traces; they never call a model, network,
//! wallet provider, or live service. The checked-in task metadata chooses a case and declares its
//! verifier source before the scripted runner executes.

use crate::economy::{MockEconomy, TransferOutcome};
use spacetime_types::{sha256_hex, Result, SpacetimeError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateVariant {
    Passing,
    PlantedBug,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskCaseReport {
    pub case_id: String,
    pub verifier_id: &'static str,
    pub checks: u64,
    pub evidence_digest: String,
}

pub fn verify_task_case(case_id: &str) -> Result<TaskCaseReport> {
    verify_task_case_with_candidate(case_id, CandidateVariant::Passing)
}

pub fn verify_task_case_with_candidate(
    case_id: &str,
    variant: CandidateVariant,
) -> Result<TaskCaseReport> {
    let (verifier_id, checks) = if case_id.starts_with("wallet_") {
        (
            "mock-economy-invariant-v1",
            verify_wallet_case(case_id, variant)?,
        )
    } else if case_id.starts_with("transform_") {
        (
            "independent-differential-v1",
            verify_transmutation_case(case_id, variant)?,
        )
    } else if case_id.starts_with("protocol_") {
        (
            "protocol-state-machine-v1",
            verify_protocol_case(case_id, variant)?,
        )
    } else {
        return Err(SpacetimeError::new(format!(
            "unknown EconBench domain case: {case_id}"
        )));
    };
    if checks == 0 {
        return Err(SpacetimeError::new(format!(
            "EconBench case performed no checks: {case_id}"
        )));
    }
    let evidence = format!("{case_id}|{verifier_id}|pass|checks={checks}");
    Ok(TaskCaseReport {
        case_id: case_id.to_string(),
        verifier_id,
        checks,
        evidence_digest: sha256_hex(evidence.as_bytes()),
    })
}

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(SpacetimeError::new(message))
    }
}

fn verify_wallet_case(case_id: &str, variant: CandidateVariant) -> Result<u64> {
    let mut economy =
        MockEconomy::with_accounts([("alice".to_string(), 100), ("bounty-board".to_string(), 10)])?;
    match case_id {
        "wallet_overdraft" => {
            let amount = match variant {
                CandidateVariant::Passing => 101,
                // Faulty candidate tries transferring within balance and skips overdraft protection test.
                CandidateVariant::PlantedBug => 100,
            };
            require(
                economy
                    .transfer("overdraft", "alice", "bounty-board", amount)
                    .is_err(),
                "overdraft fixture was accepted",
            )?;
            require(
                economy.balance("alice") == Some(100)
                    && economy.balance("bounty-board") == Some(10)
                    && economy.events().is_empty(),
                "overdraft fixture partially mutated wallet state",
            )?;
            Ok(2)
        }
        "wallet_exact_balance" => {
            let amount = match variant {
                CandidateVariant::Passing => 100,
                CandidateVariant::PlantedBug => 99,
            };
            require(
                economy.transfer("exact", "alice", "bounty-board", amount)?
                    == TransferOutcome::Applied,
                "exact-balance transfer did not apply",
            )?;
            require(
                economy.balance("alice") == Some(0) && economy.balance("bounty-board") == Some(110),
                "exact-balance transfer produced the wrong balances",
            )?;
            Ok(2)
        }
        "wallet_idempotent_replay" => {
            let second_id = match variant {
                CandidateVariant::Passing => "retry",
                CandidateVariant::PlantedBug => "retry-2",
            };
            let first = economy.transfer("retry", "alice", "bounty-board", 25)?;
            let retry = economy.transfer(second_id, "alice", "bounty-board", 25)?;
            require(
                first == TransferOutcome::Applied && retry == TransferOutcome::Duplicate,
                "identical payout retry was not idempotent",
            )?;
            require(
                economy.balance("alice") == Some(75)
                    && economy.balance("bounty-board") == Some(35)
                    && economy.events().len() == 1,
                "duplicate payout changed balance or appended another event",
            )?;
            Ok(2)
        }
        "wallet_id_reuse" => {
            let reuse_amount = match variant {
                CandidateVariant::Passing => 26,
                CandidateVariant::PlantedBug => 25,
            };
            economy.transfer("reuse", "alice", "bounty-board", 25)?;
            require(
                economy
                    .transfer("reuse", "alice", "bounty-board", reuse_amount)
                    .is_err(),
                "transfer ID reuse with changed arguments was accepted",
            )?;
            require(
                economy.balance("alice") == Some(75)
                    && economy.balance("bounty-board") == Some(35)
                    && economy.events().len() == 1,
                "rejected transfer ID reuse changed wallet state",
            )?;
            Ok(2)
        }
        "wallet_overflow_atomicity" => {
            let dest_init = match variant {
                CandidateVariant::Passing => u64::MAX,
                CandidateVariant::PlantedBug => u64::MAX - 1,
            };
            let mut full = MockEconomy::with_accounts([
                ("alice".to_string(), 100),
                ("bounty-board".to_string(), dest_init),
            ])?;
            require(
                full.transfer("overflow", "alice", "bounty-board", 1)
                    .is_err(),
                "destination overflow was accepted",
            )?;
            require(
                full.balance("alice") == Some(100)
                    && full.balance("bounty-board") == Some(dest_init)
                    && full.events().is_empty(),
                "destination overflow partially debited source",
            )?;
            Ok(2)
        }
        "wallet_invalid_requests" => {
            let invalid_amt = match variant {
                CandidateVariant::Passing => 0,
                CandidateVariant::PlantedBug => 1,
            };
            require(
                economy
                    .transfer("zero", "alice", "bounty-board", invalid_amt)
                    .is_err()
                    && economy.transfer("self", "alice", "alice", 1).is_err()
                    && economy.transfer("missing", "alice", "absent", 1).is_err(),
                "invalid transfer request was accepted",
            )?;
            require(
                economy.events().is_empty()
                    && economy.balance("alice") == Some(100)
                    && economy.balance("bounty-board") == Some(10),
                "invalid transfer request changed wallet state",
            )?;
            Ok(2)
        }
        "wallet_tamper_detection" => {
            economy.transfer("one", "alice", "bounty-board", 25)?;
            economy.transfer("two", "bounty-board", "alice", 5)?;
            require(
                MockEconomy::verify_events(economy.events())?,
                "valid wallet event chain did not verify",
            )?;
            let mut tampered = economy.events().to_vec();
            if variant == CandidateVariant::Passing {
                tampered[0].amount += 1;
            }
            require(
                !MockEconomy::verify_events(&tampered)?,
                "tampered wallet event chain was accepted",
            )?;
            Ok(2)
        }
        _ => Err(SpacetimeError::new(format!(
            "unknown planted-invariant EconBench case: {case_id}"
        ))),
    }
}

fn sample_input(index: u64) -> u64 {
    const EDGES: [u64; 8] = [
        0,
        1,
        2,
        u64::MAX,
        u64::MAX - 1,
        u64::MAX - 2,
        1_u64 << 63,
        (1_u64 << 63) - 1,
    ];
    if let Ok(edge_index) = usize::try_from(index) {
        if let Some(value) = EDGES.get(edge_index) {
            return *value;
        }
    }
    index
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}

fn reference_bit_reverse(mut value: u64) -> u64 {
    let mut reversed = 0_u64;
    for _ in 0..u64::BITS {
        reversed = (reversed << 1) | (value & 1);
        value >>= 1;
    }
    reversed
}

fn reference_byte_swap(value: u64) -> u64 {
    let mut swapped = 0_u64;
    for index in 0..8 {
        let byte = (value >> (index * 8)) & 0xff;
        swapped |= byte << ((7 - index) * 8);
    }
    swapped
}

fn reference_rotate_left(value: u64, amount: u64) -> u64 {
    let shift = u32::try_from(amount % u64::from(u64::BITS)).unwrap_or(0);
    let mut rotated = 0_u64;
    for destination in 0..u64::BITS {
        let source = (destination + u64::BITS - shift) % u64::BITS;
        rotated |= ((value >> source) & 1) << destination;
    }
    rotated
}

fn reference_rotate_right(value: u64, amount: u64) -> u64 {
    let shift = u32::try_from(amount % u64::from(u64::BITS)).unwrap_or(0);
    let mut rotated = 0_u64;
    for destination in 0..u64::BITS {
        let source = (destination + shift) % u64::BITS;
        rotated |= ((value >> source) & 1) << destination;
    }
    rotated
}

fn reference_saturating_add(left: u64, right: u64) -> u64 {
    let sum = u128::from(left) + u128::from(right);
    sum.min(u128::from(u64::MAX)) as u64
}

fn reference_checked_mul(left: u64, right: u64) -> Option<u64> {
    let product = u128::from(left) * u128::from(right);
    if product <= u128::from(u64::MAX) {
        Some(product as u64)
    } else {
        None
    }
}

fn reference_parity(mut value: u64) -> bool {
    let mut parity = false;
    while value != 0 {
        parity ^= value & 1 == 1;
        value >>= 1;
    }
    parity
}

fn verify_transmutation_case(case_id: &str, variant: CandidateVariant) -> Result<u64> {
    let checks = 1_024_u64;
    for index in 0..checks {
        let left = sample_input(index.wrapping_mul(2));
        let right = sample_input(index.wrapping_mul(2).wrapping_add(1));
        let rot = (right % 64) as u32;
        let bad_rot = (rot + 1) % 64;
        let matches = match (case_id, variant) {
            ("transform_bit_reverse", CandidateVariant::Passing) => {
                reference_bit_reverse(left) == left.reverse_bits()
            }
            ("transform_bit_reverse", CandidateVariant::PlantedBug) => {
                reference_bit_reverse(left) == (left.reverse_bits() & !1)
            }
            ("transform_byte_swap", CandidateVariant::Passing) => {
                reference_byte_swap(left) == left.swap_bytes()
            }
            ("transform_byte_swap", CandidateVariant::PlantedBug) => {
                reference_byte_swap(left) == (left.swap_bytes() ^ 0xff)
            }
            ("transform_rotate_left", CandidateVariant::Passing) => {
                reference_rotate_left(left, right) == left.rotate_left(rot)
            }
            ("transform_rotate_left", CandidateVariant::PlantedBug) => {
                reference_rotate_left(left, right) == left.rotate_left(bad_rot)
            }
            ("transform_rotate_right", CandidateVariant::Passing) => {
                reference_rotate_right(left, right) == left.rotate_right(rot)
            }
            ("transform_rotate_right", CandidateVariant::PlantedBug) => {
                reference_rotate_right(left, right) == left.rotate_right(bad_rot)
            }
            ("transform_saturating_add", CandidateVariant::Passing) => {
                reference_saturating_add(left, right) == left.saturating_add(right)
            }
            ("transform_saturating_add", CandidateVariant::PlantedBug) => {
                reference_saturating_add(left, right) == left.wrapping_add(right)
            }
            ("transform_checked_mul", CandidateVariant::Passing) => {
                reference_checked_mul(left, right) == left.checked_mul(right)
            }
            ("transform_checked_mul", CandidateVariant::PlantedBug) => {
                reference_checked_mul(left, right) == Some(left.wrapping_mul(right))
            }
            ("transform_parity", CandidateVariant::Passing) => {
                reference_parity(left) == (left.count_ones() % 2 == 1)
            }
            ("transform_parity", CandidateVariant::PlantedBug) => {
                reference_parity(left) == (left.count_ones() % 2 == 0)
            }
            _ => {
                return Err(SpacetimeError::new(format!(
                    "unknown transmutation EconBench case: {case_id}"
                )))
            }
        };
        require(matches, "independent reference and candidate disagree")?;
    }
    Ok(checks)
}

#[derive(Debug, Clone)]
enum ProtocolOp {
    Begin,
    Put {
        sequence: usize,
        value: &'static str,
    },
    Commit,
    Abort,
    Read,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProtocolState {
    begun: bool,
    committed: bool,
    aborted: bool,
    records: Vec<&'static str>,
    reads: u64,
}

fn replay_protocol(operations: &[ProtocolOp]) -> Result<ProtocolState> {
    let mut state = ProtocolState {
        begun: false,
        committed: false,
        aborted: false,
        records: Vec::new(),
        reads: 0,
    };
    for operation in operations {
        match operation {
            ProtocolOp::Begin if !state.begun && !state.committed && !state.aborted => {
                state.begun = true;
            }
            ProtocolOp::Put { sequence, value }
                if state.begun && !state.committed && *sequence == state.records.len() =>
            {
                state.records.push(value);
            }
            ProtocolOp::Commit if state.begun && !state.committed && !state.aborted => {
                state.begun = false;
                state.committed = true;
            }
            ProtocolOp::Abort if state.begun && !state.committed => {
                state.begun = false;
                state.aborted = true;
                state.records.clear();
            }
            ProtocolOp::Read if state.committed && !state.aborted => {
                state.reads = state
                    .reads
                    .checked_add(1)
                    .ok_or_else(|| SpacetimeError::new("protocol read counter overflow"))?;
            }
            _ => {
                return Err(SpacetimeError::new(
                    "protocol trace violates the service contract",
                ))
            }
        }
    }
    Ok(state)
}

fn verify_protocol_case(case_id: &str, variant: CandidateVariant) -> Result<u64> {
    let (mut trace, expected_state, expected_error) = match case_id {
        "protocol_empty_transaction" => (
            vec![ProtocolOp::Begin, ProtocolOp::Commit, ProtocolOp::Read],
            ProtocolState {
                begun: false,
                committed: true,
                aborted: false,
                records: vec![],
                reads: 1,
            },
            false,
        ),
        "protocol_two_records" => (
            vec![
                ProtocolOp::Begin,
                ProtocolOp::Put {
                    sequence: 0,
                    value: "alpha",
                },
                ProtocolOp::Put {
                    sequence: 1,
                    value: "beta",
                },
                ProtocolOp::Commit,
                ProtocolOp::Read,
            ],
            ProtocolState {
                begun: false,
                committed: true,
                aborted: false,
                records: vec!["alpha", "beta"],
                reads: 1,
            },
            false,
        ),
        "protocol_abort_discards" => (
            vec![
                ProtocolOp::Begin,
                ProtocolOp::Put {
                    sequence: 0,
                    value: "temporary",
                },
                ProtocolOp::Abort,
            ],
            ProtocolState {
                begun: false,
                committed: false,
                aborted: true,
                records: vec![],
                reads: 0,
            },
            false,
        ),
        "protocol_out_of_order" => (
            vec![ProtocolOp::Commit],
            ProtocolState {
                begun: false,
                committed: false,
                aborted: false,
                records: vec![],
                reads: 0,
            },
            true,
        ),
        "protocol_duplicate_sequence" => (
            vec![
                ProtocolOp::Begin,
                ProtocolOp::Put {
                    sequence: 0,
                    value: "first",
                },
                ProtocolOp::Put {
                    sequence: 0,
                    value: "duplicate",
                },
            ],
            ProtocolState {
                begun: false,
                committed: false,
                aborted: false,
                records: vec![],
                reads: 0,
            },
            true,
        ),
        "protocol_write_after_commit" => (
            vec![
                ProtocolOp::Begin,
                ProtocolOp::Commit,
                ProtocolOp::Put {
                    sequence: 0,
                    value: "late",
                },
            ],
            ProtocolState {
                begun: false,
                committed: false,
                aborted: false,
                records: vec![],
                reads: 0,
            },
            true,
        ),
        _ => {
            return Err(SpacetimeError::new(format!(
                "unknown protocol EconBench case: {case_id}"
            )))
        }
    };

    if variant == CandidateVariant::PlantedBug {
        if expected_error {
            // Faulty negative test omits the invalid op so it unexpectedly succeeds.
            trace = vec![ProtocolOp::Begin, ProtocolOp::Commit];
        } else {
            // Faulty positive trace drops final state operation or reverses records.
            trace.pop();
        }
    }

    let replay = replay_protocol(&trace);
    if expected_error {
        require(replay.is_err(), "invalid protocol trace was accepted")?;
    } else {
        require(
            replay? == expected_state,
            "protocol trace replay differed from the captured oracle state",
        )?;
    }
    u64::try_from(trace.len()).map_err(|_| SpacetimeError::new("protocol trace is too long"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct BountyBoardManifest {
        fake_credits_total: u64,
        public_internet: bool,
        real_money: bool,
        real_wallet: bool,
        bounties: Vec<BountyEntry>,
    }

    #[derive(Deserialize)]
    struct BountyEntry {
        id: String,
        reward_fake_credits: u64,
        verifier_source: String,
    }

    #[test]
    fn all_twenty_domain_cases_accept_passing_and_reject_planted_bugs() -> Result<()> {
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
            let report = verify_task_case(case_id)?;
            assert_eq!(report.case_id, case_id);
            assert!(report.checks > 0);
            assert_eq!(report.evidence_digest.len(), 64);
            assert!(
                verify_task_case_with_candidate(case_id, CandidateVariant::PlantedBug).is_err(),
                "planted bug was not caught on {case_id}"
            );
        }
        Ok(())
    }

    #[test]
    fn bounties_manifest_matches_sum_and_verifier_sources() -> Result<()> {
        let raw = include_str!("../../../bench/economy/bounties.json");
        let manifest: BountyBoardManifest = spacetime_types::from_json_str(raw)?;
        assert_eq!(manifest.bounties.len(), 20);
        assert!(!manifest.public_internet);
        assert!(!manifest.real_money);
        assert!(!manifest.real_wallet);
        let sum: u64 = manifest
            .bounties
            .iter()
            .map(|entry| entry.reward_fake_credits)
            .sum();
        assert_eq!(sum, manifest.fake_credits_total);
        for entry in &manifest.bounties {
            if entry.id.ends_with("transmutation") {
                assert_eq!(entry.verifier_source, "differential_reference");
            } else {
                assert_eq!(entry.verifier_source, "spec_oracle");
            }
        }
        Ok(())
    }

    #[test]
    fn unknown_case_is_rejected() {
        assert!(verify_task_case("wallet_not_registered").is_err());
        assert!(verify_task_case("unrecognized").is_err());
    }
}
