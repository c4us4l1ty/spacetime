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
  workspaces: [
    { id: "ws-014", name: "Parser port · candidate set", description: "Differential checks against a pinned reference implementation.", tier: "wasi", status: "Running", network: "None", memory: "214 MB", updated: "2 min ago", verifier: "Differential equality", evidence: "12,480 / 12,480 inputs" },
    { id: "ws-013", name: "Benchmark harness · local", description: "Runs a bounded test suite against local fixtures.", tier: "ark", status: "Running", network: "None", memory: "386 MB", updated: "6 min ago", verifier: "Test suite", evidence: "26 / 26 checks" },
    { id: "ws-012", name: "Operator health sweep", description: "Checks a public registry endpoint through a cell-scoped egress broker.", tier: "ark", status: "Running", network: "Brokered", memory: "342 MB", updated: "11 min ago", verifier: "Trace replay", evidence: "18 / 19 checks" },
    { id: "ws-011", name: "repo-218 · build audit", description: "Unreviewed build scripts are held in a disposable quarantine context.", tier: "quarantine", status: "Needs review", network: "None", memory: "562 MB", updated: "14 min ago", verifier: "Build + test", evidence: "184 / 188 checks" }
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

function renderAll() {
  renderOverview();
  renderWorkspaces();
  renderOperators();
  renderVerifiers();
  renderLedger();
  renderBlockers();
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
  window.location.hash = view === "overview" ? "overview" : view;
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
  addActivity(`<strong>${escapeHtml(name)}</strong> added to the demo workspace list (${escapeHtml(tierInfo[tier].short)}).`, "blue");
  appendLedger("Workspace added to demo state", `${name} · ${tierInfo[tier].label}`, "Session");
  renderAll();
  closeDialog(document.getElementById("workspace-dialog"));
  document.getElementById("workspace-form").reset();
  updateTierExplainer();
  navigate("workspaces");
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

document.getElementById("workspace-tier").addEventListener("change", updateTierExplainer);
document.getElementById("workspace-network").addEventListener("change", updateTierExplainer);
document.getElementById("workspace-search").addEventListener("input", renderWorkspaces);
document.getElementById("operator-search").addEventListener("input", renderOperators);
document.getElementById("ledger-search").addEventListener("input", renderLedger);

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
else navigate("overview");
