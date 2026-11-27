"use strict";

const FALLBACK_REPORT = {
  schema_version: 1,
  report: "bundled_offline_fallback",
  demo: true,
  resource_gate: "unmeasured_target_4gb",
  performance_claims: "unmeasured",
  block3_done: false,
  execution: { net: "none", egress_class: "none", execution_tier: "scripted_local_fixture", trust_domain: "single_host_process" },
  warnings: ["fetch of report.json failed; load a CLI-exported JSON report or open via a static server"],
  goal_tree: [],
  operators: [],
  verifier: { econbench_fixtures: 0, fixture_population_fixtures: 26, false_pass_rate: 0, real_operator_study: "unmeasured" },
  ledger: { source: "none", event_count: 0, last_hash: "unmeasured", tail: [] },
  workspaces: [
    { id: "ws-human-terminal", surface_id: "surface-human-terminal", trust_tier: "host_trusted", liveness: "running", preview_mode: "terminal_buffer", egress_class: "none", last_action: "offline fallback" },
    { id: "ws-ai-upload", surface_id: "surface-ai-upload", trust_tier: "ark_cell", liveness: "running", preview_mode: "snapshot_on_action", egress_class: "none", last_action: "offline fallback" }
  ],
  snapshot_strip: [],
  blockers: [],
  resource_meters: { measured_peak_rss_kb: "unmeasured", boot_ms: "unmeasured", tab_switch_ms: "unmeasured", cpu_percent: "unmeasured" },
  cost_meters: { tokens_configured: 0, tokens_consumed: 0, fake_credits: 0, real_cost: 0, real_money: false },
  mock_economy: { bounties: 0, fake_credits: 0, payments_confirmed: 0, real_money: false, wallet_ledger_verified: false },
  leak_surface: []
};

const state = {
  report: null,
  activeTile: "human",
  snapshotIndex: 0,
  streamTicks: 0
};

