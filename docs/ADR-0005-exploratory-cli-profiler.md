# ADR-0005 — Exploratory profiling for existing headless CLI workloads

Date: 2026-11-16
Status: implemented on `arena/faa483b2-spacetime` (branched from
`33149fe9ecac077bd1d37468c96197fefe94bca2`). This ADR does not establish a measured resource gate,
physical provenance, or Block 3 `DONE`.

## Context

`BUILD-PLAN.md` keeps the measured `4gb_potato` baseline as the next release gate. The actual target
machine has not been identified in the repository or supplied for this run, and a hosted coding
sandbox or GitHub Actions runner cannot stand in for it. The existing static HTML console is a
review artifact, not a product web application or a native workspace that can yield cold-start,
tab-switch, and snapshot measurements.

There is still value in collecting repeatable resource observations for the offline command-line
work that already exists. The collection mechanism must not turn CLI numbers, a caller-supplied host
label, or detected RAM into product measurements or release evidence.

## Decision

1. Add `bench/measurements/profile_cli.py`, a Linux-only, standard-library helper for exactly three
   existing, headless `spacetime-ctl` workloads: `verify`, `econbench`, and `console-export`.
2. Require a registered target profile and a safe, caller-supplied host label. Record detected host
   facts without inferring or changing the selected profile. Do not record the machine hostname.
3. Invoke the built executable with fixed argument lists and no shell. Sample the command's process
   group through `/proc`; label RSS and CPU as sampled observations, not exact whole-workspace
   metrics. Write a JSON report only to a new path outside the repository, publishing it atomically
   with create-if-absent semantics so prior evidence is not overwritten or accidentally committed.
4. Put an explicit non-authoritative boundary in every report: `gate_eligible = false`,
   `resource_gate = "unmeasured_target_4gb"`, `performance_claims = "unmeasured"`, and `block3_done = false`.
   Do not emit the bounded measurement-file format or add a path that consumes physical provenance.
5. Name the omitted product measurements: whole-workspace/concurrent peak RSS, cold product startup,
   tab-switch and fresh-snapshot latency, snapshot CPU/memory, and accepted out-of-band provenance.
6. Run Python unit tests in CI with a temporary fake executable. CI exercises argument bounds and
   report honesty only; it does not profile the runner or supply target evidence.

## Consequences

- A maintainer can compare the cost of the current offline CLI workloads on an explicitly selected
  machine without a browser, web server, live model, or external account.
- The report is not accepted by `machine gate` or `verify --measurement` and cannot satisfy the
  resource gate. A physical provenance review and a measured product workload remain necessary.
- The First Law still blocks Block 3 `DONE` and later Block 4 work until the exact baseline machine,
  full workload, evidence, and reviewed provenance-consuming gate are available.
- The process sampler is intentionally modest; short-lived processes can be missed, shared RSS can
  be double-counted, and sampling can affect host load and wall time. Treat its output as diagnostic
  and retain it out of band.

## Non-goals

- No native shell, GUI, tab system, browser, VM, cell, or surface is implemented here.
- No web application, local web server, or persistent service is introduced.
- No inference that this hosted environment is the 4 GB baseline.
- No performance claim, measured gate, or provenance attestation.
