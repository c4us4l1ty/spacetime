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

## Development — Blocks 1-2 functional baseline and bounded intake (merged)

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
PR #9's bounded measurement-intake and bug-hunt increment was merged to `main` at
`90ceeb20db603c52e313ef718b82710241ad6080` at explicit user request after strict CI passed 25
gates; strict `main` CI run `43829756506` verified all 14 steps. The measured 4 GB resource gate
remains outstanding, so both were functional correctness merges, not resource sign-offs.

The active branch is `arena/3782c8f5-spacetime` (status `active`, branched from
`90ceeb20db603c52e313ef718b82710241ad6080` of `main`). It adds
`bench/measurements/capture-host-facts.sh`, a read-only host-fact collector, and
`bench/measurements/PROVENANCE-TEMPLATE.md`, a human out-of-band provenance checklist, plus stale
branch/status doc hygiene. The collector does not write a measurement file; the template is not
evidence until filled on the recorded target machine and kept out of band. No target measurement
file, filled provenance record, or physical attestation is committed.
`resource_gate = "unmeasured_target_4gb"` and `performance_claims = "unmeasured"` remain; a bounded
file can only be `pass_shape_only` until physical provenance is accepted and a reviewed code path
consumes it. Strict push run `43831944305` verified 25 gates and all 14 CI steps; the full
evidence is recorded in `docs/VERIFICATION-2026-11-15.md`.

See `BUILD-PLAN.md` for the blocked roadmap, `docs/ADR-0001-block1.md` for Block 1 decisions, and
`docs/ADR-0002-block2.md` for the Block 2 boundary.
