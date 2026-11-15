# Spacetime — Design Review & Second Opinion

> Companion to `BRAINSTORM.md`. No code. This assumes you read the dump and the two prior AI replies.
> Everything here is written to be argued with.

---

## 0. TL;DR — the ten decisions that actually matter

1. **The VMM is the solved part; the agent loop is the unsolved part. Build the agent loop first, on a boring VM.** Your project's risk is 90% in "can an AI run a business unattended", 10% in "can I boot Linux in 80ms". `krunvm`, `muvm`, `microvm.nix` already boot microVMs. Nobody has a working unattended economic agent.
2. **Do not embed the VMM in-process.** A virtio device bug is the real attack surface; if the VMM shares your process, one guest escape = your host shell. Run one **supervised child process per tab**, sandboxed (Landlock + seccomp + bwrap + cgroup v2). This also gives you crash isolation, per-tab cgroup accounting, and clean kill semantics for free.
3. **Branch on disk (reflinks), not on RAM.** RAM forking is a 10× cost for a 1× benefit on your hardware. `cp --reflink` an overlay dir on btrfs/xfs = instant, free, and survives reboot. Snapshot RAM only for "teleport" and only on demand.
4. **The browser is your enemy, not your ally.** A browser in a microVM on a no-GPU laptop falls back to `llvmpipe` software GL: ~100% CPU on scroll, 400–900MB. On 4GB that is *the* binding constraint on the entire product. Design everything else around it.
5. **"Seamless guest window in a host tab" is the hardest unsolved piece you listed** — Alyssa Ross has spent years on it for Spectrum and WSLg compromised with Weston chrome. Do not make it a Phase-1 feature. Frame-per-tab texture is v1; cross-domain Wayland is a research track.
6. **`smoltcp` is not an Ethernet switch.** Use libkrun's **TSI** or **passt** (userspace, rootless, TCP+UDP). But TSI/passt is also a *hole into host localhost* — filter at `connect()`, or the whole isolation story dies on line one.
7. **You cannot DLP-scan TLS egress.** The "Aho-Corasick scrubber on outbound traffic" only works on plaintext. Secrets must be protected by **never giving the guest the secret**, not by scanning bytes on the way out. → session-cookie airlock + virtual WebAuthn + vault tokens.
8. **The #1 security risk of the AI design is prompt injection, not malware.** An autonomous agent with a wallet that reads web pages *will* be hijacked by a malicious page. Therefore **the constitution must be policy-as-code (Cedar), not prompt text.** Prompt guardrails fail exactly when they matter.
9. **Your "+1000× leverage" comes from reuse and parallelism, not from a smarter model — and the bottleneck is verification bandwidth.** The human is the trust bottleneck, so the system must manufacture *proof*, not claims. Build a verifiable eval harness (call it `EconBench`) before you build the business.
10. **`.space` should be a git repo.** You already wanted "git-style branching for environments". Make the memory directory a git repo with LFS, and you get history, diff, remotes, P2P transfer, and merge — for free, with a mental model you already have.

---

## 1. Corrections: where the previous replies were wrong or optimistic

These matter because they're load-bearing in your plan.

### 1.1 `virtio-fs` + DAX is not a portable memory trick
The previous reply's core memory story ("DAX = host page cache shared across all VMs, so 10 VMs cost 1× RAM") is real technology — but:
- Upstream **rust `virtiofsd` has no DAX**; the C virtiofsd that had out-of-tree DAX patches is deprecated and gone.
- DAX in virtiofs survives only because **libkrun ships its own in-process virtio-fs** (Asahi depends on it) and because Kata carried patches. See the 2025 kvm-forum thread where Vivek Goyal asks "how are you even testing DAX, rust virtiofsd doesn't have it".
- **Conclusion:** DAX is available *only if* you commit to libkrun's filesystem implementation. If you ever move to Cloud Hypervisor + rust virtiofsd (which you might, for snapshot/restore and device richness), **your memory story evaporates**. Don't build the architecture on it.
- **What to rely on instead** (all portable): host page cache sharing via virtio-fs (one copy of `libc.so` on the host, all guests read through FUSE into it), `virtio-balloon` free-page-reporting, `madvise(MADV_PAGEOUT)`, zram, and a small per-guest page-cache budget (`vm.vfs_cache_pressure`, low dirty ratios). Also: keep the base rootfs **EROFS** and let the host's page cache serve it.

### 1.2 Firecracker cannot be your primary engine
Firecracker's whole point is *fewer devices*. You need `virtio-fs` (dependencies, CAS, autopsies), `virtio-gpu` (windows), and a balloon with real semantics. Firecracker has none of the first two. Keep Firecracker on the shelf for exactly two jobs where it wins: (a) **snapshot/fork of RAM** — it is the most mature at full VM snapshot with UFFD lazy-load, and (b) headless offline compute. Its security model (jailer, one process per VM, tiny device set) is also the model you should copy even if you don't use it.

### 1.3 "50–100ms to a rendered app window" — split the claim
- **Boot to a shell prompt:** 30–80ms is realistic with a custom Rust PID 1 and a tiny kernel. Real.
- **Boot to a *rendered GUI app window*:** 250ms–2s for GTK/Qt (fontconfig + icon theme + dbus + first paint), and 3–8s for a browser. Not real.
- **Fix:** pre-boot a **pool of warm, paused VMs** (snapshot after init, before the app). Then "open tab" = `krun_restore` + exec = a few ms *perceived* + the app's own startup. This is the single biggest UX win available to you and costs almost nothing to implement once snapshot/restore exists. (libkrun added snapshot/restore in 2026, HVF first; Cloud Hypervisor and Firecracker both have it mature.)

### 1.4 `smoltcp` — right instinct, wrong layer
A `virtio-net` device hands you raw Ethernet frames. `smoltcp` is a TCP/IP stack, not a switch: you'd have to terminate every guest connection and re-originate it. That is exactly what **libkrun TSI** and **passt** already do, in ~0 of your time, including UDP. Writing smoltcp glue yourself costs weeks and you'd immediately hit the same wall they did: **QUIC/HTTP3 is UDP**, and a TCP-only path breaks a large fraction of modern web traffic.

**Use:** libkrun TSI (in-VMM, zero host setup, TCP+UDP) or passt/pasta (external, more policy surface) — and keep `smoltcp` for the *inspection* layer only if you need per-flow rewriting (TLS morphing, DNS pinning). Which, honestly, you probably don't in v1.

