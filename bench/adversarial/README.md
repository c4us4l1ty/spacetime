# Block 2 offline adversarial corpus

These twenty fixtures are model-authored **from the Block 2 spec only**. They are offline task
files, not live model calls, and they do not inspect a generated candidate at runtime. Each task
declares its own verifier source, expected verdict, determinism run count, mutation threshold, and
candidate mutation score.

The corpus deliberately mixes deterministic PASS cases, wrong-digest FAIL cases, trivial
mutation-score FAIL cases, flaky ERROR cases, and timeout ERROR cases. `spacetime-ctl crucible
adversarial --dir bench/adversarial` rejects any mismatch and requires at least twenty fixtures.
This is a cheap substitute for a live separate-model adversarial suite, which remains later work.
