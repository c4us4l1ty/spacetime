//! CANDIDATE EVENT ABI — pinned for the Block 3 fake-credit wallet task family.
//!
//! `bench/taskpack/assemble.sh` prepends this file byte-identically to every candidate, every
//! reference solution, and every oracle copy, so the contract cannot drift between them. It models
//! fake credits only: no payment provider, account, network, or persistence.
//!
//! ## Required candidate surface
//!
//! The candidate `wallet::Wallet` type must expose exactly these inherent methods:
//!
//! ```text
//! fn new(accounts: &[(&str, u64)]) -> Result<Wallet, WalletError>
//! fn balance(&self, account: &str) -> Option<u64>
//! fn transfer(&mut self, transfer_id: &str, from: &str, to: &str, amount: u64)
//!     -> Result<TransferOutcome, WalletError>
//! fn events(&self) -> &[CandidateEvent]
//! fn tamper_event_hash(&mut self, index: usize) -> bool
//! fn verify_events(events: &[CandidateEvent]) -> bool
//! ```
//!
//! ## Request check order (normative)
//!
//! `transfer` validates in this order, so a single-fault request always yields one error:
//!
//! 1. `transfer_id` — `EmptyIdentifier`, then `IdentifierTooLong`
//! 2. `from` and `to` — `EmptyIdentifier`, then `IdentifierTooLong`
//! 3. source equal to destination — `SameAccount`
//! 4. `amount == 0` — `ZeroAmount`
//! 5. an already-applied `transfer_id` — `Duplicate` for identical arguments,
//!    `TransferIdReuse` for different arguments
//! 6. unknown account — `MissingAccount`
//! 7. insufficient source balance — `Overdraft`
//! 8. destination overflow — `DestinationOverflow`
//!
//! A rejected request must leave balances, applied transfer IDs, and events unchanged.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateEvent {
    pub sequence: u64,
    pub transfer_id: String,
    pub from_account: String,
    pub to_account: String,
    pub amount: u64,
    pub previous_hash: u64,
    pub hash: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferOutcome {
    Applied,
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletError {
    EmptyIdentifier,
    IdentifierTooLong,
    DuplicateAccount,
    TooFewAccounts,
    SameAccount,
    ZeroAmount,
    TransferIdReuse,
    MissingAccount,
    Overdraft,
    DestinationOverflow,
}

impl WalletError {
    pub fn as_str(&self) -> &'static str {
        match self {
            WalletError::EmptyIdentifier => "empty_identifier",
            WalletError::IdentifierTooLong => "identifier_too_long",
            WalletError::DuplicateAccount => "duplicate_account",
            WalletError::TooFewAccounts => "too_few_accounts",
            WalletError::SameAccount => "same_account",
            WalletError::ZeroAmount => "zero_amount",
            WalletError::TransferIdReuse => "transfer_id_reuse",
            WalletError::MissingAccount => "missing_account",
            WalletError::Overdraft => "overdraft",
            WalletError::DestinationOverflow => "destination_overflow",
        }
    }
}

/// One request the independent oracle can issue against a candidate wallet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletOp {
    Transfer {
        transfer_id: String,
        from: String,
        to: String,
        amount: u64,
    },
    /// Transfer once, then repeat the identical request under the same transfer ID.
    TransferTwice {
        transfer_id: String,
        from: String,
        to: String,
        amount: u64,
    },
    /// Transfer once, then reuse the same transfer ID with a different amount.
    TransferThenReuse {
        transfer_id: String,
        from: String,
        to: String,
        amount: u64,
        reuse_amount: u64,
    },
    /// Transfer once, then repeat it to a different destination under the same transfer ID.
    TransferThenReroute {
        transfer_id: String,
        from: String,
        to: String,
        amount: u64,
        other_to: String,
    },
    /// Rewrite the hash of the last recorded event, then re-verify the chain. The last event has no
    /// successor, so only hash recomputation inside `verify_events` can detect this.
    TamperLastEvent,
}

