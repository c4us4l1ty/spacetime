const tierInfo = {
  wasi: {
    label: "WASI worker",
    short: "WASI",
    className: "wasi",
    domain: "WASI pool",
    summary: "Capability-limited module",
    caution: "Use for small operators that fit a narrow WASI interface. Wasmtime and its host calls remain trusted components."
  },
  ark: {
    label: "Shared Ark cell",
    short: "Ark cell",
    className: "ark",
    domain: "Ark-Work",
    summary: "Shared guest-kernel trust domain",
    caution: "Cells in this Ark share one guest kernel. Namespaces do not protect a cell from a guest-kernel exploit in another cell."
  },
  quarantine: {
    label: "Quarantine microVM",
    short: "Quarantine VM",
    className: "quarantine",
    domain: "Ark-Quarantine",
    summary: "Separate guest-kernel trust domain",
    caution: "A separate guest kernel raises the boundary, but the VMM, virtual devices, and host-side services remain attack surfaces."
  }
};

const state = {
  workspaceFilter: "all",
  selectedWorkspaceId: "ws-013",
  selectedBranchId: null,
  checkpointIndex: 3,
  inspectorTab: "activity",
  proposalPreviewed: false,
  workspaces: [
    { id: "ws-014", name: "Parser port · candidate set", description: "Differential checks against a pinned reference implementation.", tier: "wasi", status: "Running", network: "None", memory: "214 MB", updated: "2 min ago", verifier: "Differential equality", evidence: "12,480 / 12,480 inputs" },
    { id: "ws-013", name: "OpenCut editor · launch trailer", description: "Local editor workspace with imported clips; no host directory is mounted.", tier: "ark", status: "Running", network: "None", memory: "410 MB", updated: "Just now", verifier: "Timeline target", evidence: "2 clips staged · duration check pending" },
    { id: "ws-012", name: "ffmpeg render · target cut", description: "Bounded render job in the same Work trust domain; awaiting a verified edit.", tier: "ark", status: "Queued", network: "None", memory: "Not allocated", updated: "Queued", verifier: "Output specification", evidence: "Pending" },
    { id: "ws-011", name: "repo-218 · build audit", description: "Unreviewed build scripts are held in a disposable quarantine context.", tier: "quarantine", status: "Needs review", network: "None", memory: "562 MB", updated: "14 min ago", verifier: "Build + test", evidence: "184 / 188 checks" }
  ],
  branches: [
    { id: "br-013-a", parentId: "ws-013", name: "Rough cut A", mode: "cow", summary: "CoW overlay · 2 edits", updated: "2 min ago" },
    { id: "br-013-b", parentId: "ws-013", name: "Color grade B", mode: "cow", summary: "CoW overlay · 1 edit", updated: "8 min ago" }
  ],
  checkpoints: [
    { name: "Clips imported", time: "09:08", evidence: "Base overlay", kind: "Initial state" },
    { name: "First timeline pass", time: "09:16", evidence: "Workspace diff", kind: "Filesystem" },
    { name: "Duration target checked", time: "09:31", evidence: "Verifier event", kind: "Evidence" },
    { name: "Current working tree", time: "09:42", evidence: "Overlay + ledger", kind: "Current" }
  ],
  workspaceActivity: [
    { time: "09:42", title: "Proposed split held for review", detail: "No application action was executed.", kind: "amber", workspaceId: "ws-013" },
    { time: "09:39", title: "Two clips staged in workspace", detail: "Import is confined to the workspace overlay.", kind: "mint", workspaceId: "ws-013" },
    { time: "09:31", title: "Duration check completed", detail: "58.2 seconds · target 60 seconds maximum.", kind: "blue", workspaceId: "ws-013" },
    { time: "09:16", title: "CoW branch created", detail: "Rough cut A · same Ark-Work kernel domain.", kind: "mint", workspaceId: "ws-013" }
  ],
  runs: [
    { id: "run-104", title: "CSV parser · differential suite", detail: "Reference implementation compared with candidate outputs.", tier: "wasi", candidates: "8 candidates", verifier: "Differential equality", evidence: "12,480 / 12,480 inputs", status: "Verified", updated: "2 min ago", memory: "88 MB" },
    { id: "run-103", title: "repo-218 · build and test", detail: "Quarantine run; four checks require a human to inspect the failure evidence.", tier: "quarantine", candidates: "1 isolated job", verifier: "Build + test suite", evidence: "184 / 188 checks", status: "Needs review", updated: "14 min ago", memory: "562 MB" },
    { id: "run-102", title: "Protocol trace replay", detail: "Replays captured local fixtures against a candidate operator.", tier: "ark", candidates: "4 candidates", verifier: "Trace replay", evidence: "18 / 19 traces", status: "Running", updated: "21 min ago", memory: "296 MB" },
    { id: "run-101", title: "Manifest normalizer", detail: "Small deterministic transform with a fixed fixture set.", tier: "wasi", candidates: "8 candidates", verifier: "Golden fixtures", evidence: "320 / 320 cases", status: "Verified", updated: "43 min ago", memory: "64 MB" }
  ],
  blockers: [
    { id: "B-014", title: "Registry access requested", detail: "A build in Ark-Quarantine requested brokered access to a package registry. Current demo policy is no egress.", target: "repo-218 · build audit", age: "5 min ago", category: "Network policy", status: "Open" },
    { id: "B-013", title: "External account verification", detail: "A task reached an external identity / verification gate. The intended behavior is to pause and queue it, not guess or bypass the gate.", target: "Protocol research", age: "18 min ago", category: "Human gate", status: "Open" }
  ],
  resolved: [
    { id: "B-012", title: "Candidate exceeded time budget", outcome: "Stopped by resource limit · 32 min ago" },
    { id: "B-011", title: "Verifier disagreement", outcome: "Returned to inspection · 1 hr ago" }
  ],
  operators: [
    { id: "op-018", name: "csv-normalize", subtitle: "parser / normalize", tier: "wasi", scope: "Input buffer → output buffer", verifier: "Differential equality", checks: "93 / 93", checked: "4 min ago", health: "Healthy" },
    { id: "op-017", name: "trace-replay", subtitle: "protocol / local fixture", tier: "ark", scope: "Fixture set only · no egress", verifier: "State transition match", checks: "71 / 72", checked: "11 min ago", health: "Watch" },
    { id: "op-014", name: "manifest-scan", subtitle: "supply-chain / metadata", tier: "quarantine", scope: "Read-only repo snapshot", verifier: "Schema + policy checks", checks: "18 / 18", checked: "26 min ago", health: "Healthy" },
    { id: "op-009", name: "legacy-login-flow", subtitle: "browser / external gate", tier: "quarantine", scope: "Disabled pending review", verifier: "Postcondition drift", checks: "2 / 7", checked: "2 days ago", health: "Quarantined" }
  ],
  verifiers: [
    { name: "Differential equality", kind: "Data / deterministic", description: "Compares candidate output with a pinned reference over a fixed corpus and randomized inputs.", result: "12,480 / 12,480", scope: "Local fixtures", health: "Healthy" },
    { name: "Build + test suite", kind: "Source / reproducibility", description: "Builds a source snapshot and runs declared tests in the assigned execution tier.", result: "184 / 188", scope: "Quarantine job", health: "Review" },
    { name: "Protocol trace replay", kind: "State / replay", description: "Replays captured requests against a local mock and checks the expected state transitions.", result: "18 / 19 traces", scope: "Mock service", health: "Watch" },
    { name: "Artifact hash", kind: "Integrity / content", description: "Checks that an artifact matches the recorded content digest; it does not prove the artifact is trustworthy.", result: "Digest matches", scope: "Local artifact", health: "Healthy" },
    { name: "Policy decision", kind: "Authorization / Cedar-like", description: "Evaluates a proposed action against the current mock policy and capability limits.", result: "Action denied", scope: "Policy fixture", health: "Healthy" },
    { name: "Independent re-check", kind: "Metrology / repeat", description: "Repeats an accepted operation from a clean fixture to expose nondeterminism or false passes.", result: "48 / 50 agree", scope: "Fresh replay", health: "Watch" }
  ],
  ledger: [
    { seq: 421, time: "09:42:08", actor: "policy-broker", title: "Egress request denied", detail: "Ark-Quarantine · registry access held for review", kind: "Policy", hash: "7f2d90c4…e18a" },
    { seq: 420, time: "09:41:52", actor: "verifier", title: "Differential equality passed", detail: "run-104 · 12,480 cases · fixture v3", kind: "Proof", hash: "21a85f3d…bd02" },
    { seq: 419, time: "09:40:17", actor: "cycler", title: "Candidate population queued", detail: "run-104 · eight candidates · WASI worker", kind: "Run", hash: "de81c449…0a71" },
    { seq: 418, time: "09:38:04", actor: "supervisor", title: "Quarantine job started", detail: "repo-218 · fresh guest kernel · network none", kind: "Execution", hash: "0a3b4c9e…8fc4" },
    { seq: 417, time: "09:36:20", actor: "operator-health", title: "Operator marked watch", detail: "trace-replay · one trace mismatch", kind: "Registry", hash: "c2e7109a…3e66" },
    { seq: 416, time: "09:34:11", actor: "supervisor", title: "Workspace created", detail: "benchmark-harness · Ark-Work cell", kind: "Execution", hash: "84bb1620…4f91" }
  ],
  activity: [
    { time: "09:42", text: "<strong>Policy broker</strong> held a registry request for human review.", kind: "amber" },
    { time: "09:41", text: "<strong>Differential equality</strong> verified all 12,480 fixture inputs.", kind: "mint" },
    { time: "09:40", text: "<strong>Eight candidates</strong> queued for the parser experiment.", kind: "blue" },
    { time: "09:38", text: "<strong>Quarantine job</strong> started with network disabled.", kind: "amber" }
  ]
};

const viewNames = {
  workspace: "WORKSPACE SHELL",
  overview: "OVERVIEW",
  workspaces: "WORKSPACES",
  runs: "RUNS & EXPERIMENTS",
  operators: "OPERATOR REGISTRY",
  verifiers: "VERIFIERS",
  ledger: "AUDIT LEDGER",
  blockers: "ATTENTION QUEUE"
};

let toastTimer = null;

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (char) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;"
  })[char]);
}

function tierChip(tier) {
  const info = tierInfo[tier] || tierInfo.ark;
  return `<span class="tier-chip tier-chip--${info.className}">${escapeHtml(info.short)}</span>`;
}

function statusBadge(status) {
  const classes = {
    "Running": "running",
    "Verified": "running",
    "Queued": "queued",
    "Needs review": "review",
    "Paused": "paused",
    "Failed": "failed"
  };
  return `<span class="status-badge status-badge--${classes[status] || "queued"}">${escapeHtml(status)}</span>`;
}

