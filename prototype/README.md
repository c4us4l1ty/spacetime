# Spacetime — UI prototype

A static, dependency-free HTML prototype of the **Spacetime workspace shell**: a
browser-like window with vertical tabs on the left and the running app on the
right, built around the isolation and telemetry model in `BRAINSTORM.md`,
`DESIGN-REVIEW.md` and `DESIGN-REVIEW-2.md`.

Open `prototype/index.html` directly in a browser, or serve the folder:

```bash
python3 -m http.server 8000 --directory prototype
# → http://localhost:8000
```

No build step, no frameworks, no network requests. Everything is HTML, CSS and
vanilla JS; icons are inline SVG symbols.

## The shape

```
┌─────────────────────────────────────────────────────────────────────┐
│ O  YT_Channel_01              │  ← → ↻ ⌂   omnibox        ✦ ⛁ ⋯     │
│ ┌─ pinned ─────────────────┐  │  read 14 · wrote 2 · net 3 · risk low│
│ │ ▣ ▣ ▣ ▣ ▣ +              │  ├─────────────────────────────────────┤
│ │                          │  │                                     │
│ │ Tabs           7         │  │        the app / world-line         │
│ │ ● OpenCut — teaser       │  │        (microVM viewport)           │
│ │ ● yt-01 · cargo build    │  │                                     │
│ │ ● Field Notes — Feather  │  │                                     │
│ │ ◐ Bounty_Hunter (cryo)   │  │                                     │
│ │ ▦ New tab                │  │                                     │
│ │ ✦ Spacetime Console      │  │                                     │
│ │ ⛶ Autopsy — tab-3a1f     │  │                                     │
│ ├──────────────────────────┤  ├─────────────────────────────────────┤
│ │ RAM     1.82 / 4.00 GB   │  │ Ark 2 · 1 awake · ledger verified   │
│ │ [████░░░░░░░░]           │  └─────────────────────────────────────┘
│ │ CPU 34% · zram 1.2 GB    │
│ └──────────────────────────┘
└─────────────────────────────────────────────────────────────────────┘
```

The layout borrows Zen's calm vertical-tab rail and floating, rounded content
surface, then layers Spacetime's own vocabulary on top: execution tiers, Arks,
egress classes, a hash-chained ledger and a Ring -3 Director.

## Screens

| Surface | Address | What it demonstrates |
| --- | --- | --- |
| **Launcher** | `spacetime://newtab` | New-tab page: app catalog with tiers, `.space` world-lines, warm VM pool |
| **OpenCut** | `opencut.app/p/launch-teaser` | A web app running inside a microVM; host overlay warns about blocked egress |
| **yt-01 build** | `spacetime://guest/yt-01` | Terminal tab on `/dev/hvc0` (virtio-console, no graphics stack), host audit tap inline |
| **Field Notes** | `spacetime://guest/feather` | Native Linux GUI app forwarded as a Wayland surface / framebuffer texture |
| **Bounty_Hunter** | `spacetime://space/bounty_hunter` | A world-line in cryo-sleep: 0 MB RSS, 0% CPU, wake in ≈ 8 ms |
| **Telemetry console** | `spacetime://console` | Branch tree, operators, verifier strength, ledger, blockers, Ark inspector |
| **Autopsy** | `spacetime://autopsy/tab-3a1f` | Blast-radius diff, network log, canary trip, tombstone, verifier evidence |

The console and autopsy are **host surfaces** opened as tabs, like browser
`about:` pages — they never belong to a guest.

## Interactions

- Click vertical tabs, pinned app tiles, or world-line cards on the launcher.
- `⌘K` / `Ctrl+K`, `⌘L`, or click the omnibox → command palette (tabs + actions,
  filterable, arrow keys + Enter).
