# ADR-0010: Seeded Candidate Workspaces for the Block 3 Task Pack

Date: 2026-11-28
Status: Implemented offline functional software. `resource_gate = "unmeasured_target_4gb"`,
`performance_claims = "unmeasured"`, and `block3_done = false` are unchanged.

## Context

`BUILD-PLAN.md` Block 3 asks for "20 local, machine-checkable tasks": seven planted-invariant tasks
(seeded bug in a small contract/lib; produce a passing local PoC), seven transmutation tasks
(reference implementation plus an empty skeleton; reach differential equality), and six
protocol-distillation tasks. `ADR-0008` added deterministic domain verifiers and `ADR-0009` wired
twenty `case_id`s into the EconBench report, but every one of those fixtures still ran through the
generic scripted runner. There was no candidate workspace, no compiled artefact, and no defect that
a candidate could plausibly ship.

`bench/econbench/README.md` and `docs/TARGET-MEASUREMENT-READINESS.md` both list the same
prerequisite before any target measurement can be defined: "implement the real Block 3 local
tasks". Measurement of cold start, tab switch, snapshot latency, and peak RSS needs a workload; the
workload needs the tasks.

Two hard constraints shaped the design. First, `static.rust-lang.org` and the Debian mirrors are not
reachable from the development sandbox, so the authoring environment has **no `rustc`**; GitHub
Actions remains the only compiler for this repository, as it has been since Block 1. Second, the
workspace is `unsafe_code = "forbid"` with a deliberately tiny dependency set (`serde`,
`serde_json`, `sha2`), and no Wasm runtime is available, so "reference implementation + empty
Rust/WASM skeleton" could not mean a Wasm build.

## Decision

Add `bench/taskpack/`: fourteen seeded candidate workspaces graded by a hidden, independent oracle,
built with plain `rustc` and no crates.

1. **Two pinned ABIs.** `abi/wallet_abi.rs` defines the fake-credit wallet surface
   (`CandidateEvent`, `TransferOutcome`, `WalletError`, `WalletOp`, `WalletObservation`, and the
   normative check order for `transfer`); `abi/transform_abi.rs` defines the seven integer functions
   and the corpus row format. `assemble.sh` prepends the ABI to a candidate, so a candidate is
   checked in *without* a private copy of the contract and cannot drift from it.

2. **Hidden oracles that never import a candidate.** `oracle/wallet_oracle.rs` re-implements the
   specification as its own model and compares complete observations after every operation.
   `oracle/transform_oracle.rs` replays `corpus/transmutation.tsv` and reports failures per function.
   Both expose named checks, so a failing seed is attributable to exactly one invariant.

3. **Seeded bugs are single-fault by construction.** Each `seeded/<seed>/` candidate compiles
   cleanly and breaks exactly one thing: wrapping subtraction on an overdraft; an inclusive balance
   boundary; a skipped idempotency guard; a reuse check that accepts any known ID; a debit committed
   before the overflow check; an unreachable zero-amount guard; chain validation that never
   recomputes a hash; a 63-step bit reversal; a byte swap that drops the low byte; a rotate reduced
   modulo 32; an inverted rotation direction; a wrapping `saturating_add`; a `checked_mul` that
   narrows away overflow; an inverted parity predicate.

4. **The transmutation corpus is a genuinely independent oracle.** Its 2,498 expected values were
   generated with Python's arbitrary-precision integers, not by the Rust reference, so a mistake in
   the reference cannot silently define correctness. The reference solution is written
   structurally (bit loops, widen-then-narrow arithmetic) and validated against the corpus.

5. **Grading is pinned literal-by-literal.** `run.sh` states, next to each seed, the one check or
   corpus function it must fail — and requires that nothing else fails, so a seed cannot hide a
   second defect. Both reference solutions must pass everything.

6. **CI owns the verdict.** A new workflow step runs `run.sh matrix`,
   `spacetime-ctl bench taskpack`, and three negative checks. `verify` gains a
   `block3_task_pack_seeded_workspaces` gate (gate count 39 → 40) that reports `skipped` rather than
   `pass` when `rustc` is absent, so a toolchain-less local run cannot look green. A Rust test
   (`apps/spacetime-ctl/tests/taskpack.rs`) enforces the same invariants inside `cargo test`.

## Consequences

- Block 3 now has real, compiled candidate workspaces with planted defects, which is what its
  "definition of done" was missing for the 7 + 7 non-protocol tasks.
- The task pack is functional evidence only. It measures nothing, so `resource_gate`,
  `performance_claims`, and `block3_done` are untouched, and Block 4 stays gated.
- Six `protocol_*` fixtures still lack a seeded workspace; that is the remaining half of Block 3.
- The grader is a benchmark harness, not an execution boundary: it runs trusted checked-in files
  with `rustc` and accepts no caller-supplied path, command, or model.
- Because CI is the only compiler, the first green run is the compile proof for every seeded file.
