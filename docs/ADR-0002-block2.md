# ADR 0002 — Block 2 Crucible boundary

Date: 2026-11-15  
Status: in progress

## Context

Block 1 is merged to `main`. `BUILD-PLAN.md` Block 2 asks for determinism and independent
verifiers before any higher-level product work. Resource usage remains the First Law; all
performance numbers stay unmeasured until the 4 GB target machine is recorded.

## Decisions

1. **Keep Block 1's `FixtureVerifier`** for ledger claims; add a separate `CandidateVerifier`
   contract for `verify(candidate, fixture)`.
2. **Two-run determinism rule.** `CrucibleVerifier` runs a candidate twice from the same
   snapshot. Any status or observation digest disagreement becomes `ERROR`, never `PASS`.
3. **Independent provenance.** `VerifierProof` records a `VerifierSource`; every `PASS` carries
   an evidence digest, verifier id, source, determinism run count, and mutation score.
4. **Cheap metrology first.** Differential equality over 10^6 inputs and an 8-mutant mutation
   gate are pure Rust functions; no model, browser, VM, clock shim, or RNG shim is required yet.
5. **Flaky fixtures.** `ActionOp::Flaky` produces alternating observations so the double-run
   rule classifies it as `ERROR`. A separate `UnstableVerifier` proves the false-PASS flip metric.
6. **Locked CI.** `Cargo.lock` remains committed and every cargo invocation uses `--locked`.

## Consequences

- Block 2 starts without adding dependencies, GUI, browser, VM, secrets, or LLM calls.
- Real fixed-clock/getrandom/socket shims, model-written adversarial tests, 50-rerun operator
  studies on real fixtures, and the measured 4 GB resource gate remain later Block 2 work.
- `bench/machine.toml` still labels performance claims unmeasured; CI host facts are not the
  target potato machine.
