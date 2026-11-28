# Verification — Block 3 seeded candidate task pack — 2026-11-28

## Authoritative result

- Branch: `arena/cc36730c-spacetime`, branched from `c10451ec7fdf480e0f60d195c9061315920b73e5` of
  `main`.
- Scope: `bench/taskpack/` (fourteen seeded candidate workspaces, two pinned ABIs, two hidden
  oracles, a 2,498-row independent differential corpus, two reference solutions), the
  `spacetime-ctl bench taskpack` command, the `block3_task_pack_seeded_workspaces` verify gate
  (gates 39 → 40), the CI task-pack step, and `apps/spacetime-ctl/tests/taskpack.rs`.
- Governing decision: `docs/ADR-0010-block3-seeded-task-pack.md`.

## What was verified, and how

The development sandbox for this branch has **no Rust toolchain**: `static.rust-lang.org`,
`sh.rustup.rs`, `index.crates.io`, `static.crates.io`, and the Debian mirrors are all unreachable
from it, and no `rustc` is preinstalled. As in every earlier block, GitHub Actions is the compiler
and the authoritative verifier.

**Status: verified.** Workflow run `46160941738` on `arena/cc36730c-spacetime` is green end to end.
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

1. `run.sh matrix` — both reference solutions pass all checks (8 wallet checks, 7 corpus functions);
   each of the 14 seeds fails exactly the check or corpus function named beside it in `run.sh`;
2. `spacetime-ctl bench taskpack --out target/taskpack.json` — `"candidates": 14`,
   `"seeded_bugs_caught": 14`, `"block3_done": false`, and a
   `"wallet_checks_without_a_seed": ["constructor_rejections"]` field that names the one graded
   check carrying no seed of its own;
3. `assemble.sh` rejects `abi/wallet_abi.rs` (not a candidate) and `run.sh` rejects an unknown seed;
4. the end-to-end step then runs `verify` with a toolchain present, so
   `block3_task_pack_seeded_workspaces` is graded, not skipped, and the report must show
   `gates = 40`.

## What this does **not** claim

- No resource or performance claim. Nothing here measures anything; `resource_gate` remains
  `"unmeasured_target_4gb"`, `performance_claims` remains `"unmeasured"`, `block3_done` remains
  `false`, and Block 4 remains gated behind the measured 4 GB baseline.
- Not a complete Block 3. The pack covers the 7 planted-invariant and 7 transmutation fixtures
  one-to-one. The six `protocol_*` protocol-distillation fixtures still have only the in-process
  state-machine verifier and no seeded candidate workspace.
- Not the product workload. The pack is independently gradeable offline task material; it is not the
  native mixed-workspace workload, so it does not supply cold-start, tab-switch, snapshot, or
  peak-RSS measurements.
- Not a sandbox. The grader runs trusted, checked-in files with plain `rustc` and accepts no
  caller-supplied path, command, or model. It is a benchmark harness, not an execution boundary.
- No efficacy claim. No model, agent, or external service was involved; the candidates are authored
  fixtures and the "solution" is the checked-in reference, not a model output.
