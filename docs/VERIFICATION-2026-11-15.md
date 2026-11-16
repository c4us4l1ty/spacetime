# End-to-end verification — 2026-11-15 / bug-hunt merge — 2026-11-16 / measurement intake — 2026-11-16

## Authoritative result

- Historical validation branch: `arena/d7d47fd2-spacetime` (status `active`, branched from `d30a31508aecc675be6661d85e38c1b95c12bb06` of `main`); merged via PR #8.
- Current active branch: `arena/f3c65268-spacetime` (status `active`, branched from `4b048bf638ca6ea6d67d6db2c6586ecd615e4a70` of `main`). The measurement-intake increment below is verified by strict CI run `43818080975`.
- Validated strict code head: `969d5cf414d25fbdd882aaec1932c2ec017f8502`.
- Strict GitHub Actions run for that code head: `43813696825`, job `133120813158`.
- All 12 job steps succeeded: Format, `clippy -D warnings`, Tests, Release build, three CLI smokes,
  Crucible smoke, **End-to-end verification**, and Machine baseline. The job's step conclusions were
  all `success`; no `continue-on-error` or relaxed Clippy/Dump setup remained.
- `spacetime-ctl verify --out target/verify.toml` passed with `status = "pass"`, `gates = 24`,
  `failed = 0`, and `resource_gate = "unmeasured_target_4gb"`.
- Temporary diagnostic workflow `75f4a57` (run `43813401408`) used `continue-on-error` only to
  collect rustfmt/Clippy findings in one pass. Strict CI was restored in `969d5cf`; that diagnostic
  run is **not** authoritative.
- Local Rust toolchain: unavailable. GitHub Actions with committed `Cargo.lock` and `--locked`
  invocations was the compiler and verifier. A local `cc` was used only to sanity-check the C shim
  before commit.
- Historical PR #8 documentation head: the commit containing this file. Any changes after `969d5cf`
  on that branch were documentation-only and were covered by a further strict CI run; they did not
  alter the validated code.
- Merged to `main` via PR #8 at explicit user request. `main` was updated from Block 1 merge
  `d30a31508aecc675be6661d85e38c1b95c12bb06`. The measured 4 GB resource gate remains outstanding,
  so this is a functional merge, not a resource sign-off.

## Bug hunt

Found and fixed before the merge:

1. `crucible false-pass --runs 0` reported zero false-PASS and exited zero. Reruns are now bounded to
   50–10,000 and validated before allocation.
2. `crucible differential --limit 1` passed a weaker one-input check. Differential limits are now
   bounded to 1,000,000–10,000,000.
3. `crucible population --runs 0` counted every synthetic operator as accepted because `all` on an
   empty verdict list is true. `population_report(0)` now accepts nothing, and a checked wrapper
   validates operator/rerun ranges.
4. Missing values for `crucible adversarial --dir` and `machine gate --measurement` silently fell
   back to defaults. `optional_flag` now returns an error when a supplied flag has no value.
5. `crucible verify` printed the fixture's expected verdict but did not enforce it. A proof verdict
   that disagrees with the declared oracle is now rejected.
6. `resource_gate` could accept a synthetic within-budget host label as `pass`. CI/synthetic/
   self-test/Azure/runner host markers now leave the gate `unmeasured`.
7. Unbounded rerun, operator, and differential inputs could exhaust memory/CPU. Validators cap them.

Regression tests were added in `spacetime-verifier`, `spacetime-bench`, and `spacetime-ctl`, plus a
24th `cli_input_validation` end-to-end gate and negative CLI smokes in strict CI.

## End-to-end report

`spacetime-ctl verify --out target/verify.toml` passed in strict CI. Report header:

```toml
schema_version = 1
report = "end_to_end_verification"
status = "pass"
gates = 24
failed = 0
resource_gate = "unmeasured_target_4gb"
performance_claims = "unmeasured"
target_profile = "4gb_potato"
resource_law = "holy_grail"
ci_host = "github_actions_not_target"
panel_sources = 5
adversarial_fixtures = 20
determinism_shim = "ld_preload_not_complete_boundary"
capability_allow_list = "local_fixture_only"
fixture_population_fixtures = 26
fixture_population_runs = 50
fixture_population_verifier_runs = 1300
synthetic_operator_population = true
real_operator_study = "unmeasured"
resource_gate_evaluator = "unmeasured_fail_pass_self_test"
cli_input_validation = "bounded"
resource_gate_host_validation = true
```