function iconForTier(tier) {
  if (tier === "quarantine") {
    return `<span class="domain-icon domain-icon--amber" aria-hidden="true"><svg viewBox="0 0 20 20"><path d="M10 2.4 16 5v4.8c0 3.5-2.5 6-6 7.8-3.5-1.8-6-4.3-6-7.8V5z"/><path d="M7.2 9.7 9.2 12l3.8-4"/></svg></span>`;
  }
  if (tier === "wasi") {
    return `<span class="domain-icon domain-icon--blue" aria-hidden="true"><svg viewBox="0 0 20 20"><path d="m7 5-4 5 4 5M13 5l4 5-4 5M11.5 3.5l-3 13"/></svg></span>`;
  }
  return `<span class="domain-icon" aria-hidden="true"><svg viewBox="0 0 20 20"><path d="M10 2.5 16.3 6v8L10 17.5 3.7 14V6z"/><path d="m3.9 6.1 6.1 3.5 6.1-3.5M10 9.6v7.6"/></svg></span>`;
}

function addActivity(text, kind = "blue") {
  const now = new Date();
  const time = now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
  state.activity.unshift({ time, text: escapeHtml(text), kind });
  state.activity = state.activity.slice(0, 8);
}

function appendLedger(title, detail, kind = "Session") {
  const nextSeq = Math.max(...state.ledger.map((event) => event.seq), 420) + 1;
  const randomPart = Math.random().toString(16).slice(2, 10).padEnd(8, "0");
  state.ledger.unshift({
    seq: nextSeq,
    time: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false }),
    actor: "local-prototype",
    title,
    detail,
    kind,
    hash: `${randomPart}…demo`
  });
}

function renderDomainCards() {
  const workCount = state.workspaces.filter((workspace) => workspace.tier === "ark").length;
  const quarantineCount = state.workspaces.filter((workspace) => workspace.tier === "quarantine" && workspace.status !== "Paused").length;
  const wasiCount = state.workspaces.filter((workspace) => workspace.tier === "wasi").length;
  const domains = [
    {
      title: "Ark-Work",
      tier: "ark",
      subtitle: "Shared guest-kernel trust domain",
      memory: "0.91 GB",
      count: `${workCount} cells`,
      tags: ["Cell-scoped egress", "Shared kernel"],
      note: "Only workloads allowed to share a kernel belong here."
    },
    {
      title: "Ark-Quarantine",
      tier: "quarantine",
      subtitle: "Separate guest kernel · disposable jobs",
      memory: "0.57 GB",
      count: `${quarantineCount} job${quarantineCount === 1 ? "" : "s"}`,
      tags: ["Network none by default", "No shared secrets"],
      note: "A new VM boundary is more expensive; it is not an absolute host guarantee."
    },
    {
      title: "WASI worker pool",
      tier: "wasi",
      subtitle: "Narrow capability-based operators",
      memory: "0.19 GB",
      count: `${wasiCount + 2} modules`,
      tags: ["Explicit host calls", "Fuel / memory caps"],
      note: "Wasmtime and its host-call interface remain part of the trusted base."
    }
  ];
  document.getElementById("domain-list").innerHTML = domains.map((domain) => `
    <article class="domain-card">
      <div class="domain-main">
        ${iconForTier(domain.tier)}
        <div class="domain-copy">
          <div class="domain-titleline"><strong>${escapeHtml(domain.title)}</strong>${tierChip(domain.tier)}</div>
          <div class="domain-subtitle">${escapeHtml(domain.subtitle)}</div>
          <div class="domain-tags">${domain.tags.map((tag, index) => `<span class="domain-tag ${index === 0 ? `domain-tag--${domain.tier === "quarantine" ? "amber" : "mint"}` : ""}">${escapeHtml(tag)}</span>`).join("")}</div>
        </div>
      </div>
      <div class="domain-side"><div class="domain-memory">${escapeHtml(domain.memory)}<small>demo estimate</small></div><div class="domain-count"><strong>${escapeHtml(domain.count)}</strong></div><button class="domain-open" type="button" data-view="workspaces">View cells →</button></div>
    </article>
  `).join("");
}

function runRow(run, compact = false) {
  const resultStatus = run.status === "Verified" ? "good" : (run.status === "Needs review" ? "warn" : "good");
  const candidateSummary = `${escapeHtml(run.candidates)} · ${escapeHtml(run.updated)}`;
  if (compact) {
    return `<tr>
      <td><div class="run-name">${iconForTier(run.tier)}<span><strong>${escapeHtml(run.title)}</strong><small>${escapeHtml(run.id)} · ${escapeHtml(run.verifier)}</small></span></div></td>
      <td>${tierChip(run.tier)}</td>
      <td><div class="result-copy">${escapeHtml(run.evidence)}<small>${escapeHtml(candidateSummary)}</small></div></td>
      <td><span class="status-pill status-pill--${resultStatus}">${escapeHtml(run.status)}</span></td>
      <td><button class="row-action" type="button" aria-label="Open ${escapeHtml(run.title)} details" data-action="open-run" data-id="${escapeHtml(run.id)}"><svg viewBox="0 0 16 16"><path d="M3 8h9M8 4l4 4-4 4"/></svg></button></td>
    </tr>`;
  }
  return `<tr>
    <td><div class="run-name">${iconForTier(run.tier)}<span><strong>${escapeHtml(run.title)}</strong><small>${escapeHtml(run.id)} · ${escapeHtml(run.detail)}</small></span></div></td>
    <td>${tierChip(run.tier)}</td>
    <td>${escapeHtml(run.candidates)}</td>
    <td><div class="result-copy">${escapeHtml(run.verifier)}<small>${escapeHtml(run.evidence)}</small></div></td>
    <td>${escapeHtml(run.updated)}</td>
    <td>${statusBadge(run.status)}</td>
    <td><button class="row-action" type="button" aria-label="Open ${escapeHtml(run.title)} details" data-action="open-run" data-id="${escapeHtml(run.id)}"><svg viewBox="0 0 16 16"><path d="M3 8h9M8 4l4 4-4 4"/></svg></button></td>
  </tr>`;
}

function renderRunTables() {
  document.getElementById("recent-runs-body").innerHTML = state.runs.slice(0, 3).map((run) => runRow(run, true)).join("");
  document.getElementById("runs-table-body").innerHTML = state.runs.map((run) => runRow(run)).join("");
  document.getElementById("run-count-label").textContent = `${state.runs.length} runs`;
  const activeRuns = state.runs.filter((run) => run.status === "Queued" || run.status === "Running");
  const activeCandidates = activeRuns.reduce((sum, run) => {
    const match = run.candidates.match(/\d+/);
    return sum + (match ? Number(match[0]) : 1);
  }, 0);
  document.getElementById("cycle-candidate-count").textContent = String(activeCandidates);
  document.getElementById("cycle-experiment-count").textContent = `Across ${activeRuns.length} active experiment${activeRuns.length === 1 ? "" : "s"}`;
}

function renderActivity() {
  const activityMarkup = state.activity.slice(0, 4).map((item) => `
    <div class="activity-item">
      <span class="activity-marker activity-marker--${escapeHtml(item.kind)}" aria-hidden="true"></span>
      <div class="activity-copy">${item.text}<small>${escapeHtml(item.time)} · demo event</small></div>
    </div>
  `).join("");
  document.getElementById("activity-list").innerHTML = activityMarkup;
}

function renderAttentionPreview() {
  const open = state.blockers.filter((blocker) => blocker.status === "Open");
  document.getElementById("attention-count").textContent = String(open.length);
  document.getElementById("attention-preview").innerHTML = open.slice(0, 2).map((blocker) => `
    <article class="attention-item">
      <span class="attention-icon">!</span>
      <div class="attention-content"><strong>${escapeHtml(blocker.title)}</strong><p>${escapeHtml(blocker.target)} · ${escapeHtml(blocker.category)}</p><button class="attention-review" type="button" data-action="open-blocker" data-id="${escapeHtml(blocker.id)}">Inspect context →</button></div>
      <span class="attention-age">${escapeHtml(blocker.age)}</span>
    </article>
  `).join("") || `<div class="empty-state empty-state--compact"><span class="empty-orbit">✓</span><h3>Queue clear</h3><p>No open demo blockers.</p></div>`;
}

function renderOverview() {
  const active = state.workspaces.filter((workspace) => workspace.status === "Running").length;
  const openBlockers = state.blockers.filter((blocker) => blocker.status === "Open").length;
  document.getElementById("metric-active").textContent = String(active);
  document.getElementById("metric-total").textContent = `of ${state.workspaces.length}`;
  const reviewCount = state.workspaces.filter((workspace) => workspace.status === "Needs review").length;
  document.getElementById("metric-review-note").textContent = `${reviewCount} workspace${reviewCount === 1 ? "" : "s"} held for review`;
  document.getElementById("metric-blockers").textContent = String(openBlockers);
  document.getElementById("nav-workspace-count").textContent = String(state.workspaces.length);
  document.getElementById("nav-blocker-count").textContent = String(openBlockers);
  document.getElementById("nav-blocker-count").hidden = openBlockers === 0;
  document.getElementById("filter-all-count").textContent = String(state.workspaces.length);
  document.getElementById("blocker-heading-count").textContent = `${openBlockers} open item${openBlockers === 1 ? "" : "s"}`;
  document.getElementById("blocker-open-count").textContent = String(openBlockers);
  document.getElementById("ledger-event-count").textContent = String(state.ledger.length);
  document.getElementById("ledger-latest").textContent = `#${String(Math.max(...state.ledger.map((item) => item.seq), 0)).padStart(5, "0")}`;
  renderDomainCards();
  renderAttentionPreview();
  renderRunTables();
  renderActivity();
}

function workspaceMatchesFilter(workspace) {
  if (state.workspaceFilter === "active") return workspace.status === "Running";
  if (state.workspaceFilter === "quarantine") return workspace.tier === "quarantine";
  return true;
}

function renderWorkspaces() {
  const search = (document.getElementById("workspace-search")?.value || "").trim().toLowerCase();
  const matches = state.workspaces.filter((workspace) => workspaceMatchesFilter(workspace) && `${workspace.name} ${workspace.description} ${workspace.id} ${tierInfo[workspace.tier].label}`.toLowerCase().includes(search));
  document.getElementById("workspace-grid").innerHTML = matches.map((workspace) => {
    const details = tierInfo[workspace.tier];
    const networkClass = workspace.network === "None" ? "domain-tag--mint" : "domain-tag--amber";
    const controls = workspace.status === "Running"
      ? `<button class="text-action text-action--warn" type="button" data-action="pause-workspace" data-id="${escapeHtml(workspace.id)}">Pause workspace</button>`
      : `<button class="text-action" type="button" data-action="open-workspace" data-id="${escapeHtml(workspace.id)}">Inspect state</button>`;
    return `<article class="workspace-card">
      <div class="workspace-card-top">
        <div class="workspace-ident">${iconForTier(workspace.tier)}<div class="workspace-title"><h3 title="${escapeHtml(workspace.name)}">${escapeHtml(workspace.name)}</h3><p>${escapeHtml(workspace.description)}</p></div></div>
        ${statusBadge(workspace.status)}
      </div>
      <div class="workspace-meta">${tierChip(workspace.tier)}<span class="domain-tag ${networkClass}">Network: ${escapeHtml(workspace.network)}</span><span class="domain-tag">${escapeHtml(details.domain)}</span></div>
      <div class="workspace-metrics"><div class="workspace-metric"><span>Demo memory</span><strong>${escapeHtml(workspace.memory)}</strong></div><div class="workspace-metric"><span>Verifier</span><strong>${escapeHtml(workspace.verifier || "Pending")}</strong></div><div class="workspace-metric"><span>Last activity</span><strong>${escapeHtml(workspace.updated)}</strong></div></div>
      <div class="workspace-actions"><button class="quiet-button" type="button" data-action="open-workspace" data-id="${escapeHtml(workspace.id)}">Inspect boundary <span>→</span></button>${controls}</div>
    </article>`;
  }).join("");
  document.getElementById("workspace-empty").hidden = matches.length !== 0;
}

