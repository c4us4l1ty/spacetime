# ADR-0004 — Offline Block 3 console and EconBench seed

Status: merged to `main` via PR #10 at `33149fe9ecac077bd1d37468c96197fefe94bca2`. This ADR does not claim a measured resource gate and does not mark Block 3 `DONE`.

The offline prototype was developed on `arena/3782c8f5-spacetime`, branched from
`90ceeb20db603c52e313ef718b82710241ad6080`.

## Context

`BUILD-PLAN.md` makes the measured 4 GB baseline resource gate a precondition for completing
Block 3. That gate remains unmeasured: no exact 4 GB machine, accepted out-of-band provenance, or
reviewed code path consuming physical provenance exists. The product still needs a visible,
offline-only foundation for the next build slice: a measuring stick, a console, and honest
mixed-workspace preview semantics.

## Decision

1. **Start Block 3 as an offline prototype.** Add 20 scripted local fixtures in
   `bench/econbench/`, a mock bounty board and hash-chained wallet ledger in `bench/economy/`, and
   a static HTML console in `apps/spacetime-console/`.
2. **Keep all execution local.** `bench run` uses the scripted cortex and local fixture runner.
   Arms A/B/C are harness metadata only; no live model, public internet, real wallet, real
   account, or live target is contacted. `wall_clock_ms` is labelled CI/local-host-only, not
   target evidence.
3. **Make the console static and offline.** It loads `report.json` through a relative fetch or a
   file input, uses a content security policy, avoids unsafe DOM injection and dynamic code
   evaluation, and requires no live backend or embedded browser runtime.
4. **Respect mixed-workspace invariants.** Tiles expose `WorkspaceId`, `SurfaceId`, trust tier,
   `LivenessState`, `PreviewMode`, and last action. Switching tiles changes `RENDERING`, not
   `RUNNING`. The AI tile uses ledger-driven snapshot-on-action stills, not continuous video.
   The console warns that Ark cells share a guest kernel and a tab is not a trust boundary.
5. **Use a real append-only ledger for the demo.** The sample upload ledger and mock wallet ledger
   are verified by `spacetime-ctl ledger verify`. `spacetime-ctl console` generates a report from
   the verified ledger; credits and payment confirmations are explicitly fake.
6. **Preserve the unmeasured boundary.** `resource_gate`, `release_gate`, and
   `performance_claims` remain unmeasured. `block3_done = false`. No RAM, CPU, boot-time,
   latency, concurrency, model-efficacy, or real-economy claim is made.
7. **Add two end-to-end gates.** `econbench_offline` checks 20 completed fixtures and the
   false-PASS boundary; `static_console_report` checks the verified ledger, snapshot strip,
   mixed-workspace tiles, static asset hygiene, and CLI report export. Strict CI remains the
   authoritative verifier with committed `Cargo.lock`, `--locked`, `clippy -D warnings`, and no
   `continue-on-error`.

## Consequences

- The next build slice is visible and reviewable without pretending the resource gate passed.
- The static console can be served locally and loaded with a CLI-exported report.
- The existing 26 functional gates remain distinct from the new 28-gate increment.
- The 4 GB baseline remains mandatory; optional 8 GB/16 GB public profiles are additive and do
  not replace it.
- CI/sandbox host facts and local fixture timings are not target-machine evidence.

## Non-goals

- No measured 4 GB, 8 GB, or 16 GB resource pass.
- No real planted-invariant/transmutation/protocol-distillation implementations yet.
- No API Cortex adapter, population-per-turn cycler, or A/B/C efficacy result.
- No live mixed-workspace shell, real browser, VM, account, money, or continuous video.
- No open-source license or public release action. PR #10 later merged this functional prototype to `main`; that merge did not establish a measured resource sign-off.

## Evidence

Strict push run `43843891738` (job `133219100433`) verified commit `e455556` with 14/14 steps and
28/28 gates green. The committed workflow had zero `continue-on-error`, a committed `Cargo.lock`,
`--locked` commands, `clippy -D warnings`, and no diagnostic capture steps. The default
`spacetime-ctl verify --out` report printed `status = "pass"`, `gates = 28`, `failed = 0`,
`resource_gate = "unmeasured_target_4gb"`, `performance_claims = "unmeasured"`, and
`block3_done = false`. The new `econbench_offline` gate reported
`fixtures=20 completed=20 claims=20 false_pass_rate=0.0000 network=none`; the
`static_console_report` gate reported `static_html=offline ledger_verified=true
snapshot_frames=3 workspaces=2`. CI also verified both checked-in hash-chained ledgers,
the offline `bench run` report, the console CLI export, negative flags, static console hygiene,
bounded measurement intake, explicit target profiles, and machine baseline commands. Diagnostic
runs `43841244588`, `43842052899`, and `43842803508` were used only to capture and fix compile,
format, and tracking errors; they are not acceptance evidence. This run is functional CI
evidence only. It does not establish a measured 4 GB baseline gate, physical provenance, release
readiness, model efficacy, or Block 3 `DONE`.
