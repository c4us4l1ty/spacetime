# Block 3 offline EconBench seed

These 20 `.task` files are **scripted offline fixtures**, not the full seeded task pack, a measured
resource gate, or a claim that Block 3 is `DONE`. Each declares an independent verifier source, an
expected verdict, a digest oracle, and a domain `case_id` before the scripted agent starts. The
EconBench runner executes the matching wallet-invariant, independent transformation, or local
protocol-state-machine verifier and includes its ID, check count, and digest in the report. These
verifiers add domain checks but do not make the generic scripted actions into candidate code-repair
tasks. They use the local scripted runner; no public internet, real wallet, real account, live target,
or live model call is involved.

`spacetime-ctl bench run --arm A|B|C --model <id> --budget <tokens> --out <file>` runs the fixtures
and writes a machine-readable report. Arms are harness metadata only; this prototype does not claim
that arm C beats arm A. `spacetime-ctl ledger verify bench/econbench/sample-ledger.jsonl` verifies
the checked-in mixed-workspace snapshot ledger. Its `.jsonl` and `.jsonl.head`
files have narrow `.gitignore` exceptions because the static console consumes
them as deterministic offline fixtures.

The seeded candidate workspaces now live in `bench/taskpack/` (ADR-0010): seven planted-invariant
wallet candidates and seven transmutation candidates, each with exactly one planted bug, graded by a
hidden independent oracle. CI runs the whole matrix through plain `rustc`, and `verify` carries it as
the `block3_task_pack_seeded_workspaces` gate. That pack measures nothing and does not set
`block3_done`.

Remaining before Block 3 is DONE: seeded candidate workspaces for the six `protocol_*`
protocol-distillation tasks and their independently captured traces and task-specific operators; a
native mixed-workspace target workload; a measured 4 GB baseline resource gate; and accepted
out-of-band provenance. The current local verifiers and the task pack are useful domain checks, not
substitutes for physical target evidence.