function renderOperators() {
  const search = (document.getElementById("operator-search")?.value || "").trim().toLowerCase();
  const operators = state.operators.filter((operator) => `${operator.name} ${operator.subtitle} ${operator.scope} ${operator.verifier}`.toLowerCase().includes(search));
  document.getElementById("operators-table-body").innerHTML = operators.map((operator) => {
    const healthClass = operator.health === "Healthy" ? "good" : (operator.health === "Watch" ? "warn" : "bad");
    return `<tr>
      <td><div class="operator-name"><span class="operator-glyph">⌘</span><span><strong>${escapeHtml(operator.name)}</strong><small>${escapeHtml(operator.id)} · ${escapeHtml(operator.subtitle)}</small></span></div></td>
      <td>${tierChip(operator.tier)}</td><td>${escapeHtml(operator.scope)}</td>
      <td><div class="result-copy">${escapeHtml(operator.verifier)}<small>${escapeHtml(operator.checks)} checks</small></div></td>
      <td>${escapeHtml(operator.checked)}</td><td><span class="health-label health-label--${healthClass}">${escapeHtml(operator.health)}</span></td>
      <td><button class="row-action" type="button" aria-label="Inspect ${escapeHtml(operator.name)}" data-action="open-operator" data-id="${escapeHtml(operator.id)}"><svg viewBox="0 0 16 16"><path d="M3 8h9M8 4l4 4-4 4"/></svg></button></td>
    </tr>`;
  }).join("");
}

function renderVerifiers() {
  document.getElementById("verifier-grid").innerHTML = state.verifiers.map((verifier) => {
    const healthClass = verifier.health === "Healthy" ? "good" : (verifier.health === "Review" ? "warn" : "warn");
    return `<article class="verifier-card">
      <div class="verifier-card-top"><div><div class="panel-kicker">${escapeHtml(verifier.kind.toUpperCase())}</div><h3>${escapeHtml(verifier.name)}</h3></div><span class="health-label health-label--${healthClass}">${escapeHtml(verifier.health)}</span></div>
      <p>${escapeHtml(verifier.description)}</p>
      <div class="verifier-card-meta"><span>Latest result</span><strong>${escapeHtml(verifier.result)}</strong></div>
      <div class="verifier-card-meta"><span>Scope</span><strong>${escapeHtml(verifier.scope)}</strong></div>
    </article>`;
  }).join("");
}

function renderLedger() {
  const search = (document.getElementById("ledger-search")?.value || "").trim().toLowerCase();
  const events = state.ledger.filter((event) => `${event.actor} ${event.title} ${event.detail} ${event.kind} ${event.seq}`.toLowerCase().includes(search));
  document.getElementById("ledger-list").innerHTML = events.map((event) => `
    <article class="ledger-event">
      <span class="ledger-sequence">#${String(event.seq).padStart(5, "0")}</span>
      <span class="ledger-time">${escapeHtml(event.time)}</span>
      <div class="ledger-event-copy"><strong>${escapeHtml(event.title)}</strong><small>${escapeHtml(event.actor)} · ${escapeHtml(event.detail)}</small></div>
      <span class="ledger-hash" title="Illustrative hash only">${escapeHtml(event.hash)}</span>
      <span class="ledger-kind"><span class="tier-chip">${escapeHtml(event.kind)}</span></span>
    </article>
  `).join("") || `<div class="empty-state empty-state--compact"><span class="empty-orbit">⌕</span><h3>No matching events</h3><p>Try another search term.</p></div>`;
}

function blockerCard(blocker) {
  return `<article class="blocker-card">
    <div class="blocker-card-head"><span class="blocker-card-symbol">!</span><div class="blocker-card-title"><h3>${escapeHtml(blocker.title)}</h3><p>${escapeHtml(blocker.detail)}</p></div><span class="status-pill status-pill--warn">${escapeHtml(blocker.status)}</span></div>
    <div class="blocker-card-meta"><span>${escapeHtml(blocker.id)}</span><span>${escapeHtml(blocker.target)}</span><span>${escapeHtml(blocker.category)} · ${escapeHtml(blocker.age)}</span></div>
    <div class="blocker-card-actions"><button class="button button--secondary" type="button" data-action="open-blocker" data-id="${escapeHtml(blocker.id)}">Inspect context</button><button class="button button--secondary" type="button" data-action="resolve-blocker" data-id="${escapeHtml(blocker.id)}">Mark reviewed</button></div>
  </article>`;
}

function renderBlockers() {
  const open = state.blockers.filter((blocker) => blocker.status === "Open");
  const resolved = [...state.blockers.filter((blocker) => blocker.status !== "Open").map((blocker) => ({ id: blocker.id, title: blocker.title, outcome: `${blocker.status} · demo only` })), ...state.resolved];
  document.getElementById("blocker-list").innerHTML = open.map(blockerCard).join("");
  document.getElementById("blocker-empty").hidden = open.length !== 0;
  document.getElementById("resolved-blockers").innerHTML = resolved.slice(0, 6).map((item) => `<div class="resolved-item"><span>✓</span><span><strong>${escapeHtml(item.title)}</strong> — ${escapeHtml(item.outcome)}</span></div>`).join("");
  document.getElementById("blocker-heading-count").textContent = `${open.length} open item${open.length === 1 ? "" : "s"}`;
  document.getElementById("blocker-open-count").textContent = String(open.length);
}

function getSelectedWorkspaceContext() {
  const workspace = state.workspaces.find((item) => item.id === state.selectedWorkspaceId) || state.workspaces[0];
  const branch = state.branches.find((item) => item.id === state.selectedBranchId) || null;
  const tier = branch?.executionTier || (branch?.mode === "quarantine" ? "quarantine" : workspace.tier);
  return {
    workspace,
    branch,
    tier,
    network: branch?.network || (tier === "quarantine" ? "None" : workspace.network),
    status: branch?.status || (branch?.mode === "quarantine" ? "Queued" : workspace.status),
    memory: branch?.memory || (tier === "quarantine" ? "Not allocated" : workspace.memory),
    verifier: branch?.verifier || workspace.verifier || "Not assigned",
    evidence: branch?.evidence || workspace.evidence || "No evidence yet",
    domain: branch?.domain || (tier === "quarantine" ? "Ark-Quarantine" : tierInfo[tier].domain)
  };
}

function treeWorkspaceButton(id, nested = false) {
  const workspace = state.workspaces.find((item) => item.id === id);
  if (!workspace) return "";
  const info = tierInfo[workspace.tier];
  const selected = state.selectedWorkspaceId === id && !state.selectedBranchId;
  const symbol = workspace.tier === "wasi" ? "λ" : (workspace.tier === "quarantine" ? "◇" : "▣");
  const status = workspace.status === "Running" ? "ACTIVE" : workspace.status === "Needs review" ? "REVIEW" : workspace.status.toUpperCase();
  return `<button type="button" class="tree-node ${nested ? "tree-node--nested" : ""} ${selected ? "is-selected" : ""}" data-action="select-workspace" data-id="${escapeHtml(id)}" aria-current="${selected ? "page" : "false"}">
    <span class="tree-node-symbol tree-node-symbol--${info.className}">${symbol}</span>
    <span class="tree-node-copy"><strong>${escapeHtml(workspace.name.split(" · ")[0])}</strong><small>${escapeHtml(info.short)} <i>·</i> ${escapeHtml(status)}</small></span>
    <span class="tree-status-dot ${workspace.status === "Running" ? "is-running" : workspace.status === "Needs review" ? "is-review" : ""}"></span>
  </button>`;
}

function treeBranchButton(branch) {
  const selected = state.selectedBranchId === branch.id;
  const isQuarantine = branch.mode === "quarantine";
  const parent = state.workspaces.find((workspace) => workspace.id === branch.parentId);
  const sourceBranch = state.branches.find((item) => item.id === branch.sourceBranchId);
  const summary = isQuarantine
    ? `Fresh VM · from ${sourceBranch ? sourceBranch.name : parent ? parent.name.split(" · ")[0] : "workspace"}`
    : branch.summary;
  return `<button type="button" class="tree-node tree-node--branch ${isQuarantine ? "tree-node--quarantine" : ""} ${selected ? "is-selected" : ""}" data-action="select-branch" data-id="${escapeHtml(branch.id)}" aria-current="${selected ? "page" : "false"}">
    <span class="tree-branch-elbow" aria-hidden="true">↳</span><span class="tree-node-symbol ${isQuarantine ? "tree-node-symbol--quarantine" : "tree-node-symbol--branch"}">${isQuarantine ? "◇" : "⑂"}</span>
    <span class="tree-node-copy"><strong>${escapeHtml(branch.name)}</strong><small>${escapeHtml(summary || "CoW overlay · same execution domain")}</small></span>
  </button>`;
}

function treeBranchNode(branch) {
  const children = state.branches.filter((item) => item.sourceBranchId === branch.id && item.mode === "cow");
  return `${treeBranchButton(branch)}${children.length ? `<div class="tree-branch-list tree-branch-list--nested">${children.map(treeBranchNode).join("")}</div>` : ""}`;
}