### 1.5 The localhost escape hatch (this will bite you)
TSI, passt, gvproxy, and every "transparent" userspace network works by intercepting the guest's `connect()`. If you don't filter, the guest can reach **the host's 127.0.0.1** — your SSH agent socket, your `~/.docker/run/docker.sock`, your Chrome `--remote-debugging-port`, your local model servers, your clipboard daemon. That is a one-request total compromise of the host, delivered by the very component you added for isolation.

**Rule:** a `connect()`-level policy check on every socket, deny-by-default on: `127.0.0.0/8`, `::1`, `169.254.0.0/16` (cloud metadata), link-local, host LAN subnets, UNIX sockets (unless on an explicit allowlist), and the host's own listening ports. Do this *before* you write any feature.

### 1.6 "0 traces on your main OS" is falsifiable — here's the leak list
You will ship this claim to yourself, so audit it now:
- **Core dumps** — a VMM crash dumps guest RAM (i.e., all malware artifacts, possibly secrets) to disk. `ulimit -c 0`, `LimitCORE=0`, `memfd` + `MADV_DONTDUMP` on guest RAM.
- **Host crash reporters** — Windows WER / macOS Crashpad upload guest memory. Disable per-process.
- **Swap** — `memfd` pages can be swapped. `mlock` what you can, accept the rest, use encrypted swap or zram only.
- **Clipboard history** — GNOME/KDE clipboard managers, Windows Win+V, macOS clipboard sync to iCloud. Your airlock must bypass *and purge* history. This is the #1 accidental leak.
- **DNS** — if the guest's resolver is the host's, every query is logged by the host resolver and your router. Guest must get DNS **through the tunnel**, never via host `resolv.conf`.
- **Filesystem traces** — `~/.local/share/spacetime`, recent-files lists (MIME DB, `~/.recently-used.xbel`, Windows jump lists), thumbnails, temp dirs, shell history.
- **GPU shader caches** — `__GL_SHADER_DISK_CACHE`, Mesa shader cache, D3D shader cache. Disable for guest rendering.
- **Audio** — PipeWire streams, and PipeWire logs. Off unless requested.
- **Router/NAT/firewall logs** downstream of your WAN — you cannot control these; say so in the docs.
- **Thermal/timing side channels** — a determined observer correlates your CPU load with traffic. Out of scope; just don't claim otherwise.

### 1.7 Anti-fraud: the honest calibration
The previous reply claims you can make a Python script inside a VM "look like real Chrome to Cloudflare". Partly true, partly cope:
- **What you *can* fix in Spacetime:** TCP/IP fingerprint (because *the proxy's* stack is what the server sees — your guest's stack never touches the wire), JA3/JA4 + HTTP/2 frame fingerprint (east-west re-origination, or a real Chrome over CDP instead of a fake one), ASN/IP reputation (residential/mobile egress), cookie/WebGL/font/timezone consistency (persona seeds), and *behaviour* (real browser, real timing, real mouse curves).
- **What you will not reliably beat:** Cloudflare/Akamai/Datadome bot management combines all of the above *plus* behavioural biometrics, TLS-in-JS attestation, and cross-site reputation graphs. Beating them at scale is an adversarial game with paid defenders — not a feature, a lifestyle.
- **Design consequence:** don't architect for "invisible to anti-fraud". Architect for **"one honest browser identity per `.space`, with its own residential-ish egress, used like a human"**. That's a defensible product. "Undetectable" is not.
- **Also a legal/ToS constraint, not just a technical one:** multi-account farms, automated account creation, and CAPTCHA-solving-for-hire violate the terms of essentially every platform you'd target, and "evading anti-fraud systems" has real legal exposure in some jurisdictions. Where money is involved you will end up as the merchant of record with a real legal identity anyway, so build the system assuming **KYC is you**, and the agent acts *as your agent under your identity* where you're permitted to automate. That constraint will shape which business models are viable (see §7.11) — better to know now than after building 4,000 lines of proxy plumbing.

---

## 2. The 4GB budget (this is the real spec)

Numbers are guest-visible RSS, typical, on a Linux host.

| Component | RAM | Notes |
|---|---|---|
| Host OS desktop (light) | 700–1200 MB | GNOME/KDE on 4GB is already a luxury; XFCE/sway matters |
| Spacetime shell (egui + compositor) | 80–150 MB | egui/iced, no Electron, no WebView |
| VMM process overhead | 8–20 MB | libkrun/CH/Firecracker |
| Guest: kernel + musl Rust init + busybox | 18–35 MB | no systemd, no dbus unless needed |
| Guest: GTK3 app | +60–120 MB | |
| Guest: Firefox, 1 tab, llvmpipe | **450–900 MB** | the killer |
| Guest: Chromium, 1 tab, llvmpipe | **350–700 MB** | the killer |
| Host-side headless Chromium (AI chat bridge, 3 tabs) | **250–500 MB** | second killer |
| zram (compressed, reclaimable) | 1000–1500 MB virtual | costs ~350MB real at 3:1 |

**Consequences you should accept up front:**

- After the host, you have ~2.6GB. That is **either** one browser tab + three light VMs, **or** five light VMs and no browser. Never both.
- On **Windows** (1.8–2.5GB gone before you start) Spacetime is essentially unusable under 8GB. Windows also needs nested virt (WSL2) or libkrun's WHPX backend — which today is single-vCPU and **has no virtio-gpu**. So: Windows is a "later, with warnings" target.
- On **Apple Silicon**, "Linux apps" means *arm64* Linux apps. x86 Linux binaries need FEX/Box emulation — fine for a text editor, hopeless for a browser. Say this in the README before someone downloads it.
- **Temporal multiplexing is not a cute name, it's a hard requirement.** Good news: you'd already arrived there.

**The one structural escape hatch:** split **Cortex** (the AI brain: needs a long-lived browser session, ~400MB, low CPU) from **Body** (the VMs: need RAM/CPU, low persistence). Put Cortex on a cheap always-on box (a €4 VPS, or that old phone), and hairpin *its* egress back through your local residential link over WireGuard so the model provider still sees a residential IP. Your 4GB laptop then runs only Bodies, and the chat-bridge RAM disappears entirely. This single decision may be what makes the whole thing usable.

---

## 3. Architecture, revised

### 3.1 The layer cake, using your own vocabulary

You asked whether `.wavefunction` belongs here. Yes — and it maps beautifully onto the quantum metaphor you already built:

```
┌─────────────────────────────────────────────────────────────────────┐
│ WORLD-LINE  (.space)      one venture / one tab / one chain of state │
│   persona · memory · egress class · overlays · snapshot chain        │
├─────────────────────────────────────────────────────────────────────┤
│ EPHEMERAL   tmpfs upperdir (RAM)  ← destroyed on close               │
├─────────────────────────────────────────────────────────────────────┤
│ BRANCH      reflink clone of a frozen overlay (disk CoW, free)       │
├─────────────────────────────────────────────────────────────────────┤
│ COLLAPSED   verified, promoted, content-addressed diff (read-only)   │
├─────────────────────────────────────────────────────────────────────┤
│ UNIVERSE    .wavefunction — immutable CAS of deduped, signed layers  │
├─────────────────────────────────────────────────────────────────────┤
│ BASE        EROFS minimal rootfs, dm-verity, digest pinned in lock   │
└─────────────────────────────────────────────────────────────────────┘
```

- **`.wavefunction` = the space of possibilities** (immutable, shared, deduplicated). Dependency `pytorch` exists there exactly once. Nothing in it is trusted *because it exists* — see provenance tiers below.
- **`.space` = a collapsed world-line** (one actual history). Deleting it kills that history and nothing else.
- **Collapse** = the operation of promoting a verified change from an ephemeral tab into the shared universe. *The wavefunction collapses when you — or the policy engine — verify it.* That's the operation that solves your "shared dependency" vs "zero traces" contradiction, and it's the one word that makes the model understandable to a user.

### 3.2 Provenance tiers (the fix for poisoning the global cache)

The previous reply said "the host verifies hashes" — necessary, not sufficient. A hash only proves *it's the same bytes as last time*, not that it's trustworthy. Add a **tier** to every CAS object:

| Tier | Meaning | Shareable? |
|---|---|---|
| `upstream-signed` | Fetched from the ecosystem's official index, signature verified (`.crate` sig, PyPI hashes + TUF, `nix` narinfo, apt InRelease) | Yes, globally |
| `repro-built` | Built inside a sandbox from `upstream-signed` inputs, build log attached | Yes, with log |
| `tab-local` | Produced inside a tab with network access | **Never** — invisible to other tabs |
| `quarantined` | Produced by a tab flagged by policy (fork bomb, weird egress, autopsy anomalies) | Never; deleted on close unless you promote it |

This means: if a tab downloads something dodgy, it cannot infect the shared pool, because the shared pool only accepts `upstream-signed`/`repro-built`. The "global container" you asked for becomes an **append-only, signed, tiered CAS** — structurally, not by convention. `composefs` (EROFS + fsverity + overlayfs) is exactly the right substrate, and `ostree`/`nix` are the right prior art for the layering.

### 3.3 Branch = three things with one button

"Branch Tab" should do all three, cheaply, in this order:

1. **Memory branch** — `git commit` in the `.space` repo (instant, free, diffable, pushable). *Your brain is versioned.*
2. **Filesystem branch** — `cp --reflink=always -r overlay/ branch-N/` (instant on btrfs/xfs, ~seconds on ext4/Windows). No RAM cost.
3. **RAM branch** — snapshot/restore the VM with `MAP_PRIVATE` on a `memfd` of guest RAM (CoW). Only when the user asks for "resume exactly here". This is the expensive one; make it opt-in and label it "deep freeze".

Nine times out of ten the user wants (1) and (2). Give them those in 50ms and the feature feels magical. Firecracker's UFFD snapshot loading is the reference implementation for (3).

### 3.4 Per-tab process model

```
spacetime-shell (UI, no privileges, no network)
├── tab-3a1f  ── supervisor (Rust, tokio)
│   ├── vmm (child, bwrap+Landlock+seccomp, cgroup tab-3a1f, ulimit -c 0)
│   ├── virtiofsd-ish exporter (only the tab's allowed dirs)
│   ├── netns: TSI/passt + policy at connect()
│   └── audit tap: every connect(), every fs mutation → ledger
└── tab-…
```

The **supervisor** is the only thing the UI talks to, the only thing the policy engine trusts, and the only thing that can kill the tab. It is your "ring -3". It should be the smallest amount of code in the project — target: a few thousand lines you can actually read in an evening.

### 3.5 Six invariants worth hard-coding

These should be **structurally impossible to violate**, not checked-and-warned:

1. A tab with `net = none` has a guest kernel built with `CONFIG_VIRTIO_NET=n`. Not "no device" — **no driver code**.
2. No guest packet reaches host `127.0.0.0/8`, host LAN, or host UNIX sockets. Ever, regardless of config.
3. Secrets are never present in the guest filesystem, environment, or argv unless a policy decision put them in a specific *input event*.
4. Any action with an irreversible external effect (spend, post, delete, send) is either within pre-authorized budget or queued for approval — never executed on model intent alone.
5. Every observable byte the AI sees passed through the redaction layer.
6. Every action + result + verifier proof appends to a hash-chained ledger that the AI cannot write to.

---

## 4. Networking — a decision, not a diagram

You already produced the right diagram. What you need is a *choice*. Mine:

| Need | Use | Why |
|---|---|---|
| v1 egress | **libkrun TSI** | zero host setup, TCP+UDP, rootless, one code path on Linux/macOS/Windows |
| Policy at the boundary | your supervisor's `connect()` hook | single choke point; where all filtering, logging, and rewriting happens |
| Per-tab isolation | one VMM process per tab; no TAP, no netns, no root | avoids the whole cross-platform iptables/netns nightmare |
| Offline/analysis tabs | kernel without virtio-net | physical impossibility, verifiable |
| Egress routes | three named classes: `none`, `proxy(name)`, `direct` | `proxy` = SOCKS5 into a relay chain (WireGuard → residential peer, 4G dongle, or a residential pool) |
| DNS | tunnel-only, pinned to a per-tab private resolver | prevents host-resolver leaks and gives you per-tab DNS policy |
| Fingerprint | the *egress node's* stack, plus optional east-west TLS re-origination | TCP/IP fingerprint is what the proxy looks like, not the guest |

**On TLS morphing (JA3/JA4):** powerful and dangerous. If you MITM your own guest, you must inject a CA into the base image, which means (a) the guest's own cert-pinning apps break, (b) anyone auditing your image finds a CA and loses trust in you, (c) you now hold the plaintext of everything, which is a *bigger* secret-handling burden. **Recommendation: skip TLS morphing in v1.** Get a real Chrome over CDP to make the request instead — the best fingerprint spoof is the real thing. Only build `rama`/`utls` based morphing for scripted clients, and keep it strictly opt-in per tab.

