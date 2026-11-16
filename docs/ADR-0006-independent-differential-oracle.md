# ADR-0006 — Make the differential smoke gate non-tautological

Date: 2026-11-16
Status: implemented on `arena/3f609a12-spacetime`; strict Rust CI pending.

## Context

The existing million-input differential check compared a candidate function that directly called
the reference function. That guaranteed equality by construction and could not detect a wrong
candidate. Its mutation operators were also exercised only on the monotonically increasing prefix
`0..limit`, which did not test `u64` wraparound behavior.

This defect weakens the current differential smoke check, but fixing it does not turn the checked-in
EconBench placeholders into the real seven transmutation tasks required by Block 3.

## Decision

- Keep the reference as `input * 3 + 7` with explicit `u64` wrapping.
- Implement the candidate independently as `input + (input << 1) + 7`, also with explicit wrapping.
- Run both the candidate comparison and mutation checks over one deterministic input stream that
  starts with zero, small values, `u64::MAX` neighbors, and the high-bit boundary, then continues
  with an odd-multiplier LCG stream.
- Add tests that compare the implementations on edge inputs and require every current mutation to
  be caught by the bounded deterministic corpus.

## Consequences

- The smoke test can now detect implementation disagreement rather than aliasing the oracle.
- Edge inputs cover modular wraparound that the previous small integer prefix omitted.
- The result is still a small synthetic differential example, not an independent task pack, a real
  transmutation benchmark, or evidence about model efficacy.
- The 4 GB baseline remains unmeasured; `resource_gate`, `performance_claims`, and `block3_done`
  remain unchanged.

## Validation

Rust tests are part of strict CI. The local checkout has no `cargo` or `rustc`, so formatting,
Clippy, and Rust tests must be verified by CI before merging.
