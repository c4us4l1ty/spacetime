# Out-of-band provenance template — any recorded Spacetime target machine

Copy this template **outside the repository** and fill it on the exact machine being recorded.
Record this filled template out of band with the raw evidence. Do not commit the filled template,
raw traces, screenshots, or a bounded measurement file unless the user explicitly asks. GitHub
Actions runners, Azure runners, self-test labels, and synthetic hosts do not qualify as recorded
target machines.

Spacetime is planned for public release, so a target is **not assumed to be a 4 GB machine**.
`4gb_potato` remains the mandatory release **baseline gate** from the First Law; other registered
profiles are additional evidence, not replacements for that gate. Do not infer a profile from
`MemTotal`; select a registered profile explicitly and prove the machine identity out of band.
This template does not change the proprietary license or legal release status.

Filling this template does **not** create a measured resource gate. The current software validates
measurement-file shape and budget arithmetic only; it cannot verify physical provenance. A later
reviewed code path must consume accepted provenance before `resource_gate` can become measured.

## 1. Explicit target profile

- Registry file: `bench/targets/profiles.tsv`
- List registered profiles: `spacetime-ctl machine profiles`
- List shell-readable profiles: `bash bench/measurements/capture-host-facts.sh --list-profiles`
- Selected `target_profile`: `<registered id>`
- Profile role: `<baseline_gate | optional_profile>`
- Profile memory budget: `<from registry, not from detected RAM>`
- Mandatory release baseline profile: `4gb_potato`
- Has separate baseline evidence been retained?: `<yes / no; no means release remains blocked>`

## 2. Exact machine identity

- Asset tag / serial (keep private): `<fill out of band>`
- Make and model: `<fill>`
- CPU model and core/thread count: `<fill>`
- Architecture: `<fill>`
- Total RAM as installed and as reported by `/proc/meminfo`: `<fill>`
- OS and kernel: `<fill>`
- Storage device and filesystem type: `<fill>`
- Copy-on-write / reflink support: `<fill>`
- Swap, zram, and zswap state: `<fill>`
- `/dev/kvm` availability: `<fill>`
- Cgroup version and controller availability: `<fill>`
- Display/snapshot capture path: `<fill>`

## 3. Capture session

- UTC date and time: `<fill>`
- Operator and reviewer: `<fill>`
- Physical location / lab: `<fill>`
- Exact repository commit or release binary hash: `<fill>`
- Power source, thermal state, and background workload: `<fill>`
- Tool versions used for timing and memory sampling: `<fill>`
- Command used to collect host facts:
  `bash bench/measurements/capture-host-facts.sh --profile <registered-id> --host-label <safe-label>`
- Host-label used: `<fill>`
- Host-facts output file and SHA-256: `<fill>`
- Raw traces, screenshots, or photos and each SHA-256: `<fill>`
- Where the original evidence is retained: `<fill>`

## 4. Measurement definitions and raw evidence

Do not copy numbers from CI, a brainstorm, or a synthetic fixture. Each number must come from a raw
trace captured on the machine above. Budget arithmetic uses the selected profile's budget; passing
a larger profile never proves the 4 GB baseline.

- `peak_rss_kb`: peak resident memory of the **whole active workspace** under the agreed realistic
  concurrency workload, including hidden surfaces, not an idle single-process reading. Tool,
  sampling interval, workload, duration, and raw trace: `<fill>`
- `boot_ms`: cold start from launch to the first usable console/workspace tile. Start/stop markers,
  number of repetitions, warm-up policy, and raw trace: `<fill>`
- `tab_switch_ms`: input to fresh snapshot or selected surface becoming usable. Repetitions and raw
  trace: `<fill>`
- `cpu_percent`: sustained CPU under the same workload, with measurement tool and interval: `<fill
  or omit>`
- Snapshot capture latency and CPU cost: `<fill>`
- Cold/warm start and concurrency observations: `<fill>`
- Cheapest `PreviewMode` that still satisfies visibility: `<fill>`
- Separate baseline `4gb_potato` evidence reference: `<fill or mark pending>`

## 5. Bounded-file recipe (outside the repository)

Only after the evidence above is retained, create a regular UTF-8 file outside the repository with
exactly these supported keys. Use unsigned decimal numbers from the raw traces and a host label that
does not contain a CI, Actions, runner, Azure, synthetic, `ci_host`, or self-test marker.

```text
schema_version=1
target_profile=<registered id>
measurement_host=<non-CI, non-synthetic target host label>
peak_rss_kb=<unsigned integer measured on the selected machine>
boot_ms=<unsigned integer measured on the selected machine>
tab_switch_ms=<unsigned integer measured on the selected machine>
cpu_percent=<optional unsigned integer measured on the selected machine>
```

Do not add provenance fields to this file; the bounded intake rejects unsupported keys. Keep the
file, its SHA-256, and this filled template out of band.

## 6. Shape-only check and sign-off

Using a trusted release build, run:

```sh
spacetime-ctl machine profiles
spacetime-ctl machine gate --measurement /path/out-of-band/target.measurement
spacetime-ctl verify --out target/verify.toml --measurement /path/out-of-band/target.measurement
```

Expected current behavior for a within-budget file: `evaluator=pass_shape_only`,
`resource_gate=unmeasured`, `release_gate=unmeasured`, `baseline_target_profile=4gb_potato`, and
`performance_claims=unmeasured`. Do not edit those fields or describe `pass_shape_only` as a
measured pass. An over-budget shape fails the shape-only evaluator and still makes no physical
claim. A larger optional profile never promotes the baseline gate to measured.

- Operator attestation that the machine and raw evidence are genuine: `<name, date, signature>`
- Reviewer attestation that provenance was checked out of band: `<name, date, signature>`
- Explicit decision to implement/approve a reviewed provenance-consuming gate: `<pending / fill>`
- Explicit legal/licensing decision for public publication: `<pending / fill>`
