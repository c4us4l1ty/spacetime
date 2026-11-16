# Spacetime

## License & Copyright

**Copyright (c) 2026 c4us4l1ty. All Rights Reserved.**

*This repository is strictly proprietary. You may 
NOT use, modify, sub-license, or redistribute this code for any purpose without permission.*

<div align="center">
<pre>
 ▄▄▄▖▗▄▄▖  ▗▄▖  ▗▄▄▖▗▄▄▄▖▗▄▄▄▖▗▄▄▄▖▗▖  ▗▖▗▄▄▄▖
▐▌   ▐▌ ▐▌▐▌ ▐▌▐▌   ▐▌     █    █  ▐▛▚▞▜▌▐▌   
 ▝▀▚▖▐▛▀▘ ▐▛▀▜▌▐▌   ▐▛▀▀▘  █    █  ▐▌  ▐▌▐▛▀▀▘
▗▄▄▞▘▐▌   ▐▌ ▐▌▝▚▄▄▖▐▙▄▄▖  █  ▗▄█▄▖▐▌  ▐▌▐▙▄▄▖
</pre>
</div>

---

## Development — Block 1

GitHub Actions is the compiler for this repository. The first Rust slice is intentionally small and
dependency-light: `serde`, `serde_json`, and `sha2` only. CI runs formatting, Clippy, unit tests,
a release build, and CLI smoke tests.

Useful local commands once a Rust toolchain is available:

```sh
cargo test --workspace --all-targets
cargo run -p spacetime-ctl -- run --task bench/tasks/block1_hello.task --cortex scripted --ledger target/block1.jsonl
cargo run -p spacetime-ctl -- ledger verify target/block1.jsonl
cargo run -p spacetime-ctl -- replay target/block1.jsonl
cargo run -p spacetime-ctl -- machine
```

The false-claim and timeout fixtures intentionally exit non-zero after writing their state; the happy path exits zero.

## Development — Blocks 1-2 functional baseline (merged); measurement intake in progress

The Crucible adds a second-run determinism check, independent verifier provenance, a flaky-fixture
family, a false-PASS flip metric, and differential/mutation metrology.

```sh
cargo run -p spacetime-ctl -- crucible verify --task bench/tasks/block1_hello.task
cargo run -p spacetime-ctl -- crucible false-pass --task bench/tasks/block1_hello.task --runs 50
cargo run -p spacetime-ctl -- crucible differential --limit 1000000
cargo run -p spacetime-ctl -- crucible sources
cargo run -p spacetime-ctl -- crucible panel --task bench/tasks/block1_hello.task
cargo run -p spacetime-ctl -- crucible adversarial --dir bench/adversarial
cargo run -p spacetime-ctl -- crucible population --operators 10 --runs 50
cargo run -p spacetime-ctl -- crucible fixture-population --runs 50
cargo run -p spacetime-ctl -- machine gate
cargo run -p spacetime-ctl -- verify --out target/verify.toml
cargo run -p spacetime-ctl -- verify --out target/verify.toml --measurement <file>
```

`verify --out` runs 25 offline Block 1 and Block 2 gates end to end and writes a TOML-style
report. The optional bounded `verify --measurement <file>` path evaluates a measurement file for
the report without ever promoting file shape to a measured resource gate.
`crucible panel` resolves five independent verifier sources conservatively; `crucible adversarial`
runs twenty offline model-authored fixtures; `crucible population` uses synthetic operators, and
`crucible fixture-population` reruns 26 real declared-oracle fixtures 50 times each. Both population
commands explicitly report `real_operator_study=false`. CLI reruns are bounded to 50–10,000,
differential limits to 1,000,000–10,000,000, and missing optional flag values are rejected. A narrow
`LD_PRELOAD` shim fixes `clock_gettime`/`getrandom` and denies sockets for local probes, while an
action allow-list enforces capability fields; the shim is **not** a kernel sandbox or complete
boundary. `machine gate` stays unmeasured until a real 4 GB machine measurement file is supplied;
CI/synthetic host labels are rejected. CI host facts are not the recorded 4 GB potato machine, so no
performance or resource-budget claim is made.

Block 2 was merged to `main` via PR #8 at explicit user request after strict CI passed 24 gates.
The measured 4 GB resource gate remains outstanding, so this is a functional merge, not a resource
sign-off.

The active branch is `arena/f3c65268-spacetime` (status `pr_open`, branched from
`4b048bf638ca6ea6d67d6db2c6586ecd615e4a70` of `main`). Its small hygiene + measurement-intake
increment adds a 25th `measurement_intake` gate, bounded `verify --measurement <file>` file
handling, and report fields derived from the evaluated file. Strict CI run `43818080975` verified
all 25 gates. A subsequent bug hunt bounded `machine gate --measurement`, rejected duplicate and
unknown CLI flags, removed a shape-pass `resource_gate=pass` claim, and rejected duplicate
measurement keys; strict CI run `43829073730` verified the hardening with 25 gates. The report keeps
`resource_gate = "unmeasured_target_4gb"` and `performance_claims = "unmeasured"`; a within-budget
file is labelled `pass_shape_only` until the recorded 4 GB machine is measured and physical
provenance is recorded out of band. No target measurement file is committed. Merged to `main` via
PR #9 at explicit user request after strict CI passed; this is a functional correctness merge, not a
resource sign-off.

See `BUILD-PLAN.md` for the blocked roadmap, `docs/ADR-0001-block1.md` for Block 1 decisions, and
`docs/ADR-0002-block2.md` for the Block 2 boundary.