function renderWorkspaceTree() {
  const videoIds = new Set(["ws-013", "ws-012"]);
  const videoWorkspaces = state.workspaces.filter((workspace) => videoIds.has(workspace.id));
  const toolsWorkspaces = state.workspaces.filter((workspace) => workspace.id === "ws-014" || (!videoIds.has(workspace.id) && workspace.tier !== "quarantine"));
  const quarantineWorkspaces = state.workspaces.filter((workspace) => workspace.tier === "quarantine");
  const workspaceNodes = (workspaces) => workspaces.map((workspace) => {
    const cowBranches = state.branches.filter((branch) => branch.parentId === workspace.id && branch.mode === "cow" && !branch.sourceBranchId);
    return `${treeWorkspaceButton(workspace.id)}${cowBranches.length ? `<div class="tree-branch-list">${cowBranches.map(treeBranchNode).join("")}</div>` : ""}`;
  }).join("");
  const freshBranches = state.branches.filter((branch) => branch.mode === "quarantine");
  document.getElementById("workspace-tree-content").innerHTML = `
    ${videoWorkspaces.length ? `<div class="tree-group"><div class="tree-group-title"><span class="tree-chevron">⌄</span><span class="tree-folder-icon">▰</span>Video project</div><div class="tree-children">${workspaceNodes(videoWorkspaces)}</div></div>` : ""}
    ${toolsWorkspaces.length ? `<div class="tree-group"><div class="tree-group-title"><span class="tree-chevron">⌄</span><span class="tree-folder-icon tree-folder-icon--blue">▰</span>Bench &amp; tools</div><div class="tree-children">${workspaceNodes(toolsWorkspaces)}</div></div>` : ""}
    ${quarantineWorkspaces.length || freshBranches.length ? `<div class="tree-group"><div class="tree-group-title"><span class="tree-chevron">⌄</span><span class="tree-folder-icon tree-folder-icon--amber">▰</span>Quarantine</div><div class="tree-children">${workspaceNodes(quarantineWorkspaces)}${freshBranches.length ? `<div class="tree-branch-list">${freshBranches.map(treeBranchNode).join("")}</div>` : ""}</div></div>` : ""}
  `;
}

function renderOpenCutSurface() {
  return `<div class="studio-app">
    <div class="studio-toolbar"><div class="studio-product"><span class="studio-product-mark">O</span><strong>OpenCut</strong><span class="studio-divider">/</span><span>Launch trailer</span><span class="save-indicator"><i></i> Local draft</span></div><div class="studio-tools"><button type="button" data-action="editor-tool" data-tool="Select" aria-label="Select tool">↖</button><button type="button" data-action="editor-tool" data-tool="Split" aria-label="Split tool">⌁</button><button type="button" data-action="editor-tool" data-tool="Trim" aria-label="Trim tool">◫</button><button type="button" data-action="editor-tool" data-tool="Undo" aria-label="Undo">↶</button></div></div>
    <div class="studio-upper">
      <aside class="media-bin"><div class="surface-section-title"><span>MEDIA BIN</span><span>02</span></div>
        <div class="media-item"><span class="media-thumb media-thumb--one"><i></i></span><span><strong>Clip_01.mp4</strong><small>00:23 · 1080p</small></span><span class="media-more">···</span></div>
        <div class="media-item"><span class="media-thumb media-thumb--two"><i></i></span><span><strong>Clip_02.mp4</strong><small>00:37 · 1080p</small></span><span class="media-more">···</span></div>
        <div class="media-bin-footer"><span>Workspace-local files</span><button type="button" data-action="editor-tool" data-tool="Import">＋</button></div>
      </aside>
      <section class="preview-column"><div class="preview-toolbar"><span>PREVIEW</span><span class="preview-resolution">FIT · 1080p</span></div>
        <div class="preview-frame ${state.proposalPreviewed ? "preview-frame--review" : ""}"><div class="preview-stars"></div><div class="preview-orb"></div><div class="preview-horizon"></div><div class="preview-caption"><small>THE NEXT CHAPTER</small><strong>BEYOND<br>THE EDGE</strong><span>LAUNCH FILM · 2026</span></div><div class="preview-fixture-tag">LOCAL FIXTURE · MOCK FRAME</div>${state.proposalPreviewed ? `<div class="preview-review-overlay"><span>PREVIEW ONLY</span><strong>Split marker · 00:08</strong></div>` : ""}</div>
        <div class="preview-controls"><button type="button" data-action="editor-tool" data-tool="Previous frame">|◀</button><button type="button" class="preview-play" data-action="editor-tool" data-tool="Play preview">▶</button><button type="button" data-action="editor-tool" data-tool="Next frame">▶|</button><span class="preview-time">00:08 <i>/</i> 00:58</span><span class="preview-zoom">− &nbsp; 100% &nbsp; +</span></div>
      </section>
    </div>
    <section class="timeline-mock"><div class="timeline-heading"><div><span class="surface-section-title">TIMELINE</span><span class="timeline-duration">00:58.2</span></div><div class="timeline-head-actions"><span class="timeline-track-count">2 tracks</span><button type="button" data-action="editor-tool" data-tool="Add track">＋ Track</button></div></div>
      <div class="timeline-ruler"><span>00:00</span><span>00:08</span><span>00:16</span><span>00:24</span><span>00:32</span><span>00:40</span><span>00:48</span><span>00:56</span></div>
      <div class="timeline-row"><span class="track-label">V1</span><div class="track-lane"><div class="clip-block clip-block--teal" style="left:0;width:39%"><strong>Clip_01.mp4</strong><small>00:00 — 00:23</small><span class="clip-wave"></span></div><div class="clip-block clip-block--violet" style="left:41%;width:55%"><strong>Clip_02.mp4</strong><small>00:23 — 00:58</small><span class="clip-wave"></span><i class="playhead-handle"></i></div><span class="timeline-playhead" style="left:48%"></span></div></div>
      <div class="timeline-row timeline-row--audio"><span class="track-label">A1</span><div class="track-lane"><div class="audio-block"><span class="audio-wave"></span></div></div></div>
    </section>
  </div>`;
}

function renderTerminalSurface(workspace) {
  const isQueued = workspace.status === "Queued";
  const isRenderJob = workspace.id === "ws-012";
  const command = isQueued
    ? isRenderJob ? "waiting for verifier: timeline-duration" : "no process started · awaiting task definition"
    : isRenderJob ? "ffmpeg -i /workspace/media/launch-cut.mp4 -f null -" : "spacetime inspect /workspace";
  const output = isQueued
    ? isRenderJob ? "Ready when the timeline is independently verified." : "No process started; execution is only a mock state."
    : isRenderJob ? "Output length: 00:58.2 · exit status: awaiting verifier" : "Workspace state shown for illustration · no command executed";
  const description = isRenderJob ? "Input: workspace-local snapshot · network: none" : "Shell is a local mock surface · host bridge: disconnected";
  return `<div class="terminal-surface"><div class="terminal-top"><span class="terminal-dots"><i></i><i></i><i></i></span><strong>${escapeHtml(workspace.name.split(" · ")[0])}</strong><span class="terminal-tier">${isQueued ? "QUEUED" : "ARK-WORK · CELL"}</span></div>
    <div class="terminal-body"><div class="terminal-line"><span class="terminal-prompt">$</span><span>${escapeHtml(command)}</span></div>
      <div class="terminal-line terminal-line--muted"><span class="terminal-prompt">›</span><span>${escapeHtml(description)}</span></div>
      <div class="terminal-progress"><span style="width:${isQueued ? "0%" : isRenderJob ? "72%" : "0%"}"></span></div>
      <div class="terminal-line terminal-line--success"><span class="terminal-prompt">✓</span><span>${escapeHtml(output)}</span></div>
      <div class="terminal-caret"><span>spacetime-cell</span><i></i></div>
    </div><div class="terminal-foot"><span>OverlayFS · ephemeral workspace</span><span>stdout is demo text</span></div></div>`;
}

function renderWasiSurface(workspace) {
  const isParserFixture = workspace.id === "ws-014";
  const statusClass = workspace.status === "Needs review" ? "warn" : workspace.status === "Queued" ? "warn" : "good";
  const result = isParserFixture ? workspace.evidence : workspace.status === "Queued" ? "No verifier result" : workspace.evidence;
  const verifier = workspace.verifier || "No verifier assigned";
  const description = isParserFixture ? "Candidate outputs compared with a pinned reference." : "Capability-limited worker surface · no execution was started by this UI.";
  const inputLabel = isParserFixture ? "PINNED REFERENCE FIXTURE" : "DEMO INPUT";
  return `<div class="operator-surface"><div class="operator-surface-head"><div><span class="panel-kicker">WASI OPERATOR · DEMO</span><h3>${escapeHtml(workspace.name.split(" · ")[0])}</h3><p>${escapeHtml(description)}</p></div><span class="status-pill status-pill--${statusClass}">${escapeHtml(workspace.status)}</span></div>
    <div class="operator-code-grid"><div class="code-panel"><div class="code-panel-title"><span>${inputLabel}</span><span>READ ONLY</span></div><pre><span class="code-muted">{</span>
  <span class="code-key">"tokens"</span>: [<span class="code-string">"parse"</span>, <span class="code-string">"verify"</span>],
  <span class="code-key">"mode"</span>: <span class="code-string">"strict"</span>,
  <span class="code-key">"seed"</span>: <span class="code-number">18427</span>
<span class="code-muted">}</span></pre></div><div class="code-panel code-panel--result"><div class="code-panel-title"><span>VERIFIER RESULT</span><span>${isParserFixture ? "FIXTURE SET" : "DEMO STATE"}</span></div><div class="operator-result"><span class="result-check">${isParserFixture ? "✓" : "·"}</span><strong>${escapeHtml(result)}</strong><small>${escapeHtml(verifier)}</small></div><div class="operator-capability">No ambient filesystem or network handles in this mock.</div></div></div>
    <div class="operator-surface-foot"><span>Runtime <strong>Wasmtime · constrained worker</strong></span><span>Resource caps <strong>memory + fuel</strong></span></div></div>`;
}

function renderQuarantineSurface(context) {
  const { workspace, branch, status, evidence } = context;
  const hasReviewEvidence = workspace.id === "ws-011" && String(evidence).includes("184 / 188");
  const title = branch ? branch.name : workspace.name.split(" · ")[0];
  const description = branch?.mode === "quarantine"
    ? "Fresh quarantine VM; only explicitly assigned inputs are available and no workspace files are shared implicitly."
    : branch?.mode === "cow"
      ? "Copy-on-write state branch inside this existing quarantine guest-kernel domain."
      : workspace.id === "ws-011"
        ? "Unreviewed build scripts are held outside Ark-Work for inspection."
        : workspace.description;
  const reportLabel = hasReviewEvidence ? "Review report" : "Boundary status";
  const checks = hasReviewEvidence ? "184 / 188" : "Not run";
  const policy = hasReviewEvidence ? "1 request held" : "No events recorded";
  const promotion = hasReviewEvidence ? "Blocked pending review" : "Not eligible · no evidence";
  return `<div class="quarantine-surface"><div class="quarantine-state"><span class="quarantine-shield">◇</span><div><span class="panel-kicker">SEPARATE GUEST-KERNEL DOMAIN · DEMO</span><h3>${escapeHtml(title)}</h3><p>${escapeHtml(description)}</p></div><span class="status-pill status-pill--warn">${escapeHtml(status)}</span></div>
    <div class="quarantine-metrics"><div><span>Network</span><strong>None</strong></div><div><span>Guest kernel</span><strong>Separate</strong></div><div><span>Secrets</span><strong>Not mounted</strong></div></div>
    <div class="quarantine-report"><div class="report-title"><span class="status-dot status-dot--amber"></span>${reportLabel} <small>${hasReviewEvidence ? "DEMO FIXTURE" : "NO RUN"}</small></div><div class="report-row"><span>Build + test checks</span><strong>${checks}</strong></div><div class="report-row"><span>Policy events</span><strong>${policy}</strong></div><div class="report-row"><span>Promotion</span><strong>${promotion}</strong></div></div>
    <div class="quarantine-footnote">A separate guest kernel raises isolation between trust domains. The VMM and host-side channels are still part of the security boundary.</div></div>`;
}