All 24 gates passed:

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
| `determinism_profile` | fixed profile digest `b9185fbdf6e5cca21f450646a227734e089e9f8b4e7ec6db253c7584b9c9b46c`, network `ld_preload_socket_denied`, shim explicitly `not_complete_boundary` |
| `machine_baseline` | host facts only; resource gate explicitly unmeasured |
| `determinism_controls` | cleared env, C locale, runner-owned cwd, null stdin, fixed clock/RNG, and shim `clock/rng/socket/connect` checks |
| `capability_enforcement` | 7 tampered actions denied, valid local fixture action completed |
| `cli_input_validation` | 7 unsafe CLI cases rejected, valid `crucible verify` still accepted |
| `verifier_panel` | 5 sources, `Pass`, zero conflicts, highest-priority provenance selected |
| `adversarial_corpus` | 20 offline spec-only fixtures, 0 mismatches, 8 PASS / 8 FAIL / 4 ERROR |
| `operator_population` | 10 synthetic operators × 50 reruns, 6 unstable, 0 promoted false-PASS leaks, `real_operator_study=false` |
| `fixture_population` | 26 real declared-oracle fixtures × 50 reruns = 1,300 verifier calls, `max_flip_rate=0.0000`, `max_leak_rate=0.0000`, `real_operator_study=false` |
| `resource_gate_unmeasured` | evaluator self-test covers unmeasured, fail, pass, and non-target-host states; real 4 GB target remains unmeasured |

The Crucible smoke step also asserted `verdict=pass`, `false_pass_rate=0.0000`,
`differential_matches=1000000`, `mutation_score_bps=10000`, both source-priority endpoints,
`panel_verdict=pass` with `sources=5`/`conflicts=0`, `adversarial_fixtures=20`/`mismatches=0`,
non-zero exits for every `block2_flaky*.task` fixture, synthetic population `runs_per_operator=50`
with `promoted_false_pass_leaks=0`, fixture population `fixtures=26`, `verifier_runs=1300`,
`max_flip_rate=0.0000`, `max_leak_rate=0.0000`, `mismatches=0`, `real_fixture_population=true`,
`real_operator_study=false`, and non-zero exits for zero-rerun false-PASS/population/fixture commands,
a tiny differential limit, a missing `--dir` value, and a missing `--measurement` value.

The Machine baseline step printed
`resource_gate=unmeasured target_profile=4gb_potato memory_budget_kb=4194304 measurement_count=0`
and `reason=no measurement file supplied; 4 GB potato machine unmeasured`.

## What this does **not** claim

- The recorded 4 GB potato machine resource budget is **not measured**. CI ran on an 8 GB Azure
  runner (`memory_total_kb="8122976"` in strict run `43813696825`); those are host facts, not
  target results. No RAM, CPU, boot-time, latency, or battery claim is valid until measured on the
  target machine.
- `resource_gate` is a shape-and-budget evaluator with synthetic self-tests. No real target-machine
  measurement file is committed; `resource_gate="unmeasured_target_4gb"` and
  `performance_claims="unmeasured"` remain explicit. The gate cannot prove physical provenance.
- The `LD_PRELOAD` shim is a **narrow local probe**, not a kernel sandbox or a complete boundary. It
  fixes `clock_gettime`, `getrandom`/`getentropy`, and denies `socket`/`socketpair`/`connect` for
  dynamically linked local probes. Static binaries, direct syscalls, inherited descriptors, and
  direct `/dev/urandom` reads are outside its coverage. Real `net=none`, namespaces, cgroups, Ark
  cells, and quarantine microVMs remain later blocks (notably Blocks 7–8).
- The capability allow-list enforces local-fixture scope, time budget, args digest, taint, expected
  verifier, and schema version before scripted execution. It is host-side policy, not a guest kernel
  or VM boundary.
- The 20-fixture adversarial corpus is offline and model-authored from the spec; it is **not** a live
  separate-model adversarial suite. Live model-written tests remain later work.
