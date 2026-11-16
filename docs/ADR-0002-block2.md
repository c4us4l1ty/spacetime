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

## Addendum — bounded measurement intake (merged via PR #9)

- `verify --out <file> [--measurement <file>]` accepts one bounded file path; missing values,
  duplicate/unknown flags, control characters, overlarge files, unsupported keys, duplicate keys,
  and non-numeric measurements are rejected as unmeasured rather than crashing the report.
- Report fields `measurement_intake`, `measurement_evaluator`, `measurement_digest`, and
  `measurement_reasons` are derived from the evaluated file. A 25th `measurement_intake` gate
  self-tests the bounded cases.
- `resource_gate = "unmeasured_target_4gb"` and `performance_claims = "unmeasured"` remain.
  A within-budget file is `pass_shape_only`: file shape and budget arithmetic are not physical
  provenance, which must be recorded out of band before any measured gate claim.
- Strict CI run `43818080975` (job `133134609083`) verified 25 gates, including the bounded
  intake smoke cases.
- Bug hunt: `machine gate --measurement` is now bounded and prints `resource_gate=unmeasured`
  with `evaluator=pass_shape_only` / `fail_shape_only` / `unmeasured` instead of a shape-based
  `resource_gate=pass`; duplicate measurement keys, duplicate/unknown CLI flags, flag-like values,
  control characters, overlong values, and unknown `machine` arguments are rejected; unreadable
  measurement digests are `"unavailable"`. Strict CI run `43829073730` (job `133169726643`) and
  PR #9 check run `43829086354` (job `133169767434`) verified 25 gates with no diagnostics.
- Merged to `main` via PR #9 at explicit user request after strict CI passed. The measured 4 GB
  resource gate remains outstanding; this is a functional merge, not a resource sign-off.

## Addendum — measurement provenance kit (merged via PR #10)

- `bench/measurements/capture-host-facts.sh` collects host identity, OS/kernel, CPU, memory,
  swap/zram, filesystem, KVM, and cgroup facts read-only. It writes no measurement file and makes no
  performance claim; its output is out-of-band evidence only when captured on the exact target
  machine.
- `bench/measurements/PROVENANCE-TEMPLATE.md` is a human checklist for machine identity, raw
  evidence hashes, measurement definitions, operator/reviewer sign-off, and a bounded-file recipe.
  Filled copies and raw evidence stay out of band; no filled template is committed here.
- Strict CI checks the collector's shell syntax and template presence; it does not run the collector
  or treat CI runner host facts as target evidence. `verify --out` had 25 gates at that point and
  `machine gate --measurement` remains shape-only at most.
- `resource_gate = "unmeasured_target_4gb"` and `performance_claims = "unmeasured"` remain.
  `pass_shape_only` is not a measured gate; a later reviewed code path must consume accepted physical
  provenance before any measured gate can be reported.
- Strict push run `43831944305` verified 25 gates, all 14 CI steps, and the new kit checks; full
  evidence is recorded in `docs/VERIFICATION-2026-11-15.md`.

## Addendum — explicit target profiles (merged via PR #10)

- Public release does not mean every target is a 4 GB machine. `bench/targets/profiles.tsv`
  declares `4gb_potato` as the mandatory `baseline_gate`, plus `8gb_reference` and
  `16gb_developer` as `optional_profile` entries. A larger profile never satisfies or replaces the
  baseline.
- `capture-host-facts.sh` requires an explicit `--profile`; it never infers a profile from detected
  RAM. `PROVENANCE-TEMPLATE.md` records selected profile, machine identity, separate baseline
  evidence, and out-of-band sign-off.
- The CLI exposes `machine profiles`; `verify --out` adds a 26th `target_profile_registry` gate and
  report fields for selected profile, baseline profile, registry count, evaluated budget, and role.
  `machine gate` prints `release_gate=unmeasured` and `baseline_target_profile=4gb_potato`.
- `resource_gate = "unmeasured_target_4gb"` and `performance_claims = "unmeasured"` remain.
  `pass_shape_only` is not a measured gate; a later reviewed code path must consume accepted physical
  provenance before any measured gate can be reported. See `docs/ADR-0003-target-profiles.md`.
- Strict push run `43836410103` (job `133194022585`) verified 26 gates and all 14 CI steps at
  `6f4745e`; full evidence is recorded in `docs/VERIFICATION-2026-11-15.md`.
