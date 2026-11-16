# Resource gate measurements

No target-machine measurement file is committed. CI host facts are **not** 4 GB potato-machine
measurements. `spacetime-ctl machine gate` reports `resource_gate=unmeasured` by default. `spacetime-ctl
verify --out <file> --measurement <file>` evaluates the same file for the end-to-end report.

When a real 4 GB machine is measured, supply an explicit file with:

```text
schema_version=1
target_profile=4gb_potato
measurement_host=<non-CI, non-synthetic target host label>
peak_rss_kb=<measured peak RSS in KiB>
boot_ms=<measured cold boot in ms>
tab_switch_ms=<measured tab switch in ms>
cpu_percent=<optional measured CPU percent>
```

The gate compares memory against 4 GiB and boot/tab-switch against 1 s hypotheses. A missing
host, missing required value, or host label containing `github`, `actions`, `runner`, `azure`,
`synthetic`, `ci_host`, or `self_test` leaves the gate unmeasured rather than claiming a pass. A
measured file that exceeds a budget fails the gate. The gate validates file shape and budget
arithmetic; it cannot prove the measurements were taken on real hardware. Record provenance out of
band.

## Bounded intake

`verify --measurement` accepts one file path and rejects missing values, duplicate or unknown
flags, control characters, and paths longer than 512 bytes. The file must be a regular UTF-8 file
no larger than 64 KiB, with at most 64 lines, at most 512 bytes per line, at most 256 bytes per
value, at most 128 bytes for `measurement_host`, only the seven documented keys, no duplicate
keys, and unsigned decimal numbers. Values may use ASCII letters, digits, `.`, `_`, and `-` only.

The report records `measurement_intake`, `measurement_evaluator`, `measurement_digest` (SHA-256 of
the file bytes), and `measurement_reasons`. A missing, unreadable, malformed, oversized,
CI-host, synthetic-host, wrong-profile, incomplete, or over-budget file stays `unmeasured`.
A file that passes all file-shape checks is labelled `pass_shape_only`, **not** a measured gate:
`resource_gate = "unmeasured_target_4gb"` and `performance_claims = "unmeasured"` remain until the
recorded 4 GB machine is measured and physical provenance is recorded out of band. No target
measurement is committed here.

## `machine gate --measurement`

`machine gate --measurement <file>` uses the same bounded file checks. Its output never claims a
measured pass: `resource_gate=unmeasured` with `evaluator=pass_shape_only`, `fail_shape_only`, or
`unmeasured`. An over-budget shape still exits non-zero. Duplicate keys (including a duplicate
`measurement_host`) are rejected, so a CI-host label cannot be overridden by a later target-looking
label. Physical provenance remains out of band.
