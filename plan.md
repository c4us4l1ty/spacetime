# Spacetime build plan

> **Audience:** every coding agent (and human) who continues this repository.
> **Authority:** `IMPORTANT.md` is the master brainstorm — it defines *what* Spacetime is. This file defines *the order we build it* and the contracts that must not be broken. `README.md` is the quick start. Never rewrite `IMPORTANT.md` to fit an implementation; change the implementation, or record the decision here.
>
> Last updated 2026-11-11, when the green simulation baseline was merged to `main`.

---

## 0. Read this first

- **This repo is a complete, tested simulation — not the product.** 7 crates, ~7,900 lines of dependency-free Rust, 118 unit tests, all CI gates green. There is **no** hypervisor, guest kernel, real network stack, native GUI, cgroups, or AI model connected. `spacetime serve` runs a deterministic demo: `engine.tick()` fabricates screens, flows, dirty pages and agent decisions.
- **Do not throw the simulation away.** Every subsystem already exposes a trait or wire protocol (§3) that the real implementation plugs into. The simulation is the conformance oracle and the reason CI runs on machines without KVM/WHPX.
- **CI is the Rust compiler.** There is typically no local toolchain. Push a commit, read the diagnostics that CI posts, push a fix. See §2.
- **Branching:** work on `arena/**` branches, open PRs into `main`, merge with `gh pr merge <n> --merge`. Do not push directly to `main`.

## 1. Status snapshot

| Crate | Exists today | Still missing |
| --- | --- | --- |
| `spacetime-core` | Tab tree/branching, trust levels, SHA-256 CAS (`MemBackend`, `DirBackend`), sparse `MemoryImage` + rewind, JSON encoder/parser | `.wavefunction` manifest, BLAKE3, real overlay/virtio-fs/DAX wiring |
| `spacetime-vmm` | `SpacetimeVmm` trait, `SimulatedBackend`, `NullBackend`, `Zygote` state machine, modelled CoW/timings | libkrun / crosvm / cloud-hypervisor backends, `mmap(MAP_PRIVATE)` clones, dirty-page logging, register/device state, anti-VM spoof |
| `spacetime-net` | `Switch` policy (allow/block/sinkhole), static GeoIP/ASN, fake replies, TTL/spoof fields in JSON | smoltcp/virtio-net byte path, DNS interception, real sinkhole socket, egress tunnels, JA3/JA4 + p0f rewriting |
| `spacetime-warden` | Focus-aware policy, `ChokeSink` trait, `RecordingSink`, balloon accounting | cgroup v2 / Job Object / `task_policy` sinks, `pids.max`, real balloon device |
| `spacetime-agent` | Goal DAG, scratchpad, compactor/session flush, heartbeat, steer channel, auto-snapshot/rollback logic, SoM formatting, `Brain` trait | real model brains, ONNX vision, `virtio-input` injection, per-tab/sub-agents, on-disk `task_graph.json`/`state.md`, CDP brain pool |
| `spacetime-shell` | std-only HTTP/JSON server, dark HTML dashboard (tabs, scrubber, Honey-Duct, DAG, ghost cursor), 9 CLI commands | native iced/egui + wgpu window, Wayland/Smithay + waypipe, `alacritty_terminal`, omnibar, clipboard/file airlock |
| `guest-init` | `HostCommand`/`GuestEvent` wire protocol, `BOOT_PLAN`, static musl cross-builds | real PID 1 syscalls (`mount`/`pivot_root`/`openpty`/`fork`/`execve`), hvc0/hvc1 agent, vsock agent, waypipe launcher |

## 2. CI as the Rust compiler — working rules

Workflows (`.github/workflows/`): `ci.yml` on **all** branches; `diag.yml` and `fmt.yml` on `arena/**` **and `main`**.

```bash
# 1. push, then watch the three runs
gh run list --limit 8

# 2. read the remote compiler output (newest comment wins; comments are oldest-first)
SHA=$(git rev-parse HEAD)
gh api "repos/c4us4l1ty/spacetime/commits/$SHA/comments" --jq '.[-1].body' \
  | sed -E 's/\x1b\[[0-9;]*[A-Za-z]//g'

# 3. per-job verdicts
gh api repos/c4us4l1ty/spacetime/actions/runs/<run-id>/jobs \
  --jq '.jobs[] | "\(.name): \(.conclusion)"'
```

