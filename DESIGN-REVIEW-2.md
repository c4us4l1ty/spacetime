# Design Review II — The Catalyst Critique

> Response to the "Anthropomorphic Mimicry" critique. Companion to `DESIGN-REVIEW.md`.
> Verdict: the best argument made about Spacetime yet. It correctly identifies the three real
> catalysts. It is also **wrong in five specific, fatal ways**, and it commits the exact error it
> accuses the earlier plan of: optimizing the cheap part while leaving the expensive part untouched.
> No code.

---

## 0. Scorecard

| Claim | Verdict |
|---|---|
| "The malware-sandbox *justification* is decorative / procrastination" | **Half right.** Defer SMBIOS, CPUID spoofing, blast-radius UI, persona games. But see §3 — the critique deletes the sandbox at the exact moment it makes running hostile code the core loop. |
| "A machine must never touch another machine through a UI more than once" | **Right instinct, wrong law.** Becomes: *one UI interaction per irreducible gate* (§5). |
| "Protocol autopsy / distill to a native operator" | **Best idea in the document.** Promote to a first-class subsystem. |
| "MicroVM = causal collider / fork-server" | **Right, but it's optimizing the free variable** (§2). Fork costs milliseconds; sampling costs tens of seconds. |
| "Best-of-8 converts 15% into 72.8%" | **Wrong math** (independence assumption) and it measures the wrong bottleneck. |
| "Markdown memory is epistemic rot" | **Right diagnosis, overcorrected prescription** (§6). Prose needs TTL and provenance, not prohibition. |
| "Executable state-graph, frontier-only prompts" | **Strong.** Adopt, with three additions: negative results, operator health, and a planner (§6, §7). |
| "Hunt high-asymmetry state space (bounties, transmutation, dark data)" | **Strongest strategic redirect in the whole project.** Adopt, but rank by *willingness to pay*, not by verifier crispness (§10). |
| "Delete the GUI/Wayland/tabs from Phase 1" | **Adopt as reordering, not deletion.** The user asked for tabs; they're a Phase-3 deliverable, not a Phase-0 one. |

---

## 1. What it gets spectacularly right (and a correction to its own history)

The three catalysts — **protocol autopsy**, **fork-and-verify cycling**, **executable graph memory** — are the correct primitives. I have nothing to add to the diagnosis of the browser as a parasitic heat-sink; it is exactly right, and the sentence *"the GUI is a cadaver to be dissected once"* deserves to survive into the README.

But look carefully at its own heroes, because it has drawn the wrong lesson from them.

**Haber–Bosch did not win by removing an intermediary.** It won because the reaction had a **continuously measurable yield** — pressure, temperature, conversion fraction — readable at every instant. There were eleven competing nitrogen-fixation processes in 1913; the ones that died did so because nobody could *see* whether a cycle had worked. **PCR did not win by removing a human.** It won because amplification is measurable *per cycle* — fluorescence, in real time, in a closed loop — so you can stop at exactly the right cycle and know it.

The lesson is not "delete the human shape." The lesson is:

> **Operate only where the output is measurable cheaply, continuously, and at every iteration.**

That is the real difference between Haber-Bosch and a thousand failed alchemists, and it is also why the critique's own hunting grounds (§IV.4) are correct: **bug bounties, differential porting, and protocol distillation are the only domains where "did it work" is a machine-checkable predicate.** The four catalysts are not a philosophy of bypassing the human. They are a philosophy of **manufacturing verifiers**, and the whole design should be re-derived from that one sentence.

Which brings us to the fatal gap: **a verifier is only as good as its metrology.** §4.

---

## 2. The optimization-allocation error: you are tuning the free variable

The critique's Primitive 1 demands: *"restore a warm 32MB image, inject via vsock, execute with zero network, return exit code + state diff in <25 milliseconds."*

Real numbers:

