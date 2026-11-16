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

## Try the experimental Rust workspace (Linux x86-64)

Spacetime is **not yet a finished or hardened product**. The early workspace is a Rust
terminal interface, not the planned graphical tabbed app. Use trusted programs only; do not use
it for malware or assume it prevents host compromise. It requires a Rust toolchain, QEMU with
microvm and seccomp support, usable KVM, a compatible Linux kernel, and a static BusyBox.

```sh
cargo run --locked -p spacetime-ctl -- workspace
```

Choose **Start a workspace** and enter the paths it asks for. You may include one static x86-64
terminal program; run it inside the guest by typing `app`. Type `exit` to shut down the guest.
Alternatively, use the one-shot command:

```sh
cargo run --locked -p spacetime-ctl -- space run --experimental \
  --kernel /path/to/linux-kernel --busybox /path/to/static-busybox \
  --app /path/to/static-program
```

Omit `--app` to launch just the guest shell. The generated guest image is placed in a private
temporary directory and normally removed when the VM exits or startup fails. Forced termination,
crashes, swap, logs and other host mechanisms can leave traces. There is no guest network, disk or
host file share configured. This is not a verified security guarantee, and neither KVM isolation
nor the 4 GB resource target has been validated on the target machine. See the experimental VM
section below for kernel and guest requirements.

## Try the experimental Linux workspace

Spacetime is still a prototype, **not a finished MVP or a malware-safe product**. The current
workspace can run one trusted static Linux terminal program in an offline disposable VM. It does
not yet run normal desktop apps, provide a graphical tabbed interface, or include the AI features.

On an x86-64 Linux computer, install Rust 1.75 or newer, QEMU, bubblewrap, and util-linux. On
Debian or Ubuntu, the system packages can be installed with:

```sh
sudo apt install qemu-system-x86 bubblewrap busybox-static util-linux libgl1 libxkbcommon-x11-0
cargo run --locked -p spacetime-ctl -- space doctor
cargo run --locked -p spacetime-ctl -- workspace
```

The workspace asks for a readable, compatible Linux kernel, a static BusyBox binary (typically
`/bin/busybox` on Debian/Ubuntu), and optionally a trusted static terminal program. When it finds
the current kernel or BusyBox, press Enter to accept the suggested path. In the guest,
type `app` to start the supplied program and `exit` to shut down the guest. Do **not** run
Spacetime with `sudo`: your normal user needs access to `/dev/kvm`. Some systems restrict
unprivileged user namespaces or access to `/boot`; `space doctor` checks the basic host
requirements, but cannot verify the supplied kernel, complete security, or 4 GB performance.

The monitor is isolated from host networking and home files where supported, but this has not
been verified against hostile software. Do **not** use the prototype to run malware or untrusted
apps. Generated one-shot guest images use a private `/dev/shm` tmpfs directory, removed on normal
exit. A crash or forced stop can leave it there; tmpfs and guest memory can be swapped. Terminal
output and crash data can also leave traces on the host. See the
experimental VM section below for the full requirements and limitations.

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

## Experimental native window

For an early Rust graphical front end on Linux with an X11-compatible desktop and Rust 1.88 or
newer, run:

```sh
CARGO_BUILD_JOBS=1 cargo run --locked -p spacetime-gui
```

The first build downloads Rust dependencies and may take time and disk space. Limiting build jobs
reduces concurrent compiler work but is not a measured 4 GB guarantee. It starts the same
experimental offline VM described above, showing a line-oriented console in a
native window. It is not a full terminal emulator: full-screen or interactive terminal programs,
multiple tabs, graphical guest apps, and AI are not supported. Use only trusted software. The VM
still requires KVM, bubblewrap, a compatible Linux kernel, and static BusyBox. Stopping the window
terminates its worker and removes its temporary guest image on normal shutdown; forced termination,
swap and logs can leave traces. No 4 GB performance or hostile-code security claims are made.

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
and a serial terminal. It configures no guest network card, disk, or host file share. The VM monitor runs in a bubblewrap namespace with a private network and limited host mounts,
and core dumps are disabled. This is not yet a fully hardened or verified sandbox; do not use it
for untrusted apps or malware.

Requirements: Linux x86-64, accessible `/dev/kvm`, `qemu-system-x86_64` built with microvm and
seccomp sandbox support, bubblewrap, util-linux `prlimit`, working unprivileged user namespaces,
and a **trusted** compatible Linux kernel and initramfs supplied by you. QEMU, bubblewrap and
prlimit must be installed in `/usr/bin`. Failure to start the sandbox does not fall back to an
unconfined VM.
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