function renderWorkspaceSurface(context) {
  const { workspace, branch, tier } = context;
  if (tier === "quarantine") return renderQuarantineSurface(context);
  if (workspace.id === "ws-013") return renderOpenCutSurface();
  if (tier === "wasi") return renderWasiSurface(workspace);
  return renderTerminalSurface(workspace);
}

function getWorkspaceCheckpoints(context = getSelectedWorkspaceContext()) {
  if (context.workspace.id === "ws-013" && context.tier === "ark") return state.checkpoints;
  if (context.tier === "quarantine") {
    const hasEvidence = !["No run yet", "Pending", "No evidence yet"].includes(context.evidence);
    const sourceAssigned = context.workspace.id === "ws-011" && context.branch?.mode !== "quarantine";
    const currentLabel = context.status === "Needs review" ? "Review gate pending" : context.status === "Queued" ? "No execution started" : "Current quarantine state";
    return [
      { name: context.branch?.mode === "quarantine" ? "Fresh guest kernel created" : "Quarantine context opened", time: "09:08", evidence: "Separate quarantine domain" },
      { name: sourceAssigned ? "Source state assigned" : "Input assignment pending", time: "09:16", evidence: sourceAssigned ? "Explicit input bundle" : "No implicit file sharing" },
      { name: hasEvidence ? "Build + test completed" : "No build run yet", time: "09:31", evidence: context.evidence },
      { name: currentLabel, time: "09:42", evidence: context.status }
    ];
  }
  if (context.workspace.id === "ws-014") {
    return [
      { name: "Workspace opened", time: "09:08", evidence: "Initial operator state" },
      { name: "Reference corpus pinned", time: "09:16", evidence: "Read-only fixtures" },
      { name: "Verifier attached", time: "09:31", evidence: context.verifier },
      { name: "Current working tree", time: "09:42", evidence: context.evidence }
    ];
  }
  if (context.workspace.id === "ws-012") {
    return [
      { name: "Render workspace queued", time: "09:08", evidence: "No execution started" },
      { name: "Input edit selected", time: "09:16", evidence: "Workspace link" },
      { name: "Duration verifier attached", time: "09:31", evidence: "Target ≤ 60 seconds" },
      { name: "Awaiting verified edit", time: "09:42", evidence: "No render allocated" }
    ];
  }
  return [
    { name: "Workspace created", time: "09:08", evidence: "Initial state" },
    { name: "Execution boundary selected", time: "09:16", evidence: tierInfo[context.tier].domain },
    { name: "Verifier context attached", time: "09:31", evidence: context.verifier },
    { name: "Current working tree", time: "09:42", evidence: context.evidence }
  ];
}

function renderCheckpoint() {
  const slider = document.getElementById("checkpoint-range");
  const checkpoints = getWorkspaceCheckpoints();
  const max = checkpoints.length - 1;
  state.checkpointIndex = Math.max(0, Math.min(max, Number(state.checkpointIndex) || 0));
  slider.max = String(max);
  slider.value = String(state.checkpointIndex);
  const checkpoint = checkpoints[state.checkpointIndex];
  document.getElementById("checkpoint-name").textContent = checkpoint.name;
  document.getElementById("checkpoint-time").textContent = checkpoint.time;
  document.getElementById("checkpoint-evidence").textContent = checkpoint.evidence;
  document.getElementById("checkpoint-markers").innerHTML = checkpoints.map((item, index) => `<button class="checkpoint-marker ${index === state.checkpointIndex ? "is-current" : ""}" type="button" data-action="select-checkpoint" data-index="${index}" aria-label="Select checkpoint ${escapeHtml(item.name)}"><span></span><small>${escapeHtml(item.time)}</small></button>`).join("");
}

function renderWorkspaceInspector(context) {
  const { workspace, branch, tier, network, domain, verifier, evidence } = context;
  document.querySelectorAll("[data-inspector-tab]").forEach((tab) => {
    const selected = tab.dataset.inspectorTab === state.inspectorTab;
    tab.classList.toggle("is-selected", selected);
    tab.setAttribute("aria-selected", String(selected));
  });
  const content = document.getElementById("workspace-inspector-content");
  if (state.inspectorTab === "evidence") {
    content.innerHTML = `<div class="inspector-data-kicker">VERIFIER · DEMO RESULT</div><strong class="inspector-evidence-value">${escapeHtml(evidence)}</strong><p>${escapeHtml(verifier)} · current state: ${escapeHtml(context.status)}</p><div class="evidence-mini-row"><span>Independent re-check</span><strong>${workspace.id === "ws-014" && !branch ? "48 / 50 agree" : "Pending"}</strong></div><div class="inspector-caution">A demo result is not a real proof. A passing claim should be independently re-verified before promotion.</div>`;
  } else if (state.inspectorTab === "egress") {
    const hasEgress = network === "Brokered";
    content.innerHTML = `<div class="egress-summary"><span class="egress-indicator ${hasEgress ? "is-brokered" : ""}"></span><div><strong>${hasEgress ? "Brokered egress" : "No network capability"}</strong><small>${hasEgress ? "Only supervisor-approved destinations" : "No interface or broker socket in this demo cell"}</small></div></div><div class="egress-event ${hasEgress ? "egress-event--allow" : "egress-event--blocked"}"><span>${hasEgress ? "ALLOW · MOCK" : "DENY · MOCK"}</span><strong>${hasEgress ? "registry endpoint via broker" : "host-local path blocked"}</strong><small>${hasEgress ? "Cell identity supplied by broker channel" : "No direct host or LAN route"}</small></div><div class="inspector-caution">Sample policy display only; there is no live router, IP address, or packet capture.</div>`;
  } else {
    const workspaceEvents = state.workspaceActivity.filter((item) => !item.workspaceId || item.workspaceId === workspace.id);
    const rows = workspaceEvents.length ? workspaceEvents : [
      { time: "09:42", title: `${context.status} · ${workspace.name.split(" · ")[0]}`, detail: workspace.description, kind: tier === "quarantine" ? "amber" : "mint" },
      { time: "09:39", title: "Verifier context attached", detail: `${verifier} · ${evidence}`, kind: "blue" },
      { time: "09:31", title: "Execution boundary selected", detail: `${tierInfo[tier].label} · ${domain}`, kind: tier === "quarantine" ? "amber" : "mint" }
    ];
    content.innerHTML = `<div class="mini-activity-list">${rows.slice(0, 3).map((item) => `<div class="mini-activity"><span class="mini-activity-dot mini-activity-dot--${escapeHtml(item.kind)}"></span><div><strong>${escapeHtml(item.title)}</strong><small>${escapeHtml(item.time)} · ${escapeHtml(item.detail)}</small></div></div>`).join("")}</div>${branch ? `<div class="branch-context"><span>Selected branch</span><strong>${escapeHtml(branch.name)}</strong><small>${escapeHtml(branch.summary || "Fresh quarantine VM · pending")}</small></div>` : ""}`;
  }
}

