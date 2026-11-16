# ADR 0002 — Block 2 Crucible boundary

Date: 2026-11-15  
Status: merged to `main` via PR #8; 24 functional gates verified; 4 GB resource gate unmeasured

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
   differential reference, adversarial model, then mutation; the CLI prints the registry.
5. **Declarative execution profile.** Observations carry a digest of the fixed profile: fixed
   clock, RNG seed, locale, runner-owned temp-root policy, cleared environment, LD_PRELOAD socket
   denial, shim boundary label, and capability allow-list. This is not a kernel sandbox.
6. **Narrow LD_PRELOAD shim.** A C shim built by `spacetime-exec`'s build script fixes
   `clock_gettime`, `getrandom`, and `getentropy`, and denies `socket`, `socketpair`, and `connect`
   for dynamically linked local probes. Static binaries, direct syscalls, inherited descriptors,
   and direct `/dev/urandom` reads remain outside the boundary; namespaces/cgroups/Ark/quarantine
   microVMs remain later blocks.
7. **Capability allow-list.** `ScriptedRunner` rejects actions with a non-local capability scope,
   zero/over budget, tampered args digest, untrusted taint, missing expected verifier, or unsupported
   schema version before running them.
8. **Cheap metrology first.** Differential equality over 10^6 inputs and an 8-mutant mutation gate
   are pure Rust functions; no model, browser, VM, or external test suite is required.
9. **Mutation gating.** A candidate below the fixture mutation threshold is `FAIL`, not promoted.
10. **Flaky fixture family.** Clock, RNG, scheduler, and general flaky fixtures alternate observations
    so the multi-run rule classifies them as `ERROR`, never as a promotion.
11. **False-PASS metrology.** `false_pass_report` records verdict flip rate and expected-verdict leak
    rate over 50 same-snapshot reruns. An unstable operator is proven detectable.
12. **End-to-end verification.** `spacetime-ctl verify --out <file>` runs 24 offline gates covering
    ledger, replay, tamper, truncation, non-canonical, false-claim, timeout, incomplete-goal,
    Crucible, provenance, differential, mutation, control/shim probe, capability enforcement, panel,
    adversarial corpus, synthetic population, real fixture population, resource-gate evaluator, and
    machine-baseline checks.
13. **Conservative verifier panel.** `resolve_panel` requires all five independent sources to agree
    on `PASS`. `ERROR`, `UNVERIFIED`, `FAIL`, or any conflict blocks promotion; the highest-priority
    source supplies the recorded provenance.
14. **Offline adversarial corpus.** Twenty model-authored spec-only fixtures cover deterministic PASS,
    wrong-digest FAIL, trivial-mutation FAIL, flaky ERROR, and timeout ERROR cases. They are offline
    task files, not live model calls.
15. **Synthetic operator population.** `population_report` runs ten deterministic synthetic operator
    kinds 50 times each, detects unstable operators, and reports zero false-PASS leaks among promoted
    operators. It is explicitly labelled `real_operator_study = false`.
16. **Real fixture population.** `fixture_population_report` reruns 26 real declared-oracle fixtures
    50 times each (1,300 verifier calls), checks declared verdicts, and enforces the 1% false-PASS
    kill gate. It is a real fixture study, not a real operator study.
17. **Explicit resource gate.** `resource_gate` accepts an optional real measurement file, rejects
    CI/synthetic host labels and wrong target profiles, fails over-budget measurements, and otherwise
    remains `unmeasured`. It validates file shape and budget arithmetic, not physical provenance.
    No measurement file is committed.
18. **Locked CI.** `Cargo.lock` remains committed and every cargo invocation uses `--locked`.
19. **Bounded CLI inputs.** Reruns are limited to 50–10,000, operator counts to 5–10,000,
    differential limits to 1,000,000–10,000,000, and optional flags with missing values are errors.
    `crucible verify` also rejects a verdict that disagrees with the fixture's declared oracle.

## Consequences

- Block 2 adds no dependency, GUI, browser, VM, secrets, or live LLM calls.
- The end-to-end report labels `resource_gate = "unmeasured_target_4gb"` and
  `performance_claims = "unmeasured"`; CI host facts are not the target potato machine.
- Live separate-model adversarial suites, full operator-population studies on real operators, and
  the measured 4 GB resource gate remain later Block 2 work. The LD_PRELOAD shim is a local probe,
  not a complete security boundary.
- Under the First Law, Block 2 is not `DONE` until it works inside the measured target budget.
- The user explicitly requested the merge to `main`; PR #8 was merged after strict CI passed. The
  measured 4 GB resource gate remains outstanding, so this is a functional merge, not a resource
  sign-off.
