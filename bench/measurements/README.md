# Resource gate measurements

No target-machine measurement file is committed. CI host facts are **not** 4 GB potato-machine
measurements. `spacetime-ctl machine gate` reports `resource_gate=unmeasured` by default.

When a real 4 GB machine is measured, supply an explicit file with:

```text
schema_version=1
target_profile=4gb_potato
measurement_host=<non-CI host label>
peak_rss_kb=<measured peak RSS in KiB>
boot_ms=<measured cold boot in ms>
tab_switch_ms=<measured tab switch in ms>
cpu_percent=<optional measured CPU percent>
```

The gate compares memory against 4 GiB and boot/tab-switch against 1 s hypotheses. A missing
host, missing required value, or CI host label leaves the gate unmeasured rather than claiming a
pass. A measured file that exceeds a budget fails the gate.
