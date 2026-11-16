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

## Development — Blocks 1-2 baseline, explicit target profiles, and Block 3 offline prototype

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
cargo run -p spacetime-ctl -- machine profiles
cargo run -p spacetime-ctl -- machine gate
cargo run -p spacetime-ctl -- verify --out target/verify.toml
cargo run -p spacetime-ctl -- verify --out target/verify.toml --measurement <file>
cargo run -p spacetime-ctl -- bench run --arm A --model scripted-fixture --budget 1000 --out target/econbench-A.json
cargo run -p spacetime-ctl -- console --ledger bench/econbench/sample-ledger.jsonl --out target/console-report.json
# Optional local preview of the static review artifact only (no product backend or web app).
python3 -m http.server 8000 --bind 0.0.0.0 --directory apps/spacetime-console
bash bench/measurements/capture-host-facts.sh --list-profiles
```

`verify --out` runs 28 offline gates end to end and writes a TOML-style report; the 26th gate
checks the explicit target-profile registry, the 27th checks the offline EconBench seed, and the
28th checks the static console report. The optional bounded `verify --measurement <file>` path
evaluates a measurement file against its selected profile for the report without ever promoting
file shape to a measured resource gate.
`crucible panel` resolves five independent verifier sources conservatively; `crucible adversarial`
runs twenty offline model-authored fixtures; `crucible population` uses synthetic operators, and
`crucible fixture-population` reruns 26 real declared-oracle fixtures 50 times each. Both population
commands explicitly report `real_operator_study=false`. CLI reruns are bounded to 50–10,000,
differential limits to 1,000,000–10,000,000, and missing optional flag values are rejected. A narrow
`LD_PRELOAD` shim fixes `clock_gettime`/`getrandom` and denies sockets for local probes, while an
action allow-list enforces capability fields; the shim is **not** a kernel sandbox or complete
boundary. `machine profiles` lists explicit registered targets; `4gb_potato` is the mandatory
release baseline, while `8gb_reference` and `16gb_developer` are optional and additive.
`machine gate` stays unmeasured until real per-profile evidence and accepted out-of-band provenance
exist; CI/synthetic host labels are rejected. CI host facts are not evidence for any recorded
target, so no performance or resource-budget claim is made.

Block 2 was merged to `main` via PR #8 at explicit user request after strict CI passed 24 gates.
PR #9's bounded measurement-intake and bug-hunt increment was merged to `main` at
`90ceeb20db603c52e313ef718b82710241ad6080` at explicit user request after strict CI passed 25
gates; strict `main` CI run `43829756506` verified all 14 steps. The measured 4 GB resource gate
remains outstanding, so both were functional correctness merges, not resource sign-offs.

PR #10 merged the measurement-provenance kit, explicit target profiles, and offline Block 3
prototype to `main` at `33149fe9ecac077bd1d37468c96197fefe94bca2`. The prototype includes 20 scripted
fixtures, a verified mock-economy ledger, `spacetime-ctl bench run`, `spacetime-ctl console`, and a
static HTML review artifact in `apps/spacetime-console/`. It is functional groundwork, not a
measured resource sign-off and not Block 3 `DONE`; the artifact is not a web application or the
product UI. `4gb_potato` remains the mandatory release baseline; 8 GB and 16 GB profiles are
optional and cannot satisfy it. No target measurement file, filled provenance record, or physical
attestation is committed. `resource_gate = "unmeasured_target_4gb"` and
`performance_claims = "unmeasured"` remain.

PR #11 merged the exploratory profiler and target-intake preparation to `main` at
`a6a722efb1068ef04861035c0e89657c49e37f41`. The active branch `arena/3f609a12-spacetime` (based
on that commit) is hardening differential verification and adding a process-local mock-economy
primitive with atomic, idempotent fake-credit transfers. These are verification and library slices;
they do not implement the full EconBench task pack or measure the native workspace, cold startup,
tab switching, snapshots, or physical provenance, and cannot mark any resource gate as passed. PR
#11's strict CI evidence is recorded in the pull request checks.

See `BUILD-PLAN.md` for the blocked roadmap, `docs/ADR-0001-block1.md` for Block 1 decisions,
`docs/ADR-0002-block2.md` for the Block 2 boundary, `docs/ADR-0003-target-profiles.md` for the
public target-profile decision, `docs/ADR-0004-block3-offline-console.md` for the offline Block 3
prototype, `docs/ADR-0005-exploratory-cli-profiler.md` for the diagnostic profiler,
`docs/TARGET-MEASUREMENT-READINESS.md` for target-side host-facts collection,
`docs/ADR-0006-independent-differential-oracle.md` for the differential-verifier correction, and
`docs/ADR-0007-offline-mock-economy-invariants.md` for local fake-credit transaction rules.

## Rust terminal dashboard (early prototype)

On Linux, install a Rust toolchain (Rust 1.75 or newer) and build from a checkout:

```sh
cargo run --locked -p spacetime-ctl -- dashboard
```

This Rust-only, read-only terminal interface verifies the included sample ledger before displaying
recent activity. To inspect another ledger, provide its path after `dashboard`; keep its `.head`
file alongside it. This is a view of recorded fixture activity, **not** a graphical application,
real AI, or a secure sandbox. Do not run untrusted software with this prototype. The intended
4 GB resource budget has not yet been measured.

## Experimental offline VM boot spike

This is a first step toward the Linux workspace, **not the MVP or a malware sandbox**. It launches
QEMU as a separate process with KVM, an offline microVM, 512 MiB of guest memory, one virtual CPU,
and a serial terminal. It configures no guest network card, disk, or host file share. The VM is
not yet enclosed in a hardened host sandbox; do not use it for untrusted apps or malware.

Requirements: Linux x86-64, accessible `/dev/kvm`, `qemu-system-x86_64` built with microvm and
seccomp sandbox support, and a **trusted** compatible Linux kernel and initramfs supplied by you.
The initramfs must contain a working `/init` and support a serial console; Spacetime does not yet
supply or build guest images. Start it from a terminal:

```sh
cargo run --locked -p spacetime-ctl -- space boot --experimental \
  --kernel /path/to/kernel --initramfs /path/to/initramfs
