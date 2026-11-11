# Spacetime

A browser-like **workspace shell** for running Linux apps inside hardware-isolated
MicroVMs. The full brainstorm (threat model, zygote snapshots, `.wavefunction`
storage, Honey-Duct networking, the autonomous AI operator and the UI wireframe)
lives in [`IMPORTANT.md`](IMPORTANT.md).

> **Status:** the workspace is a *simulated-but-faithful* engine. Every subsystem
> below runs today, dependency-free, on any host — including CI runners that are
> themselves VMs. Real KVM / Hypervisor.framework / WHPX backends, `virtio-fs`,
> `smoltcp` and ONNX vision are phase 3; the traits and wire formats they will
> implement already exist.

## Quick start

```bash
cargo run -p spacetime-shell -- serve --port 7420     # dashboard + JSON API
cargo run -p spacetime-shell -- run --ticks 40        # headless heartbeat log
cargo run -p spacetime-shell -- state --pretty        # one JSON state snapshot
cargo run -p spacetime-shell -- tabs                  # the git-style tab tree
cargo run -p spacetime-shell -- branch --tab 1        # fork a tab from a CoW snapshot
cargo run -p spacetime-shell -- store Cargo.toml Cargo.toml   # CAS dedup demo
cargo run -p spacetime-shell -- versions
cargo test --workspace                                # 118 unit tests
```

`serve` exposes a zero-dependency dark-mode dashboard at `/` and the engine's
state at `/api/state`. Commands such as `/api/open`, `/api/branch`, `/api/rewind`,
`/api/steer` and `/api/trap` drive the same engine the native shell will use.

## Workspace map

| Crate | Role |
| --- | --- |
| `spacetime-core` | Tabs & trust levels, SHA-256 CAS store, CoW memory images, time-travel ring, JSON wire format |
| `spacetime-vmm` | `SpacetimeVmm` trait, deterministic simulated backend, zygote clones, on-disk CAS backend |
| `spacetime-net` | User-space switch, GeoIP/ASN attribution, Honey-Duct sinkhole, p0f/JA4 egress policy |
| `spacetime-warden` | Focus-aware CPU/RAM/PID choking and `virtio-balloon` reclaim |
| `spacetime-agent` | Heartbeat loop, Goal DAG, context compactor, live steering, Set-of-Mark vision, pluggable `Brain` |
| `spacetime-shell` | The engine, HTTP daemon, dashboard and `spacetime` CLI |
| `guest-init` | Static musl PID 1 protocol (EXEC / RESIZE / CLIP / POWEROFF) |

## CI as the Rust compiler

This repo is developed on machines **without a local Rust toolchain**; GitHub
Actions is the compiler. On every push to `arena/**`, three workflows run:

* **`.github/workflows/ci.yml`** — the gate: `cargo fmt --check`, clippy with
  `RUSTFLAGS: -D warnings` on Linux/macOS/Windows, `cargo test --workspace`, a
  release build + live `/api/state` smoke test, and static `x86_64`/`aarch64`
  musl guest cross-builds. On failure, the `report` job posts the compiler logs
  as a commit comment.
* **`.github/workflows/diag.yml`** — the remote compiler. One job runs
  `check / clippy / test / fmt / guest` and posts the complete ANSI-stripped logs
  as a commit comment, even when everything is green:
  ```bash
  gh api repos/:owner/:repo/commits/<sha>/comments --jq '.[0].body'
  ```
* **`.github/workflows/fmt.yml`** — applies `cargo fmt --all` and pushes the
  result, for machines that cannot run rustfmt. Request it with a push whose
  commit message contains `[fmt]`:
  ```bash
  git commit --allow-empty -m "style: apply rustfmt [fmt]" && git push
  ```
  The generated commit carries no marker, so there is no loop. GitHub does not
  re-trigger workflows for commits pushed with `GITHUB_TOKEN`, so follow a format
  commit with an ordinary push (or a manual `workflow_dispatch`) to re-run the
  gate.

---

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
