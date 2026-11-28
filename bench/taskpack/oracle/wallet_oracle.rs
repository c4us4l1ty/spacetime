//! Hidden independent oracle for the Block 3 fake-credit wallet task family.
//!
//! This file is never shipped inside a candidate workspace. `bench/taskpack/run.sh` prepends the
//! pinned ABI, appends exactly one candidate `wallet.rs`, and compiles the result; the oracle then
//! drives `wallet::Wallet` only through the pinned ABI. A broken or hostile candidate therefore
//! cannot influence the expectation it is graded against.
//!
//! Every check is independent: it builds a fresh wallet, so one failing invariant cannot mask
//! another. Output is machine-readable:
//!
//! ```text
//! check <name> pass|fail <detail>
//! oracle failed_checks=<n> failed=<comma-separated names>
//! ```
//!
//! Exit status is zero only when every check passes.

use crate::wallet::Wallet;
use std::panic::{catch_unwind, AssertUnwindSafe};

const FUNDED: [(&str, u64); 8] = [
    ("alice", 100),
    ("bob", 50),
    ("bounty-board", 10),
    ("carol", 25),
    ("dave", 0),
    ("absent", 0),
    ("overflow-sink", u64::MAX),
    ("underflow", 0),
];

fn funded() -> Wallet {
    Wallet::new(&FUNDED).expect("the shared ABI allocation must be accepted")
}

type Failure = String;

fn require(condition: bool, message: &str) -> Result<(), Failure> {
    if condition {
        Ok(())
    } else {
        Err(message.to_string())
    }
}

fn balance_is(wallet: &Wallet, account: &str, expected: u64) -> Result<(), Failure> {
    match Wallet::balance(wallet, account) {
        Some(actual) if actual == expected => Ok(()),
        other => Err(format!(
            "balance({account}) = {other:?}, expected {expected}"
        )),
    }
}

fn observed(wallet: &Wallet) -> WalletObservation {
    let events = wallet.events();
    observe(
        |account| Wallet::balance(wallet, account),
        events,
        Wallet::verify_events(events),
        None,
    )
}

/// A clean chain must validate, and the observation must agree with the events.
fn require_clean_chain(wallet: &Wallet) -> Result<(), Failure> {
    let events = wallet.events();
    require(
        Wallet::verify_events(events),
        "the candidate rejected its own clean event chain",
    )?;
    let observation = observed(wallet);
    require(
        observation.last_hash_ok && observation.events_verified,
        format!("a clean chain observed as {observation:?}"),
    )?;
    require(
        observation.event_count == events.len() as u64,
        "event count disagreed with the event list",
    )
}

fn constructor_rejections() -> Result<(), Failure> {
    require(
        Wallet::new(&[("alice", 1_u64), ("alice", 2)]).err() == Some(WalletError::DuplicateAccount),
        "duplicate accounts were accepted",
    )?;
    require(
        Wallet::new(&[("alice", 1_u64)]).err() == Some(WalletError::TooFewAccounts),
        "a one-account wallet was accepted",
    )?;
    require(
        Wallet::new(&[("", 1_u64), ("bob", 1)]).err() == Some(WalletError::EmptyIdentifier),
        "an empty account identifier was accepted",
    )
}

/// A transfer for exactly the source balance must be allowed and must empty the source.
fn exact_balance_is_allowed() -> Result<(), Failure> {
    let mut wallet = funded();
    let outcome = Wallet::transfer(&mut wallet, "exact", "alice", "bob", 100);
    require(
        outcome == Ok(TransferOutcome::Applied),
        format!("spending the whole balance returned {outcome:?}"),
    )?;
    drop(outcome);
    balance_is(&wallet, "alice", 0)?;
    balance_is(&wallet, "bob", 150)?;
    require(wallet.events().len() == 1, "the exact balance added no event")?;
    require_clean_chain(&wallet)
}

