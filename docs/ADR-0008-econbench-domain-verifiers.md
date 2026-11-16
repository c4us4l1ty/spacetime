# ADR-0008 — Add deterministic domain-case verifiers to EconBench

Date: 2026-11-16
Status: implemented; CI validation pending.

## Context

The 20 checked-in EconBench task manifests were all driven through a generic scripted action path.
The mock-economy primitive and independent differential oracle existed, but the EconBench report did
not invoke them per task. Task metadata also did not identify a distinct, machine-checkable domain
case.

## Decision

Register a deterministic domain-case verifier in `spacetime-bench` and require each of the 20 task
manifests to select exactly one case. The offline runner executes that verifier alongside its
existing scripted ledger task and adds the case ID, verifier ID, check count, and a stable evidence
digest to each report item. It rejects missing cases and verifier-source metadata that does not
match the selected verifier.

The case pack currently covers seven mock-wallet invariants, seven independent `u64`
transformation comparisons, and six locally specified protocol state-machine traces. The protocol
cases are executable contract traces in source, not claims of externally captured or real-service
traces. The transformation cases compare independent reference algorithms with Rust primitives;
they are not WASM task skeletons or candidate code-repair tasks.

## Consequences

- The existing EconBench run now produces domain-level evidence for all 20 manifest entries and
  refuses duplicate-free-count regressions only where covered by the runner test (each fixture must
  report a nonempty verifier result).
- The additional checks remain deterministic, offline, and independent of model output or network
  services. No real wallet, account, target, or payment path is introduced.
- Existing scripted actions remain placeholders; this ADR does not claim the full planted-bug task
  pack, native mixed-workspace workload, physical target measurements, or accepted provenance.
- `resource_gate = "unmeasured_target_4gb"`, `performance_claims = "unmeasured"`, and
  `block3_done = false` remain mandatory.

## Validation

Manifest audit confirmed 20 unique case IDs with offline network metadata and verifier-source labels
matching the registered verifier families. Rust formatting and `git diff --check` passed locally;
the local environment has no Rust toolchain. Strict CI compilation, tests, and end-to-end EconBench
validation are pending.
