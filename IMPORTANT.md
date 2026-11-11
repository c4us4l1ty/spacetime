# Project Brainstorm & Master Architecture: **Spacetime**

**Core Vision:**
An ultra-lightweight, cross-platform Rust desktop application (Windows, macOS, Linux hosts) that acts as a **Workspace Shell (like a web browser with tabs)** for running any Linux application—webapps, GitHub repos, desktop GUI apps, CLI tools, and untrusted malware—inside hardware-isolated MicroVMs. 
* **The Malware Guarantee:** Even if lethal malware (WannaCry/EternalBlue, rootkits, fork bombs, ransomware) executes inside a tab, it hits a hardware virtualization wall (KVM / Hypervisor.framework / WHPX). It cannot exploit the host kernel because it runs its own isolated guest kernel. Closing the tab vaporizes every trace of the execution from RAM.
* **The UX Guarantee:** Switching between isolated VMs feels identical to switching browser tabs. Booting a new tab takes **5ms to 100ms**, with near-zero idle RAM/CPU overhead.

---

## 1. The Hypervisor & MicroVM Engine (Why Containers Are Security Theater)
* **The Problem with Containers:** Docker, Bubblewrap, LXC, and Linux namespaces share the host OS kernel. WannaCry used EternalBlue to attack a kernel driver (`srv.sys`). Container escapes (`DirtyCow`, `DirtyPipe`, `runc` vulnerabilities) compromise the host OS.
* **The MicroVM Solution:** Use hardware-assisted virtualization under the hood without the 30-second boot time or clunky UI of VirtualBox/VMware. Strip out legacy hardware emulation (BIOS, UEFI, PCI buses, IDE controllers).
* **Direct Kernel Boot & Zygote Snapshotting (5ms–50ms Tab Creation):**
  * Cold-booting an uncompressed, stripped Linux kernel (`vmlinux`, ~5–8MB) takes ~50–100ms.
  * **Instant Tab Trick (Zygote VM):** When Spacetime opens, it boots one hidden "Golden MicroVM" in the background, pauses it once `init` is ready, and snapshots its memory. Every time you click `+ New Tab`, Spacetime clones that snapshot using Copy-on-Write (`mmap` with `MAP_PRIVATE`). The tab opens in **~5–15ms**, and 10 running tabs share the exact same physical host RAM pages for the base kernel and OS.
* **Cross-Platform VMM Backend:**
  * Do not write a VMM from scratch (multi-year effort). Embed existing Rust VMM engines via a unified internal trait (`SpacetimeVMM`):
  * **Linux & macOS Hosts:** Embed `libkrun` or `cloud-hypervisor` / `firecracker`.
  * **Windows Hosts:** Embed Google’s `crosvm` (which supports Windows Hypervisor Platform / WHPX) or `cloud-hypervisor`'s MSHV backend.
* **GitHub Repos to Study / Fork / Embed:**
  * `https://github.com/containers/libkrun` (Rust library that embeds KVM/Mac Hypervisor MicroVMs directly into a process with `virtio-fs` and GPU support).
  * `https://github.com/firecracker-microvm/firecracker` (AWS’s Rust VMM: ~5ms boot, <5MB memory overhead, minimal attack surface).
  * `https://github.com/cloud-hypervisor/cloud-hypervisor` (Rust VMM built on `rust-vmm`, great device support).
  * `https://github.com/google/crosvm` (ChromeOS’s Rust VMM: best-in-class `virtio-gpu`, Wayland forwarding, and Windows WHPX support).
  * `https://github.com/rust-vmm` (Collection of modular Rust crates for building custom VMM pipelines).
  * `https://github.com/QubesOS/qubes-core-admin` & `https://github.com/kata-containers/kata-containers` (The holy grails of VM isolation architecture to study).

---

## 2. `.wavefunction` Storage, Global Deduplication & Git-Style Branching
* **Taking Inspiration from `c4us4l1ty/holonomy`:**
  * Formalize `.wavefunction` as Spacetime’s container/environment manifest. Before launch, an environment is an immutable `.wavefunction` definition; once launched in a tab, it "collapses" into a live MicroVM with an ephemeral RAM upper layer.
* **Zero-Trace Ephemeral Execution:**
  * **Root Filesystem:** An immutable base `squashfs` or `EROFS` image paired with an ephemeral in-memory `tmpfs` (or host-backed sparse CoW file) via `OverlayFS`.
  * When a tab is closed or deleted, the `tmpfs` layer is unmounted and dropped. Dependencies, temp files, and malware artifacts vanish as if they never existed.
* **The Global Trusted Store (Solving Dependency Bloat):**
  * **Problem:** Tab 1 and Tab 2 both download the same 5GB PyTorch/Node/Rust dependencies, bloating the hard drive.
  * **Solution (Content-Addressable Global Store + DAX):** Spacetime maintains a host-side Content-Addressable Store (`/spacetime/store/blake3/...`, similar to Nix or OSTree).
  * Trusted base packages and explicitly "promoted" dependencies are hashed and stored once on the host, then mounted **Read-Only** into every MicroVM via `virtio-fs` (`virtiofsd`).
  * By enabling **DAX (Direct Access)** on `virtio-fs`, multiple MicroVMs map the host's RAM page cache directly into guest space—meaning 5 tabs using the same library share **one copy** on disk and **one copy** in RAM.
