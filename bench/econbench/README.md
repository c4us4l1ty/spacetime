# Block 3 offline EconBench seed

These 20 `.task` files are **scripted offline fixtures**, not a measured resource gate and not a
claim that Block 3 is `DONE`. Each declares an independent verifier source, an expected verdict,
and a digest oracle before the scripted agent starts. They use the local scripted runner; no public
internet, real wallet, real account, live target, or live model call is involved.

`spacetime-ctl bench run --arm A|B|C --model <id> --budget <tokens> --out <file>` runs the fixtures
and writes a machine-readable report. Arms are harness metadata only; this prototype does not claim
that arm C beats arm A. `spacetime-ctl ledger verify bench/econbench/sample-ledger.jsonl` verifies
the checked-in mixed-workspace snapshot ledger. Its `.jsonl` and `.jsonl.head`
files have narrow `.gitignore` exceptions because the static console consumes
them as deterministic offline fixtures.

Remaining before Block 3 is DONE: real planted-invariant/transmutation/protocol-distillation
code, independent verifiers implemented for those tasks, a measured 4 GB baseline resource gate,
and accepted out-of-band provenance.
