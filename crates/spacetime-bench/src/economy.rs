//! Deterministic, process-local mock economy primitives for offline benchmark fixtures.
//!
//! This module models fake credits only. It has no payment provider, account integration, network
//! access, or persistence; callers that need a durable ledger must append these events to the
//! separately verified Spacetime ledger.

use serde::Serialize;
use spacetime_types::{canonical_json_bytes, sha256_hex, Result, SpacetimeError};
use std::collections::{BTreeMap, BTreeSet};

const GENESIS_HASH: &str = "genesis";
const MAX_ID_BYTES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WalletEvent {
    pub sequence: u64,
    pub transfer_id: String,
    pub from_account: String,
    pub to_account: String,
    pub amount: u64,
    pub previous_hash: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WalletEventPayload<'a> {
    sequence: u64,
    transfer_id: &'a str,
    from_account: &'a str,
    to_account: &'a str,
    amount: u64,
    previous_hash: &'a str,
}

impl WalletEvent {
    fn payload(&self) -> WalletEventPayload<'_> {
        WalletEventPayload {
            sequence: self.sequence,
            transfer_id: &self.transfer_id,
            from_account: &self.from_account,
            to_account: &self.to_account,
            amount: self.amount,
            previous_hash: &self.previous_hash,
        }
    }

    fn calculate_hash(&self) -> Result<String> {
        Ok(sha256_hex(&canonical_json_bytes(&self.payload())?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferOutcome {
    Applied,
    Duplicate,
}

/// Local fake-credit wallet with atomic transfers and idempotency protection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockEconomy {
    balances: BTreeMap<String, u64>,
    applied_transfers: BTreeMap<String, String>,
    events: Vec<WalletEvent>,
}

impl MockEconomy {
    /// Create a wallet from explicit fake-credit balances. Duplicate or unsafe account IDs fail.
    pub fn with_accounts(accounts: impl IntoIterator<Item = (String, u64)>) -> Result<Self> {
        let mut balances = BTreeMap::new();
        for (account, balance) in accounts {
            validate_id(&account, "account")?;
            if balances.insert(account.clone(), balance).is_some() {
                return Err(SpacetimeError::new(format!(
                    "duplicate mock-economy account: {account}"
                )));
            }
        }
        if balances.len() < 2 {
            return Err(SpacetimeError::new(
                "mock economy requires at least two accounts",
            ));
        }
        Ok(Self {
            balances,
            applied_transfers: BTreeMap::new(),
            events: Vec::new(),
        })
    }

    pub fn balance(&self, account: &str) -> Option<u64> {
        self.balances.get(account).copied()
    }

    pub fn events(&self) -> &[WalletEvent] {
        &self.events
    }

    /// Transfer fake credits atomically. Replaying an identical transfer ID is a no-op;
    /// reusing that ID with different arguments is rejected.
    pub fn transfer(
        &mut self,
        transfer_id: &str,
        from_account: &str,
        to_account: &str,
        amount: u64,
    ) -> Result<TransferOutcome> {
        validate_id(transfer_id, "transfer_id")?;
        validate_id(from_account, "from_account")?;
        validate_id(to_account, "to_account")?;
        if from_account == to_account {
            return Err(SpacetimeError::new(
                "mock transfer source and destination must differ",
            ));
        }
        if amount == 0 {
            return Err(SpacetimeError::new("mock transfer amount must be positive"));
        }

        let request_hash = request_hash(transfer_id, from_account, to_account, amount)?;
        if let Some(previous_request_hash) = self.applied_transfers.get(transfer_id) {
            if previous_request_hash == &request_hash {
                return Ok(TransferOutcome::Duplicate);
            }
            return Err(SpacetimeError::new(
                "mock transfer ID was already used with different arguments",
            ));
        }

        let source_balance = self
            .balances
            .get(from_account)
            .copied()
            .ok_or_else(|| SpacetimeError::new("missing mock transfer source"))?;
        let destination_balance = self
            .balances
            .get(to_account)
            .copied()
            .ok_or_else(|| SpacetimeError::new("missing mock transfer destination"))?;
        let next_source = source_balance
            .checked_sub(amount)
            .ok_or_else(|| SpacetimeError::new("mock transfer overdraft"))?;
        let next_destination = destination_balance
            .checked_add(amount)
            .ok_or_else(|| SpacetimeError::new("mock transfer destination overflow"))?;
        let sequence = u64::try_from(self.events.len())
            .map_err(|_| SpacetimeError::new("mock economy event sequence overflow"))?;
        let previous_hash = self
            .events
            .last()
            .map(|event| event.hash.as_str())
            .unwrap_or(GENESIS_HASH)
            .to_string();
        let mut event = WalletEvent {
            sequence,
            transfer_id: transfer_id.to_string(),
            from_account: from_account.to_string(),
            to_account: to_account.to_string(),
            amount,
            previous_hash,
            hash: String::new(),
        };
        event.hash = event.calculate_hash()?;

        // Mutate only after every validation and arithmetic operation has succeeded.
        self.balances.insert(from_account.to_string(), next_source);
        self.balances
            .insert(to_account.to_string(), next_destination);
        self.applied_transfers
            .insert(transfer_id.to_string(), request_hash);
        self.events.push(event);
        Ok(TransferOutcome::Applied)
    }

    /// Validate sequence numbering, the hash chain, and unique transfer IDs.
    pub fn verify_events(events: &[WalletEvent]) -> Result<bool> {
        let mut previous_hash = GENESIS_HASH;
        let mut transfer_ids = BTreeSet::new();
        for (index, event) in events.iter().enumerate() {
            let expected_sequence = u64::try_from(index)
                .map_err(|_| SpacetimeError::new("mock economy event sequence overflow"))?;
            if event.sequence != expected_sequence
                || event.previous_hash != previous_hash
                || !transfer_ids.insert(event.transfer_id.as_str())
                || event.hash != event.calculate_hash()?
            {
                return Ok(false);
            }
            previous_hash = event.hash.as_str();
        }
        Ok(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TransferRequest<'a> {
    transfer_id: &'a str,
    from_account: &'a str,
    to_account: &'a str,
    amount: u64,
}

fn request_hash(
    transfer_id: &str,
    from_account: &str,
    to_account: &str,
    amount: u64,
) -> Result<String> {
    let request = TransferRequest {
        transfer_id,
        from_account,
        to_account,
        amount,
    };
    Ok(sha256_hex(&canonical_json_bytes(&request)?))
}

fn validate_id(value: &str, field: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(SpacetimeError::new(format!(
            "invalid mock economy {field}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wallet(source: u64, destination: u64) -> Result<MockEconomy> {
        MockEconomy::with_accounts([
            ("source".to_string(), source),
            ("destination".to_string(), destination),
        ])
    }

    #[test]
    fn transfer_is_atomic_and_never_allows_a_negative_balance() -> Result<()> {
        let mut economy = wallet(10, 2)?;
        assert!(economy.transfer("t1", "source", "destination", 11).is_err());
        assert_eq!(economy.balance("source"), Some(10));
        assert_eq!(economy.balance("destination"), Some(2));
        assert!(economy.events().is_empty());
        Ok(())
    }

    #[test]
    fn exact_balance_transfer_succeeds_and_is_hash_chained() -> Result<()> {
        let mut economy = wallet(10, 2)?;
        assert_eq!(
            economy.transfer("t1", "source", "destination", 10)?,
            TransferOutcome::Applied
        );
        assert_eq!(economy.balance("source"), Some(0));
        assert_eq!(economy.balance("destination"), Some(12));
        assert!(MockEconomy::verify_events(economy.events())?);
        Ok(())
    }

    #[test]
    fn transfer_replay_is_idempotent_but_id_reuse_is_rejected() -> Result<()> {
        let mut economy = wallet(20, 0)?;
        assert_eq!(
            economy.transfer("once", "source", "destination", 5)?,
            TransferOutcome::Applied
        );
        assert_eq!(
            economy.transfer("once", "source", "destination", 5)?,
            TransferOutcome::Duplicate
        );
        assert!(economy
            .transfer("once", "source", "destination", 6)
            .is_err());
        assert_eq!(economy.balance("source"), Some(15));
        assert_eq!(economy.balance("destination"), Some(5));
        assert_eq!(economy.events().len(), 1);
        Ok(())
    }

    #[test]
    fn destination_overflow_does_not_debit_source() -> Result<()> {
        let mut economy = wallet(10, u64::MAX)?;
        assert!(economy.transfer("overflow", "source", "destination", 1).is_err());
        assert_eq!(economy.balance("source"), Some(10));
        assert_eq!(economy.balance("destination"), Some(u64::MAX));
        assert!(economy.events().is_empty());
        Ok(())
    }

    #[test]
    fn invalid_transfers_are_rejected_without_events() -> Result<()> {
        let mut economy = wallet(10, 0)?;
        assert!(economy.transfer("zero", "source", "destination", 0).is_err());
        assert!(economy.transfer("self", "source", "source", 1).is_err());
        assert!(economy.transfer("missing", "source", "other", 1).is_err());
        assert!(economy.transfer("bad id", "source", "destination", 1).is_err());
        assert!(economy.events().is_empty());
        Ok(())
    }

    #[test]
    fn hash_chain_detects_tampering_and_reordering() -> Result<()> {
        let mut economy = wallet(20, 0)?;
        economy.transfer("t1", "source", "destination", 3)?;
        economy.transfer("t2", "destination", "source", 1)?;
        let mut tampered = economy.events().to_vec();
        tampered[0].amount += 1;
        assert!(!MockEconomy::verify_events(&tampered)?);

        let mut reordered = economy.events().to_vec();
        reordered.swap(0, 1);
        assert!(!MockEconomy::verify_events(&reordered)?);
        assert!(MockEconomy::verify_events(economy.events())?);
        Ok(())
    }

    #[test]
    fn duplicate_and_unsafe_initial_accounts_are_rejected() {
        assert!(MockEconomy::with_accounts([
            ("same".to_string(), 1),
            ("same".to_string(), 2),
        ])
        .is_err());
        assert!(MockEconomy::with_accounts([("bad name".to_string(), 1), ("ok".to_string(), 2)])
            .is_err());
        assert!(MockEconomy::with_accounts([("only".to_string(), 1)]).is_err());
    }
}