* **Git-Style Tab Branching ("Branch Tab" Button):**
  * Scenario: You clone a repo, install 5GB of dependencies, and reach a stable point. Now you want to test two sketchy scripts simultaneously without redownloading anything.
  * Clicking **"Branch Tab"** pauses the MicroVM (`<5ms`), freezes its current writable `OverlayFS` layer into a read-only lower layer, snapshots the VM's RAM via Copy-on-Write, and spawns two new tabs with fresh RAM upper layers starting from that exact second.
* **Time-Travel Scrubber (Bonus Feature):**
  * Periodically take lightweight CoW dirty-page memory snapshots every few seconds into a ring buffer. Add a timeline slider at the bottom of the tab: if a sketchy script bricks the app or encrypts files inside the VM, drag the slider back 15 seconds to rewind the entire OS and RAM state.
* **GitHub Repos to Study / Fork:**
  * `https://github.com/c4us4l1ty/holonomy` (Your `.wavefunction` container paradigm).
  * `https://gitlab.com/virtio-fs/virtiofsd` (Official Rust daemon for sharing host directories with MicroVMs at near-native speed).
  * `https://github.com/containers/composefs` (Read-only content-addressable filesystem using EROFS + OverlayFS for instant deduplicated sharing).
  * `https://github.com/nix-community/nix-ld` & `https://github.com/ostreedev/ostree` (Inspiration for content-addressed dependency deduplication).

---

## 3. Workspace Shell: Multiplexing GUIs & Terminals Into Tabs
* **No Heavy Guest Desktops:**
  * Never run a full GNOME/XFCE desktop or slow VNC server inside the MicroVM. The guest runs headless and forwards *only* the individual application window or terminal buffer into the Spacetime Rust tab.
* **Terminal Applications (Zero-SSH Instant Streams):**
  * Do not use SSH (handshake and encryption overhead is too slow).
  * Attach a `virtio-console` (`/dev/hvc0` for data, `/dev/hvc1` for PTY window-resize signals) directly to the guest.
  * The Rust host reads the raw byte stream and renders it at 144 FPS using an embedded GPU-accelerated terminal widget (`alacritty_terminal`).
* **Graphical & Web Applications (Per-Tab Wayland Multiplexing):**
  * Avoid X11 (insecure, allows keylogging within the display server).
  * Spacetime embeds a tiny, pure-Rust Wayland compositor (`smithay`) inside each GUI tab on the host.
  * Inside the MicroVM, a lightweight proxy (`waypipe` or `sommelier`) forwards the guest app’s Wayland surface and GPU buffers (`virtio-gpu` / `virtio-wl` / `virtio-vsock`) directly into the Rust tab texture.
* **Zero-Trust Clipboard & File Teleportation (Host $\leftrightarrow$ Spacetime):**
  * **Clipboard:** Never auto-sync the clipboard (prevents clipboard-hijacking malware and parser exploits). Pressing a host-captured shortcut sanitizes the clipboard into strict plain UTF-8 text or raw stripped RGBA bitmaps before sending it across `virtio-vsock`.
  * **File Copy/Paste:** Dragging files from Host $\rightarrow$ Tab streams them over `virtio-vsock` into the VM's ephemeral `/Downloads`. Dragging files from Tab $\rightarrow$ Host runs them through an airgapped **50ms Sanitizer MicroVM** (Qubes style) that strips metadata/macros and rebuilds clean PDFs/images before touching your host disk.
* **GitHub Repos to Study / Fork:**
  * `https://github.com/Smithay/smithay` (Pure-Rust Wayland compositor toolkit—use this to render guest Linux GUI apps inside your Rust app’s tabs).
  * `https://gitlab.freedesktop.org/mstoeckl/waypipe` (Transparent proxy for forwarding Wayland GUI apps over a single socket/vsock).
  * `https://github.com/alacritty/alacritty/tree/master/alacritty_terminal` (Embeddable Rust terminal emulator state machine).
  * `https://github.com/iced-rs/iced` or `https://github.com/emilk/egui` or `https://github.com/Lapce/floem` (Pure-Rust GPU-rendered GUI frameworks for building the Spacetime Workspace Shell).
  * `https://github.com/QubesOS/qubes-gui-daemon` (Study for secure window & clipboard sanitization).

---

