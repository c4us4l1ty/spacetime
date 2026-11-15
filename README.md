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

## Development — Block 2 (in progress)

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
```

`verify --out` runs 23 offline Block 1 and Block 2 gates end to end and writes a TOML-style report.
`crucible panel` resolves five independent verifier sources conservatively; `crucible adversarial`
runs twenty offline model-authored fixtures; `crucible population` uses synthetic operators, and
`crucible fixture-population` reruns 26 real declared-oracle fixtures 50 times each. Both population
commands explicitly report `real_operator_study=false`. A narrow `LD_PRELOAD` shim fixes
`clock_gettime`/`getrandom` and denies sockets for local probes, while an action allow-list enforces
capability fields; the shim is **not** a kernel sandbox or complete boundary. `machine gate` stays
unmeasured until a real 4 GB machine measurement file is supplied. CI host facts are not the
recorded 4 GB potato machine, so no performance or resource-budget claim is made.

See `BUILD-PLAN.md` for the blocked roadmap, `docs/ADR-0001-block1.md` for Block 1 decisions, and
`docs/ADR-0002-block2.md` for the Block 2 boundary.
