# Verification — Block 3 seeded candidate task pack — 2026-11-28

## Authoritative result

- Branch: `arena/cc36730c-spacetime`, branched from `c10451ec7fdf480e0f60d195c9061315920b73e5` of
  `main`.
- Scope: `bench/taskpack/` (twenty seeded candidate workspaces, three pinned ABIs, three hidden
  oracles, a 2,498-row independent differential corpus, a 21-trace captured protocol recording,
  three reference solutions), the
  `spacetime-ctl bench taskpack` command, the `block3_task_pack_seeded_workspaces` verify gate
  (gates 39 → 40), the CI task-pack step, and `apps/spacetime-ctl/tests/taskpack.rs`.
- Governing decision: `docs/ADR-0010-block3-seeded-task-pack.md`.

## What was verified, and how

The development sandbox for this branch has **no Rust toolchain**: `static.rust-lang.org`,
`sh.rustup.rs`, `index.crates.io`, `static.crates.io`, and the Debian mirrors are all unreachable
from it, and no `rustc` is preinstalled. As in every earlier block, GitHub Actions is the compiler
and the authoritative verifier.

**Status: verified at twenty.** Workflow run `46166597322` on `arena/cc36730c-spacetime` is green
end to end: all twenty steps pass, `bench taskpack` reports twenty candidates with twenty planted
bugs caught, and the end-to-end report shows `gates = 40` and `failed = 0`.

The first verified run, `46160941738`, covered the fourteen-seed pack before the protocol family
was added. Both are recorded because the second one found defects the first could not.
Steps 1–20 all pass, including `Format check`, `Clippy`, `Tests`, `Release build`, the
**Block 3 task pack** step, **End-to-end verification** (40 gates, 0 failed), `Measurement intake
smoke`, `Block 3 offline console and EconBench`, and `Machine baseline`. The `run.sh matrix` output
records both references passing and all 14 seeds caught by their pinned signal.

Until this run, CI could not compile the pack at all. Making it compile required finding defects
that no amount of source review had caught — each is now fixed and covered by the run above:

| Defect | Why CI alone could see it |
| --- | --- |
| `Vec::values()` on a `Vec` in the report | type error, now `.iter().map(\|(_, rows)\| rows.0).sum()` |
| the bash scripts were invoked through `sh` (dash) | `${BASH_SOURCE[0]}` and `[[ ]]` failed at parse time |
| candidate paths arrive through `..` from the manifest dir | the layout `case` never matched, though the file existed |
| appended files carried `//!` inner-doc headers | `E0753`, inner doc comments are only legal first in a crate |
| rustc inferred the crate name from `<label>.grader.rs` | dots are illegal in a crate name |
| the pinned ABI still declared the abandoned `WalletOp` | dead-code error under the graders' denied warnings |
| candidates could not see the ABI at the crate root | `E0425`, they now import exactly what they use |
| `Option::map(WalletError::as_str)` | `map` passes an owned error, `as_str` takes a reference |
| `require` took `&str` while call sites passed `format!` | `E0308`, it now takes `impl Into<String>` |
| a test asserted one row count for every corpus function | the corpus ranges from 216 to 488 rows per function |

The lesson is worth recording: the Python pairings proved the *intent* of every seed, and none of
that caught any of these ten defects. A seed pack that has never been compiled is not graded task
material, whatever its transcripts say.

Because CI is the only compiler, the seed/expectation pairings were additionally checked
**independently of the Rust implementation** before the first CI run:

1. **Wallet family — model replay.** A Python transcription of the oracle's model, its eight checks,
   and all seven seeded defects was run against the same operations. The reference configuration
   passed every check; each seeded configuration failed exactly its own check and nothing else.
2. **Transmutation family — corpus cross-check.** The corpus expected values were generated with
   Python's arbitrary-precision integers, not with the Rust reference. All 2,498 rows are matched by
   the reference formulas, and each of the seven mutated implementations (transcribed from the
   seeded `func.rs` files) breaks exactly its own function and no other.

These two checks prove the *pairings*. They cannot prove the Rust sources compile; that is what CI
does, and why `run.sh matrix`, `bench taskpack`, and `cargo test` all run inside the workflow.

## What CI must assert

The **Block 3 task pack** step requires, in order:

1. `run.sh matrix` — all three reference solutions pass every check (8 wallet checks, 7 corpus
   functions, 21 captured traces); each of the 20 seeds fails exactly the check, corpus function or
   trace set named beside it in `run.sh`;
2. `spacetime-ctl bench taskpack --out target/taskpack.json` — `"candidates": 20`,
   `"seeded_bugs_caught": 20`, `"protocol_tasks": 6`, `"block3_done": false`, and a
   `"wallet_checks_without_a_seed": ["constructor_rejections"]` field that names the one graded
   check carrying no seed of its own;
3. `assemble.sh` rejects `abi/wallet_abi.rs` (not a candidate) and `run.sh` rejects an unknown seed;
4. the end-to-end step then runs `verify` with a toolchain present, so
   `block3_task_pack_seeded_workspaces` is graded, not skipped, and the report must show
   `gates = 40`.


## The protocol family

The six protocol-distillation tasks replay captured traces of a small commit protocol. The
expectation is the recording, not the candidate's output and not the oracle's opinion: each trace
carries the outcome an independent auditor observed, and the candidate must reproduce it operation
for operation.

Three design decisions came out of building it, and all three were found by measurement rather
than by reasoning:

1. **`committed && aborted` is unreachable.** `commit` requires `begun`, `abort` requires `begun`
   and clears it, and `begin` never runs once a transaction has aborted. An exhaustive search over
   all operation sequences up to length six confirms it, so a guard on that pair would have been
   dead code sold as an invariant.
2. **A seed must be observable, and must be *only* observable through itself.** The first abort seed
   I wrote changed only `begun`, which every abort trace also exposes through the records bug, so it
   was indistinguishable from another seed. The policy is now: each seed is pinned to the exact set
   of traces it must break, the sets are pairwise disjoint, and the reference breaks none of them.
3. **The recording needs unpinned decoys.** Eight traces are pinned to no seed. Without them a
   candidate could pass every pinned trace by rejecting legal protocol behaviour outright, which is
   the degenerate solution to any replay test.

| Family | Seeds | Graded by | Independent expectation |
| --- | --- | --- | --- |
| planted invariant | 7 | model inside the oracle | hand-written specification |
| transmutation | 7 | 2,498-row corpus | Python arbitrary-precision integers |
| protocol distillation | 6 | 21 captured traces | outcomes recorded by the service auditor |

## What the second CI round found

Adding the family surfaced nine more defects that no amount of local review caught, including two
that made the grader grade nothing:

- operation rows end with an empty third column, and the loader trimmed trailing whitespace before
  splitting, so every recording was rejected;
- the recording-line splitter had been copied into the oracle, so the assembled crate defined it
  twice and did not compile;
- `ProtocolState` was defined by each candidate, but the oracle parses one out of the recording, so
  it had to be pinned in the ABI — which also stops a candidate widening the state until a wrong
  replay compares equal;
- `ProtocolError` carried neither the refused operation nor the reason, so the oracle could not
  report where a replay stopped;
- the seed generator replaced only the arm it was editing, leaving the reference's catch-all behind,
  so five seeds refused nearly every operation: they were caught, they compiled, and they were not
  the defects their names described.

The last one is the sharpest lesson of the whole exercise. A seed that is caught is not thereby a
*good* seed; it has to be caught by the specific recording that its specific bug breaks, and that
has to be checked against the pinned set rather than assumed from a green run.

## What this does **not** claim

- No resource or performance claim. Nothing here measures anything; `resource_gate` remains
  `"unmeasured_target_4gb"`, `performance_claims` remains `"unmeasured"`, `block3_done` remains
  `false`, and Block 4 remains gated behind the measured 4 GB baseline.
- The pack now carries twenty seeded candidate workspaces, which is the count Block 3 asks for,
  but `block3_done` still reads `false` on purpose: the two halves are linked only by count, the
  EconBench fixtures still run their own in-process verifiers, and no target measurement exists.
- Not the product workload. The pack is independently gradeable offline task material; it is not the
  native mixed-workspace workload, so it does not supply cold-start, tab-switch, snapshot, or
  peak-RSS measurements.
- Not a sandbox. The grader runs trusted, checked-in files with plain `rustc` and accepts no
  caller-supplied path, command, or model. It is a benchmark harness, not an execution boundary.
- No efficacy claim. No model, agent, or external service was involved; the candidates are authored
  fixtures and the "solution" is the checked-in reference, not a model output.
