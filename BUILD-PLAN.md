# Spacetime — Build Plan (blocked roadmap)

**Status:** active roadmap; Block 1 and Block 2 functional increments are merged to `main` via PRs #7 and #8; PR #9's bounded measurement intake and bug hunt are merged at `90ceeb20db603c52e313ef718b82710241ad6080`, with strict `main` CI run `43829756506` green (25 gates, 14 steps). The 4 GB resource gate is unmeasured. Current branch prepares reproducible target-machine provenance intake and branch-doc hygiene; strict push run `43831944305` is green and it claims no measured gate.
**Date:** 2026-11-15 (roadmap origin; branch context updated after PR #9 merge)
**Branch:** `arena/3782c8f5-spacetime` (status `active`, branched from `90ceeb20db603c52e313ef718b82710241ad6080` of `main`; PRs #7, #8, and #9 are merged history)

**Confirmed product decisions (user, 2026-11-15):**

- **One VM per app is rejected.** Use the tiered execution ladder from `DESIGN-REVIEW-2.md`: WASI workers, namespace/cgroup cells inside a shared Ark, and disposable quarantine microVMs only where a separate kernel trust domain is needed.
- **Mixed live workspace is mandatory.** GitHub programs, terminal apps, browser apps, and AI-driven workspaces must coexist in one tabbed shell. Switching to the AI tab must show the AI's real changes happening inside the app — typing a title, filling a description, choosing a thumbnail — not just a chat log or progress bar.
- **Snapshot-first preview is the low-resource default.** Continuous live video is optional. On the 4 GB machine, show a cheap sequence of still screenshots taken on tab switch and after meaningful AI actions. The AI keeps working headlessly between frames; the human still sees the work unfold like a flipbook.
- **Resource usage is the holy grail.** The recorded 4 GB, weak-CPU, no-GPU "potato machine" is the primary product constraint. If a feature competes with RAM, CPU, startup time, or battery, the feature loses.

---

## 0. Executive summary

Spacetime is **two products wearing one coat**:

1. **The organism** — an unattended agent that turns a goal into *verified* work. This is the unsolved, high-risk part.
2. **The body** — a Rust workspace shell that runs isolated Linux apps in tabs. This is expensive and polished, but comparatively solved-ish.

The original brief starts with (2). The reviews invert the *build order*: build the **employer** — supervisor, verifiers, policy, ledger, memory, blockers — before building a beautiful place for the employee to sit. But the user has now confirmed that the body is not optional or a vague later feature: GitHub programs, terminal apps, browser apps, and AI-driven workspaces must coexist in one tabbed shell, and switching to the AI tab must show the AI's changes happening inside the app. So the mixed workspace is a **first-class product requirement** with an early thin integration slice; the full visual shell is still sequenced after the safety/verification gates, to avoid polishing a facade around an unproven agent.

The plan below is deliberately block-shaped. **One block = one reviewable PR.** No block starts until the previous block's acceptance gate passes. Every block has a measurable definition of done and, where useful, a kill criterion that can stop the project before years are spent polishing the wrong thing.

## 0.1 The First Law — Resource usage is the holy grail

> **If it costs RAM, CPU, startup time, or battery, it must prove its worth on the recorded 4 GB machine. Otherwise it is downgraded, suspended, queued, or deleted.**

This law overrides visual fidelity, convenience, feature count, and optimism. Concretely:

1. **Cheapest sufficient surface wins:** text buffer → snapshot/flipbook → live pixels → real browser → full GUI. Never pay for a heavier surface when a lighter one proves the same thing.
2. **No one-VM-per-app.** Use WASI / Ark cells / quarantine microVMs by risk, not one kernel per tab.
3. **At most one heavy browser or GUI surface awake at a time.** Hidden AI work continues headlessly; switching tabs reattaches to current state.
4. **The resource governor has real authority:** downgrade `PreviewMode`, suspend cells, queue work, kill a runaway tab, or refuse a feature. It is not advisory.
5. **Measure before claiming.** Every block records host RSS, CPU, peak branch memory, snapshot cost, tab-switch latency, and cold/warm start on `bench/machine.toml`'s machine.
6. **Every block has a resource gate.** A block is not `DONE` because it works; it is `DONE` because it works *inside the measured budget*.
7. **No expensive default dependencies:** no Electron/WebView shell, no always-on browser farm, no heavy local vision model, no per-tab desktop environment, no custom VMM/TCP/TLS/compositor stack.
8. **Reuse before building.** Fork/adapt measured components; reject redundant daemons and duplicate caches.
9. **Temporal multiplexing is mandatory.** Only one or two heavy workspaces are awake; the rest are frozen or queued.
10. **When in doubt, show less.** A labelled snapshot plus a ledger action is better than a smooth video that freezes the laptop.

### The one-sentence architecture

> A host-owned, event-sourced **Supervisor** turns model output into typed actions, validates them against policy, executes them in the lightest sufficient isolation tier **within a measured resource budget**, and refuses to advance a goal until an **independent verifier** supplies a deterministic proof recorded in a hash-chained ledger.

### The first honest v1

A Linux-only, 4 GB-friendly, local-only system that can run an overnight verifiable task unattended, survive crashes and model swaps, prove its own results, keep secrets out of execution cells, and show its state in a telemetry console. It must also include a **thin mixed-workspace slice**: a human GitHub/terminal tab beside an AI-driven tab whose surface shows real UI changes through cheap snapshots. **No real money, no live business accounts, no browser farms, no seamless cross-domain Wayland yet.** Heavy continuous live rendering and external accounts come only after measurement and the 90-day gate. **The First Law applies to every block, including the telemetry console and the snapshot path.**

---

## 1. What is actually in this repository

| File | Role | How to treat it |
|---|---|---|
| `README.md` | Name, proprietary notice, ASCII logo | Keep; add links to specs later |
| `BRAINSTORM.md` | Original user brief + long LLM replies | **Vision and feature backlog**, not a spec |
| `DESIGN-REVIEW.md` | First correction: security, policy, 4 GB budget, revised phases | **Security/product baseline** |
| `DESIGN-REVIEW-2.md` | Latest critique: metrology, population sampling, tiered execution, console-first | **Override where it conflicts** |

There is no prototype to recover in this checkout: the only commit is a grafted `Delete prototype directory` commit with no parent tree. Work starts from an empty codebase.

I also checked `c4us4l1ty/holonomy`. Its `.wavefunction` is a fixed-size encrypted document container with a USB-pad threat model. **Spacetime should reuse the vocabulary, not that implementation or cryptography.** Here, `.wavefunction` means an immutable, signed, content-addressed store of shared dependency layers.

### Source-of-truth precedence

When documents disagree, apply this order:

1. Live decisions from the user.
2. Legal, safety, and security constraints.
3. `DESIGN-REVIEW-2.md` §12 concessions and architecture updates.
4. `DESIGN-REVIEW.md` invariants, threat model, and security design.
5. `BRAINSTORM.md` original user requirements.
6. LLM-generated replies inside `BRAINSTORM.md` as an idea backlog only.
7. Performance numbers in any document as **hypotheses to benchmark**, never product promises.

---

## 2. Recommended defaults before Block 1

These are proposals, not unilateral product decisions. The rows marked **Confirmed** come directly from the user. If the remaining defaults are not overridden, Block 1 proceeds with them.

| Decision | Default | Consequence |
|---|---|---|
| Supreme constraint | **Confirmed: resource usage is the holy grail** | The 4 GB, weak-CPU, no-GPU machine outranks visual fidelity, convenience, and feature count |
| Host target | **Linux first**, exact 4 GB laptop/CPU/arch recorded in `bench/machine.toml` | Windows/macOS parity is later; Windows needs ≥8 GB and nested virt/WHPX caveats |
| First 90-day target | **Planted-invariant hunting on local fixtures** | Crisp verifier, no live targets, no KYC, no exploit of third-party systems |
| Cortex | **Scripted adapter → API adapter → web-CDP adapter behind a canary** | Web chat is a degradable channel, not the foundation |
| Browser | **One heavy browser surface awake at a time**: hardened gate-pass Chrome plus a live AI-driven browser tab when the task truly needs a page | No browser farm; no always-on browser as the default worker |
| Money | **Mock economy only** until explicit human/legal sign-off | Fake wallet with a real ledger; no trading, live payouts, or live wallets |
| Execution model | **Confirmed: no one-VM-per-app.** Tiered execution: WASI / Ark cell / disposable quarantine microVM | A tab is a view, not a kernel; trust domain is chosen by risk |
| Workspace concurrency | **Confirmed: human apps and AI workspaces run side by side** | Switching tabs does not pause the AI; active tab shows its surface |
| AI visibility | **Confirmed: the AI tab shows real app changes** | Title/description/thumbnail actions appear in the app UI, not only in a log |
| Preview mode | **Confirmed: snapshot-first on the 4 GB machine**; live damage stream only if measured cheap | Still frames on tab switch and after meaningful actions; AI continues headlessly between frames |
| Screenshot hygiene | Screenshots are RAM-only, human-display-only, masked for Cortex, and destroyed on tab close | No disk thumbnails, no host recent-files entry, no raw secret-bearing pixels sent to the AI |
| Display channels | Human sees real pixels/stills; AI receives a sanitized semantic view | Password fields and secrets are masked in the AI channel by construction |
| Unknown repos and build scripts | **Always quarantine trust domain** | Never placed in a shared Work Ark merely because they have namespaces |
| UI | **Static HTML console first, thin snapshot-based mixed-workspace slice in Block 5, Rust `egui` shell later** | Console is situational awareness; the full tab multiplexer comes later, but mixed tabs are confirmed |
| `.space` | **A git-backed workspace directory + manifest**, exported as a zstd archive | RAM snapshots are opt-in "deep freeze", not the default branch operation |
| `.wavefunction` | **New immutable CAS with provenance tiers** | No reuse of Holonomy's encrypted container or pad design |
| Legal line | Own identity, in-scope local forks, disclosure norms, permitted automation only | No synthetic identities, mass-account farms, CAPTCHA bypass, or off-scope probing |
| Claims | Replace "0 traces" and "undetectable" with explicit leak audit and honest identity claims | Avoids shipping falsifiable marketing as architecture |

---

## 3. Non-negotiable invariants

These are structural requirements. A block is not done because a warning was shown; it is done when the violation is impossible through the supported paths.

1. **Resource usage is the holy grail.** The recorded 4 GB, weak-CPU, no-GPU machine is the primary product constraint. If a feature competes with RAM/CPU/startup cost, the feature loses: downgrade the preview, suspend the cell, queue the work, or kill the feature. No block passes without a measured resource gate.

2. **No model-generated or untrusted code in the host UI/supervisor process.**
3. **A `net = none` cell/VM has no usable egress path**: no network interface, no inherited sockets, no unrestricted vsock service, no DNS via host resolver.
4. **No guest/cell packet reaches** `127.0.0.0/8`, `::1`, `169.254.0.0/16`, link-local, host LAN subnets, host UNIX sockets, or host listening ports. Enforce at a supervisor-owned broker.
5. **The broker identifies a cell from its supervisor-owned channel credential**, never from a guest-supplied `tab_id`.
6. **Secrets are never present in guest/cell filesystem, environment, or argv** unless a policy decision injects them into a specific input event.
7. **Every observable byte the AI sees has provenance/taint** and passes the observation firewall. Untrusted content can inform, never instruct spend, credentials, or destructive actions.
8. **Every irreversible external effect** — spend, post, delete, send — is either inside a pre-authorized capability token or queued as a blocker. It is never executed on model intent alone.
9. **A claim without an independent verifier proof is `unverified` and cannot advance a goal.**
10. **Every action, observation, policy decision, and verifier proof appends to a host-owned hash-chained ledger** the AI cannot rewrite.
11. **Unknown repositories, package install/build scripts, and suspected-hostile code go to quarantine.** Ark cells sharing a kernel are an explicit trust domain, not per-app hardware isolation.
12. **No live money, live exploits outside authorized scope, or real-world account automation** until an explicit human decision and legal review.
13. **All RAM, latency, boot-time, and concurrency numbers are measured** on the recorded 4 GB machine before being quoted.
14. **No "0 traces" claim.** Document the leak surface: core dumps, crash reporters, swap, clipboard history, DNS, recent-files lists, thumbnails, GPU shader caches, audio, router logs, and timing side channels.
15. **Do not build** a custom VMM, compositor, TLS stack, TCP/IP stack, container runtime, or crypto. Use measured forks/adapters or don't ship the feature.
16. **A tab is a view onto an execution domain**, not a trust boundary and not a kernel. One tab does not imply one VM.
17. **Switching tabs must not stop AI work.** Only the active tab needs a surface; background work continues under the resource governor.
18. **Human display and AI observation are separate channels.** The human may see real pixels or stills; Cortex sees a sanitized semantic tree. Raw secret-bearing screenshots are never sent to Cortex.
19. **AI input injection goes through the supervisor control channel** into the cell/VM. The display/surface broker is read-only and cannot drive the guest.
20. **Continuous live preview is optional.** On a constrained host, snapshot-on-action / snapshot-on-switch is a first-class mode, not a degraded error state.
21. **Screenshots are ephemeral**: RAM/tmpfs only, no host thumbnailer, no disk cache, no recent-files entry, destroyed on tab close.
22. **At most one heavy browser/GUI surface is awake at a time** on the recorded 4 GB machine; hidden AI work can continue headlessly and reattach on tab switch. No claim of many simultaneous Chromium tabs.

---

## 4. Target architecture

```text
                         Human / slow clock
                    steering, approvals, kill/scale
                                │
                                ▼
                   Telemetry console (HTML → Rust)
        ledger · blockers · operators · verifier health · resources
                                │
                                ▼
 ┌──────────────────────────────────────────────────────────────────┐
 │ HOST SUPERVISOR — "Ring -3" — trusted, event-sourced, no model code │
 │ typed syscall ABI · policy · taint · capability tokens · ledger     │
 │ goal tree · steering mailbox · blocker queue · model handoff        │
 └───────┬───────────────────────┬───────────────────────┬──────────┘
         │                       │                       │
         ▼                       ▼                       ▼
     Cortex adapters        Verifier / Crucible     Execution-domain manager
 scripted · API · web-CDP   independent proofs      resource governor · broker
 human fallback             determinism · mutation        │
                                                         ▼
                 ┌───────────────────────────────────────────────────────┐
                 │ Tier 1  WASI worker        pure operators/parsers      │
                 │ Tier 2  Ark cell           namespaces + cgroups        │
                 │ Tier 3  quarantine microVM unknown/hostile code        │
                 └───────────────────────────────────────────────────────┘
                                         │
                                         ▼
                  .wavefunction CAS  +  .space git workspace
        immutable shared layers     memory · operators · skills · ledger head
                                         │
                           rare ▼
                 hardened gate-pass Chrome / CDP
                 login · attestation · trace capture only
```

The **resource governor** is not advisory: it can downgrade `PreviewMode`, suspend hidden cells, queue work, or kill a tab. It has higher priority than visual fidelity.

### Execution ladder

| Tier | Mechanism | Use when | Boundary and caveat |
|---|---|---|---|
| 0 | Host trusted code | UI, supervisor, small audited helpers | Never runs untrusted code |
| 1 | WASI module in constrained worker process | Pure transforms, parsers, narrow verifiers | Capability-limited; Wasmtime and host calls remain TCB |
| 2 | Cell inside a shared Ark | Ordinary Linux tools allowed to share one guest kernel | Namespaces/cgroups/seccomp are **not** kernel isolation between cells |
| 3 | Disposable quarantine microVM | Unknown binaries, hostile build scripts, separate kernel trust domain | Higher cost; no shared secrets; destroyed after job |

Tabs are **user-facing views**, not machines and not kernels. This preserves the original isolation ambition while respecting the 4 GB budget.

### 4.1 Confirmed mixed workspace and cheap preview

```text
Tab strip (host shell; shows surfaces, runs no untrusted code)
  │
  ├─ Tab A: GitHub program someone else wrote
  │        └─► quarantine Ark cell / disposable microVM ──► terminal or framebuffer surface
  ├─ Tab B: terminal app
  │        └─► Ark-Work cell ──► text buffer (cheapest surface)
  ├─ Tab C: browser app
  │        └─► one real browser cell ──► page pixels or page snapshot
  └─ Tab D: AI workspace
           └─► supervisor + Cortex + isolated app/browser cell
                    ├─► human: snapshot / live surface
                    └─► Cortex: sanitized semantic tree, never raw secret pixels
```

The AI tab is **not a chat window**. It is a view into the same isolated app the AI is driving. If the AI is uploading a video, the human sees the title being typed, the description being filled, and the thumbnail being selected. The supervisor injects those actions; the surface broker only displays the result.

| What the user opens | Where it runs | What the user sees |
|---|---|---|
| A GitHub program someone else wrote | Quarantine cell or disposable VM, depending on trust | Terminal output or app window/still |
| A terminal app | Light Ark cell | Text buffer, not a video stream |
| A browser app | One dedicated browser cell | Page pixels if cheap, otherwise page snapshots |
| An AI-driven workspace | Same isolated cell, driven by supervisor actions | A flipbook of stills after each meaningful action, plus an action ribbon |
| A hidden AI tab | Still `RUNNING`; surface not `RENDERING` | Fresh snapshot immediately on tab switch |

#### Preview modes

| Mode | When used | Human sees | Cost |
|---|---|---|---|
| `LiveDamage` | Active tab, measured cheap, no secret-bearing field | Near-live damage rectangles / pixel stream | Highest; optional |
| `SnapshotOnAction` | **Default for AI tabs on the 4 GB machine** | A new still after each meaningful action, with caption like "typed title" | Low |
| `SnapshotOnSwitch` | Background tabs | Fresh still when selected, then `SnapshotOnAction` while active | Lowest useful |
| `Headless` | Heavy background work or secret-bearing surface | Action ribbon/semantic state only; masked still on return | Lowest |
| `TerminalBuffer` | Terminal/GitHub CLI apps | Plain text buffer | Tiny |

A screenshot fallback still satisfies "I see the AI working": it is a sequence of verified stills, not 60 fps video. The AI continues between frames. Screenshots are never written to disk, never indexed by the host thumbnailer, and never handed to Cortex raw.

### Core vocabulary

| Term | Meaning in Spacetime |
|---|---|
| `Action` | Typed syscall emitted by Cortex: `goal_id`, `op`, `args_digest`, `taint`, `capability`, `budget`, `expected_verifier` |
| `Observation` | Sanitized result with provenance, redactions, digest, and source tier |
| `VerifierProof` | `verifier_id`, verdict, evidence digest, determinism runs, mutation score, timestamp |
| `Event` | Append-only ledger entry: `seq`, `prev_hash`, `hash`, action, observation, proof, policy decision |
| `WorkspaceId` / `SurfaceId` | A tab and the view it renders; stable across tab switches, suspension, and reattachment |
| `LivenessState` | `RUNNING`, `RENDERING`, `SUSPENDED`, `DEAD`; AI work can be `RUNNING` while its tab is not `RENDERING` |
| `PreviewMode` | `LiveDamage`, `SnapshotOnAction`, `SnapshotOnSwitch`, `Headless`, `TerminalBuffer` |
| `SurfaceBroker` | Host-side read-only display router; it can show a surface but cannot inject input |
| `ResourceGovernor` | Enforces the First Law: measured budgets, preview downgrade, suspend/queue/kill decisions per block |
| `Operator` | Versioned typed `(pre, op, post + verifier)` with health status and mutation score |
| `.space` | One venture/tab/chain of state: git-backed memory, manifest, operator/skill registry, ledger head, optional encrypted vault index |
| `.wavefunction` | Immutable, tiered CAS of shared base layers and verified dependencies |
| `Collapse` | Promoting a verified `tab-local` change into the shared CAS |
| `Gate-pass` | Rare real-browser interaction for an irreducible human/attestation gate |
| `Ark` | One guest-kernel trust domain containing many lighter cells |
| `Crucible` | Deterministic candidate runner plus independent verifier |

### Proposed workspace layout for future code

This is a target, not created yet. Keep crate boundaries small and readable.

```text
Cargo.toml                         # workspace, strict lints, no unwrap policy in TCB
crates/
  spacetime-types/                 # IDs, Action, Observation, Event, SurfaceId, PreviewMode, LivenessState
  spacetime-ledger/                # hash-chained JSONL, replay, verify, tombstones
  spacetime-policy/                # Cedar policy, capability tokens, taint checks
  spacetime-memory/                # operators, facts/DEAD/TTL, NOW/BRIEF, .space git
  spacetime-cortex/                # Cortex trait + scripted/api/web-cdp/human adapters
  spacetime-verifier/              # independent verifier registry, determinism, mutation score
  spacetime-exec/                  # Runner trait, tiers, resource governor, SurfaceBroker contract
  spacetime-surface/               # snapshot capture, RAM-only frames, terminal/framebuffer routing
  spacetime-bench/                 # EconBench tasks, arms, metrics, reports
apps/
  spacetime-ctl/                   # CLI supervisor + bench runner
  spacetime-console/               # static HTML first; Rust egui later
  spacetime-shell/                 # live mixed human/AI workspace, snapshot-first
guest/
  init/                            # tiny static Rust PID 1 for quarantine VM
  cell-agent/                      # in-Ark cell supervisor
  base-image/                      # reproducible EROFS/squashfs build
bench/
  tasks/                           # 20 local tasks, fixtures, mock services
  machine.toml                     # exact 4 GB host and measurement baseline
docs/
  ADR-*.md                         # one decision record per architecture choice
```

---

## 5. Block plan at a glance

| Block | Name | Size | Depends | Gate |
|---:|---|---|---|---|
| 1 | Core contract, ledger, scripted supervisor | S | — | Replay and false-claim tests pass |
| 2 | Deterministic Crucible and independent verifiers | M | 1 | Flaky candidate becomes `ERROR`; false-PASS ≤1% |
| 3 | EconBench fixtures + static HTML console with mixed-tab tiles | M | 1, 2 | 20 local tasks run end-to-end; console shows ledger and snapshot tiles |
| 4 | Population-per-turn cycler + 90-day A/B/C experiment | L | 2, 3 | **Main thesis kill gate** |
| 5 | Durable supervisor, memory, thin mixed-workspace slice | L | 1, 4 | Crash/model-swap/blocker/steering + snapshot AI-tab tests pass |
| 6 | Ring -3 policy, vault, observation firewall, red-team | L | 1, 5 | Secret-exfiltration red team fails |
| 7 | WASI worker, Ark cells, governor, SurfaceBroker | L | 1, 6 | `net=none` truly has no egress; display cannot inject input |
| 8 | Disposable quarantine microVM and autopsy | L | 6, 7 | No host-local reach; VMM crash cannot dump guest RAM |
| 9 | `.wavefunction` CAS, reflink branch, `.space` import/export | M | 5, 8 | Untrusted tab cannot poison CAS; branch reproduces state |
| 10 | Egress broker, airlock, gate-pass + snapshot AI browser | L | 6, 7, 8 | No host-local/DNS leak; AI actions visible as stills |
| 11 | Operator registry, distillation, skills, health checks | M | 2, 4, 10 | Drift quarantines operator; stale skill disabled; 0-LLM replay |
| 12 | Live mixed workspace shell: human apps + AI work side by side | XL | 9, 11 | Switch tabs without pausing AI; see changes via live or snapshot surface |
| 13 | Optional polish and real-business pilot | continuous | 12 | Legal sign-off + 30-day metrics; many items may stay unbuilt |

**Every block also has a resource gate:** it must pass the measured RAM/CPU/startup budget on `bench/machine.toml` before it can be marked `DONE`. If it fails, downgrade the surface or defer the feature; do not raise the hardware requirement silently.

After Block 4, a single contributor should still follow the order. With multiple contributors, Blocks 5–7 ("organism + policy") and 8–10 ("body") can proceed in parallel, but **no full UI block starts before the organism's policy/verifier gates pass**. The thin mixed-workspace slice in Block 5 is the deliberate exception: it proves the confirmed human+AI visibility requirement early with snapshots, without building the whole shell.

---

## 6. Blocks in detail

### Block 1 — Core contract, ledger, and a scripted supervisor

**Why first:** Before any VM, browser, or model, we need a tiny trustworthy spine. This is the smallest thing that can distinguish "the agent said it worked" from "the supervisor recorded a proof".

### Build

- Create the Rust workspace and `crates/spacetime-types`.
- Define versioned core types:
  - `SpaceId`, `WorkspaceId`, `SurfaceId`, `GoalId`, `ActionId`, `EventSeq`, `TrustTier`, `EgressClass`, `Taint`, `Capability`.
  - `SurfaceKind { Terminal, Framebuffer, BrowserCDP }`, `PreviewMode`, `LivenessState { RUNNING, RENDERING, SUSPENDED, DEAD }`.
  - `Action`, `Observation`, `VerifierProof`, `PolicyDecision`, `Event`.
  - Serialize with a stable canonical JSON/TOML schema; reject unknown fields in the TCB.
- Implement `crates/spacetime-ledger`:
  - Append-only JSONL with `hash = H(prev_hash || canonical(event))`.
  - `spacetime-ctl ledger verify <path>`.
  - `spacetime-ctl replay <path>` reconstructs state from events only.
  - Tamper detection and truncation detection.
- Implement `crates/spacetime-cortex` with a **deterministic scripted adapter** first. No network, no browser, no API keys.
- Implement a minimal `Cortex` trait and `Runner` trait in `crates/spacetime-exec`. The initial runner is a constrained local child process with fixed cwd, fixed environment, timeout, and `net = none` by construction.
- Implement `apps/spacetime-ctl run --task <fixture> --cortex scripted`:
  - read goal → emit action → policy placeholder → run → observation → verifier → ledger event.
  - `claim_done` without a verifier proof is logged as `unverified` and does **not** advance the goal.
- Add `bench/machine.toml` and a baseline measurement script: host RSS, CPU, disk, OS, kernel, arch, available KVM, zram/swap state, and filesystem CoW support. No product claims yet.

### Tests / definition of done

- `ledger verify` passes on a fresh run and fails after flipping one byte or deleting one line.
- `replay` produces byte-identical state to the live run.
- A scripted false `claim_done` cannot mark a goal complete.
- A timed-out child produces a deterministic `ERROR`, not a pass.
- No test requires internet, a browser, a VM, secrets, or an LLM.
- All numbers from the docs are labelled "unmeasured" in `bench/machine.toml`.
- `WorkspaceId`/`SurfaceId` remain stable when a tab is hidden and reattached; `PreviewMode` is data, not a hard-coded UI decision.

### Kill / stop criterion

If the ledger/replay contract cannot be made deterministic with a scripted cortex, **stop**. Everything later depends on this.

### Explicitly out of scope

VMM, real GUI, web chat, CDP, real money, real credentials, Ark, CAS, live surfaces. (Tab/surface identifiers are in scope as types only.)

---

### Block 2 — The Crucible: determinism and independent verifiers

**Why now:** `DESIGN-REVIEW-2.md` identifies metrology as the missing catalyst. Best-of-N selection is worthless if the verdict is flaky or the model wrote its own easy test.

### Build

- Implement `crates/spacetime-verifier` with a `Verifier` trait:
  - `verify(candidate, fixture) -> Verdict { PASS, FAIL, ERROR, UNVERIFIED }`.
  - `verifier_id`, evidence digest, independent provenance, determinism run count, mutation score.
- Add deterministic execution controls for the local runner:
  - fixed clock, fixed RNG seed, fixed temp paths, fixed environment, no network, fixed locale.
  - cheap 80% rule from the review: run every accepted candidate **twice** from the same snapshot; disagreement is `ERROR`, never `PASS`.
  - active-branch increment: a narrow `LD_PRELOAD` shim fixes `clock_gettime`, `getrandom`/`getentropy`, and denies `socket`/`socketpair`/`connect` for dynamically linked local probes; an action capability allow-list enforces `local_fixture`, time budget, args digest, taint, and expected verifier before execution.
  - `LD_PRELOAD` alone is never treated as a complete boundary. Static binaries, direct syscalls, inherited descriptors, direct `/dev/urandom` reads, namespaces, cgroups, Ark cells, and quarantine microVMs remain later work.
- Add verifier sources in priority order:
  1. fixture's existing test suite,
  2. published spec/oracle implementation,
  3. differential equality against a reference implementation,
  4. a separate model that sees only the spec and writes adversarial tests,
  5. mutation testing: deliberately break candidate N ways and require the tests to catch ≥X%.
- Add a "flaky fixture" family: a candidate whose test flips on time/RNG/scheduler must be classified `ERROR`.
- Add a `false-PASS` metric: run each accepted operator 50× from the same snapshot; any flip is a leak.
- Record `UNVERIFIED` for any claim lacking a verifier reference.

### Tests / definition of done

- A deliberately flaky test never yields a promotion.
- A model-written trivial test is rejected or mutation-gated.
- Differential equality over 10⁶ inputs works for at least one transmutation fixture.
- `false-PASS rate ≤ 1%` on the local fixture set.
- Every `PASS` includes an evidence digest and verifier provenance.

### Kill / stop criterion

If false-PASS exceeds 1%, **stop feature work and fix determinism first**. All later metrics are noise until this holds.

---

### Block 3 — EconBench fixtures and the static mixed-workspace console

**Why now:** Build the measuring stick before the product. The console comes before the Rust UI because the organism needs situational awareness, not a window manager — but it also previews the confirmed mixed-tab requirement with snapshot tiles.

### Build

- Create 20 local, machine-checkable tasks in `bench/tasks/`:
  - **7 planted-invariant tasks:** seeded bug in a small contract/lib; produce a passing local PoC.
  - **7 transmutation tasks:** reference implementation + empty Rust/WASM skeleton; reach differential equality.
  - **6 protocol-distillation tasks:** local mock service + captured trace; reproduce N state transitions with a native operator.
- Create a mock economy: fake wallet with a real append-only ledger, mock bounty board, mock payment confirmation. Payouts are verifiable local events only.
- Implement `spacetime-bench run --arm A|B|C --model <id> --budget <tokens>` and a machine-readable report.
- Build a **static HTML** console prototype in `apps/spacetime-console/`:
  - branch/goal tree, operator registry, verifier pass rates, ledger tail, blockers, resource/cost meters, execution tier, Ark/trust domain, egress class.
  - **Mixed-workspace tiles**: one human GitHub/terminal tile and one AI-driven app tile, each with `WorkspaceId`, `SurfaceId`, trust tier, `LivenessState`, `PreviewMode`, and last action.
  - The AI tile renders a scripted **snapshot strip**: still placeholders after "typed title", "filled description", "selected thumbnail", driven by ledger events. This is the cheap screenshot/flipbook preview, not continuous video.
  - Explicit warning: cells in one Ark share a guest kernel.
  - Reads exported JSON/JSONL reports; no Electron, no WebView runtime, no live backend required.
- Document the leak list and replace "0 traces" wording in all future docs.

### Tests / definition of done

- All 20 tasks run offline end-to-end with the scripted cortex.
- Each task declares an independent verifier **before** the agent starts.
- No fixture touches the public internet, a real wallet, a real account, or a live target.
- The console renders a real ledger and correctly labels trust tier and `net = none`.
- The AI snapshot strip updates from ledger events while the human tile stays available; selecting a different tile does not stop the event stream.
- No tile pretends to be a live video stream when `PreviewMode` is `SnapshotOnAction`.
- Bench output includes completion, interventions, wall-clock, tokens, cost, claims surviving re-verification, and false-PASS.

### Kill / stop criterion

If a task cannot be made independently verifiable locally, remove it from the benchmark. Do not replace a verifier with model judgment.

---

### Block 4 — Population-per-turn cycler and the 90-day go/no-go gate

**Why now:** This tests the central claim: one model turn should yield 8–12 candidates, then local selection should run cheaply and in parallel without another model call.

### Build

- Add an **API Cortex adapter** first (config-driven provider list, per-provider budget, canary self-test, health state). Treat provider free-tier lists as config, not code.
- Implement a batch sampler that emits 8–12 delimited candidates in one reply.
- Implement the cycler:
  - parse candidates into typed actions,
  - cheap WASI/process filter for narrow candidates,
  - expensive Ark/quarantine execution only for survivors,
  - independent verifier per candidate,
  - promote only deterministic, independently verified winners.
- Implement the three benchmark arms from `DESIGN-REVIEW-2.md`:
  - **A:** single-shot, no forks, no graph.
  - **B:** naive best-of-8 independent samples.
  - **C:** population-per-turn + cycler + operator promotion.
- Keep all arms on the same token budget and same fixtures.
- Add per-model competence tracking from ledger results; add a `human` fallback adapter.
- Add model hot-swap handoff: supervisor-generated `BRIEF.md` containing verified state, blockers, goal tree, and prohibitions.
- Defer the **web-CDP adapter** until API and scripted paths are stable; when added, put it behind a canary that self-tests parsing and silently shifts weight to API on failure.

### Metrics

- Task success.
- **Tokens per success.**
- Wall-clock per success.
- Human interventions per task.
- **% of claims that survive independent re-verification.**
- **False-PASS rate** over 50 repeated runs.
- Population size, cheap-tier kill rate, expensive-tier concurrency, peak RSS/CPU on the recorded 4 GB machine.

### Kill criteria

- If **C does not beat A by ≥2× on success at equal tokens**, the cycler's complexity is not paying for itself.
- If **C's false-PASS rate >1%**, stop and fix determinism.
- If any arm needs **>10 human interventions per task**, the autonomy thesis is not demonstrated.
- Early sanity check from `DESIGN-REVIEW.md`: after ~4 weeks, if fewer than 3/10 tasks complete with <2 interventions, reconsider the thesis before building the shell.

### Definition of done

A reproducible report shows A/B/C results, raw ledger paths, machine baseline, and an explicit `GO` / `PIVOT` / `STOP` decision recorded in an ADR. **This is the most important gate in the plan.**

---

### Block 5 — Durable autonomous supervisor, three-layer memory, and the thin mixed-workspace slice

**Why now:** Only after the loop shows measurable value do we invest in long-running autonomy. This block also delivers the smallest honest proof of the user-confirmed requirement: a human app tab and a snapshot-based AI app tab coexisting.

### Build

- Event-sourced state machine in the supervisor:
  - atomic goal checkout,
  - two clocks: fast tool loop and slow strategy/budget review,
  - steering mailbox with TTL,
  - blocker queue instead of modal approvals,
  - capability tokens: `spend ≤ X/day`, `post ≤ N/day`, no irreversible deletes, etc.,
  - crash/sleep/restart from ledger, no duplicate side effects,
  - model swap via `BRIEF.md`.
- `.space` as a **git repository** plus `manifest.toml`:
  - `00_CONSTITUTION.md` compiled to policy later in Block 6,
  - `01_GOALS.md` with YAML front-matter `id, parent, budget, kill_metric, deadline`,
  - `02_NOW.md` regenerated each turn, ≤80 lines, never appended,
  - `03_LEDGER.jsonl`,
  - `04_FACTS` typed records,
  - `07_EXPERIMENTS.md`, `09_BLOCKERS.md`, `BRIEF.md`.
- Three-layer memory from `DESIGN-REVIEW-2.md`:
  1. **Operators:** compiled typed `(pre, op, post+verifier)`, versioned, health-checked.
  2. **Facts / negative results:** typed, evidence pointer, TTL, first-class `DEAD(repo, reason, date, evidence)`.
  3. **Prose:** `NOW.md`, `BRIEF.md`, hypotheses; capped, regenerated, never load-bearing.
- BM25 retrieval via `tantivy` over long-term memory; do not dump all `.md` into prompts.
- Frontier-only prompt while the graph is small; document that >40 operators will need a planner (beam/A* with LLM-written heuristics).
- Kill-or-scale policy: experiment declares kill metric before spend; supervisor flips to `DEAD` when expired.
- **Thin mixed-workspace slice (confirmed requirement):**
  - One lightweight human tab: a scripted terminal/GitHub-program cell with a text surface.
  - One AI-driven tab: a mock upload form with `title`, `description`, and `thumbnail` fields, backed by a local fixture rather than a real website.
  - The supervisor injects AI actions into the AI cell; `SurfaceBroker` renders a RAM-only still after each meaningful action and on tab switch.
  - `RUNNING` means the AI keeps working while the tab is hidden; `RENDERING` means the active tab receives a snapshot. Switching tabs changes `RENDERING`, not `RUNNING`.
  - Two channels from day one: the human sees the snapshot; Cortex sees only a sanitized semantic observation with password-like fields masked.
  - No real browser, VM, account, money, or continuous video is required for this slice.

### Tests / definition of done

- Kill the process mid-run; restart produces exactly one continuation and no duplicate external action.
- Swap Cortex adapter mid-task; `BRIEF.md` is sufficient to continue.
- A blocker does not halt the queue; agent pivots and the human reviews in a batch.
- A steering message expires after N turns.
- A `DEAD` fact with expired TTL is reintroduced as an open problem.
- `02_NOW.md` never grows past its cap and is regenerated, not appended.
- Switch away from the AI tab for N actions, switch back, and see the current title/description/thumbnail state as a fresh snapshot — not a log-only summary.
- The human tab remains usable while the AI tab continues in the background.
- A queued "publish" action does not execute until its capability token or approval is satisfied.
- No password-like field value appears in an AI observation; no snapshot is written to disk.

### Kill / stop criterion

If durable restart cannot be made exactly-once for external effects, do not connect real credentials or money. If the AI tab is only a chat log or progress bar rather than a visible app surface, this block is not done.

---

### Block 6 — Ring -3 policy, secrets, observation firewall, red-team

**Why now:** Prompt injection, not malware, is the #1 risk to a wallet-bearing agent. Guardrails in prose are suggestions.

### Build

- Compile `00_CONSTITUTION.md` into a machine-readable Cedar policy. A violation is a type error, not a warning.
- Validate every action against: goal, capability token, budget, egress class, taint, trust tier, and verifier requirement.
- Taint propagation: `trusted(user)`, `trusted(constitution)`, `untrusted(web)`, `untrusted(tool-output)`. Policy rule: untrusted-derived actions cannot spend, use credentials, or delete without explicit approval.
- Observation firewall:
  - render semantic tree rather than raw screenshots for Cortex; the human tab may receive real pixels or RAM-only stills, but the AI channel never receives raw secret-bearing screenshots,
  - mask `type=password` by construction,
  - redact PII patterns before Cortex sees them,
  - screenshots only for tabs with no injected secret.
- Symbolic vault tokens `@vault:name`; host substitutes at the last possible moment. Add per-goal injection counters and canary fake secrets.
- Keep secret values out of guest/cell fs, env, argv, and ledger. Store only handles and digests.
- Add `spacetime-ctl ledger verify` as a first-class command; hash-chain includes policy decision and verifier proof.
- Add a red-team suite: malicious README, tool output with a fake secret, prompt injection asking to spend/delete, ledger tampering, capability expiry, canary exfiltration.

### Tests / definition of done

- A red-team attempt to extract a secret using only the agent's legitimate channels fails in under one hour of adversarial testing.
- A tainted web page cannot authorize a spend even if the model complies.
- Canary token appearing anywhere triggers an alarm.
- Ledger verification detects any edit.
- No secret value appears in fixtures' process listing, env dump, filesystem diff, or observation log.

### Kill / stop criterion

If a secret can be exfiltrated through a normal observation, **do not proceed to browser login or real accounts**.

### Deferred to Block 10

Session-cookie airlock, virtual WebAuthn, and gate-pass/snapshot-AI CDP browser. They need a real browser and are unsafe before this block passes.

---

### Block 7 — WASI worker, Ark cells, resource governor, and SurfaceBroker

**Why now:** This is the first real "body", but still headless and local. It implements the tiered execution ladder instead of one VM per app/tab.

### Build

- Define `ExecutionDomain`, `TrustDomain`, `ResourceBudget`, `EgressClass`, `Runner` trait, and the read-only `SurfaceBroker` contract in `spacetime-exec`.
- The surface stream is display-only: a tab can show a cell/VM surface, but only the supervisor control channel can inject input. This keeps "I see the AI typing" from becoming "the UI process drives the guest".
- Implement `PreviewMode` selection in the resource governor: `LiveDamage` only when measured cheap; otherwise `SnapshotOnAction` / `SnapshotOnSwitch` / `Headless` / `TerminalBuffer`.
- **Tier 1:** Wasmtime/WASI module in a separate constrained worker process. Explicit input/output handles only; memory, fuel/time, and concurrency limits. Never run Wasmtime inside the UI process.
- **Tier 2:** Ark cell manager inside a shared guest kernel:
  - mount/PID/net/IPC namespaces,
  - read-only base image + ephemeral OverlayFS upper,
  - cgroups v2 CPU/memory/process limits,
  - seccomp and capability drops,
  - no inherited network sockets, no unrestricted vsock, no access to Ark control socket,
  - supervisor-owned channel credential identifies the cell.
- Default domains: `Ark-Work` for compatible ordinary tools, `Ark-Quarantine` for unknown code. Unknown repos/build scripts never enter Work by default.
- Resource governor:
  - temporal multiplexing, queue rather than oversubscribe,
  - measure host RSS, cell private memory, shared page cache, overlay growth, zram CPU, branch peak, and wall-clock,
  - suspend/stop idle cells,
  - no fixed "cells per VM" claim until measured.
- `net = none` test must prove no usable path, not merely check a label.
- Add an intent ribbon data source: `read N files · wrote M · DNS 0 · will spend $0.00 · risk: low`.

### Tests / definition of done

- A `net = none` cell cannot connect to host localhost, LAN, metadata IP, or a supervisor test listener.
- A fork bomb / memory hog is killed by cgroup without freezing the host.
- A WASI module cannot open an undeclared file or socket.
- A cell cannot forge another cell's identity at the broker.
- Resource report is generated from actual measurements, not the brainstorm's table.
- Two cells in one Ark are clearly labelled as sharing a kernel in policy and console.
- A surface consumer cannot inject input; only the supervisor control channel can.
- A hidden AI workspace stays `RUNNING` while its surface is not `RENDERING`; reattaching produces a fresh current snapshot.
- Snapshot capture stays within a measured CPU/RAM budget and writes no file to disk.

### Kill / stop criterion

If any cell reaches a host-local port, **stop everything** until the broker boundary is fixed.

---

### Block 8 — Disposable quarantine microVM, host defense, and autopsy

**Why now:** Hardware virtualization remains essential for unknown/hostile code. It is a **trust-domain tool**, not a per-tab default.

### Build

- Run a measured VMM spike before committing:
  - candidates: `libkrun`/`muvm`, `cloud-hypervisor`/`microvm.nix`, `firecracker` for snapshot/offline compute only.
  - compare on the recorded 4 GB machine: boot/restore time, RSS, virtio-fs, virtio-console, snapshot support, rootless setup, sandboxability, and device attack surface.
  - write an ADR; do not assume DAX, WHPX, or snapshot/restore support.
- One **supervised child process per quarantine VM**, never embedded in the UI process.
- Sandbox the VMM itself: bwrap/Landlock/seccomp, cgroup v2, `ulimit -c 0` / `LimitCORE=0`, no host network, minimal filesystem, `memfd` + `MADV_DONTDUMP` where supported.
- Reproducible minimal guest:
  - direct kernel boot, EROFS/squashfs base with pinned digest,
  - tiny static Rust PID 1, no systemd/dbus unless measured,
  - `virtio-console` for terminal evidence,
  - ephemeral tmpfs/OverlayFS upper destroyed on close,
  - kernel without virtio-net for offline tabs.
- Host leak audit automation: core dumps, crash reporters, swap/zram, clipboard history, DNS, recent-files lists, thumbnails, GPU shader caches, audio.
- Autopsy: upperdir diff, whiteouts, egress summary, process/resource summary, `blast_radius_hash`; write a few-KB **tombstone ledger** on close without retaining malware.
- Warm paused VM pool only after cold boot is measured; label perceived-launch vs app-startup separately.

### Tests / definition of done

- A hostile fixture cannot reach host localhost, LAN, metadata, or host UNIX sockets.
- VMM crash produces no core dump containing guest RAM.
- Deleting a quarantine tab removes its upperdir and leaves only the declared tombstone.
- Autopsy correctly reports a canary file read and a blocked connection.
- Cold boot, restore, and first-paint times are recorded separately on the target machine.

### Kill / stop criterion

If a guest can reach any host-local port or the VMM crash leaks guest memory, this block is not done.

---

### Block 9 — `.wavefunction` CAS, reflink branching, and `.space` portability

**Why now:** Solves the original "shared dependencies vs zero traces" contradiction with structure, not convention.

### Build

- Implement immutable append-only CAS:
  - base EROFS image digest pinned in a lockfile,
  - content-addressed layers,
  - provenance tiers: `upstream-signed`, `repro-built`, `tab-local`, `quarantined`,
  - only `upstream-signed`/`repro-built` may be shared globally,
  - `tab-local` is invisible to other tabs; `quarantined` is deleted unless explicitly promoted.
- `Collapse` operation: promote a verified diff from a tab into the CAS with verifier proof and build log.
- Branch Tab = three cheap operations in order:
  1. `git commit` in `.space` memory repo,
  2. `cp --reflink=always` of frozen overlay on btrfs/xfs; measured fallback on ext4,
  3. opt-in RAM "deep freeze" snapshot only when exact resume is requested.
- Export/import `.space` as zstd archive:
  - `manifest.toml`, memory git repo, operator/skill registry, vault **index only**, ledger head, rootfs delta, compatibility metadata.
  - compatibility gate: CPUID mask, kernel/VMM version, base digest; fall back to cold boot from delta.
  - Promise "resumes at the exact **state**", never "exact millisecond on another machine".
- Add `spacetime-ctl space branch|export|import|verify`.

### Tests / definition of done

- A quarantined or tab-local artifact cannot appear in another tab's CAS view.
- Promoting without a verifier proof fails.
- Branch reproduces memory/filesystem state exactly on a CoW filesystem; measured target <200 ms where supported.
- Import on incompatible host falls back to cold boot rather than failing or pretending.
- Export contains no secret values, only handles/index entries.

### Kill / stop criterion

If branch is not fast and reproducible on the target filesystem, defer the "franchise" story rather than build on it.

---

### Block 10 — Egress broker, airlock, and gate-pass + snapshot AI browser tab

**Why now:** Only after isolation, policy, and secrets are proven does external interaction become admissible.

### Build

- Supervisor-owned egress broker:
  - identify cell by channel credential, not guest data,
  - deny host-local, LAN, link-local, metadata, and host UNIX sockets,
  - classes: `none`, `proxy(name)`, `direct`, optional explicit `tor`,
  - DNS tunnel-only, pinned per-tab resolver; never host `resolv.conf`,
  - per-flow audit tap into ledger/console,
  - no `smoltcp` as a homegrown switch; evaluate TSI/passt instead,
  - no TLS morphing by default; no "undetectable" promise.
- File/clipboard airlock:
  - clipboard pull-only, gesture-initiated, strip ANSI/zero-width/bidi, cap size, purge host clipboard history,
  - files in: per-tab volatile drop directory, read-only export, no auto-sync,
  - files out: quarantine xattr / Mark-of-the-Web / `com.apple.quarantine`, one-line autopsy warning,
  - `dangerzone`-style UX language.
- Gate-pass + AI-driven browser:
  - one hardened real Chrome under CDP, per-`.space` stable profile, no other traffic,
  - used for login, attestation, CAPTCHA/verification gates, trace capture, **and AI workspaces that genuinely require a real page** (for example filling a YouTube upload form),
  - AI actions are injected by the supervisor through CDP/virtio-input; the active tab receives a live surface only if cheap, otherwise a RAM-only still after each meaningful action,
  - Cortex receives only the sanitized semantic tree, never raw pixels from a secret-bearing surface,
  - virtual WebAuthn/passkeys where supported; private key in host vault,
  - session-cookie airlock: login in ephemeral minimal tab, transfer only resulting cookie, destroy login tab,
  - one interaction per **irreducible gate**; compile everything downstream into native operators,
  - budget rule: at most one heavy browser surface awake at a time; hidden AI work continues headlessly and reattaches on tab switch,
  - posting/uploading is an irreversible external effect and remains inside a capability token or blocker queue.
- Legal guard: no CAPTCHA-solving-for-hire, no multi-account farm, no synthetic identities, no off-scope automation. KYC is the human; the agent acts as the human's permitted agent.

### Tests / definition of done

- A networked cell cannot reach a host-local test server, host LAN, or `169.254.169.254`.
- DNS query never appears in host resolver logs.
- Clipboard push/pull cannot leak rich content or retain history.
- Exported file carries quarantine metadata and autopsy summary.
- Password field never appears in an observation; virtual authenticator credential never enters guest fs.
- An AI-driven browser tab shows live-or-snapshot field changes to the human while the AI observation contains no password values.
- Switching away from the browser tab does not stop the AI; switching back reattaches to the current page state.
- A "publish video" action without approval/capability remains queued.
- No screenshot is persisted to disk or host thumbnail cache.
- Browser gate canary fails safely over to blocker queue + API/human channel.

### Kill / stop criterion

If any guest/cell reaches a host-local port, stop all external work.

---

### Block 11 — Operator registry, protocol distillation, skill health

**Why now:** This is where repeated GUI/CLI work becomes a durable, testable machine capability rather than a brittle prompt loop.

### Build

- Operator registry with:
  - typed `(pre, op, post + verifier)`,
  - provenance (model/turn/trace), version, rollback,
  - mutation score, coverage/differential input count,
  - health check schedule, `SUSPECT`/`DEAD` state,
  - drift quarantine: verifier flip reintroduces the edge as an open problem,
  - periodic re-derivation from spec; disagreement is a signal.
- Protocol distillation chamber:
  - capture a trace from gate-pass browser or smoltcp-like inspection only where permitted,
  - compile downstream of an irreducible gate into a native WASI/Rust operator,
  - require declared postcondition and verifier strength evidence,
  - keep 5–20 maintained operators rather than 500 brittle scrapers.
- Skill crystallization:
  - every skill ships with a test and `last-green` timestamp,
  - nightly self-test disables stale skills and falls back to exploration,
  - 0-LLM deterministic replay for known-good runs,
  - skill provenance and success rate in ledger.
- Add critic/scribe "parliament" only as measured improvement: executor, independent critic with trace only, scribe compressing `NOW.md`.
- Planner for frontier-only prompts once operator count makes enumeration expensive.

### Tests / definition of done

- A deliberately drifted mock service flips its operator to `SUSPECT` and reopens the frontier.
- A failing nightly skill test disables that skill without crashing the run.
- A known-good run replays with zero model calls and byte-identical verifier evidence.
- A negative result (`DEAD(repo, reason, date, evidence)`) prevents re-testing until TTL expiry.

### Kill / stop criterion

If operators silently pass while wrong, tighten mutation/independence requirements before promoting any more.

---

### Block 12 — Live mixed workspace shell: human apps and AI work side by side

**Why last, but not optional:** This is the full realization of the user-confirmed requirement. The thin slice in Block 5 already proved the interaction with snapshots; this block makes it real for GitHub programs, terminals, browser apps, and AI-driven workspaces. Cross-domain Wayland remains a research track, and continuous live video remains optional.

### Build

- Port the static console to a small Rust `egui` application (no Electron/WebView):
  - ledger, blockers, operator health, verifier rates, resource/cost meters, trust-domain labels, intent ribbon.
- Tab model: `WorkspaceId → SurfaceId → execution domain`. A tab is a **view**, never a kernel. One VM per app is explicitly not the architecture.
- Support heterogeneous tabs in one strip:
  - GitHub program someone else wrote → quarantine cell or disposable microVM, terminal/framebuffer surface.
  - Terminal app → light Ark cell, text buffer.
  - Browser app → one dedicated browser cell, page surface.
  - AI workspace → supervisor-driven isolated app/browser cell, snapshot or live surface.
- `SurfaceBroker`:
  - read-only display routing; cannot inject input,
  - active tab attaches to a surface; background tabs keep `RUNNING` without `RENDERING`,
  - `PreviewMode` chosen by resource governor: `LiveDamage` only if measured cheap, otherwise `SnapshotOnAction`, `SnapshotOnSwitch`, `Headless`, or `TerminalBuffer`,
  - snapshot strip shows a captioned flipbook: "typed title", "filled description", "selected thumbnail", with a fresh still on tab switch.
- Two views of the same workspace:
  - human sees real pixels/stills,
  - Cortex sees the sanitized semantic tree; password fields and secrets are masked by construction.
- AI actions are injected only through the supervisor control channel. The shell never sends raw host mouse/keyboard events into a guest.
- Concurrency on the 4 GB machine:
  - one heavy browser/GUI surface awake at a time,
  - terminal and headless AI work can continue in background,
  - switching tabs reattaches to current state without replaying from scratch,
  - temporal multiplexing applies to heavy surfaces, not to the AI's right to keep working.
- Irreversible effects such as "publish video" remain gated by capability token or blocker queue; the UI shows the pending action next to the snapshot.
- Terminal tabs: `virtio-console` → `alacritty_terminal`/`egui_term`; make this path excellent because it is the cheapest instant surface.
- One GUI path only: per-VM framebuffer → host texture, damage-limited repaint, software-rendering assumption; guest may need a tiny headless compositor/Xvfb. Defer `waypipe`, cross-domain Wayland, virtio-gpu context types.
- Warm-VM pool after cold boot/restore are measured; report boot-to-shell, restore-to-exec, app-first-paint, and snapshot-capture latency separately.
- cgroup limits, autopsy UI, tombstone view, attention/electricity meter, capability-drift diff.
- Import/export `.space` through the UI; compatibility gate visible to user.
- Document Windows/macOS caveats and Apple Silicon arm64 Linux guest constraint.

### Tests / definition of done

- Scenario: Tab A runs a GitHub program, Tab B is a terminal, Tab C is the AI preparing a video upload. Switch to Tab C and see the title, description, and thumbnail selection change via live or snapshot updates — not a chat log.
- Switch away from Tab C for N AI actions, return, and immediately see the current state; the AI did not pause.
- The human GitHub/terminal tab remains usable while the AI tab works in the background.
- A `SnapshotOnAction` tab is visibly labelled as snapshot mode; it does not pretend to be 60 fps video.
- No raw secret-bearing screenshot reaches Cortex; no screenshot touches disk or host thumbnailer.
- Terminal tab is indistinguishable from a native terminal for ordinary use.
- Tab open time and snapshot reattach time are measured on the recorded 4 GB machine; target <1 s perceived, but report actual numbers.
- No host-local egress from any tab; console warns when cells share an Ark kernel.
- No per-frame full-screen compositing; host shell stays within measured memory budget.

### Kill / stop criterion

If switching tabs freezes the AI, shows stale state, or reduces the AI tab to a text log, fix that before adding polish. If continuous live rendering breaks the 4 GB budget, fall back to snapshots rather than removing visibility.


---

### Block 13 — Optional polish, external pilot, and continuous organism work

These are explicitly **not required for v1**. Each needs its own measurement, legal review, and ADR. Some should stay unbuilt.

- Reality Oracle with external read-only probes, only after real-account decision.
- Remote Cortex / local Body hairpin over WireGuard for 4 GB relief.
- Parliament routing by observed model competence.
- Shadow twins for business decisions against mock verifiers.
- Persona seeds: stable SMBIOS/CPUID/MAC/locale/canvas profile per `.space`; no "anti-fraud invisible" claim, no per-load randomization.
- Honeytokens / canary files / capability-drift detection / malware zoo autopilot.
- Cross-machine teleport with compatibility gate; cold-boot fallback.
- TLS/HTTP2 morphing, TCP fingerprint normalization: opt-in research only.
- Smart-contract audit, OSS vulnerability rewards, differential porting as **in-scope, local-first** pilot domains.
- Real money only with explicit human identity, KYC, merchant-of-record, platform ToS, and disclosure decisions.
- 30-day unattended run with metrics; if completion, interventions, tokens/success, and false-PASS do not improve, do not scale.

### Permanently out of scope unless the user explicitly overrides with legal review

- Live exploits outside authorized program scope.
- Mass synthetic accounts, ban evasion, CAPTCHA bypass, ad/affiliate farms, "10 YouTube channels" as synthetic identities.
- Live trading/arbitrage as an early revenue line.
- Claims of absolute host invisibility, zero traces, or undetectability.

---

## 7. Cross-block measurement protocol

Every block that touches execution must update `bench/machine.toml` and a report:

- Exact host: OS, kernel, CPU, arch, RAM, swap/zram, filesystem, KVM availability.
- Host RSS and CPU; guest/Ark/cell RSS; private vs shared pages.
- Overlay growth, CAS hit rate, page-cache reuse, branch peak memory.
- Cold boot, restore, exec, verifier, app-first-paint, end-to-end task times.
- zram compression ratio and CPU cost under realistic concurrency.
- Browser working set only when the gate-pass or AI-driven browser is enabled.
- Active vs background surface cost, snapshot capture latency/CPU, tab-switch reattach latency, AI actions completed while a tab was hidden, and number of simultaneously `RENDERING` surfaces.
- Tokens, wall-clock, human interventions, false-PASS, claims surviving re-verification.

### Resource gate (applies to every block)

A block is not `DONE` until its report shows: host RSS and CPU within budget, no hidden always-on browser/daemon, peak memory under concurrent work, snapshot/capture cost, tab-switch reattach latency, and the cheapest `PreviewMode` that still satisfies the user's visibility requirement. If the budget is exceeded, the block must downgrade, suspend, queue, or kill work — not rewrite the target machine.

No number from `BRAINSTORM.md` or the LLM replies becomes a README claim until it appears in a measured report for the recorded machine.

---

## 8. Decision gates the user should confirm

**Already confirmed by the user:**

- No one-VM-per-app; use tiered execution (WASI / Ark cells / quarantine microVMs).
- GitHub programs, terminal apps, browser apps, and AI workspaces must coexist in one tabbed shell.
- The AI tab must show real app changes when selected.
- If continuous live preview is too expensive on the 4 GB machine, use screenshots / a snapshot flipbook instead.
- **Resource usage is the holy grail.** Every block has a measured resource gate; no feature ships merely because it is impressive.

**Still to confirm:**

1. **Which single 90-day hunting ground?** Default: planted-invariant local fixtures.
2. **Is Linux-only v1 acceptable?** Default: yes; Windows/macOS later with warnings.
3. **Is one hardened gate-pass Chrome allowed?** Default: yes, rare; otherwise keep all targets local/OTS.
4. **Accept the legal line?** Default: own identity, in-scope local forks, disclosure norms, no synthetic identities or live exploits.
5. **How much wall-clock per decision?** Sets population size and concurrency budget.
6. **Which workloads may share an Ark kernel?** Default: only ordinary compatible tools; unknown repos/build scripts go to quarantine.
7. **Which exact 4 GB laptop defines the benchmark?** Needed before any performance claim.
8. **When may real money/accounts be introduced?** Default: not before Block 13 and explicit sign-off.
9. **Should hidden AI tabs keep rendering?** Default: **no**. The AI stays `RUNNING`; only the active tab is `RENDERING`. Switching back produces a fresh snapshot. If true background pixel streaming is required, it becomes a measured RAM/CPU decision.

If these defaults are accepted, **the measured 4 GB resource gate is the next unblocked unit**. Block 1 and Block 2 functional increments are merged to `main` via PRs #7 and #8; PR #9's bounded measurement intake, CLI bug hunt, and doc hygiene are merged history at `90ceeb2`, with strict `main` CI run `43829756506` green. The current active branch adds a read-only target-machine host-fact collector and an out-of-band provenance template; it commits no measurement file, no filled provenance record, and no code path that accepts physical provenance as a measured pass. After the recorded target machine is identified, measured with accepted out-of-band provenance, and a reviewed gate implementation consumes that provenance, Block 3 becomes the next implementation unit. The thin mixed-workspace slice in Block 5 is the earliest visible proof of the confirmed requirement, using snapshots rather than expensive live video. If you want the full shell pulled forward, we can swap Block 12 earlier, but keep Block 5 as the minimum honest integration test.


---

## 9. Maintenance rule

After each block:

- Update the block status table with `DONE` / `IN PROGRESS` / `BLOCKED` / `KILLED`.
- Add one ADR for every architecture choice made by a spike.
- Attach the benchmark report, **resource report**, and ledger sample to the block PR.
- Record the chosen `PreviewMode`, peak RSS/CPU, and any downgrade/suspend/kill decision.
- If a kill criterion fires, write an ADR explaining pivot or stop. **Do not silently continue.**
- Keep this plan as the map; move detailed schemas into `docs/` only when they are implemented.
