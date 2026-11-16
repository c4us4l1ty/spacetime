# 4 GB target-machine measurement readiness

Date: 2026-11-16
Status: candidate target partially identified; **no target measurements collected**.

## User-reported candidate

The user reports a 2017-era machine with a dual-core Celeron, 4 GB RAM, and Lubuntu. This is a
useful lead for the mandatory `4gb_potato` baseline, but it is not yet an exact machine identity or
physical evidence. The year, broad CPU family, RAM capacity, and distribution name do not establish
the exact make/model, CPU SKU, installed/usable memory, Lubuntu release, kernel, storage, or workload
conditions. No profile is inferred by software from detected RAM; the baseline profile must still
be selected explicitly when the host facts are collected.

## Next collection step on that machine

After synchronizing the current branch to the target machine, run this read-only collector there
from the repository checkout. Retain the full output and its SHA-256 outside the repository. Do not
paste serial numbers, asset tags, or other private identifiers into a public report.

```sh
mkdir -p "$HOME/spacetime-evidence"
bash bench/measurements/capture-host-facts.sh \
  --profile 4gb_potato \
  --host-label potato-lab \
  > "$HOME/spacetime-evidence/host-facts.txt"
sha256sum "$HOME/spacetime-evidence/host-facts.txt"
```

The collector now reports DMI model/vendor/version where exposed (never serial numbers), CPU model
and topology, OS/kernel, memory/swap/zram, storage and filesystem for the checkout, KVM, cgroups,
and zswap. Some firmware or utility details may be unavailable. Filesystem identification does not
prove reflink/copy-on-write capability; record that separately if relevant. The out-of-band output
also includes the current hostname and working directory; redact those, plus any private asset or
storage identifiers, before sharing excerpts. This command collects host facts only; it does not run
a workload, create a measurement file, or pass the resource gate.

## Exact details still needed

Review the collected output and complete the out-of-band `bench/measurements/PROVENANCE-TEMPLATE.md`
with:

- Manufacturer and exact model (and keep serial/asset tag private).
- Exact Celeron SKU, physical cores and logical threads.
- Lubuntu release, kernel, and architecture.
- Installed RAM versus `/proc/meminfo` `MemTotal`.
- Storage device/type and the filesystem backing the checkout; swap, zram, and zswap state.
- Power/thermal/background-workload conditions and measurement-tool versions.

## Build and gate status

The machine report does **not** unblock the full 4 GB resource gate by itself. This checkout has no
native mixed-workspace product workload yet, and the exploratory CLI profiler cannot measure cold
product startup, full workspace/concurrent RSS, tab switching, or fresh-snapshot cost. Do not use
CLI-only samples, idle measurements, CI results, or the user-reported specifications as gate values.

Before recording any performance result, implement the real Block 3 local tasks and full workload,
then define and run its cold-start, mixed-workspace concurrency, active/hidden-tab, snapshot, and
CPU/RSS protocol on the identified target. Keep raw traces and completed provenance out of band.
Until the resulting evidence has been reviewed and a provenance-consuming gate exists,
`resource_gate = "unmeasured_target_4gb"`, `performance_claims = "unmeasured"`, and
`block3_done = false`; Block 4 remains gated.