**On Tor:** keep it as an explicit egress class (`tor`), clearly labelled, for the cases where "I'd rather be blocked than identified" — tor browser usage, research, journalism, threat intel. Never let it be the default egress for business tabs; it's a rate-limit-and-ban machine.

---

## 5. Display — ranked for *your* hardware

| Path | Cost | Cost to build | Verdict |
|---|---|---|---|
| **A. Per-VM framebuffer → host texture** (guest runs a headless wlroots compositor or Xvfb; pixels via virtio-gpu 2D or `waypipe` damage stream; host composites into the tab) | ~5–15MB per VM + damage bandwidth | Low (weeks) | **v1. Do this.** Uglier, works everywhere, on llvmpipe. |
| **B. Client-side Wayland proxy** (`waypipe` over vsock) | ~10MB + zstd CPU | Low-med | Good for GTK apps; still needs a guest compositor; fights with a11y tree. |
| **C. Cross-domain Wayland via virtio-gpu context types** (guest windows become host surfaces — Spectrum/ChromeOS ARC model) | Low CPU, best UX | **Very high** (patched VMM, agent memory sharing, dmabuf direction problems) | Research track. Study, don't ship. |
| **D. VirtualBox-style full desktop** | 300MB+, slow | — | Never. You already know. |

Two notes the previous replies missed:
- **`waypipe` needs a compositor inside the guest** (it proxies *to* a compositor; it isn't one). Budget for a headless `wlroots`/`labwc`/`cage` inside the image (+10–20MB) or use X11 + `Xvfb`/`Xwayland rootful` for old apps.
- **GPU acceleration in a guest requires DRM *native context*** (freedreno/amdgpu/asahi) or virgl/venus. On a typical Intel iGPU laptop you get **virgl at best, llvmpipe at worst**. Plan for software rendering and make the shell's own compositing as cheap as possible (`egui` with `glow`, damage-limited repaints, no per-frame full-screen passes).

**Terminal tabs:** your instinct (virtio-console → `alacritty_terminal`) is correct and cheap. Use `egui_term` (exists) or `vt100`/`avt` if you want fewer deps. This is the one thing that will be truly instant, so make it excellent.

---

## 6. Clipboard & file airlock

The previous reply is right; add the parts that keep you alive:

- **Clipboard** is *pull-only and gesture-initiated*. The guest cannot push. On pull: strip ANSI, strip zero-width/bidi-override Unicode, cap size, and **purge the host clipboard manager's history**. On push: paste as plain text by default; rich text requires an explicit "trust" click.
- **Files in:** drop zone directory, exported read-only into that tab only. Volatile (`tmpfs`) so a reboot cleans it.
- **Files out:** the dangerous direction. Always mark *quarantine*: `com.apple.quarantine` + xattr on macOS, `Zone.Identifier` ADS / Mark-of-the-Web on Windows, and a "from Spacetime" xattr on Linux. Show a one-line warning with the autopsy summary of the tab it came from. **This is the only place the host can actually get infected, so treat it as the crown jewel.** `dangerzone` (Qubes) is the reference product for exactly this hazard.
- **Never auto-sync a directory.** No shared folder between host and guest, ever. That's how every VM escape story starts.

---

## 7. The AI (the heart) — where I'd spend 70% of the effort

### 7.1 Be honest about *why* current agents fail

Ranked by how much they'll hurt *you*, not by how often people talk about them:

1. **Verification.** The agent says it built a site; nobody checks. Every failure mode downstream (myopia, hallucinated success, dead ideas kept alive) is a symptom of unverified claims. → **Fix: the supervisor, not the model, decides what's true.**
2. **Blockers.** CAPTCHA, phone verification, KYC, email confirmation, "your account is under review", 2FA. An agent that blocks is an agent that's idle. → **Fix: never block; queue and pivot (§7.4).**
3. **Brittle GUI.** A DOM selector or a coordinate breaks and the agent burns 20 turns confused. → **Fix: crystallize (§7.8) + self-test.**
4. **Prompt injection.** A page (or a GitHub README, or a "helpful" comment) tells your wallet-bearing agent to send funds. → **Fix: policy-as-code (§7.5) and taint-separated observation (§7.6).**
5. **Context degradation.** Real, but the *least* important of the five, and the one everyone obsesses over. If 1–4 are fixed, a fresh-context agent with good external state beats a 1M-token agent with bad state every time.
6. **Account linkage.** Two "different" businesses on the same laptop, same canvas hash, same cookies → platform links and bans both. → **Fix: persona seeds per `.space` (§7.12).**

### 7.2 The Cortex/Body split, and the adapter contract

You want web chat models as the primary brain because they're free. Fine — but architect it as **one contract, many channels**:

```
Cortex (any brain)  ── emits text containing typed syscalls ──►  Supervisor
                                                                   │ validates vs policy
                                                                   │ executes in Body
                                                                   └─ returns observation
```

- **The "sandbox problem" isn't a problem.** A web model is a text function. It has no hands to sandbox. Spacetime doesn't *break* its sandbox; it **gives it a syscall ABI**. The security boundary is your parser + policy engine, not the model's goodwill. This reframing matters because it tells you where to spend effort.
- **Adapters**, all implementing the same trait: `web-cdp` (arena.ai/Claude/ChatGPT/DeepSeek/Kimi via CDP), `api` (OpenRouter/DeepSeek/Gemini/Groq/Cerebras/GitHub Models/NVIDIA NIM/free tiers), `local` (a 0.5–1.5B model for classification/routing only), `human` (you, as a fallback channel — sometimes the best oracle is one message).
- **The web-cdp channel is a degradable channel, not your foundation.** It breaks when a site ships a redesign; it's rate-limited; it's against most providers' ToS; it has 5–30s latency. So: put it behind a **canary** — on startup, and every N hours, the adapter runs a self-test (open chat, send a fixed probe, verify the reply parses). If the canary fails, the router silently shifts weight to API channels and files a `BLOCKER`. This is the difference between a system that runs for a year and one that dies on a Tuesday.
- **Free-tier pool (concrete):** Gemini Flash free tier, Groq free tier, Cerebras free tier, GitHub Models (free with a PAT), OpenRouter `:free` models, Mistral free tier, Cloudflare Workers AI daily free, NVIDIA NIM credits. Rotate them behind one router with per-provider budget and health. Limits change constantly — treat the list as config, not code.
- **Latency hiding** (novel, cheap): while waiting 15s for a web chat reply, the supervisor runs *speculative read-only work* — prefetch the next likely DOM snapshot, prewarm the next tab from the warm pool, pre-run the verifier probes. Turns dead latency into progress.

### 7.3 Memory: "context is a view, not a store"

The `.md` file idea is right; the missing principle is that **the authoritative state lives on disk and the prompt is *reconstructed* every turn.** Nothing important is ever only in the context window.

Concrete layout (this is a spec, not code):

```
memory/
  00_CONSTITUTION.md   # goals, boundaries, kill metrics — compiled to policy (see §7.5)
  01_GOALS.md          # goal tree, YAML front-matter: id, parent, budget, kill_metric, deadline
  02_NOW.md            # regenerated each turn: last ≤20 events, ≤80 lines. Never grows.
  03_LEDGER.jsonl      # append-only: syscall, args_hash, result, verifier proof, tokens, cost
  04_LESSONS.md        # each entry: claim · provenance(model,turn) · confidence · last_verified · TTL
  05_SCAVENGER.md      # repos evaluated: url, license, stars, build result, why rejected/accepted
  06_SKILLS.md         # index; each skill has a test + last-green timestamp
  07_EXPERIMENTS.md    # hypothesis · spend · metric · verdict(dead/alive/scaling)
  08_PERSONAS.md       # accounts: handle → @vault:token, persona_seed, egress class, created-on
  09_BLOCKERS.md       # what needs a human: exact steps, why, what was tried
```

Rules that make it work:
- **Retrieval, not dumping.** Don't paste `04_LESSONS.md` into the prompt. Index it with **BM25 (`tantivy`)** and retrieve top-k for the current task. Embeddings are optional and on a 4GB CPU-only box, lexical + LLM-written summaries beat a 30MB embedder. Zero model RAM.
- **Provenance + decay on lessons.** Every lesson records which model/turn produced it and when it was last verified. Lessons expire. This is how a *new* model safely inherits an *old* agent's brain: it gets the lesson **and** the right to distrust it. (Directly answers your "when I switch models, the new agent inherits what the old one did" — including, crucially, "and knows what to re-test".)
- **`02_NOW.md` is the only file always in context.** Everything else is retrieved. This is the anti-hallucination device: a small, always-true window + a big, searchable, *audited* store.
- **Make `memory/` a git repo.** Free history, diff, blame, remotes, and the `.space` "teleportation" story becomes `git push`. It also gives you free merge semantics if you ever run two agents on one venture (you will).

### 7.4 Never block: the interrupt budget

This is my direct answer to "the last thing I want is the AI waiting for me to click Allow".

- **Approvals are a queue, not a modal.** A blocked action is written to `09_BLOCKERS.md` with full context and the agent immediately pivots to the next goal in the tree. You review the queue once, in a batch, when *you* choose.
- **Capability tokens instead of prompts.** Pre-authorize scopes: `spend ≤ $2/day`, `post ≤ 3/day`, `spend only from earned balance`, `no new accounts`, `no irreversible deletes`. Anything inside the token executes without asking. Anything outside is queued. You'll find you set 8 tokens once and never see a dialog again.
- **Steering mailbox.** You can inject a message at any turn boundary; the supervisor merges it as a high-priority directive with a TTL (it decays after N turns so an old instruction doesn't haunt the agent forever). This is exactly the "I can steer while it works, my choice whether to" you described.
- **Model as a durable workflow, not a loop.** The agent run is an event-sourced state machine: every step is a persisted event, so it survives laptop sleep, crashes, model swaps, and *your* absence. Steering prompts and approvals are **signals** into that machine. You don't need Temporal for this, but you do need to *design* for it from day one — retrofitting durability into a `while true { }` loop is miserable.
- **Two clocks.** Fast clock: tool calls (seconds). Slow clock: strategy review, budget, kill/scale decisions (hourly). **You only interface with the slow clock.** That's what makes "1 hour of your day" plausible.

### 7.5 Ring -3 as *policy as code*, not vibes

You said: "I'm ring -3, AI is ring 0." Enforce it. Not with prompt text — with a policy engine.

- `00_CONSTITUTION.md` is **compiled** into a machine-readable policy (Cedar is a good fit: Rust-native, designed for exactly this, human-auditable). If it's only prose, it's a suggestion; if it's compiled, a violation is a *type error*.
- Every syscall is validated against: current goal, capability tokens, budget, egress class, taint level of the inputs that produced it.
- **Hash-chain the ledger.** Each entry = hash(prev_hash + action + result + proof). That gives you: tamper-evident audit, exact replay, "what did the AI do at 03:14 last Tuesday", and a defensible story if a business decision goes wrong. It costs ~30 lines and it's the difference between a toy and a system.
- **Taint tracking** (this is the anti-prompt-injection core): every byte of observation carries a provenance tag — `trusted(user)`, `trusted(constitution)`, `untrusted(web)`, `untrusted(tool-output)`. Policy rule: *untrusted content can inform, never instruct.* Concretely: an action whose justification derives from `untrusted` content may not touch a spend, a credential, or a destructive op without approval. This is the only defense that survives a clever injection, because it doesn't depend on the model noticing.

### 7.6 Secrets: three mechanisms, layered

You asked how to secure credentials the AI creates when prompts are logged. Combine:

1. **Symbolic tokens (`@vault:gh_alpha`)** — the model never sees or invents a secret; it emits a *handle*. The host substitutes at the last possible moment. (The previous reply had this; it's correct and essential.) Add: **per-goal injection counters** (an injected password cannot be re-typed 40 times — that's a leak signal) and **canary tokens** (fake secrets in the vault whose appearance anywhere = exfiltration alarm).
2. **Session-cookie airlock** — this is the part people miss. Logging in requires the plaintext password to exist *somewhere*. So: put it in a **dedicated, ephemeral, network-minimal "login tab"** with nothing else installed and no other browsing, do the login, then **transfer only the resulting session cookie** into the working tab and destroy the login tab. The password exists for ~60 seconds, in a VM that has run nothing but a browser. This is how you get to "the working tab never held the password".
3. **Virtual WebAuthn / passkeys** — best-in-class, and cheap given you already control the browser: CDP supports `WebAuthn.addVirtualAuthenticator`, and credentials are **exportable**, so the private key can live in the host vault and be re-injected at login. Result: phishing-resistant, non-guessable, password-free accounts for the AI, with the key material never in the guest's filesystem. When a site supports passkeys, prefer them over passwords *every time* — it removes an entire class of "the AI pasted the password into a chat" failure.

Plus: **the observation firewall**. The AI's view of the screen is *rendered by the host* from the semantic tree, with `type=password` fields masked by construction, PII patterns redacted, and screenshots available only for tabs with no secret ever injected. The AI cannot leak what it was never shown — that's a stronger guarantee than any scrubber.

### 7.7 The Reality Oracle, and the "prove it" protocol

The previous reply's scoreboard is right. Make it mechanical:

- **Verifiers are code, owned by the supervisor, and run *after* the agent claims success.** HTTP 200 + content hash for a deployed site; `gh api` for a PR's state; an RPC call for a balance; a Playwright smoke test for a UI flow; a diff for a file change.
- **A claim without a verifier is not a claim.** It's logged as `unverified` and cannot advance a goal. This is the single highest-leverage rule in the whole project.
- **Immutable metrics with timestamps.** The scoreboard is regenerated from the ledger + external probes, never from the agent's memory of what happened.
- **Kill-or-scale is a policy, not a mood.** Every experiment declares its kill metric *before* spending begins (`07_EXPERIMENTS.md`); the supervisor flips it to `DEAD` when the metric expires. An agent cannot be optimistic about a metric it doesn't control.

### 7.8 Skill crystallization — plus the part everyone forgets

Crystallizing a GUI trace into a deterministic script is your best idea for throughput. Add:
- **Every skill ships with a test.** A skill without a green test in the last N hours is *disabled* and the agent falls back to GUI exploration. Sites change; crystallized skills rot silently, and a rotted skill is worse than no skill (it fails in confusing ways).
- **A nightly skill self-test cron** with a small, cheap model or none at all — this is the maintenance loop that keeps the organism alive while you sleep.
- **Skill provenance** — which model produced it, from which trace, success rate. You'll discover that some models write skills that generalize and others write overfitted garbage.

### 7.9 The parliament (multi-model, free)

Since web chats are ~free, don't run one mind — run three cheap ones:
- **Executor** (does the work), **Critic** (reads the trace, says what's wrong — this catches most hallucinated success for free), **Scribe** (compresses the turn into `02_NOW.md` and lesson candidates).
- The Critic is the highest-value role and the one everyone skips, because a model grading its own homework is worthless. A *different* model with a *different* prompt and *only the trace* is a genuinely independent signal.
- Also useful: **model portfolio diversification** — route tasks by observed competence (coders to one, writers to another, planners to a third), tracked in the ledger. Empirically, model-competence-per-task is stable enough to exploit.

### 7.10 `EconBench` — the thing that makes this a project instead of a dream

**You cannot optimize an agent loop you cannot measure.** Before real money, build a harness:

- 30–50 **sandboxed tasks** with *verifiable* outcomes: "fork repo X, deploy to a local pages-like host, produce a passing smoke test", "scan a mock bounty board, pick 3 eligible, submit 3 PRs to local mirrors, get 3 merges", "produce a 60s video from clips with ffmpeg and hit a spec", "write and pass a 400-line spec through a coder agent".
- A **mock economy**: fake wallet with a real ledger, mock freelance board, mock payment confirmation. Payouts are *verifiable events*, not vibes.
- Metrics that matter: **completion rate, human-interventions-per-task, wall-clock, tokens, $ , % of claims that survived verification.** Track them over time, per model, per architecture.
- Then: change one thing (prompt, memory layout, critic, crystallization) → measure. That's how you'll find the 10× instead of guessing.

This is my strongest recommendation in the entire document. Everything else is infrastructure; this is the instrument.

### 7.11 Which businesses actually work with this stack

Being blunt, because it saves months:

| Model | Fit | Why |
|---|---|---|
| Micro-SaaS / niche tools (fork + 5% change + deploy) | **Excellent** | Deterministic verifiers (HTTP 200, smoke test), no KYC to build, your identity as publisher |
| Content pipelines (video/text/SEO) | Good | Fully automatable, verifiable output, monetization needs accounts you already have |
| Freelance/bounty work | Medium | Real money, verifiable, but account reputation is the moat and platform ToS matters |
| Repo maintenance / OSS bounties (Algora-style) | **Good** | Verifiable (PR merged), crypto rails, no KYC on the receiving side |
| Arbitrage / trading | Bad for v1 | Irreversible, adversarial, needs capital, verifiers can't catch "it worked but you lost money" |
| Ad/affiliate farms, mass accounts, "10 YouTube channels" | **Legally/ToS hostile** | Account linkage detection + ToS = both entities die. Automate *your* channels, not 10 synthetic identities |
| Anything requiring KYC (exchanges, payments, banking) | Blocked | KYC is you. Design the agent to *prepare* and you to *sign* |

The pattern: **the agent is great at work with a crisp verifier and terrible at anything requiring a legal identity.** Build toward the former. That's not a limitation of your design, it's a limitation of the world, and knowing it early is worth more than any VMM optimization.

### 7.12 Persona seeds and the "one identity per `.space`" rule

`persona.toml` derives, from a single seed: SMBIOS strings, CPUID leaves (hide `KVMKVMKVM`, present a plausible Dell/Lenovo/ASUS board), MAC OUI, timezone/locale, font list, canvas/WebGL/audio noise, screen geometry, and a battery profile via virtio. Store the seed, not the outputs — the outputs must be *reproducible* so a `.space` moved to another machine "is still the same device" and doesn't trigger a "new device" logout.

Two honest caveats: (a) hiding virtualization from *commodity malware* is achievable and worth doing; hiding it from *anti-fraud ML* is a losing arms race, so don't build your value proposition on it; (b) **do not randomize per-page-load** — real machines are stable. A device fingerprint that changes between requests is *more* suspicious than a VM signature.

---

## 8. Novel ideas not in your dump (cheap, high-leverage, in rough value order)

1. **Warm-VM pool.** Keep 3–5 paused, post-init snapshots. "New tab" = restore + exec. Perceived launch becomes 5–20ms *and* saves energy. Costs one snapshot per base image.
2. **`.space` as a git repo** (§7.3). History, diff, remotes, push-based teleport, merge.
3. **Ledger-tap live "intent ribbon".** A thin bar under each tab: `read 14 files · wrote 2 · DNS 3 · will spend $0.00 · risk: low`. Streaming from the audit tap. Turns opaque autonomy into legible autonomy — and it's how you'll *notice* the agent going wrong before it costs money.
4. **Provenance tiers for the CAS** (§3.2). Prevents cross-tab supply-chain infection with no runtime cost.
5. **Deterministic record/replay per tab.** Intercept time (`clock_gettime` shim), RNG (`getrandom`), and network replies; store the trace. Gives you: reproducible bug reports, "replay this attack", and the ability to *test policy changes against historical malware behaviour*. Cheap version: only intercept at the vsock/net boundary, don't try to be `rr`.
6. **Tombstone ledger.** On tab close, store `{space_id, base_digest, blast_radius_hash, egress_summary, duration}` forever (a few KB). You now have an audit trail of *everything that ever executed*, without keeping any of it. Pairs perfectly with your Autopsy view.
7. **Observation firewall** (§7.6). Redaction at the render layer, not the string layer.
8. **Taint tracking for prompt injection** (§7.5). The only structural defense.
9. **`EconBench`** (§7.10). The instrument.
10. **Remote-brain / local-body hairpin** (§2). Cortex on a cheap always-on box; *its* egress hairpinned back through your local residential link over WireGuard. Solves the 4GB ceiling without giving up residential IPs.
11. **Constitution compiler** (§7.5). Prose → policy. Makes "ring -3" real.
12. **Capability drift detection.** Baseline each app's syscall/network/file profile on first runs; alert when a known app's behaviour changes (telemetry added in an update, or an app that "suddenly" wants `/etc/crontab`). This is a *product feature* nobody else has: "your app updated and started phoning home" — and it's just a diff on the autopsy reports.
13. **Malware zoo autopilot.** Feed real samples (EICAR, public WannaCry/ILOVEYOU specimens, CTF ransomware) into throwaway tabs on a schedule; auto-generate autopsy reports. You get a test suite for your own isolation claims — and, honestly, the best demo material imaginable for the project.
14. **The "0-LLM mode".** Once a run is known-good, replay it deterministically with zero model calls. Turns a 30-turn agent task into a 0-token script — your crystallization endpoint, and it's free to run forever.
15. **Cross-machine teleport with a compatibility gate.** RAM snapshots are CPU-feature-sensitive. Ship a compatibility check (CPUID mask, kernel version, base digest) and fall back to "cold boot from the `.space` delta" instead of failing. Never promise "resumes at the exact millisecond" on a different machine — promise "resumes at the exact *state*", and be right.
16. **Attention/electricity meter per tab.** Cost of ownership in € and % of your day. Cheap to compute from the ledger; it makes temporal multiplexing a *visible* design principle instead of a hack.
17. **Handoff protocol.** A structured `BRIEF.md` that the supervisor generates when switching models mid-task ("here's the goal tree, here's the last verified state, here's what's blocked, here are the 3 things you must not do"). The difference between a model swap and a *successful* model swap.
18. **Shadow twin.** Fork a live `.space` into a mock-environment clone and let it A/B a *business* decision (pricing page, outreach copy) against mock verifiers. You already wanted this for code; business strategy is the better use.
19. **Deception layer (honeytokens).** Canary files with unique content in `/home/user/Documents`; if a tab reads them and then makes an egress attempt, that's a *high-confidence* exfiltration signal — not a heuristic. Nearly free, dramatically improves the sniffer's signal-to-noise.
20. **`.space` freeze = "franchise" artifact.** Once a venture is proven profitable, freezing it into a versioned, self-testing bundle is a genuine product (`spacetime publish my.venture.space`) — even if only you ever use it.

---

## 9. Repos to fork / steal (including the ones you missed)

| Subsystem | Repo | Take | Notes |
|---|---|---|---|
| **The closest thing to your product** | **`spectrum-os`** (Spectrum) | The *entire architecture*: CH + patched virtio-gpu cross-domain Wayland, one VM per app, file portals, Nix-built images | Alyssa Ross has spent years here. **Read everything she wrote about GPU/windowing before writing a line** — it will save you six months. Also the honest evidence that seamless windowing is the hard part. |
| **Closest runnable prototype** | **`AsahiLinux/muvm`** | libkrun + `sommelier` (Wayland/X11 forwarding) + passt/TSI + GPU native context, ~600 lines | Literally "run programs from your system in a microVM". Your Phase-1 body is muvm + tabs + autopsy. |
| **VMM** | **`libkrun`** | The C API, TSI, virtio-fs (with DAX), snapshot/restore (new, HVF-first), WHPX/Windows backend | Your primary. Note the Windows backend today: single vCPU, **no virtio-gpu**. |
| **VMM alt** | **`cloud-hypervisor`** | virtio-mem, balloon, snapshot/restore, `--api-socket`, MSHV backend, and the Spectrum virtio-gpu patches | Your upgrade path when you need device richness. Also the LTS-ish choice for "VMM as a supervised child process". |
| **VMM alt** | **`google/crosvm`** | Best `virtio-gpu` (virgl/venus/cross-domain), WHPX | Fork it if/when seamless windows become the priority. Heavy. |
| **VMM alt** | **`firecracker`** | `jailer`, `cpu_template` (CPUID normalization), snapshot+UFFD lazy load | Steal the security *model* even if you don't use the VMM. |
| **Image building** | **`microvm-nix/microvm.nix`** | Declarative microVM images: 8 hypervisors, squashfs/erofs root, balloon, **and an experimental CH + Wayland-forwarding graphics demo** | Use it to build your base images. It also gives you a working "graphics in a microVM" reference *today*. |
| **macOS GPU** | **`containers/krunkit`** | libkrun + venus on macOS | Your macOS story. |
| **Networking** | **`containers/passt`** | rootless user-mode net, TCP+UDP+ICMP, no TAP, no netns | Compare against TSI; passt has better policy hooks. |
| **Networking** | **`containers/gvisor-tap-vsock`** + `gvproxy` | A complete userspace TCP/IP + DNS + port-forward stack for VMs (podman machine) | Go, but the *design* and its edge cases are a free education. |
| **Dedup/CAS** | **`containers/composefs`** | EROFS + fsverity + overlayfs, content-addressed | The substrate for `.wavefunction`. |
| **Dedup/CAS** | `ostree`, `nix`/`attic`/`cachix` | Layered content-addressed stores, delta transfer | Prior art for your tiered store. |
| **Reflink branch** | `btrfs`/`xfs` `cp --reflink`, `bees` | Instant env branching | Your "Branch Tab" (disk), zero code. |
| **Clipboard/file airlock** | **`freedomofpress/dangerzone`** | The entire "sanitize untrusted document" UX + threat model | Steal the UX language. |
| **GUI forwarding** | `waypipe`, `sommelier` | Damage-tracked Wayland proxying / X11 translation | |
| **Terminal** | `alacritty_terminal` + **`egui_term`** | Already-solved embedded terminal | Day-1 win. |
| **Shell UI** | `egui` + `egui_tiles` (+ `iced` if you prefer retained) | Tabs, split panes, GPU-light | No Electron. |
| **Accessibility tree** | **`odilia` / `atspi`** crate | The AI's semantic view of native apps, ~2MB, 0 GPU | Your Tier-2 perception. |
| **DOM extraction** | `browser-use` `buildDomTree.js`, `nanobrowser` | The selector-to-numbered-box algorithm | Your Tier-1 perception. |
| **Browser control** | `chromiumoxide`, Playwright | CDP, virtual authenticators, cookie transfer | Also your secrets story (§7.6). |
| **GUI grounding** | `UI-TARS`, `OmniParser`, `ocrs` | Only as Tier 3; `ocrs` is pure Rust and tiny | Don't run Florence-2 on 4GB. |
| **Agent memory** | `letta-ai/letta` (memory blocks, eviction), `paperclipai/paperclip` (heartbeat, atomic checkout, goal ancestry) | The two best prior designs for your loop | |
| **Policy** | **`cedar-policy`** | Rust-native, auditable, designed for exactly this | Your ring -3. |
| **Search** | `tantivy` | BM25 over `memory/` — 0 model RAM | Beats embeddings on your hardware. |
| **Vault** | `age`, `keyring`, `secrecy`/`zeroize`, **`opensk`** | Encryption, OS keystore, CTAP2 reference | |
| **Proxy/TLS** | `eycorsican/leaf`, `rama`, `utls` | Only if you build TLS morphing (I'd defer) | |
| **Snapshots** | Firecracker UFFD, CH snapshot, libkrun `krun_restore` | Warm pool + teleport | |
| **Sandbox your own VMM** | `bubblewrap`, `landlock`, `seccompiler` (Firecracker's) | Cheap defence-in-depth: run the VMM inside bwrap with no network and a tiny filesystem | |

**Don't build:** your own VMM, your own compositor, your own TLS stack, your own TCP/IP stack, your own container runtime, your own crypto. (You already said this. Hold the line — it's the #1 way this project dies.)

---

## 10. Revised roadmap — smallest thing that is *useful*, per phase

The previous roadmap was ordered by *layer*, which is how you spend 18 months and have no users. Order by *risk* and *usefulness* instead. Each phase has a kill criterion.

### Phase 0 — "The Instrument" (2–4 weeks, no VMM work)
Agent loop + `EconBench` + **one** plain VM (krunvm or `microvm.nix` example — shell only, no GUI) + API cortex only. Goal: an agent that picks up a verifiable task from a queue, does it, and *proves* it, unattended, overnight.
**Kill criterion:** if after 4 weeks the agent can't complete 3/10 sandboxed tasks with <2 human interventions, the business thesis is wrong for this design — you just saved 2 years.

### Phase 1 — "The Shell" (4–8 weeks)
Tab shell (egui), process-per-tab supervisor, terminal tabs (virtio-console → egui_term), **one** GUI path (framebuffer→texture), cgroup limits, autopsy diff view, tombstone ledger, warm-VM pool. No networking beyond `none` + `direct`.
**Kill:** if a tab open takes >1s or a terminal tab isn't indistinguishable from a native one, fix that before anything else.

### Phase 2 — "The Universe" (4–6 weeks)
`.wavefunction` CAS with provenance tiers, `.space` git repos, reflink branching, snapshot/freeze (opt-in), import/export as a zip, session restore across machines.
**Kill:** if branch-tab isn't <200ms and doesn't reproduce state exactly, the whole "franchise" idea is dead.

### Phase 3 — "The Airlock" (3–5 weeks)
Clipboard/file airlock with quarantine marks, vault (age + keychain), symbolic tokens, session-cookie airlock, CDP virtual authenticator, observation firewall, Cedar policy compiled from the constitution, hash-chained ledger + `ledger verify`.
**Kill:** red-team it yourself. Try to get a secret out of a tab using only the agent's legitimate channels. If you succeed in under an hour, it's not done.

### Phase 4 — "The Egress" (3–6 weeks)
Three egress classes, `connect()` policy, per-tab DNS, honeytokens, egress drawer UI, persona seeds, provenance in the sniff log. Defer TLS morphing indefinitely.
**Kill:** if any guest can reach a host-local port, stop everything.

### Phase 5 — "The Organism" (continuous)
Parliament (critic/scribe), skill crystallization + nightly self-tests, reality oracle verifiers, two-clock scheduling, blocker queue + steering mailbox, shadow twins, `EconBench` expansion. Then: run it for 30 days and watch the metrics. **This is the project.** Phases 0–4 are just the body it lives in.

---

## 11. Decisions I need from you

1. **Host priority:** Linux-only for v1 (I'd say yes), or is Windows/macOS parity a launch requirement? This changes the VMM choice, the display path, and the memory budget more than anything else.
2. **Is a browser in a VM actually required?** If yes, accept ~1GB and one-at-a-time. If no (and web work happens in a *host* browser with a hardened profile), your RAM problems mostly vanish and the isolation story weakens in exactly one place — say so honestly rather than pretending.
3. **Is the AI-heart thesis worth proving before the VM-shell exists?** (My strong recommendation: yes. Phase 0.)
4. **Cortex on a second box?** (§2/§8.10) This is the single biggest usability unlock for 4GB, but it adds a network dependency and a second trust domain.
5. **How much of your identity are you willing to put behind the automation?** (KYC, merchant of record, platform accounts.) This determines which of §7.11 is even on the table.

---

## 12. One paragraph, if you read nothing else

Spacetime is two projects wearing one coat: a **microVM app runtime** (solved-ish; fork `muvm`/`microvm.nix`, add tabs, autopsy, a reflink branch button, and an airlock — a year of polish, no research) and an **unattended economic agent** (unsolved; nobody has it; it's the reason to build the thing). The prior replies optimized the first and romanticized the second. The inversion that matters: **treat the agent as an untrusted, hijackable, forgetful, optimistic employee, and build the *employer* — the ledger, the verifiers, the policy engine, the blocker queue, the memory that decays — not the employee.** That employer is the product. And build the measuring stick (`EconBench`) before the product, or you'll never know if it works.
