# Resource gate measurements

No target-machine measurement file is committed. CI host facts are **not** target-machine
measurements. `spacetime-ctl machine gate` reports `resource_gate=unmeasured` by default.
`spacetime-ctl verify --out <file> --measurement <file>` evaluates the same file for the
end-to-end report.

## Explicit target profiles

Spacetime is planned for public release, so not every target is a 4 GB machine. Profiles are
declared in `bench/targets/profiles.tsv` and exposed by `spacetime-ctl machine profiles`:

| Profile | Memory label | Role |
|---|---|---|
| `4gb_potato` | 4 GB | mandatory release `baseline_gate` |
| `8gb_reference` | 8 GB | additional `optional_profile` |
| `16gb_developer` | 16 GB | additional `optional_profile` |

`4gb_potato` remains the First Law baseline. A passing measurement on a larger optional profile
does **not** satisfy or replace the baseline gate. Profiles are explicit; the software never infers
a profile from detected RAM. Adding a profile requires a reviewable registry change, its own
physical evidence, and a decision about whether it changes the release baseline.

When a real target machine is measured, supply an explicit file with:

```text
schema_version=1
target_profile=<registered id>
measurement_host=<non-CI, non-synthetic target host label>
peak_rss_kb=<measured peak RSS in KiB>
boot_ms=<measured cold boot in ms>
tab_switch_ms=<measured tab switch in ms>
cpu_percent=<optional measured CPU percent>
```

The gate compares memory, boot, tab-switch, and optional CPU values against the **selected
profile's** registered budgets. A missing host, unregistered profile, missing required value, or
host label containing `github`, `actions`, `runner`, `azure`, `synthetic`, `ci_host`, or
`self_test` leaves the gate unmeasured rather than claiming a pass. A measured file that exceeds a
budget fails the gate. The gate validates file shape and budget arithmetic; it cannot prove the
measurements were taken on real hardware. Record provenance out of band.

## Bounded intake

`verify --measurement` accepts one file path and rejects missing values, duplicate or unknown
flags, control characters, and paths longer than 512 bytes. The file must be a regular UTF-8 file
no larger than 64 KiB, with at most 64 lines, at most 512 bytes per line, at most 256 bytes per
value, at most 128 bytes for `measurement_host`, only the seven documented keys, no duplicate
keys, and unsigned decimal numbers. Values may use ASCII letters, digits, `.`, `_`, and `-` only.

The report records `measurement_intake`, `measurement_evaluator`, `measurement_digest` (SHA-256 of
the file bytes), `measurement_reasons`, `target_profile`, `baseline_target_profile`,
`target_profile_registry`, `evaluated_memory_budget_kb`, and `profile_role`. A missing, unreadable,
malformed, oversized, CI-host, synthetic-host, wrong-profile, incomplete, or over-budget file stays
`unmeasured`. A file that passes all file-shape checks is labelled `pass_shape_only`, **not** a
measured gate: `resource_gate = "unmeasured_target_4gb"` and `performance_claims = "unmeasured"`
remain until the baseline and selected machine are measured and physical provenance is recorded out
of band. No target measurement is committed here.

## `machine gate --measurement`

`machine gate --measurement <file>` uses the same bounded file checks. Its output never claims a
measured pass: `resource_gate=unmeasured`, `release_gate=unmeasured`, with
`evaluator=pass_shape_only`, `fail_shape_only`, or `unmeasured`. It prints the selected
`target_profile` and the unchanged `baseline_target_profile=4gb_potato`. An over-budget shape still
exits non-zero. Duplicate keys (including a duplicate `measurement_host`) are rejected, so a
CI-host label cannot be overridden by a later target-looking label. Physical provenance remains out
of band.

## Reproducible provenance kit

`capture-host-facts.sh` is a read-only host-fact collector. It requires `--profile <registered-id>`,
optionally accepts `--host-label <safe-label>`, and never invents a default target class. Use
`--list-profiles` to print the registry without collecting host facts. Run the collector on the
recorded machine and retain its output out of band with `PROVENANCE-TEMPLATE.md`, a human checklist
covering machine identity, profile selection, raw evidence hashes, measurement definitions, and
sign-off. The script does not write a measurement file; the template is not evidence until filled
on the target machine. No filled template, raw evidence, or target measurement file is committed
here.

The current code can only label a bounded file `pass_shape_only`; `resource_gate` and
`performance_claims` remain unmeasured until physical provenance is accepted out of band and a
reviewed code path consumes it. Strict CI checks the collector's shell syntax, argument rejection,
and registry listing only; CI host facts are never target evidence.