/// Spending more than the source balance must be rejected and must not move anything.
fn overdraft_is_rejected() -> Result<(), Failure> {
    let mut wallet = funded();
    let before = observed(&wallet);
    let outcome = Wallet::transfer(&mut wallet, "overdraft", "alice", "bob", 101);
    require(
        outcome == Err(WalletError::Overdraft),
        format!("an overdraft returned {outcome:?}"),
    )?;
    drop(outcome);
    require(
        observed(&wallet) == before,
        "a rejected overdraft mutated wallet state",
    )?;
    // The same rejection applies to an account that exists but holds nothing.
    let underfunded = Wallet::transfer(&mut wallet, "underfunded", "underflow", "alice", 1);
    require(
        underfunded == Err(WalletError::Overdraft),
        format!("an underfunded transfer returned {underfunded:?}"),
    )
}

/// Repeating an identical request must be an idempotent duplicate.
fn duplicate_retry_is_idempotent() -> Result<(), Failure> {
    let mut wallet = funded();
    let first = Wallet::transfer(&mut wallet, "retry", "alice", "bob", 25);
    require(
        first == Ok(TransferOutcome::Applied),
        format!("the first transfer returned {first:?}"),
    )?;
    let retry = Wallet::transfer(&mut wallet, "retry", "alice", "bob", 25);
    require(
        retry == Ok(TransferOutcome::Duplicate),
        format!("the identical retry returned {retry:?}"),
    )?;
    drop(retry);
    balance_is(&wallet, "alice", 75)?;
    balance_is(&wallet, "bob", 75)?;
    require(
        wallet.events().len() == 1,
        format!(
            "the duplicate retry appended {} event(s)",
            wallet.events().len()
        ),
    )?;
    require_clean_chain(&wallet)
}

/// Reusing an applied transfer ID with different arguments must be rejected.
fn transfer_id_reuse_is_rejected() -> Result<(), Failure> {
    let mut wallet = funded();
    let first = Wallet::transfer(&mut wallet, "reuse", "alice", "dave", 10);
    require(
        first == Ok(TransferOutcome::Applied),
        format!("the first transfer returned {first:?}"),
    )?;
    let before = observed(&wallet);
    let changed_amount = Wallet::transfer(&mut wallet, "reuse", "alice", "dave", 11);
    require(
        changed_amount == Err(WalletError::TransferIdReuse),
        format!("reusing an ID with a new amount returned {changed_amount:?}"),
    )?;
    let changed_destination = Wallet::transfer(&mut wallet, "reuse", "alice", "bob", 10);
    require(
        changed_destination == Err(WalletError::TransferIdReuse),
        format!("reusing an ID with a new destination returned {changed_destination:?}"),
    )?;
    drop(changed_amount);
    drop(changed_destination);
    require(
        observed(&wallet) == before,
        "a rejected transfer-ID reuse mutated wallet state",
    )
}

/// A destination overflow must be rejected without debiting the source.
fn overflow_is_atomic() -> Result<(), Failure> {
    let mut wallet = funded();
    let before = observed(&wallet);
    let outcome = Wallet::transfer(&mut wallet, "overflow", "alice", "overflow-sink", 1);
    require(
        outcome == Err(WalletError::DestinationOverflow),
        format!("a destination overflow returned {outcome:?}"),
    )?;
    drop(outcome);
    balance_is(&wallet, "alice", 100)?;
    balance_is(&wallet, "overflow-sink", u64::MAX)?;
    require(
        wallet.events().is_empty(),
        "a rejected overflow recorded an event",
    )?;
    require(
        observed(&wallet) == before,
        "a rejected overflow mutated wallet state",
    )
}