```

Exit or shut down from the guest to stop QEMU. There is no persistent guest disk, but guest RAM,
terminal output and crash data can still leave host traces (for example in swap or logs). No
performance, security, cleanup, or 4 GB compatibility claim has been verified. This spike is not
the final VMM choice; isolation, resource limits, recovery and hostile-guest tests remain to be done.

### Preparing a minimal guest shell

As an alternative to finding a ready-made initramfs, Spacetime can create one locally from a
**trusted, statically linked x86-64 BusyBox** binary. Obtain that binary through a source you trust;
Spacetime does not download it or verify its origin. The BusyBox build needs the shell, mount and
poweroff applets. This command creates a new file and will not overwrite an existing one:

```sh
cargo run --locked -p spacetime-ctl -- space initramfs \
  --busybox /path/to/static-busybox --out guest-initramfs.cpio
cargo run --locked -p spacetime-ctl -- space boot --experimental \
  --kernel /path/to/linux-kernel --initramfs guest-initramfs.cpio
```

The Linux kernel must support initramfs, the x86 serial console, and the QEMU microvm machine;
required drivers must be built into the kernel rather than supplied as modules. In the guest,
`exit` asks BusyBox to power off. This is a minimal shell, not yet an app installer or desktop.
These steps have not yet been boot-tested on a physical KVM machine; see the safety limitations
above. The image contains the supplied executable, so use only software you trust.

To include one trusted, statically linked x86-64 terminal program in the image, add
`--app /path/to/program` when creating it. Once the guest shell starts, run it by typing `app`.
Dynamic executables, graphical apps, and programs requiring files outside the image are not yet
supported. The generated image remains on the host and includes the program; remove that image
when it is no longer wanted.
