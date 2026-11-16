# ADR-0003 — Explicit target profiles for a public release

Status: merged to `main` via PR #10 at `33149fe9ecac077bd1d37468c96197fefe94bca2`; this ADR does not claim a measured resource gate.

The target-profile increment was developed on `arena/3782c8f5-spacetime`, branched from
`90ceeb20db603c52e313ef718b82710241ad6080`.

## Context

The First Law makes the recorded 4 GB, weak-CPU, no-GPU potato machine the primary product
constraint. Earlier code and docs used `4gb_potato` as the only `TARGET_PROFILE`, and
`capture-host-facts.sh` described every host as that machine. The user plans to publish Spacetime
publicly and corrected that assumption: public targets are not all 4 GB machines. A larger machine
must not silently redefine the resource law, and a measurement on a larger machine must not be
mistaken for baseline evidence.

## Decision

1. **Keep `4gb_potato` as the mandatory release baseline.** It is the minimum gate from the First
   Law. No feature becomes release-ready by measuring only on a larger profile.
2. **Introduce an explicit registry** at `bench/targets/profiles.tsv`. It declares profile IDs,
   memory labels, memory/boot/tab-switch/CPU budgets, and roles. Initial profiles are
   `4gb_potato` (`baseline_gate`), `8gb_reference` (`optional_profile`), and `16gb_developer`
   (`optional_profile`).
3. **Never infer a profile from detected RAM.** Host facts are evidence about a machine, not a
   classification. `capture-host-facts.sh` requires `--profile <registered-id>`, rejects unknown
   profiles and flag-like values, and offers `--list-profiles` without collecting host facts.
4. **Evaluate measurement-file shape against the selected profile's budget.** The end-to-end
   report prints `target_profile`, `baseline_target_profile`, `target_profile_registry`,
   `evaluated_memory_budget_kb`, and `profile_role`. `machine gate` prints the same distinction.
5. **Preserve the unmeasured boundary.** `resource_gate`, `release_gate`, and
   `performance_claims` remain unmeasured. A within-budget file is `pass_shape_only`; physical
   provenance stays out of band and a later reviewed code path is required before any measured
   gate can be reported.
6. **Add a 26th end-to-end gate** `target_profile_registry`. It checks one baseline, three
   registered profiles, no duplicate IDs, non-decreasing budgets relative to the baseline, and a
   shape-only 8 GB profile evaluation that does not promote the baseline gate.
7. **Publication/licensing is out of scope.** This ADR changes the hardware target model only.
   The repository remains proprietary until an explicit legal/licensing decision is made.

## Consequences

- A public build can record evidence for 4 GB, 8 GB, and 16 GB machines without pretending they
  are the same target.
- The 4 GB baseline remains a hard release gate; optional profiles are additive.
- CI host facts remain non-target evidence. CI checks registry syntax, listing, argument
  rejection, report fields, and shape-only behavior; it never supplies physical provenance.
- `bench/machine.toml` uses `host_profile = "unspecified"` and points to the registry instead of
  assigning every host to `4gb_potato`.
- Strict CI with committed `Cargo.lock`, `--locked`, `clippy -D warnings`, and no
  `continue-on-error` remains authoritative. Diagnostic runs, if any, are not acceptance evidence.
- Strict push run `43836410103` (job `133194022585`) verified this increment at `6f4745e`:
  14/14 steps green, 26/26 gates passed, default baseline profile fields, and an 8 GB
  `pass_shape_only` smoke that preserved `resource_gate = "unmeasured_target_4gb"`.

## Non-goals

- No measured 4 GB, 8 GB, or 16 GB resource pass.
- No automatic hardware detection or dynamic hardware requirement.
- No replacement of the First Law baseline with a larger public reference machine.
- No open-source license, public release action, live backend, or Block 3 implementation.
