# ADR 0002 — Block 2 Crucible boundary

Date: 2026-11-15  
Status: in progress; functional verification command added

## Context

Block 1 is merged to `main` via PR #7. `BUILD-PLAN.md` Block 2 asks for determinism and independent
verifiers before any higher-level product work. Resource usage remains the First Law; all
performance numbers stay unmeasured until the 4 GB target machine is recorded.

## Decisions

1. **Keep Block 1's `FixtureVerifier`** for ledger claims; add a separate `CandidateVerifier`
   contract for `verify(candidate, fixture)`.
2. **Configurable determinism runs.** `CrucibleVerifier` requires at least two runs from the same
   snapshot. Any status or observation disagreement becomes `ERROR`, never `PASS`.
3. **Independent provenance.** `VerifierProof` records verifier id, source, source priority,
   evidence digest, execution profile digest, determinism run count, and mutation score.
4. **Verifier-source priority.** The accepted order is fixture suite, published spec/oracle,
   differential reference, adversarial model, then mutation; the CLI can print the registry.
5. **Declarative execution profile.** Observations carry a digest of the fixed scripted profile:
   fixed clock, RNG seed, locale, runner-owned temp-root policy, cleared environment, and no network
   by construction. This is not a kernel sandbox and not a clock/RNG/socket shim.
6. **Cheap metrology first.** Differential equality over 10^6 inputs and an 8-mutant mutation gate
   are pure Rust functions; no model, browser, VM, or external test suite is required.
7. **Mutation gating.** A candidate below the fixture mutation threshold is `FAIL`, not promoted.
   This rejects model-written trivial tests until they catch deliberate mutants.
8. **Flaky fixture family.** Clock, RNG, scheduler, and general flaky fixtures alternate observations
   so the multi-run rule classifies them as `ERROR`, never as a promotion.
9. **False-PASS metrology.** `false_pass_report` records verdict flip rate and expected-verdict leak
   rate over 50 same-snapshot reruns. An unstable operator is proven detectable.
10. **End-to-end verification.** `spacetime-ctl verify --out <file>` runs ledger, replay, tamper,
   truncation, non-canonical, false-claim, timeout, incomplete-goal, Crucible, provenance,
   differential, mutation, and machine-baseline gates in one command.
11. **Locked CI.** `Cargo.lock` remains committed and every cargo invocation uses `--locked`.

## Consequences

- Block 2 adds no dependencies, GUI, browser, VM, secrets, or live LLM calls.
- The end-to-end report labels `resource_gate = "unmeasured_target_4gb"` and
  `performance_claims = "unmeasured"`; CI host facts are not the target potato machine.
- Real clock/getrandom/socket shims, live model-written adversarial tests, full operator-population
  studies on real fixtures, and the measured 4 GB resource gate remain later Block 2 work.
- Under the First Law, Block 2 is not `DONE` until it works inside the measured target budget.