| Step | Cost |
|---|---|
| Snapshot restore, small VM (CH / Firecracker UFFD / `krun_restore`) | 5–50 ms |
| First-touch CoW page-fault storm after fork (working set dependent) | 20–100 ms |
| Execute a small static binary | 5–50 ms |
| Compile one mutated Rust crate (deps pre-built, sccache hot) | 50–500 ms |
| Compile a real project | 1–10 s |
| Run a meaningful test suite | 0.1–30 s |
| **One LLM sample from a web chat** | **5–30 s** |
| One round trip through a slow free API | 2–15 s |

The fork-server — the thing the critique wants to spend the first month optimizing to 25 ms — is **0.1–2% of the cycle.** The cycle is dominated by *sampling latency and compile/test time*, i.e. by things that have nothing to do with the hypervisor. Optimizing fork cost by 100× moves end-to-end throughput by under 2%.

Worse: the "8 branches × 5,000 ms, forked sequentially or in small batches that fit inside 4GB" plan is a **40-second decision** in the best case, and it needs 8 concurrent LLM sessions to keep three cores busy. On a 4GB laptop with a weak CPU, three concurrent VMs each running a compiler and a test suite is already optimistic.

### The fix, and it's the single most important architectural change in this document

**Population per turn, selection per millisecond.**

Do not ask the model for *one* candidate, fork, and wait. Ask the model for **8–12 candidates in a single reply** — 8 patches, 8 payloads, 8 payload variants, 8 hypotheses — each emitted as a delimited block. One LLM turn (15 s) now yields a population. Then the *entire* selection apparatus — which is microseconds-to-milliseconds per candidate for the cheap tier, seconds for the expensive tier — runs locally, in parallel, with no model in the loop.

This inverts the economics:
- LLM turns per task: **≥ 10× fewer.**
- Thermal cycles per unit wall-clock: **governed by your CPU, not by chat latency.**
- Failure isolation: a bad sample costs 40 ms and a `SIGKILL`, not a chat turn.

And it makes the PCR metaphor *actually true*: one thermal cycle (one prompt) denatures a population; the cycler anneals and selects at machine speed. Right now the design has one molecule per thermal cycle, which is not PCR, it's just a slow retry loop.

**Corollary:** your parallelism ceiling is set by **how many model sessions you can sustain**, not by RAM. That is a hard cap (4–10 concurrent web chats, realistically). Batch sampling is the only way to get a wide population without violating it.

---

## 3. The self-contradiction: you deleted the sandbox the moment you needed it

The critique's thesis: the malware sandbox is decorative procrastination; hardware virtualization's only purpose is controlling causal time.

Then its own four catalysts require, as a *core loop*:

- Cloning arbitrary GitHub repos and running their build scripts (transmutation, §IV.4.2).
- Dissecting and executing scraped JS bundles and adversarial portals (§IV.4.3).
- Running n-day exploits and synthesized PoCs (invariant hunting, §IV.4.1).
- Feeding fuzzer output into parsers that were never designed to be safe.

So: **the moment you delete the isolation story as "decorative", the organism's primary activity is executing hostile code, autonomously, unattended, holding credentials and a wallet.** That is not procrastination — that is the entire difference between a research notebook and a machine that gets you robbed or indicted.

The synthesis is not "keep the museum" and not "burn it":

> **The fork *is* the isolation.** They are the same primitive. You don't build two systems; you build one CoW-microVM fork-server and it serves *both* the thermal cycler and the hostile-code boundary. Delete the *museum* (SMBIOS tables, synthetic Dell serials, blast-radius dashboards, persona generators) from Phase 1–2. Do not delete the boundary.

Concretely: `persona seeds`, `CPUID spoofing`, `honeytokens`, `forensic UI` → Phase 4+, or never for the organism. `Process-per-VM + seccomp + cgroup + no-network kernel` → Phase 0, because your catalysts are useless without it.