function renderWorkspaceShell() {
  const context = getSelectedWorkspaceContext();
  const { workspace, branch, tier, network, memory, domain, verifier, evidence, status } = context;
  const tierLabels = { wasi: "WASI MODULE", ark: "ARK CELL", quarantine: "QUARANTINE VM" };
  const cellLabels = {
    wasi: "WASI pool · Module 01",
    ark: branch ? "Ark-Work · same guest kernel" : "Ark-Work · Cell 04",
    quarantine: branch?.mode === "quarantine" ? "Ark-Quarantine · fresh guest kernel" : branch ? "Ark-Quarantine · same guest kernel" : "Ark-Quarantine · Fresh VM"
  };
  const isVideoProject = workspace.id === "ws-013" || workspace.id === "ws-012";
  const contextLabel = branch?.mode === "quarantine" || tier === "quarantine"
    ? "QUARANTINE"
    : isVideoProject
      ? "VIDEO PROJECT"
      : workspace.id === "ws-014"
        ? "BENCH & TOOLS"
        : tier === "ark" ? "ARK-WORK" : "WASI WORKER";
  const taskLabel = workspace.id === "ws-013" ? "ACTIVE GOAL · TASK 08"
    : workspace.id === "ws-014" ? "ACTIVE GOAL · TASK 14"
      : workspace.id === "ws-012" ? "ACTIVE GOAL · TASK 12"
        : workspace.id === "ws-011" ? "ACTIVE GOAL · TASK 11" : "ACTIVE GOAL · UNASSIGNED";
  const displayName = branch ? `${workspace.name.split(" · ")[0]} / ${branch.name}` : workspace.name;
  const heartbeatStatus = status === "Running" ? "Working · simulated"
    : status === "Needs review" ? "Paused at review gate · simulated"
      : status === "Queued" ? "No execution started · simulated" : `${status} · simulated`;
  const trustZone = tier === "quarantine" ? "Ark-Quarantine separate guest kernel"
    : tier === "ark" ? "Ark-Work shared guest kernel" : "WASI capability-limited worker";
  const scopePath = branch?.mode === "quarantine" ? "/quarantine/input (explicit only)"
    : tier === "quarantine" ? "/workspace/source (quarantine)"
      : workspace.id === "ws-013" || workspace.id === "ws-012" ? "/workspace/media"
        : workspace.id === "ws-014" ? "/fixtures/reference" : "/workspace";

  document.getElementById("workspace-context-label").textContent = contextLabel;
  document.getElementById("workspace-task-label").textContent = taskLabel;
  document.getElementById("active-surface-name").textContent = displayName;
  document.getElementById("active-surface-subtitle").textContent = `${tierInfo[tier].label} · ${workspace.id} · ${status}`;
  document.getElementById("app-glyph").textContent = tier === "quarantine" ? "◇" : workspace.id === "ws-013" ? "OC" : tier === "wasi" ? "λ" : "A";
  const executionChip = document.getElementById("active-execution-chip");
  executionChip.className = `execution-chip execution-chip--${tier}`;
  executionChip.textContent = tierLabels[tier];
  document.getElementById("active-domain-label").textContent = cellLabels[tier];
  const networkLabel = document.getElementById("active-network-label");
  networkLabel.innerHTML = `<i class="${network === "Brokered" ? "is-brokered" : ""}"></i> ${network === "Brokered" ? "Brokered egress" : "Network none"}`;
  document.getElementById("surface-state-label").textContent = tier === "quarantine" ? "Disposable trust domain · demo only" : "Workspace-local surface · demo only";
  document.getElementById("surface-memory-label").textContent = `Demo memory ${memory}`;
  document.getElementById("app-surface").innerHTML = renderWorkspaceSurface(context);
  document.getElementById("heartbeat-number").textContent = "#12";
  document.getElementById("heartbeat-status").textContent = heartbeatStatus;
  document.getElementById("workspace-status-heartbeat").textContent = "Heartbeat #12";
  document.getElementById("workspace-status-heartbeat-state").textContent = heartbeatStatus;
  document.getElementById("workspace-trust-zone").textContent = trustZone;
  document.getElementById("workspace-scope-path").textContent = scopePath;

  const goal = branch?.mode === "quarantine" ? "Review explicitly assigned inputs in a fresh quarantine VM"
    : workspace.id === "ws-013" ? "Assemble a 60-second launch cut from local clips"
      : workspace.id === "ws-014" ? "Port the parser and verify output equivalence"
        : workspace.id === "ws-012" ? "Render only after the edit passes its duration check"
          : workspace.id === "ws-011" ? "Inspect unreviewed source inside quarantine"
            : tier === "quarantine" ? "Inspect source and evidence inside the quarantine domain"
              : `Define a bounded goal for ${workspace.name.split(" · ")[0]}`;
  document.getElementById("workspace-goal-title").textContent = goal;

  const task = branch?.mode === "quarantine" ? "Review the explicitly assigned input bundle; no workspace files are shared implicitly."
    : workspace.id === "ws-013" ? "Build the first cut, then verify its duration and clip order."
      : workspace.id === "ws-014" ? "Compare candidate outputs with the pinned reference corpus."
        : workspace.id === "ws-012" ? "Wait for a verified timeline before starting the render."
          : workspace.id === "ws-011" ? "Review build failures and policy events; do not promote yet."
            : tier === "quarantine" ? "No task is running. Inspect inputs and assign a verifier before execution."
              : "No task is running. Define a bounded instruction and verifier before execution.";
  document.getElementById("agent-task-copy").textContent = task;

  const lastClaim = branch?.mode === "quarantine" ? "Fresh guest-kernel context; no workspace files were shared implicitly."
    : branch?.mode === "cow" ? `CoW state branch; execution remains in the ${domain} trust domain.`
      : workspace.id === "ws-013" ? "Two clips imported; timeline still needs an independent duration check."
        : `${verifier}: ${evidence}.`;
  document.getElementById("agent-last-claim").textContent = lastClaim;
  const progress = branch?.mode === "quarantine" ? { label: "0", total: "1", width: "0%" }
    : workspace.id === "ws-013" ? { label: "3", total: "5", width: "60%" }
      : workspace.id === "ws-014" ? { label: "4", total: "6", width: "66%" }
        : workspace.id === "ws-011" ? { label: "1", total: "4", width: "25%" }
          : workspace.id === "ws-012" ? { label: "2", total: "5", width: "40%" }
            : { label: "0", total: "1", width: "0%" };
  document.getElementById("agent-progress-label").innerHTML = `${progress.label} <i>/ ${progress.total} steps</i>`;
  document.querySelector(".agent-progress > span").style.width = progress.width;

  const proposalCopy = document.getElementById("proposal-copy");
  const proposalTarget = document.getElementById("proposal-target");
  const proposalEffect = document.getElementById("proposal-effect");
  if (branch?.mode === "quarantine") {
    proposalCopy.innerHTML = "Review the <strong>explicit input bundle</strong> in the fresh quarantine domain before considering any next step.";
    proposalTarget.textContent = "Quarantine VM · input review";
  } else if (tier === "quarantine" && workspace.id === "ws-011") {
    proposalCopy.innerHTML = "Inspect the <strong>four failing checks</strong> and keep this workspace quarantined until the evidence is reviewed.";
    proposalTarget.textContent = "Quarantine review · build report";
  } else if (tier === "quarantine") {
    proposalCopy.innerHTML = "Inspect the assigned source and verifier state; do not promote this quarantine workspace without human review.";
    proposalTarget.textContent = "Quarantine workspace · review";
  } else if (workspace.id === "ws-014") {
    proposalCopy.innerHTML = "Compare candidate output at <code>case 8,431</code> before promoting the parser operator.";
    proposalTarget.textContent = "CSV parser · candidate output";
  } else if (workspace.id === "ws-012") {
    proposalCopy.innerHTML = "Wait for the <strong>timeline verifier</strong> before allocating render memory.";
    proposalTarget.textContent = "ffmpeg render · output artifact";
  } else if (workspace.id === "ws-013") {
    proposalCopy.innerHTML = "Split <strong>Clip_02.mp4</strong> at <code>00:08</code> to keep the launch beat under the target duration.";
    proposalTarget.textContent = "OpenCut · timeline";
  } else {
    proposalCopy.textContent = "No proposed action is queued. Define a bounded task and verifier before execution.";
    proposalTarget.textContent = displayName;
  }
  proposalEffect.textContent = tier === "quarantine" ? "inspection only · no egress" : tier === "wasi" ? "operator-local" : "workspace-local";
  const previewButton = document.getElementById("proposal-preview-button");
  const previewVisible = state.proposalPreviewed && workspace.id === "ws-013" && tier === "ark";
  previewButton.textContent = previewVisible ? "Preview shown" : "Preview in demo";
  previewButton.setAttribute("aria-pressed", String(previewVisible));
  renderWorkspaceTree();
  renderWorkspaceInspector(context);
  renderCheckpoint();
}

function renderAll() {
  renderOverview();
  renderWorkspaces();
  renderOperators();
  renderVerifiers();
  renderLedger();
  renderBlockers();
  renderWorkspaceShell();
}

function navigate(view) {
  if (!viewNames[view]) return;
  document.querySelectorAll("[data-view-panel]").forEach((panel) => {
    const active = panel.dataset.viewPanel === view;
    panel.hidden = !active;
    panel.classList.toggle("is-visible", active);
  });
  document.querySelectorAll(".nav-link[data-view]").forEach((button) => {
    const active = button.dataset.view === view;
    button.classList.toggle("is-active", active);
    if (active) button.setAttribute("aria-current", "page");
    else button.removeAttribute("aria-current");
  });
  document.getElementById("breadcrumb-current").textContent = viewNames[view];
  document.getElementById("sidebar").classList.remove("is-open");
  document.querySelector(".app-shell").classList.toggle("is-workspace-mode", view === "workspace");
  window.location.hash = view === "workspace" ? "workspace" : view;
}

function showToast(message) {
  const toast = document.getElementById("toast");
  toast.textContent = message;
  toast.classList.add("is-visible");
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => toast.classList.remove("is-visible"), 2800);
}

function openDialog(id) {
  const dialog = document.getElementById(id);
  if (!dialog) return;
  if (typeof dialog.showModal === "function") dialog.showModal();
  else dialog.setAttribute("open", "");
  if (id === "workspace-dialog") window.setTimeout(() => document.getElementById("workspace-name").focus(), 30);
  if (id === "run-dialog") window.setTimeout(() => document.getElementById("run-name").focus(), 30);
  if (id === "branch-dialog") window.setTimeout(() => document.getElementById("branch-name").focus(), 30);
}

function closeDialog(dialog) {
  if (!dialog) return;
  if (typeof dialog.close === "function") dialog.close();
  else dialog.removeAttribute("open");
}

function showDetail(kicker, title, body) {
  document.getElementById("detail-kicker").textContent = kicker;
  document.getElementById("detail-title").textContent = title;
  document.getElementById("detail-body").innerHTML = body;
  openDialog("detail-dialog");
}

function openWorkspace(id) {
  const workspace = state.workspaces.find((item) => item.id === id);
  if (!workspace) return;
  const tier = tierInfo[workspace.tier];
  const caution = workspace.tier === "ark"
    ? `<div class="detail-caution"><strong>Shared-kernel warning:</strong> ${escapeHtml(tier.caution)}</div>`
    : `<div class="detail-caution">${escapeHtml(tier.caution)}</div>`;
  showDetail("WORKSPACE · DEMO STATE", workspace.name, `
    <p>${escapeHtml(workspace.description)}</p>
    <div class="detail-list">
      <div class="detail-row"><span>Execution tier</span><strong>${escapeHtml(tier.label)}</strong></div>
      <div class="detail-row"><span>Trust domain</span><strong>${escapeHtml(tier.domain)}</strong></div>
      <div class="detail-row"><span>Network</span><strong>${escapeHtml(workspace.network)}</strong></div>
      <div class="detail-row"><span>Workspace state</span><strong>${escapeHtml(workspace.status)}</strong></div>
      <div class="detail-row"><span>Demo memory</span><strong>${escapeHtml(workspace.memory)}</strong></div>
      <div class="detail-row"><span>Verifier evidence</span><strong>${escapeHtml(workspace.evidence || "Pending")}</strong></div>
    </div>
    ${caution}
    <p class="detail-demo-note">All details are illustrative. This prototype does not launch or inspect a real process.</p>
  `);
}

function openRun(id) {
  const run = state.runs.find((item) => item.id === id);
  if (!run) return;
  const tier = tierInfo[run.tier];
  showDetail("RUN · DEMO EVIDENCE", run.title, `
    <p>${escapeHtml(run.detail)}</p>
    <div class="detail-list">
      <div class="detail-row"><span>Execution tier</span><strong>${escapeHtml(tier.label)}</strong></div>
      <div class="detail-row"><span>Trust domain</span><strong>${escapeHtml(tier.domain)}</strong></div>
      <div class="detail-row"><span>Candidate population</span><strong>${escapeHtml(run.candidates)}</strong></div>
      <div class="detail-row"><span>Verifier</span><strong>${escapeHtml(run.verifier)}</strong></div>
      <div class="detail-row"><span>Evidence</span><strong>${escapeHtml(run.evidence)}</strong></div>
      <div class="detail-row"><span>Status</span><strong>${escapeHtml(run.status)}</strong></div>
    </div>
    <div class="detail-caution">A passing demo result is not proof of a real outcome. The intended supervisor owns verifiers and evidence.</div>
  `);
}

function openBlocker(id) {
  const blocker = state.blockers.find((item) => item.id === id);
  if (!blocker) return;
  showDetail(`BLOCKER · ${blocker.id}`, blocker.title, `
    <p>${escapeHtml(blocker.detail)}</p>
    <div class="detail-list">
      <div class="detail-row"><span>Workspace / task</span><strong>${escapeHtml(blocker.target)}</strong></div>
      <div class="detail-row"><span>Category</span><strong>${escapeHtml(blocker.category)}</strong></div>
      <div class="detail-row"><span>State</span><strong>${escapeHtml(blocker.status)}</strong></div>
      <div class="detail-row"><span>Age</span><strong>${escapeHtml(blocker.age)}</strong></div>
    </div>
    <div class="detail-caution">Inspecting or marking this item reviewed only changes prototype state. It does not approve egress, credentials, spending, or an external action.</div>
  `);
}

