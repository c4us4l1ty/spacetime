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

Remaining before Block 3 is DONE: seeded planted-invariant candidate workspaces, reference-plus-skeleton
transmutation tasks, independently captured protocol traces and task-specific operators; a native
mixed-workspace target workload; a measured 4 GB baseline resource gate; and accepted out-of-band
provenance. The current local verifiers are useful domain checks, not substitutes for those task
artifacts or physical target evidence.
