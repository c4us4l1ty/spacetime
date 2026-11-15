# ADR 0001 — Block 1 build boundary

Date: 2026-11-15  
Status: accepted

## Context

The user requested that the build begin in Rust and that GitHub CI be used as the compiler. The
sandbox has no local Rust toolchain and cannot reach `crates.io`; GitHub Actions can resolve and
compile crates. The First Law in `BUILD-PLAN.md` makes resource usage on the 4 GB "potato machine"
the supreme constraint.

## Decisions

1. **CI is authoritative.** No local `cargo build` is assumed. `.github/workflows/ci.yml` runs
   `cargo fmt`, `cargo clippy -D warnings`, `cargo test`, `cargo build --release`, and CLI smoke tests.
2. **Dependency-light workspace.** Only `serde`, `serde_json`, and `sha2` are used. No GUI, browser,
   VMM, async runtime, HTTP client, or local model runtime is introduced in Block 1.
3. **Audited crypto, not homegrown crypto.** Ledger hashes use the `sha2` crate. Canonical JSON uses
   `serde_json` with sorted object keys. The project does not implement its own cryptographic primitive.
4. **Canonical JSONL ledger.** Each event is one compact JSON object. Its `hash` is
   `SHA256(prev_hash || canonical_event_without_hash)`. A sidecar `.head` file stores event count and
   last hash so truncation is detectable.
5. **Scripted Cortex only.** The first brain is a deterministic local task script. No API keys, web
   chat, browser, or network calls exist.
6. **Capability-limited runner.** Block 1's runner exposes only four scripted operations. Its `sleep`
   operation uses an allowlisted absolute binary with cleared environment, fixed working directory,
   and a timeout. It grants no network capability, but it is **not** a kernel sandbox. Real `net = none`
   enforcement, namespaces, cgroups, Ark cells, and quarantine microVMs arrive in Blocks 7–8.
7. **False claims do not advance goals.** `claim_done` without a matching verified evidence digest is
   recorded as `unverified`; the goal stays incomplete.
8. **Snapshot-first types exist early.** `WorkspaceId`, `SurfaceId`, `PreviewMode`, and
   `LivenessState` are first-class data even though no live surface is implemented yet.
9. **Canonical ledger bytes.** Stored JSONL lines are canonicalized; verification rejects
   non-canonical rewrites, whitespace changes, or key reordering instead of only semantic tampering.
10. **Failure is not success.** `spacetime-ctl run` exits non-zero when the final goal is not
    `Verified`, while still writing the state sidecar for inspection.
11. **Reproducible dependency resolution.** `Cargo.lock` is committed and CI uses `--locked` so
    local builds and CI resolve identical crate versions.

## Consequences

- CI can prove the ledger/replay contract without a local toolchain.
- Block 1 is deterministic, offline, and cheap enough to satisfy the First Law.
- Security claims are intentionally narrow: this block proves tamper evidence and replay, not host
  isolation. The plan's later blocks remain responsible for the real isolation boundary.
- Ledger verification is byte-canonical; an attacker cannot rewrite a line with reordered keys or
  extra whitespace and keep the chain valid.
- Unverified, failed, and incomplete goals are reported as command failures, not silent success.
- The committed `Cargo.lock` pins the dependency versions proven by CI.