function openOperator(id) {
  const operator = state.operators.find((item) => item.id === id);
  if (!operator) return;
  showDetail("OPERATOR · REGISTRY ENTRY", operator.name, `
    <p>${escapeHtml(operator.subtitle)}</p>
    <div class="detail-list">
      <div class="detail-row"><span>Runtime</span><strong>${escapeHtml(tierInfo[operator.tier].label)}</strong></div>
      <div class="detail-row"><span>Effect scope</span><strong>${escapeHtml(operator.scope)}</strong></div>
      <div class="detail-row"><span>Verifier</span><strong>${escapeHtml(operator.verifier)}</strong></div>
      <div class="detail-row"><span>Recent checks</span><strong>${escapeHtml(operator.checks)}</strong></div>
      <div class="detail-row"><span>Health</span><strong>${escapeHtml(operator.health)}</strong></div>
    </div>
    <div class="detail-caution">Registry health and promotion are mock state. A real operator should be quarantined on drift and re-enabled only after independent verification.</div>
  `);
}

function selectWorkspace(id) {
  if (!state.workspaces.some((workspace) => workspace.id === id)) return;
  state.selectedWorkspaceId = id;
  state.selectedBranchId = null;
  state.checkpointIndex = 3;
  state.proposalPreviewed = false;
  renderWorkspaceShell();
  navigate("workspace");
}

function selectBranch(id) {
  const branch = state.branches.find((item) => item.id === id);
  if (!branch) return;
  state.selectedWorkspaceId = branch.parentId;
  state.selectedBranchId = branch.id;
  state.checkpointIndex = 3;
  state.proposalPreviewed = false;
  renderWorkspaceShell();
  navigate("workspace");
}

function updateBranchExplainer() {
  const mode = document.getElementById("branch-mode").value;
  const context = getSelectedWorkspaceContext();
  const explainer = document.getElementById("branch-explainer");
  const isQuarantine = mode === "quarantine";
  const cowOption = document.querySelector('#branch-mode option[value="cow"]');
  cowOption.textContent = context.tier === "ark"
    ? "Workspace CoW branch · same Ark kernel"
    : context.tier === "quarantine"
      ? "Workspace CoW branch · same quarantine kernel"
      : "Workspace data CoW branch · WASI worker tier";
  explainer.classList.toggle("tier-explainer--amber", isQuarantine || context.tier !== "wasi");
  explainer.classList.toggle("tier-explainer--mint", !isQuarantine && context.tier === "wasi");
  explainer.querySelector(".info-strip-icon").textContent = isQuarantine || context.tier !== "wasi" ? "!" : "i";
  const copy = isQuarantine
    ? "Start a fresh quarantine VM with a separate guest kernel. Higher memory cost; workspace files are not implicitly shared and network stays off by default."
    : context.tier === "ark"
      ? "Create a copy-on-write workspace branch inside the current Ark. It shares the Ark guest kernel; this is a filesystem/state branch, not a new kernel boundary."
      : context.tier === "quarantine"
        ? "Create a copy-on-write state branch inside the current quarantine VM. It keeps the existing guest kernel and does not create a new isolation boundary."
        : "Create a copy-on-write workspace data branch. Execution remains in the WASI worker tier; this does not create a VM or guest-kernel boundary.";
  explainer.querySelector("p").textContent = copy;
  const kicker = document.getElementById("branch-dialog-kicker");
  const warning = document.getElementById("branch-warning-copy");
  kicker.textContent = isQuarantine
    ? "FRESH QUARANTINE VM · SEPARATE GUEST KERNEL"
    : context.tier === "ark" ? "COPY-ON-WRITE · SAME ARK KERNEL"
      : context.tier === "quarantine" ? "COPY-ON-WRITE · SAME QUARANTINE KERNEL"
        : "COPY-ON-WRITE · WASI EXECUTION";
  warning.textContent = isQuarantine
    ? "Demo only. This records a fresh quarantine-VM boundary; no VM is launched and no workspace files are shared implicitly."
    : context.tier === "ark" ? "Demo only. The CoW state branch stays in the same Ark guest-kernel domain; no guest-RAM snapshot or rollback is performed."
      : context.tier === "quarantine" ? "Demo only. This CoW branch stays in the same quarantine guest-kernel domain; no VM snapshot or rollback is performed."
        : "Demo only. This CoW data branch stays in the WASI worker tier; it does not create a VM or guest-kernel boundary.";
}

function createBranch(formData) {
  const source = getSelectedWorkspaceContext();
  const parent = source.workspace;
  const name = String(formData.get("name") || "").trim();
  const mode = formData.get("mode") === "quarantine" ? "quarantine" : "cow";
  if (!parent || !name) return;
  let sequence = state.branches.filter((branch) => branch.parentId === parent.id).length + 1;
  let id = `br-${parent.id.slice(3)}-${String(sequence).padStart(2, "0")}`;
  while (state.branches.some((branch) => branch.id === id)) {
    sequence += 1;
    id = `br-${parent.id.slice(3)}-${String(sequence).padStart(2, "0")}`;
  }
  const executionTier = mode === "quarantine" ? "quarantine" : source.tier;
  const domain = mode === "quarantine" ? "Ark-Quarantine" : source.domain;
  const sameDomainLabel = executionTier === "ark" ? "same Ark kernel" : executionTier === "quarantine" ? "same quarantine guest kernel" : "WASI worker tier";
  const branch = {
    id,
    parentId: parent.id,
    sourceBranchId: source.branch?.id || null,
    name,
    mode,
    executionTier,
    domain,
    network: mode === "quarantine" ? "None" : source.network,
    status: mode === "quarantine" ? "Queued" : source.status,
    memory: mode === "quarantine" ? "Not allocated" : source.memory,
    verifier: mode === "quarantine" ? "Not assigned" : source.verifier,
    evidence: mode === "quarantine" ? "No run yet" : source.evidence,
    summary: mode === "cow" ? `CoW overlay · ${sameDomainLabel}` : "Fresh VM · separate guest kernel",
    updated: "Just now"
  };
  state.branches.unshift(branch);
  state.selectedBranchId = id;
  state.checkpointIndex = 3;
  state.proposalPreviewed = false;
  state.workspaceActivity.unshift({
    time: "Just now",
    title: `${mode === "cow" ? "CoW branch" : "Quarantine branch"} created`,
    detail: mode === "cow" ? `${name} · filesystem state only; ${sameDomainLabel}.` : `${name} · fresh guest kernel; workspace files are not implicitly shared.`,
    kind: mode === "cow" ? "mint" : "amber",
    workspaceId: parent.id
  });
  state.workspaceActivity = state.workspaceActivity.slice(0, 8);
  appendLedger("Workspace branch added in demo", `${name} · ${mode === "cow" ? `CoW / ${sameDomainLabel}` : "fresh quarantine VM / separate guest kernel"}`, "Session");
  renderAll();
  closeDialog(document.getElementById("branch-dialog"));
  document.getElementById("branch-form").reset();
  updateBranchExplainer();
  navigate("workspace");
  const message = mode === "quarantine"
    ? "Fresh quarantine branch added; no VM was actually created."
    : executionTier === "ark"
      ? "CoW branch added; it remains in the same Ark guest-kernel domain."
      : executionTier === "quarantine"
        ? "CoW branch added inside the same quarantine guest-kernel domain."
        : "CoW data branch added; no VM or guest-kernel boundary was created.";
  showToast(message);
}

function inspectSelectedBoundary() {
  const context = getSelectedWorkspaceContext();
  const tier = tierInfo[context.tier];
  const description = context.branch?.mode === "quarantine"
    ? "A fresh quarantine VM with a separate guest kernel. Only explicitly assigned inputs are available; no workspace files are shared implicitly."
    : context.branch?.mode === "cow"
      ? `A copy-on-write state branch. Execution remains in the ${context.domain} trust domain.`
      : context.workspace.description;
  showDetail("EXECUTION BOUNDARY · DEMO", tier.label, `
    <p>${escapeHtml(description)}</p>
    <div class="detail-list">
      <div class="detail-row"><span>Trust domain</span><strong>${escapeHtml(context.domain)}</strong></div>
      <div class="detail-row"><span>Network</span><strong>${escapeHtml(context.network)}</strong></div>
      <div class="detail-row"><span>Memory</span><strong>${escapeHtml(context.memory)}</strong></div>
      <div class="detail-row"><span>Branch</span><strong>${escapeHtml(context.branch ? context.branch.name : "Base workspace")}</strong></div>
    </div>
    <div class="detail-caution">${escapeHtml(tier.caution)} This is a UI illustration; no runtime policy is enforced here.</div>
  `);
}

function inspectProposal() {
  const context = getSelectedWorkspaceContext();
  const proposal = document.getElementById("proposal-copy").textContent;
  const target = context.branch ? `${context.workspace.name.split(" · ")[0]} / ${context.branch.name}` : context.workspace.name;
  showDetail("PROPOSED ACTION · INSPECTION", "Review before apply", `
    <p>${escapeHtml(proposal)}</p>
    <div class="detail-list">
      <div class="detail-row"><span>Target</span><strong>${escapeHtml(target)}</strong></div>
      <div class="detail-row"><span>Execution tier</span><strong>${escapeHtml(tierInfo[context.tier].label)}</strong></div>
      <div class="detail-row"><span>Current status</span><strong>Awaiting human review</strong></div>
    </div>
    <div class="detail-caution">Inspecting is safe; this prototype never sends clicks or commands to the mock application. Real external or destructive effects should be bounded by policy and verified.</div>
  `);
}

function updateTierExplainer() {
  const tier = document.getElementById("workspace-tier").value;
  const network = document.getElementById("workspace-network").value;
  const explainer = document.getElementById("tier-explainer");
  const descriptions = {
    wasi: "Capability-limited module. Use for a small operator that fits the WASI interface; runtime and host calls remain trusted components.",
    ark: "Shares the Ark-Work guest kernel with other cells. Suitable only when those workloads may share a kernel-compromise domain; keep unknown build scripts in quarantine.",
    quarantine: "Separate guest kernel for this trust domain. Higher memory cost; discard the VM after the job. The VMM and host communication path remain attack surfaces."
  };
  explainer.classList.toggle("tier-explainer--amber", tier !== "wasi");
  explainer.classList.toggle("tier-explainer--mint", tier === "wasi");
  const prefix = tier === "ark" ? "!" : "i";
  const networkNote = network === "brokered" ? " Brokered egress is still a controlled host-side capability, not direct access." : " Network is set to none in this demo.";
  explainer.querySelector(".info-strip-icon").textContent = prefix;
  explainer.querySelector("p").textContent = descriptions[tier] + networkNote;
}

