# Spacetime static console review artifact

This is an optional **static HTML review artifact** for Block 3, not a web application, service, or
product UI. Spacetime is a Rust/Linux system. The artifact reads `report.json` with a relative
`fetch`, or a CLI-exported JSON report through the file input. It has no embedded browser runtime,
live backend, public internet dependency, real wallet, or real account. Inline scripts, inline
styles, remote assets, dynamic HTML injection, and dynamic code evaluation are intentionally
avoided.

The following local static-file server is only a convenience for browser file-loading restrictions;
it is not a Spacetime backend or product service:

```sh
python3 -m http.server 8000 --bind 0.0.0.0 --directory apps/spacetime-console
```

Then open the local static-file page and use **Render human tile**, **Render AI tile**, and
**Advance AI action**. This is only an optional static-artifact preview. The AI tile uses
snapshot-on-action still placeholders from ledger events; it does not pretend to be continuous
live video. Switching tiles changes `RENDERING`, not `RUNNING`. The background event-stream tick
continues either way.

Generate a verified report from a hash-chained ledger:

```sh
spacetime-ctl ledger verify bench/econbench/sample-ledger.jsonl
spacetime-ctl console --ledger bench/econbench/sample-ledger.jsonl --out target/console-report.json
```

Then load `target/console-report.json` with the file input. The checked-in `report.json` is a
deterministic offline demo over the same ledger. `resource_gate` and `performance_claims` remain
unmeasured; the console is a functional prototype, not a measured resource sign-off.
