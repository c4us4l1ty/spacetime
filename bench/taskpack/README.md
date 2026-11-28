# Block 3 task pack — seeded candidate workspaces

The EconBench seed in `bench/econbench/` declares an oracle, a digest, and a domain `case_id`, then
runs an in-process verifier. What it never had was a **candidate**: a real workspace that compiles,
contains a plausible defect, and must be rejected by something that is not the candidate itself.

This directory is that missing piece. It is offline, dependency-free, and deterministic: plain
`rustc`, no crates, no network, no model call, no wallet provider, no service.

```
bench/taskpack/
  abi/wallet_abi.rs          pinned contract for the planted-invariant family
  abi/transform_abi.rs       pinned contract for the transmutation family
  oracle/wallet_oracle.rs    hidden model; never shipped inside a candidate
  oracle/transform_oracle.rs hidden differential grader
  corpus/transmutation.tsv   2,498 expected values computed with Python big integers
  reference/wallet_ref.rs    post-fix solution for the wallet family
  reference/transform_ref.rs post-fix solution for the transmutation family
  seeded/<seed>/             twenty candidates, each with exactly one planted bug
  assemble.sh                ABI + candidate -> one self-contained file
  run.sh                     grade one candidate, or the whole matrix
```

## The two families

**Planted invariant (7 tasks, `seeded/01..07_*`).** Each candidate is a fake-credit wallet that
satisfies the pinned ABI and compiles cleanly, but breaks one invariant: an overdraft is applied, an
exact balance is refused, a retry is not idempotent, a transfer ID is reused silently, an overflow
leaves the source debited, an invalid request is accepted, or a tampered hash chain still validates.
The oracle carries its own model of the specification, so a candidate cannot define its own success.

**Transmutation (7 tasks, `seeded/08..14_*`).** Each candidate implements seven integer functions —
`bit_reverse`, `byte_swap`, `rotate_left`, `rotate_right`, `saturating_add`, `checked_mul`, `parity`
— and got one of them wrong. The expectation comes from `corpus/transmutation.tsv`, whose values
were computed with Python's arbitrary-precision integers rather than by the Rust reference, so the
corpus is a genuinely independent second oracle. The grader replays every row and reports failures
per function.

## Why a hidden oracle

The usual failure mode of a self-graded task is that the grader imports the thing it grades. Here:

- `oracle/*.rs` never imports `seeded/` or `reference/`; the wallet oracle drives the candidate only
  through `crate::wallet::Wallet`, and the transform oracle only through the ABI's `dispatch`;
- `run.sh` assembles `ABI + candidate + oracle` into a fresh crate and compiles it with `rustc
  --edition 2021 -D warnings`;
- `assemble.sh` prepends the pinned ABI and refuses a candidate that embeds its own copy, uses
  `include!`, or lacks the required surface;
- every expectation in `run.sh` is a literal written next to the seed, not derived from the run.

## What CI proves

**Verified.** Workflow run `46170510950`, on `main` after the pack was merged, passes the whole CI
job, including this step. The branch run that was merged is `46170113816`; the earlier `46167417243`
verified the same twenty candidates before the hardening described at the end of
`docs/VERIFICATION-2026-11-28.md`, and `46166597322` before the closing documentation commit. The claim below is not aspirational: `cargo test --workspace --all-targets`
also runs `apps/spacetime-ctl/tests/taskpack.rs`, which assembles and grades every seed, and
`verify` grades `block3_task_pack_seeded_workspaces` with a toolchain present rather than skipping
it.

The **Block 3 task pack** step in `.github/workflows/ci.yml` runs `run.sh matrix`, which requires:

1. all three reference solutions to pass every check (wallet: 8 checks; transform: 7 functions;
   protocol: every captured trace);
2. every seeded candidate to fail *exactly* the check or corpus function its planted bug explains —
   not more, so a seed cannot hide a second defect, and not fewer, so a bug cannot escape;
3. `spacetime-ctl bench taskpack --out ...` to report `candidates = 20`,
   `seeded_bugs_caught = 20`, seven planted-invariant, seven transmutation and six
   protocol-distillation tasks, and `block3_done = false`;
4. `assemble.sh` to reject a file that is not a candidate;
5. the task pack not to smuggle a measured resource gate: `resource_gate` stays
   `"unmeasured_target_4gb"` and `performance_claims` stays `"unmeasured"`.

`verify` carries the same evidence as the `block3_task_pack_seeded_workspaces` gate. Because that
gate shells out to `rustc`, it reports `skipped` rather than `pass` when no toolchain is on `PATH`,
so a local run without Rust can never look like a green gate.

## Honest limits

- **Twenty tasks, but not yet a closed loop with EconBench.** The pack covers all three families —
  7 planted-invariant, 7 transmutation, 6 protocol-distillation — one seeded workspace per fixture.
  The EconBench fixtures still run their own in-process verifiers, so the two halves agree on count
  and coverage but are not yet wired to each other. `block3_done` stays `false` for that reason.
- **The protocol family grades traces, not a service.** A candidate replays a captured recording; it
  is never started, connected to, or timed. Passing means it reproduces the recorded outcomes, not
  that it would survive a hostile peer or a concurrent one.
- **Not the product workload.** This proves the task pack is independently gradeable. It is not the
  native mixed-workspace workload, so it does not feed cold-start, tab-switch, snapshot, or peak-RSS
  measurement.
- **No resource claim.** Nothing here measures anything. `resource_gate` and `performance_claims`
  remain `"unmeasured"`, `block3_done` remains `false`, and Block 4 stays gated.
- **Not a sandbox.** The grader runs trusted, checked-in files with plain `rustc`; it accepts no
  caller-supplied path, command, or model. It is a benchmark harness, not an execution boundary.

## Running it

```sh
# Whole matrix (needs a Rust toolchain; CI runs this on every push).
bench/taskpack/run.sh matrix

# One family or one seed.
bench/taskpack/run.sh reference wallet
bench/taskpack/run.sh candidate 03_wallet_idempotent_replay

# The machine-readable report.
cargo run --locked -p spacetime-ctl -- bench taskpack --out target/taskpack.json
```

The Rust tests in `apps/spacetime-ctl/tests/taskpack.rs` perform the same checks inside `cargo test`
and skip with a visible message when `rustc` is unavailable, so the suite stays usable without a
toolchain while CI remains authoritative.