- The 1,300-call fixture population is a real declared-oracle fixture study, not a real operator
  study. Synthetic operators are explicitly labelled `real_operator_study=false`. A full 90-day
  operator-population study remains later work.

## Reproduce

```sh
cargo run --release --locked -p spacetime-ctl -- verify --out target/verify.toml
cat target/verify.toml
cargo run --release --locked -p spacetime-ctl -- crucible panel --task bench/tasks/block1_hello.task
cargo run --release --locked -p spacetime-ctl -- crucible adversarial --dir bench/adversarial
cargo run --release --locked -p spacetime-ctl -- crucible population --operators 10 --runs 50
cargo run --release --locked -p spacetime-ctl -- crucible fixture-population --runs 50
cargo run --release --locked -p spacetime-ctl -- machine
cargo run --release --locked -p spacetime-ctl -- machine gate
```

CI additionally runs `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
--locked -- -D warnings`, `cargo test --workspace --all-targets --locked`, release build, and the
Block 1/Block 2 smoke gates. The `spacetime-exec` build script invokes the system C compiler to
build the local shim and probe; it adds no crates.io dependency and no Rust `unsafe` code.

## Remaining Block 2 work before `DONE`

1. Live separate-model adversarial tests that must survive the mutation gate and independence rules.
2. Full 50-rerun operator-population studies on real operators, not synthetic kinds or fixtures.
3. Measured 4 GB resource gate on the recorded target machine, including cold boot, tab switch,
   peak RSS, and CPU/latency numbers.
4. A complete execution boundary for real `net=none` and untrusted guest code, deferred to the later
   Ark/quarantine blocks; the local `LD_PRELOAD` shim is not that boundary.
5. Block 2 was merged to `main` via PR #8 at the user's explicit request after strict CI passed.
   Under the First Law, Block 2 is still not `DONE` until the measured 4 GB target budget is proven.

## Current increment — hygiene + measurement intake (strict CI verified)

- Active branch: `arena/f3c65268-spacetime`, status `active`, base `4b048bf638ca6ea6d67d6db2c6586ecd615e4a70`.
- Strict code head: `d0d6ce662e3f2ed28bfdd20b9be0d8963dc58263`.
- Strict GitHub Actions run `43818080975`, job `133134609083`: all 14 numbered steps succeeded —
  Set up job, Checkout, Install Rust, Format, `clippy --workspace --all-targets --locked --
  -D warnings`, Tests, Release build, three CLI smokes, Crucible smoke, **End-to-end
  verification**, **Measurement intake smoke**, and Machine baseline; post-job steps also
  succeeded. The workflow has no `continue-on-error` and no relaxed Clippy; committed
  `Cargo.lock` and `--locked` are authoritative.
- `spacetime-ctl verify --out target/verify.toml` passed with `status = "pass"`, `gates = 25`,
  `failed = 0`, `resource_gate = "unmeasured_target_4gb"`,
  `performance_claims = "unmeasured"`, `measurement_intake = "none"`,
  `measurement_evaluator = "none"`, `measurement_digest = "none"`, and the new
  `measurement_intake` gate. All other 24 PR #8 gates still pass.
- `verify --measurement` smoke cases: CI-host file → `measurement_evaluator = "unmeasured"` with
  a `non-target host` reason; non-numeric value → `unmeasured` with `bounded unsigned integer`;
  70,000-byte file → `unmeasured` with `64 KiB bound`; within-budget shape file →
  `pass_shape_only` while `resource_gate` and `performance_claims` stay unmeasured and reasons
  include `physical provenance out of band`.
- No real target measurement file is committed. Physical provenance remains out of band; this is
  bounded measurement intake, **not** a measured 4 GB resource gate or a performance claim.
- The stale branch/status, README, BUILD-PLAN, and plan next-step text are corrected for the
  active f3 branch; PR #8's 24-gate result above remains historical evidence, not evidence for
  this increment.
- Submitted as a PR; not merged to `main` unless explicitly requested.
- A temporary diagnostic workflow at `23db3ea` (run `43817571481`, job `133133013340`) used
  `continue-on-error` only to collect rustfmt/Clippy findings in one pass. Strict CI was restored
  at `d0d6ce6`; that diagnostic run is **not** authoritative.
