# ADR-0007 — Add an atomic, idempotent mock-economy core

Date: 2026-11-16
Status: implemented on `arena/3f609a12-spacetime`; strict CI passed at code head `3f99d91f6936895525dfa9a002b16a7cc53e2704` (run `43855570315`).

## Context

Block 3 has checked-in fake-credit ledger data and a mock bounty board, but no Rust-side transaction
primitive. The benchmark needs local economic events that cannot overdraw balances, partially apply,
or pay twice when a recorded request is retried. This must remain fake-credit-only and offline.

## Decision

Add `spacetime_bench::economy::MockEconomy` as a process-local library primitive:

- Require at least two distinct, bounded account IDs.
- Accept only positive, non-self transfers between existing accounts.
- Check source subtraction and destination addition before mutating either balance.
- Bind each transfer ID to a canonical request digest. Replaying an identical request is a no-op;
  reusing the ID with different arguments is rejected.
- Append a monotonically sequenced, SHA-256 hash-chained transfer event only after the full
  transfer passes validation.
- Verify event sequence, hash links, event hashes, and transfer-ID uniqueness.
- Exercise its invariants from `spacetime-ctl verify` as a separate end-to-end gate.

No persistence, bank/payment API, external account, or network access is added. The existing
checked-in JSONL mock-wallet ledger remains a separate durable fixture verified by `spacetime-ctl
ledger verify`.

## Consequences

- The mock economy now has executable atomicity, overdraft, idempotency, and tamper-detection
  invariants rather than documentation alone.
- The transfer events are process-local; they are not yet serialized into the existing generic
  hash-chained ledger.
- This is one local primitive, not the seven real planted-invariant tasks or a complete Block 3
  task runner. `resource_gate`, `performance_claims`, and `block3_done` remain unchanged.

## Validation

The library tests cover successful exact-balance transfers, overdraft rejection, destination
overflow, duplicate replay, ID reuse, malformed inputs, and event-chain tampering/reordering. The
end-to-end `mock_economy_invariants` gate checks applied and duplicate transfers, rejects an
overspend without state change, and verifies the resulting event chain. Strict push run `43855570315`
passed all 14 CI steps on code head `3f99d91f6936895525dfa9a002b16a7cc53e2704`, including format,
Clippy, workspace tests, release build, and end-to-end verification. The local environment does not
have the Rust toolchain.
