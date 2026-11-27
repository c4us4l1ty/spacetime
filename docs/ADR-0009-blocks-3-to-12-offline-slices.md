# ADR-0009: Offline Pure-Rust/Linux Slices for Blocks 3–12

Date: 2026-11-27
Status: Implemented offline functional software; hardware 4 GB gate (`resource_gate = "unmeasured_target_4gb"`) and external-model benchmarks (`performance_claims = "unmeasured"`) remain open.

## Decision

Proceed with all feasible offline, zero-network, deterministic Rust/Linux software slices across `BUILD-PLAN.md` Blocks 3–12 while honoring all §8 defaults and without modifying `Cargo.lock` or introducing unmeasured hardware/LLM claims:

1. **Block 3 completion & provenance gate:**
   - Every one of the 20 `EconBench` cases (`wallet_*`, `transform_*`, `protocol_*`) now validates both a passing candidate and a realistic `PlantedBug` candidate in `spacetime-bench::econbench`.
   - `bench/economy/bounties.json` aligns `"verifier_source": "spec_oracle"` on `15`..`20` and `"fake_credits_total": 60` with the actual reward sum.
   - `evaluate_provenance_gate` defines the strict attestation boundary connecting bounded measurement files to out-of-band provenance digests (`host_facts_sha256`, `measurement_sha256`, distinct `operator` and `reviewer`, non-CI/non-synthetic target host).

2. **Block 4 (Cortex adapters + population-per-turn + A/B/C harness):**
   - `spacetime_cortex::adapters` adds `CortexAdapterKind` (`Scripted`, `CliAgent`, `LocalModel`, `WebChatTab`) and `CortexPromptEnvelope` which enforces `raw_screenshot_allowed_for_cortex = false` and masks `is_secret` / `password` / `token` fields.
   - `spacetime_cortex::population` enforces 8–12 population-per-turn cycling (`RejectedAll` when zero candidates verify) and evaluates Arm A vs B vs C offline while labeling `performance_claims = "unmeasured"`.

3. **Blocks 5–11 (Supervisor, Security, Execution, Surface, Quarantine, CAS, Airlock, Distillation):**
   - `apps/spacetime-ctl/src/memory.rs`: Durable `.space` directory state (`goals.json`, `steering.json`, `facts.json`, `blockers.json`) + pure-Rust rebuildable Layer 2 `Bm25Index`.
   - `apps/spacetime-ctl/src/security.rs`: Ring -3 `ConstitutionPolicy` + `CapabilityToken`, `ObservationFirewall`, `HostVault` (`@vault:<name>` symbolic handles + canary alarms), and a 10-vector adversarial `run_redteam_suite`.
   - `crates/spacetime-exec`: Tier 1 `ConstrainedWasiWorker` (`wasi.rs`), Tier 2 `ArkCellSpec`/`ArkBroker` + active `probe_net_none_denial` TCP listener check (`ark.rs`), `ResourceGovernor` (`governor.rs`), read-only `SurfaceBroker` with RAM-only RGBA snapshot generation (`surface.rs`), `EgressBroker` blocking host-local/LAN/metadata/UNIX targets and host DNS leakage (`egress.rs`), upperdir `autopsy.rs`, and honest `leak_audit.rs` (`zero_traces_claimed = false`).
   - `apps/spacetime-ctl/src/{cas,airlock,distill}.rs`: Provenance-tiered `.wavefunction` CAS + `Collapse` + `branch_space` (reflink-first with explicit boolean reporting) + export/import bundles; one-shot `ClipboardAirlock` & symlink-rejecting `airlock_copy_file`; `DistillationStore` with operator drift quarantine (`Healthy -> Suspect -> Dead`) and self-tested skills.

4. **Block 12 (`apps/spacetime-gui`):**
   - Native X11 `eframe`/`egui` mixed-workspace shell hosting both the `vt100` VM terminal tab and the `ws-ai-upload` AI surface tab (`LivenessState::Running` in background, RAM-only `SnapshotOnAction` textures in foreground, Ring -3 human blocker queue, non-blocking steering, and `SurfaceReader` isolation).

## Explicit Boundaries Remaining Out of Scope Until Target/Live Runs

- Physical `4gb_potato` measurement (`peak_rss_kb`, `boot_ms`, `tab_switch_ms`, `cpu_percent`) and recorded out-of-band provenance attestation.
- 90-day live-model A/B/C empirical trials (`performance_claims = "unmeasured"`).
- External Wayland compositor / `bubblewrap` GPU / live YouTube or real-money integrations (mock offline dry-runs only per §8).