function node(id) { return document.getElementById(id); }
function text(value) { return value === undefined || value === null || value === "" ? "unmeasured" : String(value); }
function shortHash(value) { const s = text(value); return s.length > 14 ? s.slice(0, 14) + "…" : s; }
function make(tag, className, value) {
  const element = document.createElement(tag);
  if (className) element.className = className;
  if (value !== undefined) element.textContent = text(value);
  return element;
}
function clear(element) { while (element.firstChild) element.removeChild(element.firstChild); }
function addRows(container, rows) {
  clear(container);
  for (const row of rows) {
    const line = make("div", "row");
    line.appendChild(make("span", "", row[0]));
    line.appendChild(make("strong", "", row[1]));
    container.appendChild(line);
  }
}
function addList(container, values) {
  clear(container);
  const items = Array.isArray(values) ? values : [];
  if (items.length === 0) container.appendChild(make("div", "row", "none"));
  for (const value of items) container.appendChild(make("div", "row", value));
}
function validateReport(report) {
  if (!report || report.schema_version !== 1) throw new Error("report schema_version must be 1");
  if (report.resource_gate !== "unmeasured_target_4gb") throw new Error("report claims a measured resource gate");
  if (report.performance_claims !== "unmeasured") throw new Error("report claims measured performance");
  if (!report.execution || report.execution.net !== "none") throw new Error("console requires net = none");
  if (!Array.isArray(report.workspaces) || report.workspaces.length < 2) throw new Error("mixed-workspace tiles are missing");
}
function showError(message) {
  const error = node("error");
  error.hidden = false;
  error.textContent = message;
}
function renderStatus(report, sourceLabel) {
  const strip = node("status-strip");
  clear(strip);
  const items = [
    ["Report", sourceLabel],
    ["Resource gate", report.resource_gate],
    ["Performance", report.performance_claims],
    ["Execution tier", report.execution && report.execution.execution_tier],
    ["Egress", report.execution && report.execution.egress_class],
    ["Net", report.execution && report.execution.net],
    ["Block 3 DONE", report.block3_done === true ? "yes" : "no — functional prototype only"]
  ];
  for (const [label, value] of items) {
    const card = make("div", "status-item");
    card.appendChild(make("strong", "", value));
    card.appendChild(make("span", "", label));
    strip.appendChild(card);
  }
  node("source-badge").textContent = sourceLabel;
}
function renderWarnings(report) {
  const container = node("warnings");
  clear(container);
  const warnings = Array.isArray(report.warnings) ? [...report.warnings] : [];
  warnings.push("Ark cells share a guest kernel. A tab is a view onto an execution domain, not a trust boundary.");
  warnings.push("Snapshot placeholders are not continuous live video; hidden AI work stays RUNNING while RENDERING changes.");
  for (const warning of warnings) container.appendChild(make("p", "", warning));
}
function renderGoalTree(report) {
  const goals = Array.isArray(report.goal_tree) ? report.goal_tree : [];
  addRows(node("goal-tree"), goals.map((goal) => [goal.id || "goal", goal.status + " / " + (goal.verifier || "unverified")]));
}
function renderOperators(report) {
  const operators = Array.isArray(report.operators) ? report.operators : [];
  addRows(node("operators"), operators.map((operator) => [operator.id || "operator", operator.health || "unmeasured"]));
}
function renderVerifier(report) {
  const verifier = report.verifier || {};
  addRows(node("verifier"), [
    ["EconBench seed fixtures", verifier.econbench_fixtures],
    ["Existing fixture population", verifier.fixture_population_fixtures],
    ["Claims surviving re-verification", verifier.claims_surviving_reverification],
    ["False-PASS rate", verifier.false_pass_rate],
    ["Real operator study", verifier.real_operator_study],
    ["Synthetic population", verifier.synthetic_operator_population]
  ]);
}
function renderLedger(report) {
  const container = node("ledger-tail");
  clear(container);
  const ledger = report.ledger || {};
  const header = make("div", "row");
  header.appendChild(make("span", "", "source"));
  header.appendChild(make("strong", "", ledger.source + " / events=" + text(ledger.event_count) + " / last=" + shortHash(ledger.last_hash)));
  container.appendChild(header);
  const tail = Array.isArray(ledger.tail) ? ledger.tail : [];
  for (const event of tail) {
    const row = make("div", "ledger-row");
    row.appendChild(make("span", "", "#" + text(event.seq)));
    const hash = make("span", "", shortHash(event.hash));
    hash.title = text(event.hash);
    row.appendChild(hash);
    row.appendChild(make("span", "", event.action));
    row.appendChild(make("span", "verdict-" + text(event.verdict), event.verdict));
    row.appendChild(make("span", "", event.decision));
    container.appendChild(row);
  }
}
function renderWorkspaces(report) {
  const container = node("workspaces");
  clear(container);
  const tiles = Array.isArray(report.workspaces) ? report.workspaces : [];
  tiles.forEach((tile, index) => {
    const active = (index === 0 && state.activeTile === "human") || (index === 1 && state.activeTile === "ai");
    const card = make("article", active ? "tile active" : "tile");
    card.appendChild(make("h3", "", (index === 0 ? "Human GitHub / terminal" : "AI-driven mock upload") + (active ? " — RENDERING" : " — RUNNING")));
    const dl = document.createElement("dl");
    const fields = [
      ["WorkspaceId", tile.id],
      ["SurfaceId", tile.surface_id],
      ["Trust tier", tile.trust_tier],
      ["LivenessState", tile.liveness],
      ["RENDERING", active ? "true" : "false"],
      ["PreviewMode", tile.preview_mode],
      ["Egress", tile.egress_class],
      ["Net", "none"]
    ];
    for (const [key, value] of fields) {
      dl.appendChild(make("dt", "", key));
      dl.appendChild(make("dd", "", value));
    }
    card.appendChild(dl);
    card.appendChild(make("p", "last-action", "Last action: " + (index === 1 && Array.isArray(report.snapshot_strip) && report.snapshot_strip.length > 0 ? report.snapshot_strip[state.snapshotIndex % report.snapshot_strip.length].label : tile.last_action)));
    container.appendChild(card);
  });
}
function renderSnapshots(report) {
  const container = node("snapshot-strip");
  clear(container);
  const frames = Array.isArray(report.snapshot_strip) ? report.snapshot_strip : [];
  if (frames.length === 0) {
    container.appendChild(make("div", "row", "No snapshot frames in this report"));
    return;
  }
  frames.forEach((frame, index) => {
    const active = index === (state.snapshotIndex % frames.length);
    const card = make("article", active ? "snapshot-frame active" : "snapshot-frame");
    card.appendChild(make("h3", "", frame.label));
    const placeholder = make("div", "snapshot-placeholder");
    const fields = frame.fields || {};
    for (const key of Object.keys(fields)) {
      placeholder.appendChild(make("strong", "", key));
      placeholder.appendChild(make("span", "", fields[key]));
      placeholder.appendChild(document.createElement("br"));
    }
    placeholder.appendChild(make("em", "", "still placeholder, not live video"));
    card.appendChild(placeholder);
    card.appendChild(make("code", "", "ledger " + shortHash(frame.ledger_event_hash)));
    container.appendChild(card);
  });
}
function renderMeters(report) {
  const resource = report.resource_meters || {};
  const cost = report.cost_meters || {};
  const container = node("meters");
  addRows(container, [
    ["Target profile", resource.target_profile],
    ["Baseline profile", resource.baseline_target_profile],
    ["Profile registry", resource.target_profile_registry],
    ["Evaluated memory budget (KiB)", resource.evaluated_memory_budget_kb],
    ["Measured peak RSS (KiB)", resource.measured_peak_rss_kb],
    ["Boot (ms)", resource.boot_ms],
    ["Tab switch (ms)", resource.tab_switch_ms],
    ["CPU (%)", resource.cpu_percent],
    ["Tokens configured", cost.tokens_configured],
    ["Tokens consumed", cost.tokens_consumed],
    ["Fake credits", cost.fake_credits],
    ["Real cost", cost.real_cost],
    ["Real money", cost.real_money]
  ]);
  const bar = make("div", "meter-bar empty");
  bar.title = "No measured resource percentage is available";
  container.appendChild(bar);
}
function renderEconomy(report) {
  const economy = report.mock_economy || {};
  addRows(node("economy"), [
    ["Bounties", economy.bounties],
    ["Fake credits", economy.fake_credits],
    ["Mock payment confirmations", economy.payments_confirmed],
    ["Wallet ledger verified", economy.wallet_ledger_verified],
    ["Real money", economy.real_money],
    ["Public internet", economy.public_internet]
  ]);
}
function renderBlockers(report) {
  const blockers = Array.isArray(report.blockers) ? report.blockers : [];
  addRows(node("blockers"), blockers.map((blocker) => [blocker.kind, blocker.reason]));
}
function renderLeaks(report) { addList(node("leak-surface"), report.leak_surface); }