function addWorkspace(formData) {
  const name = String(formData.get("name") || "").trim();
  const tier = String(formData.get("tier") || "wasi");
  const network = formData.get("network") === "brokered" ? "Brokered" : "None";
  const memory = `${formData.get("memory")} MB`;
  if (!name || !tierInfo[tier]) return;
  const id = `ws-${String(Math.max(0, ...state.workspaces.map((workspace) => Number(workspace.id.split("-")[1]) || 0)) + 1).padStart(3, "0")}`;
  state.workspaces.unshift({
    id,
    name,
    description: "Newly created demo workspace; no process has been launched.",
    tier,
    status: "Queued",
    network,
    memory,
    updated: "Just now",
    verifier: "Not assigned",
    evidence: "No run yet"
  });
  state.selectedWorkspaceId = id;
  state.selectedBranchId = null;
  state.checkpointIndex = 3;
  state.proposalPreviewed = false;
  state.workspaceActivity.unshift({ time: "Just now", title: "Workspace added to demo state", detail: `${name} · ${tierInfo[tier].label} · no execution started.`, kind: "blue", workspaceId: id });
  state.workspaceActivity = state.workspaceActivity.slice(0, 8);
  addActivity(`<strong>${escapeHtml(name)}</strong> added to the demo workspace list (${escapeHtml(tierInfo[tier].short)}).`, "blue");
  appendLedger("Workspace added to demo state", `${name} · ${tierInfo[tier].label}`, "Session");
  renderAll();
  closeDialog(document.getElementById("workspace-dialog"));
  document.getElementById("workspace-form").reset();
  updateTierExplainer();
  navigate("workspace");
  showToast("Workspace added to this prototype session. No process was launched.");
}

function addRun(formData) {
  const name = String(formData.get("name") || "").trim();
  const tier = String(formData.get("tier") || "wasi");
  const candidates = String(formData.get("candidates") || "8");
  const verifier = String(formData.get("verifier") || "Differential equality");
  if (!name || !tierInfo[tier]) return;
  const id = `run-${String(Math.max(100, ...state.runs.map((run) => Number(run.id.split("-")[1]) || 0)) + 1)}`;
  state.runs.unshift({
    id,
    title: name,
    detail: "New demo experiment. Awaiting a real backend and verifier integration.",
    tier,
    candidates: `${candidates} candidate${candidates === "1" ? "" : "s"}`,
    verifier,
    evidence: "Not run · no proof",
    status: "Queued",
    updated: "Just now",
    memory: "Not allocated"
  });
  addActivity(`<strong>${escapeHtml(name)}</strong> queued with ${candidates} candidate${candidates === "1" ? "" : "s"}; no execution started.`, "blue");
  appendLedger("Experiment queued in demo state", `${name} · ${tierInfo[tier].label} · ${candidates} candidates`, "Session");
  renderAll();
  closeDialog(document.getElementById("run-dialog"));
  document.getElementById("run-form").reset();
  navigate("runs");
  showToast("Experiment added to the local demo queue. Nothing was executed.");
}

function submitSteering(text) {
  const note = String(text || "").trim();
  if (!note) return false;
  addActivity(`<strong>Steering note queued:</strong> ${escapeHtml(note)}`, "blue");
  state.workspaceActivity.unshift({ time: "Just now", title: "Steering note queued", detail: note, kind: "blue", workspaceId: state.selectedWorkspaceId });
  state.workspaceActivity = state.workspaceActivity.slice(0, 8);
  appendLedger("Steering note added", note, "Human input");
  renderAll();
  showToast("Steering note recorded in demo state only.");
  return true;
}

function resolveBlocker(id) {
  const blocker = state.blockers.find((item) => item.id === id && item.status === "Open");
  if (!blocker) return;
  blocker.status = "Reviewed";
  blocker.age = "Just now";
  addActivity(`<strong>${escapeHtml(blocker.title)}</strong> marked reviewed in the local prototype.`, "amber");
  appendLedger("Blocker marked reviewed in demo", `${blocker.id} · ${blocker.title}`, "Human input");
  renderAll();
  showToast("Marked reviewed locally. No policy or external action changed.");
}

function showAbout() {
  showDetail("PROTOTYPE NOTES", "What this UI is modeling", `
    <p>This is an HTML-first concept for Spacetime's control plane. It uses local mock data and browser-only interactions.</p>
    <div class="detail-list">
      <div class="detail-row"><span>WASI worker</span><strong>Narrow, capability-limited operators</strong></div>
      <div class="detail-row"><span>Shared Ark cell</span><strong>Many cells may share one guest kernel</strong></div>
      <div class="detail-row"><span>Quarantine VM</span><strong>Separate guest-kernel trust domain</strong></div>
      <div class="detail-row"><span>Verification</span><strong>Claims need independent evidence</strong></div>
      <div class="detail-row"><span>Human control</span><strong>Blockers and steering stay visible</strong></div>
    </div>
    <div class="detail-caution">No Rust has been written for this prototype. No VM, WASI module, model, network proxy, policy engine, or cryptographic ledger is connected.</div>
  `);
}

function exportLedger() {
  const payload = {
    notice: "Illustrative prototype data. Hash values are not cryptographic proofs.",
    exportedAt: new Date().toISOString(),
    events: state.ledger
  };
  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = "spacetime-demo-ledger.json";
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
  showToast("Downloaded illustrative demo ledger JSON.");
}

document.addEventListener("click", (event) => {
  const viewButton = event.target.closest("[data-view]");
  if (viewButton) {
    navigate(viewButton.dataset.view);
    return;
  }

  const openButton = event.target.closest("[data-open-dialog]");
  if (openButton) {
    openDialog(openButton.dataset.openDialog === "workspace" ? "workspace-dialog" : "run-dialog");
    return;
  }

  const filterButton = event.target.closest("[data-workspace-filter]");
  if (filterButton) {
    state.workspaceFilter = filterButton.dataset.workspaceFilter;
    document.querySelectorAll("[data-workspace-filter]").forEach((button) => button.classList.toggle("is-selected", button === filterButton));
    renderWorkspaces();
    return;
  }

  const inspectorTab = event.target.closest("[data-inspector-tab]");
  if (inspectorTab) {
    state.inspectorTab = inspectorTab.dataset.inspectorTab;
    renderWorkspaceInspector(getSelectedWorkspaceContext());
    return;
  }

  const actionButton = event.target.closest("[data-action]");
  if (!actionButton) return;
  const action = actionButton.dataset.action;
  const id = actionButton.dataset.id;

  if (action === "close-dialog") closeDialog(actionButton.closest("dialog"));
  if (action === "dismiss-banner") document.getElementById("prototype-banner").remove();
  if (action === "toggle-sidebar") document.getElementById("sidebar").classList.toggle("is-open");
  if (action === "show-about") showAbout();
  if (action === "open-workspace") openWorkspace(id);
  if (action === "open-run") openRun(id);
  if (action === "open-blocker") openBlocker(id);
  if (action === "open-operator") openOperator(id);
  if (action === "select-workspace") selectWorkspace(id);
  if (action === "select-branch") selectBranch(id);
  if (action === "inspect-boundary") inspectSelectedBoundary();
  if (action === "inspect-proposal") inspectProposal();
  if (action === "preview-proposal") {
    const context = getSelectedWorkspaceContext();
    state.proposalPreviewed = context.workspace.id === "ws-013" && context.tier === "ark";
    state.workspaceActivity.unshift({ time: "Just now", title: "Proposal previewed in UI", detail: "No app action or command was sent.", kind: "blue", workspaceId: context.workspace.id });
    state.workspaceActivity = state.workspaceActivity.slice(0, 8);
    renderWorkspaceShell();
    showToast("Visual preview only. No application action was executed.");
  }
  if (action === "editor-tool") showToast(`${actionButton.dataset.tool || "Tool"} selected in the mock editor; no files changed.`);
  if (action === "branch-workspace") {
    const context = getSelectedWorkspaceContext();
    const branchCount = state.branches.filter((branch) => branch.parentId === context.workspace.id).length;
    const branchLetter = String.fromCharCode(65 + (branchCount % 26));
    const sourceName = context.branch ? context.branch.name : context.workspace.name.split(" · ")[0];
    document.getElementById("branch-name").value = `${sourceName} · branch ${branchLetter}`;
    document.getElementById("branch-mode").value = "cow";
    updateBranchExplainer();
    openDialog("branch-dialog");
  }
  if (action === "select-checkpoint") {
    state.checkpointIndex = Number(actionButton.dataset.index);
    renderCheckpoint();
  }
  if (action === "resolve-blocker") resolveBlocker(id);
  if (action === "export-ledger") exportLedger();
  if (action === "refresh-runs") showToast("Demo queue refreshed. No runtime is connected.");
  if (action === "pause-workspace") {
    const workspace = state.workspaces.find((item) => item.id === id);
    if (workspace && workspace.status === "Running") {
      workspace.status = "Paused";
      addActivity(`<strong>${escapeHtml(workspace.name)}</strong> paused in the demo UI.`, "amber");
      appendLedger("Workspace paused in demo state", workspace.name, "Session");
      renderAll();
      showToast("Workspace paused in local demo state only.");
    }
  }
});

document.getElementById("workspace-form").addEventListener("submit", (event) => {
  event.preventDefault();
  addWorkspace(new FormData(event.currentTarget));
});

document.getElementById("run-form").addEventListener("submit", (event) => {
  event.preventDefault();
  addRun(new FormData(event.currentTarget));
});

document.getElementById("branch-form").addEventListener("submit", (event) => {
  event.preventDefault();
  createBranch(new FormData(event.currentTarget));
});

document.getElementById("branch-mode").addEventListener("change", updateBranchExplainer);
document.getElementById("checkpoint-range").addEventListener("input", (event) => {
  state.checkpointIndex = Number(event.target.value);
  renderCheckpoint();
});

document.getElementById("workspace-tier").addEventListener("change", updateTierExplainer);
document.getElementById("workspace-network").addEventListener("change", updateTierExplainer);
document.getElementById("workspace-search").addEventListener("input", renderWorkspaces);
document.getElementById("operator-search").addEventListener("input", renderOperators);
document.getElementById("ledger-search").addEventListener("input", renderLedger);

document.getElementById("workspace-steering-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const input = document.getElementById("workspace-steering-input");
  if (submitSteering(input.value)) input.value = "";
});

document.getElementById("steering-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const input = document.getElementById("steering-input");
  if (submitSteering(input.value)) input.value = "";
});

document.getElementById("blocker-steering-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const input = document.getElementById("blocker-steering-input");
  if (submitSteering(input.value)) {
    input.value = "";
    document.getElementById("steering-char-count").textContent = "0 / 300";
  }
});

document.getElementById("blocker-steering-input").addEventListener("input", (event) => {
  document.getElementById("steering-char-count").textContent = `${event.target.value.length} / 300`;
});

document.querySelectorAll("dialog").forEach((dialog) => {
  dialog.addEventListener("click", (event) => {
    const bounds = dialog.getBoundingClientRect();
    const inside = event.clientX >= bounds.left && event.clientX <= bounds.right && event.clientY >= bounds.top && event.clientY <= bounds.bottom;
    if (!inside) closeDialog(dialog);
  });
});

const initialView = window.location.hash.replace(/^#/, "");
renderAll();
if (viewNames[initialView]) navigate(initialView);
else navigate("workspace");