- `⌘B` / `Ctrl+B` → collapse the sidebar to a compact icon rail.
- `⌘⇧D` → Director panel; `⌘⇧J` → Honey-Duct network drawer.
- `Esc` closes palette → Director → drawer, in that order.
- Approve/deny buttons resolve gates, decrement the blocker count and append to
  the Director's live action stream.
- The Director input appends a steering line — the UI communicates that it is a
  Ring -3 interrupt, not a restart of the loop.
- Small simulated telemetry updates arrive every few seconds (CPU/RAM jitter,
  new ledger/action lines). No random UI state is persisted.

## Design decisions worth auditing

1. **Vertical tabs, one environment per tab.** The rail carries the state a
   browser hides: tier dot, memory, network class, and a moon glyph for
   cryo-sleep. The active tab is a soft neutral fill with a 2 px accent bar —
   Zen's "quiet chrome" rather than a saturated tab strip.
2. **The ribbon is the honesty layer.** Under the toolbar, every surface states
   what it just read, wrote, sent, spent and how risky it is. This is the
   "intent ribbon" from `DESIGN-REVIEW.md` §8.3.
3. **Tier is visible everywhere.** `microVM` (own kernel), `cell` (namespaces in
   a shared Ark), `wasi` (capability-limited module), `host` (Ring -3). The
   console shows the explicit warning that cells inside Ark 2 **share a guest
   kernel**, per `DESIGN-REVIEW-2.md`.
4. **Guest vs host is a visual distinction.** Guest content is the app; host
   overlays use dashed violet borders, mono type and a hexagon motif. The
   Director, drawer and console never pretend to be inside the guest.
5. **No "allow permission" popups during normal work.** Approvals only appear
   for irreversible external effects (spend, publish), and they are queued, not
   modal.
6. **Dark only, for now.** The palette is tuned for a low-RAM ambient UI: near
   black surfaces, one violet accent for host/AI, one teal accent for telemetry,
   amber/red reserved for gates and denials.

## Mapping to the documents

- App-in-a-tab, vertical tabs, workspace shell → `BRAINSTORM.md` §"Workspace
  Shell", `DESIGN-REVIEW.md` §5
- MicroVM vs cell vs WASI, Ark trust domain, shared-kernel warning →
  `DESIGN-REVIEW-2.md` §3, §8
- Autopsy / blast radius / tombstone / canaries → `BRAINSTORM.md` novel ideas,
  `DESIGN-REVIEW.md` §8
- Honey-Duct / egress classes / DNS pinning / localhost denial → `DESIGN-REVIEW.md` §4
- Director, reality oracle, goal tree, blockers, ledger, operator registry,
  verifier pass rates → `DESIGN-REVIEW.md` §7, `DESIGN-REVIEW-2.md` §5–§7
- Cryo-sleep and temporal multiplexing → `BRAINSTORM.md` (4 GB survival, factory queue)
- "Prototype the telemetry console before choosing the Rust UI" →
  `DESIGN-REVIEW-2.md` (UI recommendation)

## Honest limitations

- Everything is mocked. No microVM boots, no network policy is enforced, no
  ledger is signed. This is a UI contract, not a security claim.
- The guest apps are static HTML pretending to be OpenCut, a terminal and a
  GTK editor. Their contents are illustrative, not real renderings.
- Close-tab, teleport, pinning and some catalog entries only fire explanatory
  toasts; they are defined so the information architecture is auditable.
- Numbers (RAM, CPU, hashes, timestamps) are plausible placeholders chosen to
  match the budgets in `DESIGN-REVIEW.md` §2, not measurements.

## Files

```
prototype/
├── index.html          all surfaces + overlays in one document
├── README.md           this file
└── assets/
    ├── tokens.css      colour, type, geometry, shared primitives
    ├── shell.css       window frame, vertical tabs, toolbar, overlays, palette
    ├── views.css       launcher, terminal, web app, GUI app, console, autopsy, cryo
    └── shell.js        tab switching, palette, panels, keyboard, mock telemetry
```