One more honest note on "causal time": the reason to control causal time is not only parallel search. It is that **untrusted execution must be reversible, and in the limit, observable.** A fork you can `SIGKILL` is the cheapest possible "undo". That is a security property as much as a search property, and it's the argument that keeps the sandbox alive after the museum burns.

---

## 4. The missing catalyst: metrology

This is the part of the critique that is not wrong so much as **absent**, and it is the part that decides whether any of it works.

"Selection by reality" assumes a verifier that is (a) deterministic, (b) independent of the generator, and (c) strong enough that passing it means something. All three fail by default.

### 4.1 Determinism (the Crucible leaks)
The PCR metaphor works because a thermal cycler is deterministic *by construction* — temperature is exactly 95 °C. **A VM fork is not deterministic.** The same snapshot, forked twice, will produce:

- different `getrandom()` output → different keys, salts, IDs, hashes;
- different wall clock → different cache keys, token expiry checks, "is it Tuesday" branches;
- different ASLR layout → different pointer values, different heap behaviour in UB-adjacent code;
- different scheduler interleaving → different race outcomes;
- different network replies → different branches (and if the verifier needs network, you've imported the entire internet's nondeterminism into your pass/fail decision).

Consequence: a verifier that passes 7 times and fails 1 time is **always a nondeterminism bug**, and you will "promote the winning universe" based on coin flips. You will then harden a broken operator into the graph, and — per the critique's own design — *the LLM will never look at that transition again*.

**Requirement:** the fork-server must virtualize clock, entropy, network, and (ideally) scheduling before it is trusted as a verifier. This is not exotic — `clock_gettime`/`getrandom`/`socket` shims in a static `LD_PRELOAD` or a `vsock`-mediated syscall proxy — but it is mandatory, and it is ~a week of work that the critique's roadmap doesn't contain. A cheap 80% version: run every candidate **twice** from the same snapshot and require identical verdicts, and treat any disagreement as `ERROR`, never as `PASS`.

### 4.2 Verifier independence (the reward-hacking hole)
If the same model writes the operator *and* its postcondition test, then best-of-N selection optimizes for **"which candidate comes with the easiest test to pass"** — not for correctness. This is not a hypothetical; it is the observed failure mode of every self-graded agent loop.

**Requirement:** the verifier must originate outside the generator:
1. The project's own existing test suite (repos ship them — free).
2. The protocol's published spec / RFC / OpenAPI document.
3. An oracle implementation (for transmutation: the reference C++ *is* the verifier).
4. On-chain state on a local fork (for contracts: `.balanceOf()` is a verifier no LLM can negotiate with).
5. A separate model that sees **only the spec**, never the candidate, and is asked to write adversarial tests.
6. Mutation testing: deliberately break the candidate in N ways and require the test suite to catch ≥ X% of them. This is the only mechanical measure of *verifier strength*, and it should gate promotion to the graph.

### 4.3 Coverage / semantic strength
"All tests pass" and "the invariant holds" are different claims. Promotion to the graph should require a **declared postcondition plus evidence of its strength** (mutation score, differential input count, coverage delta) — otherwise the graph fills with operators that are green and wrong.

### 4.4 Silent wrongness accumulates — the tax on "never think about it again"
The critique's best line is *"once solved, never thought about again."* True, and it is also how a single subtly-wrong operator becomes a **permanent systemic bug that no model will ever inspect again.** The graph is a codebase; codebases rot. So "compile away thought" is only safe with:

- **operator versioning + rollback** (it's a git repo — you already decided that);
- **health checks on a schedule** (re-run the canary input set; a partner changing a field shouldn't be discovered by a silent wrong answer);
- **drift quarantining** (verifier flips → operator goes `SUSPECT`, and the frontier reintroduces the edge as an open problem, so attention returns to it automatically);
- **periodic re-derivation** (have a model re-derive the operator from the spec and diff against the incumbent; disagreement is a signal, even if the incumbent still passes).

Without these, the Pauling graph is a ratchet with no pawl: it only turns one way, and it turns toward silent corruption.

---

## 5. "Never touch a UI twice" → "one interaction per irreducible gate"

The law as stated fails on precisely the targets with the most money.

Concrete reasons a distilled `operator.rs` cannot replace a browser on a modern high-value target:

1. **Signed request bodies.** The JS bundle computes an HMAC/attestation over the body using a key derived at runtime (obfuscated, sometimes WASM). You can reverse it — for a few weeks, until they rotate it.
2. **Persisted GraphQL queries.** The query hash must match a server-side allowlist; new operations require an APQ round trip; schema changes monthly.
3. **TLS/session-bound clearance.** `cf_clearance` and friends are bound to the TLS session, UA, and often an attestation token. Replaying a cookie from curl is a *guaranteed* challenge. This is the whole point of bot management: it is a **provenance check on the client**, and a compiled operator has the wrong provenance.
4. **Client attestation** (Play Integrity, App Attest, Arkose/Akamai behavioural tokens): by construction, not reproducible outside a real runtime.
5. **The gate is the value.** KYC, phone verification, "verified seller" badges, credit-card-on-file. The human gate *is* the moat; there's nothing to compile around it.
6. **Terms and law.** Reimplementing an API for a service you're entitled to use is ordinary engineering. **Systematically bypassing an access-control or anti-bot mechanism** is materially different: breach of contract at minimum, and after *Van Buren* the CFAA line is narrower but still live where you evade authentication or exceed authorization. "My agent distilled the protocol" is not a defence; it is a log entry.

So the correct principle:

> **Minimize UI interactions to one per *irreducible gate*.** Compile everything downstream of the gate into native operators; keep a hardened, real browser for the gates themselves (login, attestation, CAPTCHA, signature ceremony).

And note the delightful consequence: the browser doesn't disappear, it **shrinks to a small, rarely-used, high-trust component** — a "gate pass" that runs a real Chrome under CDP with rendering intact (because the challenge *needs* rendering), a per-`.space` profile, and no other traffic. That is a much better design than either "always-on browser" or "browser is dead": it puts the expensive thing on the rare path.

**The same logic attacks the critique's own opening move.** If the browser is a cadaver to be dissected once, then so is the tab bar. The human still needs *situational awareness* — what's running, what's blocked, what it costs, what it found. That's not a window manager; it's a **telemetry console** (branch trees, operator registry, verifier pass rates, ledger, blockers). Build the console in Phase 1; the tabbed GUI multiplexer in Phase 3, for actual *use* (editing files, browsing), not for the organism. The user asked for both; sequence them.

---

## 6. Markdown rot → prose with a TTL, plus a first-class "dead" state

The diagnosis is correct and I concede it: **prose with no formal semantics accumulates contradiction, and by hour 4 it actively misleads the next model.** My earlier review said "lessons with provenance and decay", which is directionally right but too gentle. Compile-what-is-load-bearing is the correct default.

But "no prose in long-term memory" breaks on four things that are not state transitions:

1. **Negative results.** "We evaluated 14 repos; 13 fail the license check; 4 are abandonware; 2 fail to build on musl." This is *the* highest-value knowledge an autonomous organism accumulates and it is not a `(pre, op, post)` tuple. If the graph can't represent `DEAD(repo, reason, date, evidence)`, the organism re-tests the same 14 repos next week, forever. **A graph without first-class negative results is a graph that loops.**
2. **Uncertainty and strategy.** "I believe the demand side is the risk here" is not verifiable and is still essential. Represent it as a typed *hypothesis* with a confidence, a kill metric, and an expiry — not as prose, but not as an operator either.
3. **Human-facing commitments.** Pricing, promises to a client, a licence choice, a legal posture. These need prose because their verifier is a human.
4. **Prompt-shaping.** Models reason better in prose. Forbidding prose in the *working set* will make the reasoning worse, not better.

**Synthesis — a three-layer memory with a hard rule per layer:**

| Layer | Form | Rule |
|---|---|---|
| **Operators** | compiled, typed `(pre, op, post + verifier)` | Nothing enters without passing the Crucible; nothing stays without a health check. |
| **Facts / negative results** | typed records with evidence pointer + TTL | "Dead" is a first-class state. Expires, forcing re-evaluation. |
| **Prose** | `NOW.md`, `BRIEF.md`, hypotheses | Capped (≤80 lines), regenerated each turn, **never load-bearing**: if a prose fact changes an action, it must first be promoted to an operator or a fact-record. |

Plus the two things the critique underrates:

- **`02_NOW.md` should be *regenerated*, not appended.** The rot came from accrual, not from prose.
- **"Frontier-only prompt, constant size"** is not free. Computing the frontier is **graph search**; with 40 operators you can enumerate, with 500 you need a planner (beam/A* over operators with LLM-written heuristics). Constant-size prompts are an aspiration that holds while N is small — plan for a planner, don't assume it's free.

---

## 7. The 500-operator treadmill

Catalyst 4.3 proposes dissecting **500 archaic portals** into 500 protocol drivers, running on a schedule. The same document mocks exactly this: *"nobody wants to maintain 500 brittle scrapers."* That's the critique's own sentence about the market it wants to serve. Both things are true, which means the market exists *because* the maintenance cost is real — and you just volunteered to pay it, alone, autonomously, against 500 third parties who change their sites without telling you.

What actually happens at scale: weekly schema drift, silent field renames, quiet 200-OK-with-error-body responses, TLS changes, rate limits, IP bans. Every one of those is a verifier failure that must be *detected* (postcondition drift), *quarantined*, and *re-distilled* (a browser round trip). That is a per-operator maintenance tax of a few minutes a month, plus a distiller that must be kept working.

**Honest scale:** 5–20 operators for things you actually care about, with a health checker and a distiller, beats 500 with none. The number is not the flex; the **self-healing loop** is. If you want "500", what you're really building is *operator infrastructure as a product* — which is a fine business, and it is a different business (you'd sell to people who need scrapers, which brings you back to the ToS/legal surface).

---

## 8. The execution ladder — the 4GB answer the critique misses

If everything is a microVM fork, you have ~3 concurrent branches on 4GB, and no headroom for compilers or test suites. But **most of the cycling doesn't need a VM at all**: LLM uncertainty usually concerns *which program to write*, not *which operating system state to be in*. You only need a full VM fork when the *environment* diverges (package installs, service state, a mutated filesystem, hostile code).

Match the boundary to the hostility:

| Tier | Mechanism | Fork cost | Use when |
|---|---|---|---|
| **0** | in-process, no sandbox, pure function | µs | your own compiled operators, deterministic transforms — the 1000s/sec tier |
| **1** | process sandbox (`bwrap` + seccomp + cgroup, no net) | ~1 ms, 5 MB | generated code you will inspect; stateless runs; compile + test |
| **2** | warm-snapshot microVM | 10–50 ms, ~30 MB | scraped repos, build scripts, anything hostile or stateful |
| **3** | microVM, no net driver, autopsy on exit | 50 ms+ | malware, n-day specimens, adversarial bundles |

Consequences: the cheap tier gives you AFL-class cycle rates on the workloads where cycling actually pays (payloads, parameters, hypotheses); the expensive tier is used only when the *state* is the variable. Tier 0–1 is where 90% of your thermal cycles should live; Tier 2–3 is where your security story lives. Same fork-server underneath, three policies — and no 500MB browser anywhere.

This is also the answer to "isolation is decorative": **Tier 2/3 are the isolation product**, and they're cheap precisely because everything above them is not a VM.

---

## 9. Revised architecture (five primitives + a console)

1. **Fork-server / cycler.** Warm golden snapshots per toolchain; `vsock`-injected candidate; deterministic shims (clock, entropy, network replay); verifier; `SIGKILL`; state diff. Two classes of cycle (data-tier, source-tier). Batch-sample from a single LLM turn.
2. **Golden images with pre-warmed toolchains and a shared build cache.** *This is the cycle-rate lever, not the hypervisor.* Dependencies compiled once into the CAS (`sccache`/`ccache` pointed at it) so a mutation recompiles only the changed crate. Without this, "5,000 ms per branch" is a fantasy on a weak CPU.
3. **Operator registry ("Pauling graph") with a supply chain.** Typed effects, verifier reference, mutation score, version, health check, quarantine, rollback, and a first-class `DEAD` state.
4. **Dissection chamber + distiller.** A real browser exists *only* here, and only to cross an irreducible gate or to capture a trace once. Output is a native operator + a captured trace as evidence.
5. **Ledger + verifier library.** Hash-chained, append-only, replayable. **This is the moat**: the library of *verifiers* you accumulate is worth more than the operators, because verifiers are the only thing that makes any of the rest trustworthy.

**UI:** a telemetry console (branch tree, operator registry, verifier pass rates, cost/attention meters, blockers). The user-requested tabbed GUI multiplexer is a Phase-3 *use* feature, not a Phase-0 *organism* feature. Same fork-server; different front end.

---

## 10. Where the money actually is — ranked by willingness to pay, not by verifier crispness

The critique ranks hunting grounds by verifier crispness and permissionlessness. It misses the binding constraint: **demand.** An autonomous organism can manufacture infinite verified artifacts that nobody wants. The rarest resource in the entire plan is *a counterparty with a crisp verifier and a budget.*

| Hunting ground | Verifier | Permissionless | Willingness to pay | Real failure mode |
|---|---|---|---|---|
| **Smart-contract audit** (Immunefi, Code4rena, Sherlock) | Excellent (local fork, `.balanceOf`) | Mostly | **High** ($10k–$1M+) | Crowded with bots; duplicates; KYC to be paid; **never exploit live state**; scope rules are legally binding |
| **OSS vulnerability rewards** (vendor VRP, project bounties, n-day PoCs) | Excellent (crash / PoC) | Yes | Medium–high | Duplicates; disclosure norms; off-scope probing is a crime |
| **Differential transmutation / porting** (reference impl → fast Rust/WASM) | **Best in class** (the oracle *is* the test) | Yes | **Low** (nobody buys "a port") | The market is thin; monetize the *artifact* (library, benchmark, contract work), not the act |
| **Protocol distillation for your own ventures** | Good (postcondition on real API) | N/A | Indirect | Maintenance treadmill (§7) |
| **Dark data / municipal records** | Medium (schema drift) | Grey (ToS, logins) | Medium–high (if you can sell) | Sales is human; legal exposure on scraped-behind-login data; 500-scraper tax |
| **Freelance / gig / content farms** | **Poor** (subjective, delayed) | No (CAPTCHA, bans) | Low | Exactly what the critique correctly demolishes |

Read the table by column, not by row:

- **The two rows with real money both require a legal identity for payout (KYC) and both are adversarial with thousands of competitors.** So the honest projection is not "+1000× labour". It is: *a solo operator with a superb verification loop can compete in a crowded technical market*, which is a genuinely good business and a completely different claim.
- **The row with the best verifier (transmutation) has the worst demand.** Which means the correct product is: use transmutation as an *internal* capability to build things you can sell (a fast library, a benchmark, a tool), not as a revenue line by itself.
- **The synthesis I'd bet on:** invariant hunting (Catalyst 4.1) as the *demonstrable* capability — it produces an artifact with a mathematical proof attached, which is the only kind of output an unattended agent can be trusted to produce — and self-owned products as the *business*.

And the legal line, stated once: **hunt inside program scope, on local forks, under disclosure norms, with your own identity.** Autonomous exploit generation pointed anywhere else is not an OPSEC problem, it's a criminal one, and the microVM does not protect you from it.

---

## 11. The 90-day bench (and the null hypothesis nobody runs)

Most "best-of-N" wins are just "more tokens". So the benchmark must include the null:

**Setup:** 20 tasks with machine-checkable outcomes, three families:
- 7 **planted-invariant** tasks (a seeded bug in a small contract/lib; must produce a passing PoC locally),
- 7 **transmutation** tasks (a reference impl + an empty Rust skeleton; must reach 10⁶-input differential equality),
- 6 **protocol distillation** tasks (a local mock service + a captured trace; must reproduce N state transitions with a native operator).

**Arms** (all on the same free-model budget):
- **A: single-shot** — one sample, one run, no forks, no graph.
- **B: naive best-of-8** — 8 independent samples, 8 forks, keep one.
- **C: population-per-turn + cycler** — one turn emits 10 candidates; cheap-tier filter; expensive tier only for survivors; operators promoted to the graph.

**Metrics:** task success; **tokens per success**; wall-clock per success; human interventions; **% of claims that survive independent re-verification**; and — the one everyone forgets — **false-PASS rate** (run each accepted operator 50× from the same snapshot; any flip means your Crucible leaks nondeterminism, §4.1).

**Kill criteria:**
- If **C** doesn't beat **A** by ≥2× on success *at equal tokens*, the cycler's complexity isn't paying for itself.
- If **C**'s false-PASS rate is >1%, stop feature work and fix determinism first. Everything downstream is noise until then.
- If any arm needs >10 human interventions per task, the autonomy thesis is not yet demonstrated, regardless of scores.

This bench is worth more than the first three months of the VMM roadmap, and it is the only thing that will tell you whether the critique's central claim — *that selection-by-verifier is the catalyst* — is true or merely beautiful.

---

## 12. Concessions (where the critique beat my earlier review)

Being explicit, because a review that concedes nothing is useless:

1. **Display/GPU/waypipe/cross-domain Wayland: demoted.** I had it as Phase 1. It belongs in Phase 3+, and only for *human use*, never for the organism. The critique is right that it's the heaviest, least load-bearing subsystem.
2. **Persona spoofing, SMBIOS, honeytokens, blast-radius dashboards: demoted to Phase 4+/optional.** I defended these as OPSEC. They are real, but they are the *polish* of a capability that doesn't exist yet. Deferring them is correct.
3. **Protocol distillation promoted to a first-class subsystem.** I had it as "skill crystallization" (replay a trace). Compiling to a native operator is strictly better, and I under-weighted it.
4. **`EconBench` retargeted.** I proposed "deploy a site, run a smoke test" — too soft. Retarget to planted invariants, differential equality, and protocol distillation: verifiers you cannot argue with.
5. **The console-first UI is a better Phase-1 target than the tab multiplexer.** I put tabs first because the user asked for tabs. The critique is right about the *organism*'s priority; the tab multiplexer is a Phase-3 product feature.

What I do not concede: the sandbox (§3), determinism and verifier independence (§4), the irreducible gates and their legal weight (§5), negative results in memory (§6), the maintenance treadmill (§7), and the fact that **demand, not verification, is the binding constraint** (§10).

---

## 13. Decisions I need

1. **Which single hunting ground is the 90-day target?** (I'd pick planted-invariant hunting — best verifier, cleanest artifact, learnable fast. Second: transmutation, because the oracle is free.)
2. **Is a browser allowed to exist at all in v1?** (My answer: one hardened gate-pass Chrome behind CDP, used rarely, never as the primary interface. If "no browser at all", then the first target must have zero attestation — which means local/OTS targets only.)
3. **How much wall-clock per decision are you willing to accept?** This sets the population size, and therefore the entire concurrency budget.
4. **Do you accept the legal line** (scope, local forks, disclosure, your identity)? It determines which of §10 is even admissible.