function render(report, sourceLabel) {
  validateReport(report);
  state.report = report;
  state.snapshotIndex = 0;
  node("app").hidden = false;
  node("error").hidden = true;
  renderStatus(report, sourceLabel);
  renderWarnings(report);
  renderGoalTree(report);
  renderOperators(report);
  renderVerifier(report);
  renderLedger(report);
  renderWorkspaces(report);
  renderSnapshots(report);
  renderBlockers(report);
  renderMeters(report);
  renderEconomy(report);
  renderLeaks(report);
}

function switchTile(tile) {
  state.activeTile = tile;
  if (state.report) renderWorkspaces(state.report);
}
function advanceSnapshot() {
  if (!state.report) return;
  const frames = Array.isArray(state.report.snapshot_strip) ? state.report.snapshot_strip : [];
  state.snapshotIndex = frames.length === 0 ? 0 : (state.snapshotIndex + 1) % frames.length;
  renderWorkspaces(state.report);
  renderSnapshots(state.report);
}

async function loadDefaultReport() {
  try {
    const response = await fetch("./report.json");
    if (!response.ok) throw new Error("report.json returned " + response.status);
    const report = await response.json();
    render(report, report.demo ? "checked-in demo report.json" : "report.json");
  } catch (error) {
    render(FALLBACK_REPORT, "bundled fallback — " + error.message);
  }
}

node("report-file").addEventListener("change", async (event) => {
  const file = event.target.files && event.target.files[0];
  if (!file) return;
  try {
    const report = JSON.parse(await file.text());
    render(report, "loaded " + file.name);
  } catch (error) {
    showError("Could not load report: " + error.message);
  }
});
node("switch-human").addEventListener("click", () => switchTile("human"));
node("switch-ai").addEventListener("click", () => switchTile("ai"));
node("advance-ai").addEventListener("click", advanceSnapshot);
setInterval(() => {
  state.streamTicks += 1;
  node("stream-ticks").textContent = String(state.streamTicks);
}, 1500);

loadDefaultReport();