/// Normalized, comparable observation after one operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletObservation {
    /// Balances that exist, sorted by account name.
    pub sorted_balances: Vec<(String, u64)>,
    pub event_count: u64,
    pub last_hash_ok: bool,
    pub events_verified: bool,
    pub error: Option<&'static str>,
}

pub const MAX_IDENTIFIER_BYTES: usize = 64;
pub const GENESIS_HASH: u64 = 0;

/// Accounts the oracle may inspect. Anything absent from the candidate is simply not reported.
pub const OBSERVED_ACCOUNTS: [&str; 8] = [
    "absent",
    "alice",
    "bob",
    "bounty-board",
    "carol",
    "dave",
    "overflow-sink",
    "underflow",
];

pub fn validate_identifier(value: &str) -> Result<(), WalletError> {
    if value.is_empty() {
        return Err(WalletError::EmptyIdentifier);
    }
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(WalletError::IdentifierTooLong);
    }
    Ok(())
}

/// Linear-time mix of the documented event fields. Defined in the shared ABI so the hash rule is
/// never ambiguous; the `u64` wraparound is part of the specification.
pub fn event_mix(
    sequence: u64,
    transfer_id: &str,
    from_account: &str,
    to_account: &str,
    amount: u64,
    previous_hash: u64,
) -> u64 {
    let mut accumulator: u64 = 0x5350_4143_4554_494d;
    let mut absorb = |value: u64| {
        accumulator = accumulator.rotate_left(7) ^ value;
        accumulator = accumulator.wrapping_mul(0x0000_0100_0000_01b3);
    };
    absorb(sequence);
    absorb(amount);
    absorb(previous_hash);
    for byte in transfer_id.bytes() {
        absorb(u64::from(byte));
    }
    absorb(0x2f);
    for byte in from_account.bytes() {
        absorb(u64::from(byte));
    }
    absorb(0x2f);
    for byte in to_account.bytes() {
        absorb(u64::from(byte));
    }
    accumulator
}

/// Build one event with its hash already filled in.
pub fn make_event(
    sequence: u64,
    transfer_id: &str,
    from_account: &str,
    to_account: &str,
    amount: u64,
    previous_hash: u64,
) -> CandidateEvent {
    CandidateEvent {
        sequence,
        transfer_id: transfer_id.to_string(),
        from_account: from_account.to_string(),
        to_account: to_account.to_string(),
        amount,
        previous_hash,
        hash: event_mix(
            sequence,
            transfer_id,
            from_account,
            to_account,
            amount,
            previous_hash,
        ),
    }
}

/// Hash an existing event from its own fields, for negative tests and chain validation.
pub fn recomputed_event_hash(events: &[CandidateEvent], index: usize) -> Option<u64> {
    let event = events.get(index)?;
    Some(event_mix(
        event.sequence,
        &event.transfer_id,
        &event.from_account,
        &event.to_account,
        event.amount,
        event.previous_hash,
    ))
}

/// Normalize a wallet into a comparable observation.
pub fn observe(
    balance: impl Fn(&str) -> Option<u64>,
    events: &[CandidateEvent],
    events_verified: bool,
    error: Option<WalletError>,
) -> WalletObservation {
    let mut sorted_balances: Vec<(String, u64)> = OBSERVED_ACCOUNTS
        .iter()
        .filter_map(|account| balance(account).map(|value| ((*account).to_string(), value)))
        .collect();
    sorted_balances.sort();
    let last_hash_ok = match events.len().checked_sub(1) {
        Some(last) => recomputed_event_hash(events, last) == Some(events[last].hash),
        None => true,
    };
    WalletObservation {
        sorted_balances,
        event_count: u64::try_from(events.len()).unwrap_or(u64::MAX),
        last_hash_ok,
        events_verified,
        error: error.map(WalletError::as_str),
    }
}