/// Zero amounts, self transfers, and unknown accounts must all be rejected without side effects.
fn invalid_requests_are_rejected() -> Result<(), Failure> {
    let mut wallet = funded();
    let before = observed(&wallet);
    let zero = Wallet::transfer(&mut wallet, "zero", "alice", "bob", 0);
    let self_transfer = Wallet::transfer(&mut wallet, "self", "alice", "alice", 1);
    let missing = Wallet::transfer(&mut wallet, "missing", "alice", "never-opened", 1);
    let empty_id = Wallet::transfer(&mut wallet, "", "alice", "bob", 1);
    let long_id = Wallet::transfer(&mut wallet, &"x".repeat(65), "alice", "bob", 1);
    require(zero == Err(WalletError::ZeroAmount), format!("a zero transfer returned {zero:?}"))?;
    require(
        self_transfer == Err(WalletError::SameAccount),
        format!("a self transfer returned {self_transfer:?}"),
    )?;
    require(
        missing == Err(WalletError::MissingAccount),
        format!("an unknown destination returned {missing:?}"),
    )?;
    require(
        empty_id == Err(WalletError::EmptyIdentifier),
        format!("an empty transfer ID returned {empty_id:?}"),
    )?;
    require(
        long_id == Err(WalletError::IdentifierTooLong),
        format!("an oversize transfer ID returned {long_id:?}"),
    )?;
    // Drop every request result before touching `wallet` again: a `Result` holding a
    // candidate-owned `String` would otherwise keep the wallet mutably borrowed.
    drop(zero);
    drop(self_transfer);
    drop(missing);
    drop(long_id);
    require(
        observed(&wallet) == before,
        "a rejected invalid request mutated wallet state",
    )
}

/// A rewritten event hash must invalidate the chain, including on the final event.
fn tamper_is_detected() -> Result<(), Failure> {
    let mut wallet = funded();
    Wallet::transfer(&mut wallet, "one", "alice", "bob", 3).expect("setup transfer failed");
    Wallet::transfer(&mut wallet, "two", "bob", "alice", 1).expect("setup transfer failed");
    require_clean_chain(&wallet)?;
    require(
        !wallet.tamper_event_hash(2),
        "tampering an out-of-range event reported success",
    )?;
    let last = wallet.events().len() - 1;
    let original = wallet.events()[last].hash;
    require(
        wallet.tamper_event_hash(last),
        "tampering an existing event reported failure",
    )?;
    require(
        wallet.events()[last].hash != original,
        "the tamper hook did not change the stored hash",
    )?;
    let events = wallet.events();
    require(
        !Wallet::verify_events(events),
        "a rewritten event hash was accepted as a valid chain",
    )?;
    let observation = observed(&wallet);
    require(
        !observation.last_hash_ok,
        "the observation did not expose the rewritten hash",
    )?;
    require(
        !observation.events_verified,
        "the observation reported a tampered chain as verified",
    )
}

fn main() {
    // Candidate panics are recorded as check failures instead of aborting the harness.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let checks: [(&str, fn() -> Result<(), Failure>); 8] = [
        ("constructor_rejections", constructor_rejections),
        ("exact_balance_is_allowed", exact_balance_is_allowed),
        ("overdraft_is_rejected", overdraft_is_rejected),
        ("duplicate_retry_is_idempotent", duplicate_retry_is_idempotent),
        (
            "transfer_id_reuse_is_rejected",
            transfer_id_reuse_is_rejected,
        ),
        ("overflow_is_atomic", overflow_is_atomic),
        ("invalid_requests_are_rejected", invalid_requests_are_rejected),
        ("tamper_is_detected", tamper_is_detected),
    ];

    let mut failed: Vec<&'static str> = Vec::new();
    for (name, check) in checks {
        let outcome = catch_unwind(AssertUnwindSafe(check));
        match outcome {
            Ok(Ok(())) => println!("check {name} pass"),
            Ok(Err(detail)) => {
                println!("check {name} fail {detail}");
                failed.push(name);
            }
            Err(_) => {
                println!("check {name} fail panicked");
                failed.push(name);
            }
        }
    }

    std::panic::set_hook(previous_hook);
    println!(
        "oracle failed_checks={} failed={}",
        failed.len(),
        failed.join(",")
    );
    if !failed.is_empty() {
        std::process::exit(1);
    }
}