- `diag.yml` posts a table `check / clippy / test / fmt / guest` with exit codes **and the full failed logs** on every push, green or red. **Every number must be `0`.** A `diag` run marked "success" only means the comment step posted — always read the table.
- Gates: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets` under `RUSTFLAGS: -D warnings` (Linux/macOS/Windows); `cargo test --workspace --no-fail-fast`; release build; `versions`, `run --ticks 40`, `serve --port 7420` returning valid `/api/state` with keys `tabs, travel, net, warden, agent, cursor, zygote, log` and `tabs.live >= 3`; static `guest-init` builds for `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` (the latter needs `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-linux-gnu-gcc`).
- **Formatting without rustfmt:** push any commit whose message contains `[fmt]` (an empty commit is fine). `fmt.yml` applies `cargo fmt --all`, commits `style: cargo fmt`, and pushes to the current branch. Commits made with `GITHUB_TOKEN` do **not** trigger new workflows, so follow a format commit with an ordinary push to re-run the gate. Now that the workflows exist on `main`, `gh workflow run fmt.yml` / `diag.yml` also works.
- Direct log downloads (`gh run view --log`) are unreliable in this environment (EOF/SSL to Azure blob storage). Use commit comments, check-run annotations, and uploaded `logs-*` artifacts instead.
- Do **not** install or run a local `cargo` to "check quickly", and do not guess at compiler errors. Iterate through commits.

## 3. The seams — contracts real implementations must satisfy

| Seam | File | What to implement |
| --- | --- | --- |
| `SpacetimeVmm` | `crates/spacetime-vmm/src/lib.rs` | `availability`, `boot`, `clone_from_zygote`, `pause/resume/stop`, `snapshot/restore`, `stats`, `list`; `VmConfig`, `VmHandle`, `VmStats`, `VmmError` are the shared vocabulary |
| `Zygote` | `crates/spacetime-vmm/src/zygote.rs` | golden VM paused after init; clones share host pages until written |
| `MemoryImage` / `TimeTravel` | `crates/spacetime-core/src/time_travel.rs` | dirty-page snapshots, ring buffer, `rewind_to` restoring RAM **plus CPU registers and device state** |
| `ObjectBackend` / `CasStore` | `crates/spacetime-core/src/cas.rs`, `crates/spacetime-vmm/src/dir_store.rs` | BLAKE3 global store, promotion/refcounts, virtio-fs + DAX read-only mounts |
| `TabTree` / `Tab` | `crates/spacetime-core/src/tab.rs` | freeze writable overlay to read-only lower, fork CoW child, zero upper on close |
| `Switch` / `Flow` / `Verdict` | `crates/spacetime-net/src/vswitch.rs` | turn real parsed packets into `Flow`, then **enforce** `Verdict` (currently only recorded) |
| `Sinkhole`, `GeoIp` | `crates/spacetime-net/src/sinkhole.rs`, `geoip.rs` | listening fake-`200 OK` responder; real ASN database |
| `ChokeSink` | `crates/spacetime-warden/src/lib.rs` | write `cpu.max`, `memory.max`, `pids.max`, `cgroup.freeze`; Job Objects; `task_policy` |
| `Brain` | `crates/spacetime-agent/src/brain.rs` | `observe → propose → act` + `feedback`; must be `Send` |
| `Screen` / `MarkedElement` | `crates/spacetime-agent/src/vision.rs` | framebuffer capture + ONNX OmniParser/YOLO boxes; keep the numbered-mark output format |
| `Heartbeat`, `GoalDag`, `Compactor`, `SteerChannel` | `crates/spacetime-agent/src/{heartbeat,dag,compactor,steer}.rs` | per-tab agents, sub-agent tabs, disk persistence, real failure detection instead of `fail_every` |
| `HostCommand` / `GuestEvent` / `BOOT_PLAN` | `crates/guest-init/src/lib.rs` | keep the line protocol exactly; add Linux-only syscall plumbing in `main.rs` |
| `/api/*` + `/api/state` JSON | `crates/spacetime-shell/src/app.rs`, `http.rs`, `engine.rs` | native GUI consumes the **same** JSON contract; the HTML dashboard remains the headless/remote client |
| `.wavefunction` | **does not exist** | new crate `spacetime-wavefunction` (per `IMPORTANT.md` §7) or a `spacetime-core` module: manifest schema, parser/validator, immutable definition → live VM, layer freezing |

**Write a shared conformance test suite** for `SpacetimeVmm`, `ObjectBackend` and `ChokeSink`, and run it against both `SimulatedBackend`/`MemBackend`/`RecordingSink` and each real backend. That is how the simulation stays an oracle instead of becoming dead code.

## 4. Phases and definitions of done

**Phase 0 — baseline (complete).** Simulation skeleton, 7 crates, 118 tests, CI-as-compiler, dashboard, README. Gate: all five `diag` exits `0`, all `ci` jobs green.

**Phase 1 — land the baseline on `main` (this change).** `plan.md`, `diag`/`fmt` triggers extended to `main`, histories unified, PR merged. Gate: `main` contains the full tree and CI/diag are green there.

**Phase 2 — guest-side reality.** `.wavefunction` schema + parser/validator; real PID 1 (`mount`, `pivot_root`, `openpty`, `fork`, `execve`, hvc0/hvc1 console agent, vsock agent, waypipe launcher); stripped `vmlinux` + squashfs/EROFS rootfs + tmpfs overlay upper; BLAKE3 CAS with virtiofsd + DAX; sanitizer MicroVM for host↔tab files. *Done when:* a `guest-init` binary boots a minimal Linux guest in a KVM-capable CI job and answers `READY 1` over `/dev/hvc1`; a `.wavefunction` file produces a bootable rootfs; two tabs provably share one CAS object.

**Phase 3 — real hypervisor backends (the security guarantee).** `libkrun` (Linux KVM, macOS HVF), `crosvm`/`cloud-hypervisor` (Windows WHPX/MSHV); real zygote CoW via `memfd` + `mmap(MAP_PRIVATE)`; dirty-page logging; snapshot/restore with registers and device state; CPUID/SMBIOS/MAC anti-VM spoof; keep `NullBackend` for unsupported hosts. *Done when:* conformance suite passes for each backend, p95 clone < 15 ms, cold boot < 100 ms, 10 tabs share base pages, and a tab close leaves no writable state.

**Phase 4 — native workspace shell.** iced/egui + wgpu window, `alacritty_terminal` over virtio-console, Smithay + waypipe/sommelier + virtio-gpu for GUI apps at 60–144 FPS, omni-bar (`URL` / `gh:` / dropped file / `@ai`), per-tab Bezier ghost cursors via `virtio-input`, trust borders, ❄️5% badges, Brain PiP, zero-trust clipboard and file airlock with sanitizer VM. *Done when:* a terminal tab and a GUI tab run inside a real VM, accept input, render frames, and every `/api/*` endpoint still behaves identically.

**Phase 5 — real network.** `virtio-net` → `smoltcp` (or netns + `aya`), DNS interception, 3 enforced modes (airgap = device omitted), sinkhole listener with fake `200 OK`, residential/SOCKS5/WireGuard/4G egress, `uTLS`/rustls JA3/JA4 and p0f TTL/window/option rewriting, real GeoIP/ASN DB. *Done when:* a malware sample's C2 attempt appears in the Honey-Duct console and is blocked/sinkholed, an airgapped tab has no NIC, and allowed flows carry the spoofed fingerprint.

**Phase 6 — real resource choking.** `ChokeSink` implementations for cgroup v2 / Job Object / `task_policy`, `pids.max` fork-bomb ceiling, virtio-balloon free-page reporting. *Done when:* unfocused tab is capped at 5 % of one core, a fork bomb fails with `ENOMEM`, and freed pages are returned to the host.

**Phase 7 — real AI operator.** OpenAI-compatible `Brain` implementations (OpenRouter/NIM/Anthropic/Ollama) + local `127.0.0.1:11434` endpoint; framebuffer capture + ONNX (`ort`) OmniParser; `virtio-input` evdev injection; per-tab agents and sub-agent child tabs returning one-line JSON then GC'd; `task_graph.json` + `state.md` on disk, 10-tick context flush/"New Chat" wipe; Phantom Brain Pool with CDP stream interception and rate-limit failover; real failure detection + auto-rewind. *Done when:* a multi-hour unattended mission completes with context resets, a live steer takes effect on the next tick, and a destructive action triggers an automatic rewind.

**Phase 8 — productization.** Installers, signing, HVF/WHPX entitlements and permission bootstrap; config, logging, crash recovery, workspace persistence across restarts; malware-escape test suite; perf regression jobs on KVM-capable runners; docs and `LICENSE`; explicit product decision on stealth-egress and web-AI automation. *Done when:* a signed build installs on Windows/macOS/Linux and passes the escape + perf suites.

### Recommended vertical slice (do this before broad parallel work)

One Linux/KVM backend (`libkrun`), one terminal tab with a real shell over virtio-console, one `.wavefunction` manifest, one real API brain driving one goal end-to-end, and one real packet path with airgap + sinkhole. That single path exercises Phases 2/3/5/7 together and proves the architecture. Wayland, WHPX/HVF, stealth egress and the brain pool can then land incrementally.

## 5. Cross-cutting rules for agents

- **Keep the simulated path alive.** Default features must stay dependency-free and runnable on any host; gate heavy/platform deps behind Cargo features (`backend-krun`, `backend-crosvm`, `gui`, `onnx-vision`, `network-stack`, `brain-http`) with `cfg`. Linux/macOS/Windows CI must remain green at all times.
- **Dependencies:** there is no `Cargo.lock` today. The moment real crates are added, commit `Cargo.lock`; keep `rust-version = 1.75`, edition 2021, `rustfmt.toml` max width 100, and `-D warnings`. Raise MSRV only deliberately.
- **JSON is a contract.** `/api/state` keys (`tabs`, `travel`, `net`, `warden`, `agent`, `cursor`, `zygote`, `log`) are consumed by `dashboard.html` and the CI smoke test. Add keys with defaults; never remove or rename existing ones.
- **Airgap means the device is omitted**, not a firewall rule. Any change touching the isolation boundary needs a test that would fail if isolation were broken.
- **No secrets in the repo.** API keys/proxy credentials come from the environment; brains must degrade gracefully when offline; never log tokens.
- **Never weaken CI to get green.** If a real backend cannot run on hosted runners, add a dedicated self-hosted/KVM-capable job — do not delete the gate.
- **Legal/product posture:** residential-proxy routing, anti-detect fingerprint spoofing and automated use of web AI UIs have provider-ToS and legal implications. Keep them off by default and get an explicit decision before shipping.

## 6. Open decisions (record the answer here when made)

1. `.wavefunction`: new crate (matches `IMPORTANT.md` §7) vs. a module in `spacetime-core`.
2. GUI toolkit: `iced` vs `egui` vs `floem`; whether the HTML dashboard stays as a remote client long-term.
3. VMM per host: `libkrun` vs `cloud-hypervisor` on Linux; `crosvm` WHPX vs MSHV on Windows.
4. Packet path: `smoltcp` in-process vs `netns` + `aya` eBPF.
5. Brain priority: OpenAI-compatible APIs first vs. CDP Phantom Brain Pool first.
6. Hash: migrate CAS to BLAKE3 or keep SHA-256 for compatibility.
7. MSRV bump if `ort`/`smithay`/`iced` require a newer stable.
8. Whether `main` becomes the daily integration branch or stays a release branch with `arena/**` as the working branches.

## 7. Landing a change

1. Work only on your `arena/**` branch; never switch to or push another branch.
2. Edit, `git add -A`, commit, push.
3. Wait for `diag` to post its comment; confirm all five exits are `0`.
4. Confirm all `ci` jobs are `success` (`report diagnostics` / `format` are expected to be `skipped` on green runs).
5. If `fmt` is `1`, push an empty `[fmt]` commit, wait for `style: cargo fmt`, then push a normal no-op commit to re-trigger the gate.
6. Open a PR into `main` and merge with `gh pr merge <number> --merge`; verify `main` afterwards with `git fetch && git ls-tree -r --name-only origin/main`.
