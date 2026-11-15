# End-to-end verification — 2026-11-15

## Authoritative result

- Active branch: `arena/d7d47fd2-spacetime`
- Validated code head: `5ac8dad974cf3c69fa2b7a34f568a5e9f9c6b4f0`
- Final branch head: `cee78c3b1d9cd5500585349ca700026e98c2f249` (documentation-only)
- Strict GitHub Actions run for code: `43742966152`, job `132899948467`
- Strict GitHub Actions run for final head: `43743191655`, job `132900646024`
- Result: all 12 job steps succeeded in both runs, including Format, `clippy -D warnings`, Tests,
  Release build, CLI smokes, Crucible smoke, **End-to-end verification**, and Machine baseline.
- Local Rust toolchain: unavailable; GitHub Actions was the compiler and verifier.
- Temporary diagnostic workflow `66594f0` used `continue-on-error` only to collect all
  rustfmt/Clippy findings at once. Strict CI was restored in `5ac8dad`.

## End-to-end report

`spacetime-ctl verify --out target/verify.toml` passed in strict CI. Report header:

```toml
schema_version = 1
report = "end_to_end_verification"
status = "pass"
gates = 16
failed = 0
resource_gate = "unmeasured_target_4gb"
performance_claims = "unmeasured"
target_profile = "4gb_potato"
resource_law = "holy_grail"
ci_host = "github_actions_not_target"
```

All 16 gates passed:

| Gate | Verified detail |
| --- | --- |
| `block1_happy` | 2 events, `Verified`, live state byte-identical to replay, PASS proofs carry complete provenance |
| `block1_false_claim` | `Unverified`, non-zero command exit, ledger still valid |
| `block1_timeout` | `Failed`, non-zero command exit, ledger still valid |
| `block1_tamper` | byte-flip ledger tamper rejected |
| `block1_truncation` | truncated ledger rejected |
| `block1_non_canonical` | non-canonical JSONL line rejected |
| `block1_incomplete_goal` | `InProgress`, non-zero command exit, ledger still valid |
| `crucible_deterministic` | `Pass`, 2 determinism runs, mutation score `10000` bps |
| `crucible_flaky_family` | 4 flaky fixtures all `Error`, zero flips, zero false-PASS leaks |
| `crucible_false_pass` | 50 reruns, `flip_rate=0.0000`, `false_pass_leaks=0.0000` |
| `crucible_unstable_operator` | 50 reruns, `flip_rate=1.0000`, `false_pass_leaks=0.5000`; instability detected |
| `crucible_trivial_mutation_gate` | trivial candidate `Fail`, mutation score 0, promotion blocked |
| `crucible_differential_mutation` | 1,000,000 inputs matched; mutation score `10000` bps |
| `verifier_source_priority` | 5 sources ordered fixture suite → mutation, priorities 1–5 |
| `determinism_profile` | fixed profile digest `f95130d391a1780ff5aa3721987823a1a0b2ff9c45c186e5dfba90ca57d416d9`, network none by construction |
| `machine_baseline` | host facts only; resource gate explicitly unmeasured |

The Crucible smoke step also asserted `verdict=pass`, `false_pass_rate=0.0000`,
`differential_matches=1000000`, `mutation_score_bps=10000`, both source-priority endpoints, and
non-zero exits for every `block2_flaky*.task` fixture.

## What this does **not** claim

- The recorded 4 GB potato machine resource budget is **not measured**. CI ran on an 8 GB Azure
  runner (`memory_total_kb="8122976"` in the strict run); those are host facts, not target results.
- No RAM, CPU, boot-time, latency, or battery claim is valid until measured on the target machine.
- `DeterminismProfile` is a declarative, hashed profile plus cleared env/null stdio/runner-owned
  temp cwd. It is **not** a kernel sandbox and **not** a fixed-clock/getrandom/socket shim.
- The flaky family and 50-rerun study are offline fixtures, not a live model-written adversarial
  suite or a full 90-day operator population study.
- `main` remains at Block 1 merge `d30a31508aecc675be6661d85e38c1b95c12bb06`. Block 2 is on the
  active branch only; no PR or merge has been requested.

## Reproduce

```sh
cargo run --release --locked -p spacetime-ctl -- verify --out target/verify.toml
cat target/verify.toml
cargo run --release --locked -p spacetime-ctl -- machine
```

CI additionally runs `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
--locked -- -D warnings`, `cargo test --workspace --all-targets --locked`, release build, and the
Block 1/Block 2 smoke gates.

## Remaining Block 2 work before `DONE`

1. Real fixed-clock/RNG/locale/socket shims and capability enforcement beyond the scripted runner.
2. Live model-written adversarial tests that must survive the mutation gate.
3. Full 50-rerun operator-population studies on real fixture populations.
4. Measured 4 GB resource gate on the recorded target machine.
5. Only then consider a PR/merge to `main`; do not merge automatically.