## 4. Real-Time Resource Choking (Optimized for Low-Core Host Machines)
* **The Problem:** Untrusted repos hide crypto miners, infinite loops, and fork bombs. Hard-limiting 2 CPU cores per tab via static allocation will starve a laptop that only has 4 or 8 cores total.
* **Focus-Aware Dynamic Throttling (`cgroups v2` / Host Thread QoS):**
  * Every MicroVM runs as isolated threads inside a strict `cgroup v2` (Linux), `Job Object` (Windows), or `task_policy` (macOS), managed by the Rust host.
  * **Active Tab:** Gets high priority (e.g., up to 75% CPU, 2GB–4GB RAM cap, `pids.max` limit to instantly crush fork bombs).
  * **Unfocused Background Tabs:** The millisecond you switch away from a tab, Spacetime automatically chokes its `cgroup` down to **5% of 1 CPU core** (unless you explicitly toggle "Allow Background Compute" on that tab). A crypto miner in a background tab is immediately suffocated.
* **Real-Time RAM Reclaim (`virtio-balloon`):**
  * Enable `virtio-balloon` with Free Page Reporting so the moment an app inside a tab frees memory, the guest kernel gives those physical RAM pages straight back to your Host OS.

---

## 5. Granular Network Sandboxing, "Honey-Duct" Sniffer & Anti-Fraud Evasion
* **User-Space Software Switch (Zero Host Kernel Exposure):**
  * Instead of relying only on host-level `iptables` (which requires root and isn't cross-platform), wire each MicroVM’s `virtio-net` interface into an **in-memory Rust User-Space TCP/IP stack (`smoltcp`)** or isolated `netns` + eBPF (`aya`).
* **3-Tier Per-Tab Network Modes:**
  1. **Airgapped (Hardware-Enforced Offline):** Omit the `virtio-net` device entirely when spawning the MicroVM. The guest kernel has no network card—packet egress is physically impossible.
  2. **Honey-Duct / Quarantined Mode:** Every DNS request and TCP/UDP packet hits the Rust virtual switch. A **bottom UI drawer** in the tab shows live telemetry: *"This tab just tried to connect to IP 185.x.x.x (Russia) on port 4444. Blocked."* Includes a **Sinkhole Toggle** that feeds fake `200 OK` responses to malware so it doesn't realize it's trapped in a sandbox.
  3. **Stealth Anonymized Egress (Zero Home IP Footprint, Zero Proxy Blocks):**
* **Bypassing Cloudflare / Akamai / Datadome Proxy Detection:**
  * **Why Tor & Datacenter VPNs Fail:** Target apps block known Tor exit nodes (public consensus list), flag Datacenter ASNs (AWS, Hetzner, Mullvad), and detect **OS/TLS Mismatches** (`p0f` TCP packet fingerprinting + `JA3`/`JA4` TLS fingerprinting).
  * **Layer 1: Legitimate Residential/Mobile Egress:** Route the tab’s virtual switch through:
    * **Strategy A:** A physical USB 4G/5G Mobile Dongle passed through to the router (Carrier CGNAT IPs are never blocked).
    * **Strategy B:** Decentralized Residential WireGuard/SOCKS5 nodes (DPN) or Rotating Residential Proxy pools (BrightData, Oxylabs, etc.).
  * **Layer 2: Packet Morphing (`p0f` & `JA4` Spoofing):** Before packets leave the Rust switch, Spacetime rewrites the TCP stack headers (changing Linux `TTL=64` to Windows `TTL=128`, adjusting TCP Window Size and TCP Option order) and routes HTTPS handshakes through a TLS-spoofing engine (`uTLS` / `rustls`) so even CLI tools look identical to a standard residential Chrome browser on Windows.
* **GitHub Repos to Study / Fork:**
  * `https://github.com/smoltcp-rs/smoltcp` (Pure-Rust user-space TCP/IP stack for intercepting and rewriting every packet from the MicroVM in memory).
  * `https://github.com/aya-rs/aya` (Pure-Rust eBPF library for kernel-level network filtering and syscall tracing).
  * `https://github.com/Shadowproxy/shadowproxy` or `https://github.com/eycorsican/leaf` (Fast Rust proxy routing frameworks for SOCKS5/Shadowsocks/Trojan chaining).
  * `https://github.com/refraction-networking/utls` or `https://github.com/bogdanfinn/tls-client` (JA3/JA4 TLS fingerprint spoofing).
  * `https://github.com/SpringQL/p0f-rs` or `https://github.com/WiFiPineapple/p0f` (Study TCP/IP OS fingerprint signatures to spoof them).

---

## 6. Autonomous AI Execution: Solving the "Heartbeat", Permission Fatigue & Context Rot

Inspired by **`paperclipai/paperclip`**, Spacetime’s AI is not a chat assistant—it is an **Autonomous Out-of-Band Operator** that lives in the Rust host process, sees the tab’s screen buffer, and drives the mouse/keyboard via hardware-level `virtio-input` (`evdev`) without ever touching the guest OS internals.

### Problem A: "Allow Permission" Fatigue
* **Why other AI agents ask for permission:** Tools like Claude Code or OpenClaw run on your real OS; if they hallucinate `rm -rf` or run malware, you lose your data.
* **Spacetime’s Solution (Zero-Permission YOLO Mode + Auto-Rollback):**
  * The AI **never** pauses to ask you for permission.
  * Before executing a high-risk command or clicking an unknown button, the Rust host automatically takes a **5ms CoW Memory Snapshot** of the tab.
  * If the command crashes the app, corrupts the repo, or triggers a kernel panic, the Rust host detects the failure, **auto-rewinds the VM to the snapshot**, and injects a system message to the AI: *"Your last action broke the environment. State has been auto-rewound by 5 seconds. Try an alternative method."*

### Problem B: Context Window Bloat & Hallucinations Over Long Tasks
* **Why AI hallucinates on big goals:** If you tell an AI to "Edit a video on OpenCut" or "Build a full project, create a GitHub account, and launch a social campaign," after 40 screenshots and 100 actions, the context window fills up with stale tokens, causing attention degradation ("context rot").
* **Spacetime’s 3-Part Architecture for Infinite-Horizon Execution:**
  1. **The Paperclip "Heartbeat" Loop + Externalized State Graph:**
     * Instead of one endless chat session, the Rust host runs a continuous **Heartbeat Engine** (`Wake -> Pull Atomic Task from DAG -> Act -> Verify -> Write State to Disk -> Wipe Context`).
     * Progress is stored outside the LLM in a structured Rust-managed **Goal DAG (`task_graph.json`)** and an episodic **Scratchpad (`state.md`)**.
     * Every ~10 actions (or whenever a sub-goal completes), Spacetime **flushes the LLM context window completely** and starts a fresh 0-token session hydrated *only* with: `[Master Goal] + [Current DAG Node] + [Summary of Completed Nodes] + [Current Tab Screenshot]`. The AI stays razor-sharp for hours or days.
  2. **Sub-Agent Tab Spawning (Process Isolation for AI Brains):**
     * If the Master AI needs to do a complex side-quest (e.g., *"Go to GitHub, register an account, and grab an API token"* or *"Read the documentation for this sketchy tool"*), it spawns a **temporary Child Tab** with a dedicated **Sub-Agent**.
     * The Sub-Agent uses 50 screenshots to finish the task, returns a 1-line JSON result (`{"github_repo": "https://..."}`) to the Master AI, and both the Child Tab and its entire context history are **garbage-collected**.
  3. **Set-of-Mark (SoM) Local Vision Compression:**
     * Instead of sending raw 4K screenshots or massive HTML DOMs that eat 10,000 tokens per turn, a tiny local ONNX model (`OmniParser` / `YOLOv8` via the `ort` crate) labels interactive UI elements on the host GPU (`[1]`, `[2]`, `[3]`) and sends a compressed coordinate map alongside a downscaled image.

### Problem C: Live Non-Blocking Human Steering
* **Autonomous by Default, Steerable on Demand:**
  * Once given a goal, the Heartbeat loop runs continuously without waiting for human prompts.
  * However, each tab has a **"Live Steer" input bar**. If you see the AI doing something suboptimal and type *"Use dark mode instead"* or *"Skip writing tests, just push the code"*, your message is pushed into a **High-Priority Interrupt Channel** in the Rust orchestrator.
  * On the very next Heartbeat tick (<1 second), the Rust host updates the Goal DAG with your steering constraint and seamlessly adjusts the AI’s trajectory without stopping execution.

### Problem D: Using `arena.ai` / Web Agents for Free Instead of Paid APIs
* **The Hidden "Brain Tab" Loopback Bridge:**
  * Spacetime supports standard APIs (OpenRouter, Nvidia NIM, Anthropic, Local Ollama), **AND** a zero-cost **Web-to-API Bridge Mode**.
  * Spacetime spins up a hidden, stealth-routed browser tab logged into `arena.ai`, ChatGPT, DeepSeek, or Claude web UIs.
  * The Rust host exposes a local OpenAI-compatible endpoint (`127.0.0.1:11434`). When the Heartbeat engine needs the next action, it injects the compressed prompt/screenshot into the `arena.ai` DOM inside the hidden tab, scrapes the streamed response via Chrome DevTools Protocol (CDP), parses the JSON tool call, and injects the hardware mouse/keyboard actions into your Target Tab!
* **GitHub Repos to Study / Fork for the AI Engine:**
  * `https://github.com/paperclipai/paperclip` (Study their **Heartbeat execution model**, atomic task checkout, goal ancestry, and context-reset orchestration).
  * `https://github.com/microsoft/OmniParser` (Vision model for turning any GUI screenshot into structured, clickable numbered bounding boxes).
  * `https://github.com/pykeio/ort` (Rust bindings for ONNX Runtime—run OmniParser/YOLO locally on the host to save tokens).
  * `https://github.com/browser-use/browser-use` & `https://github.com/nanobrowser/nanobrowser` (Study their DOM extraction and multi-agent planner/validator prompts).
  * `https://github.com/bytedance/UI-TARS` & `https://github.com/anthropics/anthropic-quickstarts/tree/main/computer-use-demo` (Study GUI coordinate grounding and OS interaction loops).
  * `https://github.com/letta-ai/letta` (Formerly MemGPT—study how they page memory in and out of a fixed context window to prevent hallucinations).

---

## 7. Proposed Rust Workspace Structure (`crates/...`)

To keep Spacetime modular, fast to compile, and maintainable, structure the repository as a Cargo Workspace:

```text
spacetime/
├── Cargo.toml                        # Workspace root definition
├── crates/
│   ├── spacetime-shell/              # Main Desktop GUI (iced / egui / wgpu)
│   │   ├── tab_manager/              # Browser-like tab lifecycle, Branch button, Rewind slider
│   │   ├── wayland_view/             # Embedded Smithay compositor rendering guest GUI textures
│   │   ├── terminal_view/            # Embedded alacritty_terminal bound to virtio-console
│   │   └── honey_drawer/             # Bottom UI drawer showing blocked IPs, ASN info & sinkholes
│   │
│   ├── spacetime-vmm/                # Hardware Virtualization Abstraction Layer
│   │   ├── backend_krun/             # Linux (KVM) & macOS (Hypervisor.framework) via libkrun
│   │   ├── backend_crosvm/           # Windows (WHPX) & virtio-gpu wrapper
│   │   ├── zygote_snapshot/          # 5ms CoW memory cloning & time-travel dirty-page ring buffer
│   │   └── anti_vm_spoof/            # Spoofs CPUID, SMBIOS, and MAC addresses to hide VM from malware
│   │
│   ├── spacetime-wavefunction/       # Storage, Deduplication & Environment Branching
│   │   ├── cas_store/                # BLAKE3 content-addressable global dependency store
│   │   ├── virtiofs_daemon/          # Embedded virtiofsd + DAX shared memory page-cache manager
│   │   ├── overlay_branch/           # Git-style CoW layer freezing and tab forking
│   │   └── file_sanitizer/           # Qubes-style disposable MicroVM pipeline for safe Host <-> Tab copy
│   │
│   ├── spacetime-net/                # User-Space Network Switch & Anti-Detect Router
│   │   ├── vswitch/                  # smoltcp user-space TCP/IP stack attached to virtio-net
│   │   ├── honey_duct/               # Packet sniffer, GeoIP/ASN lookup, and C2 sinkhole responder
│   │   ├── chameleon_spoof/          # Rewrites TCP TTL/Window (p0f) and TLS ClientHello (JA3/JA4)
│   │   └── egress_tunnels/           # Residential SOCKS5 pool, WireGuard DPN, and USB 4G/5G modem routing
│   │
│   ├── spacetime-warden/             # Resource Choking & Hardware Telemetry
│   │   ├── cgroup_choke/             # Focus-aware CPU/RAM/PID throttling (5% cap on unfocused tabs)
│   │   └── balloon_reclaim/          # Real-time virtio-balloon free page reporting back to host
│   │
│   └── spacetime-agent/              # Out-of-Band Autonomous AI Operator
│       ├── heartbeat_engine/         # Paperclip-style tick loop, auto-snapshot before risky actions
│       ├── context_compactor/        # External Goal DAG, Scratchpad, and periodic 0-token context reset
│       ├── steer_channel/            # Non-blocking async interrupt queue for live human steering prompts
│       ├── som_vision/               # Zero-copy framebuffer capture + local ONNX OmniParser box tagging
│       ├── virtio_hid/               # Hardware-level evdev mouse (Bezier human curves) & keyboard injection
│       └── arena_bridge/             # Hidden browser tab CDP scraper turning arena.ai / web UIs into an API
│
└── guest-init/                       # Tiny static Rust binary compiled for Linux guest (/sbin/init)
    ├── vsock_agent/                  # Listens for instant commands, PTY resize, and sanitized clipboard
    └── waypipe_launcher/             # Auto-forwards any launched GUI window to the host tab
```



### Part 1: Web Models (`arena.ai/agent`) as the Primary AI Engine

Making web-based AI platforms (`arena.ai/agent`, ChatGPT Web, Claude Web, DeepSeek Web, Google AI Studio) the **primary** brain—and treating paid APIs as a secondary fallback—actually gives Spacetime a massive unfair advantage: **$0 inference cost and access to frontier models/agents**.

However, web UIs are designed for humans chatting, not autonomous loops. To solve **Context Rot, Hallucinations, Rate Limits, and Permission Prompts** using purely web models, Spacetime uses a **"Phantom Brain Pool" Architecture**:

```text
+-----------------------------------------------------------------------------------+
| SPACETIME RUST HOST (The "Heartbeat" Orchestrator)                                |
|                                                                                   |
|  [ Goal DAG & state.md ]  <--- (Persisted in Host RAM, NEVER lost on New Chat)    |
|            |                                                                      |
|            +------------------------+------------------------+                    |
|            | (CDP Network Hook)     | (virtio-input + Vision)|                    |
|            v                        v                        v                    |
|  +--------------------+   +--------------------+   +---------------------------+  |
|  | PHANTOM BRAIN TAB 1|   | PHANTOM BRAIN TAB 2|   | TARGET EXECUTION TAB      |  |
|  | (arena.ai/agent)   |   | (deepseek / claude)|   | (OpenCut / Sketchy App)   |  |
|  | Routed: Res IP #1  |   | Routed: Res IP #2  |   | Routed: Airgap / Res IP #3|  |
|  +--------------------+   +--------------------+   +---------------------------+  |
+-----------------------------------------------------------------------------------+
```

#### 1. How Spacetime Controls Web Models Without Scraping Messy HTML
* **Phantom Brain Tabs:** Spacetime maintains a background pool of 1 to 3 stealth MicroVM browser tabs logged into `arena.ai/agent` (and backup web models), each routed through a separate Residential IP.
* **CDP Network Stream Interception (Zero DOM Scraping Flakiness):** Instead of trying to scrape HTML `<div>` tags to read what `arena.ai` wrote, Spacetime attaches to the browser via **Chrome DevTools Protocol (CDP)** (`Network.webSocketFrameReceived` and `Fetch` SSE streams). The Rust host intercepts the raw JSON token stream directly from the network buffer the millisecond `arena.ai` generates it.
* **Hardware-Level Prompt Injection:** To send a prompt or upload a screenshot to `arena.ai`, Spacetime doesn't use detectable JavaScript `element.click()`. It uses the MicroVM's `virtio-input` (virtual hardware mouse/keyboard) + sanitized clipboard paste to drop the screenshot and prompt into the `arena.ai` input box like a real human.

#### 2. Two Modes of Web-AI Execution
* **Mode A: Direct Web-Agent Delegation (`arena.ai/agent` as the Builder):**
  * When you tell Spacetime: *"Build a full-stack crypto tracker and push it to a new GitHub repo"*, Spacetime doesn't need to write every line of code itself.
  * It opens `arena.ai/agent` (or Bolt/v0/ Lovable/ Replit Agent) inside a Brain Tab, types your specification, lets the remote web agent spin up its own cloud sandbox to write and debug the code, and once finished, Spacetime downloads the generated repo zip directly into your local Execution Tab to compile, run, or deploy!
* **Mode B: Web-Chat as Local OS Operator (Vision-Action Loop):**
  * When you tell Spacetime: *"Use OpenCut to edit the clips in `/storage/X`"*, or *"Run this sketchy Russian `.elf` binary and extract the data"*:
  * On every **Heartbeat tick**, the Rust host captures the Target Tab’s screen, stamps numbered boxes (`[1]`, `[2]`, `[3]`) over every button using local `OmniParser`, and sends that image to the `arena.ai` tab with a strict system wrapper:
    > *"Current Goal Node: Import video clips. Look at the numbered screenshot. Reply ONLY with a JSON block containing your `next_action` (`click #14`, `type`, `drag`) and `scratchpad_note`."*

#### 3. Solving Context Window Bloat on Web UIs ("The Auto-New-Chat Wipe")
* On `arena.ai` or web models, after ~15–20 back-and-forth screenshots, the web page gets laggy and the model starts hallucinating.
* **The Fix:** The Rust Heartbeat Engine tracks turn count. Every **10 Heartbeat ticks** (or as soon as a sub-task finishes):
  1. The Rust host forces the AI to summarize its current state into the host-side `state.md` file and updates the **Goal DAG** (`[x] Step 1: Opened OpenCut`, `[x] Step 2: Imported clips`, `[ ] Step 3: Cut dead silence`).
  2. The Rust host automatically clicks the **"New Chat" / "Reset"** button inside `arena.ai`!
  3. It injects the tiny `state.md` summary + the current screenshot as **Message #1** of the fresh chat.
  4. **Result:** The web model *never* exceeds 15% of its context window. It can run autonomously for 12 hours across 500 "New Chats" without ever losing its train of thought or hallucinating!

#### 4. Solving Rate-Limits & CAPTCHAs Automatically (Multi-Brain Failover)
* Because `state.md` and the Goal DAG live in your Rust host (not inside `arena.ai`), the AI's "memory" is decoupled from any single web session.
* If `arena.ai` throws a "Rate limit reached" banner or a Cloudflare Turnstile check:
  * First, because the tab uses **Hardware `virtio-input` Bezier mouse curves + Residential IP + `p0f`/`JA4` spoofing**, clicking the Cloudflare Turnstile box via `virtio-input` passes 99% of the time automatically.
  * Second, if a session is hard rate-limited, the Rust router instantly hands `state.md` over to **Phantom Brain Tab #2** (running a fresh `arena.ai` session on a different residential IP, or DeepSeek/Claude Web) on the very next Heartbeat tick—zero downtime, zero human intervention.

---

### Part 2: How the Spacetime UI Should Look and Function

Spacetime’s UI should not look like a clunky VM manager (VirtualBox/Proxmox). It should look like a high-contrast, dark-mode blend of **Arc Browser (vertical tree tabs) + Linear (minimalist telemetry) + Qubes OS (color-coded security borders) + DaVinci Resolve (bottom time-travel scrubber).**

#### Full Workspace Wireframe

```text
+----------------------------------------------------------------------------------------------------------+
| 🔴 🟡 🟢  [🧠 Brain Pool: 3 Active]   [🔍 @ai edit clips in /storage/X on opencut.app      ] [🌿 Branch] |
+----------------------+-----------------------------------------------------------------------------------+
| WORKSPACE TREE       | 🟣 TAB: opencut.app [MicroVM #04 | Boot: 8ms | RAM: 410MB | IP: 73.14.x.x (Res)] |
|                      +-----------------------------------------------------------------------------------+
| 📁 Video Project     |  +-----------------------------------------------------------------------------+  |
| ├─🟣 opencut.app  <--|--|  OpenCut Web App / Native Linux GUI / Terminal Rendered at 144 FPS          |  |
| │  ├─🟣 branch-fx-A  |  |                                                                             |  |
| │  └─🟣 branch-fx-B  |  |     [Clip_01.mp4]    [Clip_02.mp4]                                          |  |
| └─🔵 ffmpeg-cli      |  |                                                                             |  |
|                      |  |                              ↖ 🤖 [AI Ghost Cursor: Clicking 'Split' #18]   |  |
| 📁 Malware Lab       |  |                                                                             |  |
| └─🔴 wannacry.exe    |  |  +-----------------------------------------------------------------------+  |  |
|    (Airgapped/RAM)   |  |  | 🟢 Heartbeat #12 | Cutting silence... | 💬 Steer AI (Type & Enter)... |  |  |
|                      |  |  +-----------------------------------------------------------------------+  |  |
| 📁 Global Store      |  +-----------------------------------------------------------------------------+  |
| └─📦 42 GB Shared    |                                                                                   |
|    (Saved 180 GB!)   +-----------------------------------------------------------------------------------+
|                      | [⏪ Time-Travel Scrubber]  [🕵️ Honey-Duct (2 Blocked)]  [🧠 AI DAG & Brain PiP]   |
| +------------------+-+-----------------------------------------------------------------------------------+
| | 🧠 arena.ai [PiP]| | ⏪ -60s ======O========================[🔥-12s: File Deleted]============= [LIVE] |
| | Streaming JSON...| | 🕵️ [14:04:02] ⚠️ Tab 'wannacry' -> 185.220.x.x:4444 (Russia) [TRAPPED IN SINKHOLE] |
+--------------------+-------------------------------------------------------------------------------------+
```

---

#### 1. The Left Sidebar: "Git-Tree Tabs" & Qubes Security Glows
Standard horizontal browser tabs fail when you start **Branching Tabs**. Spacetime uses a collapsible **Vertical Tree Sidebar** (like Arc Browser meets Git):
* **Visual Branching Tree:** When you are in `Tab 1` (after installing 5GB of dependencies) and click **`[🌿 Branch]`**, two indented child tabs (`├── branch-A` and `└── branch-B`) sprout directly underneath `Tab 1` in 10 milliseconds. You can see the exact lineage of every environment.
* **Color-Coded Trust Borders (The Qubes Inspiration):** Every tab has a 2px neon border around its viewport and an icon in the sidebar that tells you its hardware isolation level at a glance:
  * 🔵 **Cyan/Blue (Airgapped Vault):** `virtio-net` is physically detached. Zero network capability.
  * 🟣 **Purple (Chameleon Stealth):** Routed through Residential Proxy / 4G Modem + `p0f`/`JA4` packet spoofing.
  * 🟠 **Orange (Honey-Duct Quarantine):** Network is connected to a local fake sinkhole to trick malware/telemetry.
  * 🔴 **Red (Disposable Hazard):** Ephemeral RAM-only mode. The moment you close the tab, a shred animation plays and the RAM pages are zeroed out.
* **Background Choke Indicator:** Unfocused tabs in the sidebar show a tiny `❄️ 5%` badge next to them, visually confirming that `cgroups v2` has frozen/throttled their CPU so your laptop stays cool and silent.

#### 2. The Top "Event Horizon" Omni-Bar
Pressing `Ctrl+T` or `Cmd+K` opens a single unified command bar at the top. It detects what you want to do automatically:
* **Paste a URL (`https://opencut.app`):** Spawns a stealth browser MicroVM tab.
* **Paste a GitHub Repo (`gh:user/sketchy-repo`):** Spawns a MicroVM, mounts the Global Dependency Store via `virtio-fs`, clones the repo, and opens a split view (Terminal + Code/GUI).
* **Drop a File (`./unknown_malware.bin`):** Spawns an airgapped 🔴 Red MicroVM and runs the binary inside a sandboxed GUI/TUI.
* **Type an `@ai` Goal (`@ai create a new crypto token, build its site, and edit a promo video`):** Initializes the **Heartbeat Orchestrator**, builds the Goal DAG, and starts spawning child tabs autonomously to execute the mission.

#### 3. The Central Viewport & The "AI Ghost Cursor"
* **Seamless GUI + Terminal Multiplexing:** Whether the tab is running a Linux terminal (`alacritty_terminal`), a desktop app (`GIMP` / `VS Code` via `Smithay` Wayland), or a browser, it renders natively inside the central canvas at 60–144 FPS.
* **The "AI Ghost Cursor" (Crucial UX Detail):**
  * In normal "Computer Use" tools, the AI hijacks your real mouse cursor, meaning you can't use your computer while it works!
  * In Spacetime, because each tab has its own isolated `virtio-input` device, **the AI has its own dedicated "Ghost Cursor" inside each tab!**
  * You can have 3 different tabs running 3 different AI agents simultaneously in the background, each moving its own virtual cursor inside its own VM, while **your physical mouse cursor remains 100% free** to switch tabs, browse, or play a game!
  * When you look at an AI-controlled tab, you see a glowing neon cursor moving along smooth Bezier curves with a small floating pill attached to it: `🤖 Heartbeat #9: Clicking 'Export MP4'`.

#### 4. The Floating "Live Steer" Pill (Bottom Center of Canvas)
Hovering at the bottom of any AI-active tab is a sleek, translucent status bar:
* **Left side:** Shows a pulsing green Heartbeat dot, the current micro-step (`Importing clips from /storage/X...`), and a **Context Freshness Gauge** (`Context: 12% [Reset 3m ago]`).
* **Center Input Box (`💬 Steer AI without pausing...`):** You never have to click "Pause", "Stop", or "Allow". While the AI is actively clicking buttons in OpenCut, you can type *"Actually, make the video 9:16 vertical format"* and press `Enter`.
* A subtle wave animation flashes across the bar (`⚡ Directive queued for next Heartbeat`), and 1 second later the AI adjusts its Goal DAG and changes the aspect ratio in OpenCut without missing a beat.

#### 5. The Bottom Collapsible Dock (3 Power-User Drawers)
Clicking the bottom bar expands a sleek dock with three specialized views:

* **Drawer 1: ⏪ The Time-Travel Scrubber & Autopsy Diff**
  * Looks like a video timeline at the bottom of the tab, spanning the last 5 minutes (backed by CoW dirty-page memory snapshots).
  * **Event Markers:** The timeline automatically places colored pins whenever the guest VM does something notable: `📄 File Created`, `⚙️ Registry/Cron Modified`, `🌐 Outbound Connection`, `🤖 AI Risky Action`.
  * **Instant Rewind:** If a sketchy program crashes or encrypts the virtual disk, you literally grab the timeline slider with your mouse and drag it left by 20 seconds. The tab viewport rewinds like a YouTube video—restoring the exact RAM, CPU registers, and GUI window state from 20 seconds ago!
  * **"Branch from Past" Button:** Rewind 30 seconds, click `🌿 Branch Here`, and fork a new tab starting from that historical moment.

* **Drawer 2: 🕵️ The Honey-Duct Network Console**
  * Shows a real-time scrolling feed of every packet leaving the tab's `virtio-net` interface (captured by `smoltcp`):
    * `🟢 ALLOWED: 104.18.x.x:443 (opencut.app) via Comcast Residential Proxy [JA4: Chrome_Win11 | TTL: 128]`
    * `🛑 BLOCKED: 185.x.x.x:4444 (ASN: AS49505 Russian Hosting) -> Process: /tmp/.hidden_miner`
  * **One-Click Deception:** Next to any blocked connection is a `[🕳️ Route to Honey-Duct]` toggle. Clicking it opens a fake local socket that feeds dummy responses back to the malware so you can watch what commands the attacker tries to send!

* **Drawer 3: 🧠 The Web-Brain PiP & Goal DAG Visualizer**
  * **Left half (Visual Goal Tree):** Displays the `paperclip`-style Goal Ancestry DAG. You can see the Master Goal at the top, broken down into sub-tasks: green checkmarks for completed steps, a glowing animated border around the active step, and gray nodes for upcoming steps. You can manually drag, delete, or edit future nodes in the graph while the AI is working!
  * **Right half (Phantom Brain Picture-in-Picture):** Shows live miniature thumbnails of the hidden `arena.ai/agent` tabs. You can literally watch the prompt & annotated screenshot get pasted into `arena.ai`, watch the text stream back, and watch Spacetime click "New Chat" every 10 turns to wipe the context clean. If you ever want to manually inspect or tweak the `arena.ai` session, click the thumbnail to expand it!

#### 6. Safe File Teleportation UX (Drag-and-Drop Airlock)
* **Host $\rightarrow$ Spacetime:** Dragging a folder of video clips from your Windows/Mac/Linux desktop onto a Spacetime tab shows a glowing blue dropzone: **`[📥 Mount to /storage/clips (Read-Only or CoW Copy)]`**.
* **Spacetime $\rightarrow$ Host (The Sanitizer Airlock):** When the AI finishes editing your video or generating code and you drag `final_video.mp4` out of the Spacetime tab onto your real Host Desktop, a 100ms **"Sanitizing via Airlock VM..."** badge pops up—stripping any potential exploit metadata in a disposable MicroVM before the clean file lands on your real hard drive.
