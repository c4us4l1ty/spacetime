/* ==========================================================================
   Spacetime — prototype interactions
   No build step, no dependencies. Everything below is host-side UI only:
   the guest apps in this prototype are static HTML, not real microVMs.
   ========================================================================== */

(() => {
  "use strict";

  const $ = (sel, root = document) => root.querySelector(sel);
  const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

  const app = $("#app");
  const scrim = $("#scrim");
  const drawer = $("#drawer");
  const director = $("#director");
  const palette = $("#palette");
  const paletteInput = $("#paletteInput");
  const paletteList = $("#paletteList");
  const toasts = $("#toasts");

  const omniUrl = $("#omniUrl");
  const omniChip = $("#omniChip");
  const omniIcon = $("#omniIcon");
  const omniEgress = $("#omniEgress");
  const ribbonLeft = $("#ribbonLeft");
  const tabCpu = $("#tabCpu");
  const tabMem = $("#tabMem");

  const tabs = $$(".tab");
  const views = $$(".view");
  const pins = $$(".pin[data-open]");

  let currentView = "web";

  /* -----------------------------------------------------------------------
     Per-surface chrome configuration
     The omnibox and the intent ribbon describe whatever surface is in front.
     ----------------------------------------------------------------------- */

  const guestRibbon = (o) => `
    <span class="d"><svg class="icon"><use href="#i-file"/></svg> read <b>${o.read}</b> files</span>
    <span class="d"><svg class="icon"><use href="#i-pen"/></svg> wrote <b>${o.wrote}</b></span>
    <span class="d"><svg class="icon"><use href="#i-network"/></svg> net <b>${o.net}</b>${
      o.blocked ? ` <span class="d bad">${o.blocked} blocked</span>` : ""
    }</span>
    <span class="sep"></span>
    <span class="d"><svg class="icon"><use href="#i-wallet"/></svg> spend <b>${o.spend}</b></span>
    <span class="d ${o.riskClass || "ok"}"><span class="dot ${
    o.riskClass || "ok"
  }"></span> risk <b>${o.risk}</b></span>`;

  const hostRibbon = (parts) => `
    <span class="d"><svg class="icon"><use href="#i-orbit"/></svg> host surface · <b>no guest</b></span>
    <span class="sep"></span>
    ${parts
      .map(
        (p) =>
          `<span class="d ${p.cls || ""}">${
            p.icon ? `<svg class="icon"><use href="#i-${p.icon}"/></svg> ` : ""
          }${p.label} <b>${p.value}</b></span>`
      )
      .join("")}`;

  const VIEWS = {
    web: {
      address: "opencut.app/p/launch-teaser",
      chip: "tab-3a1f",
      icon: "lock",
      iconClass: "",
      egress: '<span class="dot ok"></span> <b>residential</b> · NL',
      meter: ["12%", "148 MB"],
      ribbon: guestRibbon({ read: 14, wrote: 2, net: 3, blocked: 1, spend: "$0.00", risk: "low" }),
    },
    terminal: {
      address: 'spacetime://guest/yt-01 <span class="muted">— hvc0</span>',
      chip: "tab-9b2e",
      icon: "lock",
      iconClass: "",
      egress: '<span class="dot ok"></span> <b>no network</b> driver',
      meter: ["4%", "96 MB"],
      ribbon: guestRibbon({ read: 3, wrote: 2, net: 0, spend: "$0.00", risk: "low" }),
    },
    editor: {
      address: 'spacetime://guest/feather <span class="muted">— wayland</span>',
      chip: "tab-c14d",
      icon: "lock",
      iconClass: "",
      egress: '<span class="dot ok"></span> <b>no network</b> driver',
      meter: ["1%", "84 MB"],
      ribbon: guestRibbon({ read: 3, wrote: 1, net: 0, spend: "$0.00", risk: "low" }),
    },
    launcher: {
      address: "spacetime://newtab",
      chip: "host",
      icon: "orbit",
      iconClass: "is-host",
      egress: '<span class="dot violet"></span> <b>host shell</b> · no guest',
      meter: ["—", "96 MB"],
      ribbon: hostRibbon([
        { icon: "snow", label: "warm pool", value: "4 paused" },
        { icon: "cpu", label: "zram", value: "1.2 GB" },
        { icon: "clock", label: "restore", value: "≈ 8 ms" },
        { icon: "shield", label: "risk", value: "none", cls: "ok" },
      ]),
    },
    console: {
      address: 'spacetime://console <span class="muted">— telemetry</span>',
      chip: "host",
      icon: "orbit",
      iconClass: "is-host",
      egress: '<span class="dot violet"></span> <b>host</b> · no egress',
      meter: ["—", "ledger 1,284"],
      ribbon: hostRibbon([
        { icon: "box", label: "arks", value: "3" },
        { icon: "layers", label: "operators", value: "5" },
        { icon: "shield", label: "verifiers", value: "4" },
        { icon: "alert", label: "gates", value: "2", cls: "warn" },
      ]),
    },
    autopsy: {
      address: 'spacetime://autopsy/tab-3a1f <span class="muted">— held</span>',
      chip: "report",
      icon: "scan",
      iconClass: "is-warn",
      egress: '<span class="dot warn"></span> <b>guest held</b> · 0 MB',
      meter: ["—", "frozen"],
      ribbon: guestRibbon({ read: 214, wrote: 188, net: 14, blocked: 6, spend: "$0.00", risk: "high", riskClass: "bad" }),
    },
    cryo: {
      address: 'spacetime://space/bounty_hunter <span class="muted">— cryo</span>',
      chip: "cryo",
      icon: "moon",
      iconClass: "is-host",
      egress: '<span class="dot"></span> <b>0 MB</b> · frozen',
      meter: ["0%", "0 MB"],
      ribbon: hostRibbon([
        { icon: "moon", label: "state", value: "cryo-sleep" },
        { icon: "clock", label: "frozen", value: "2h 14m" },
        { icon: "box", label: "snapshot", value: "46.2 MB" },
        { icon: "shield", label: "risk", value: "none", cls: "ok" },
      ]),
    },
  };

  /* -----------------------------------------------------------------------
     Tab switching
     ----------------------------------------------------------------------- */

  function setView(name, opts = {}) {
    const cfg = VIEWS[name] || VIEWS.launcher;
    currentView = name;

    views.forEach((v) => v.classList.toggle("is-active", v.dataset.view === name));

    tabs.forEach((t) => {
      const on = t.dataset.view === name;
      t.setAttribute("aria-selected", String(on));
      if (on && opts.scroll !== false) t.scrollIntoView?.({ block: "nearest" });
    });

    pins.forEach((p) => p.classList.toggle("is-active", p.dataset.open === name));

    omniUrl.innerHTML = cfg.address;
    omniChip.textContent = cfg.chip;
    omniIcon.className = "omni-icon " + (cfg.iconClass || "");
    omniIcon.innerHTML = `<svg class="icon"><use href="#i-${cfg.icon}"/></svg>`;
    omniEgress.innerHTML = cfg.egress;
    tabCpu.textContent = cfg.meter[0];
    tabMem.textContent = cfg.meter[1];
    ribbonLeft.innerHTML = cfg.ribbon;
    document.title = `Spacetime — ${name}`;
  }

  /* -----------------------------------------------------------------------
     Toasts
     ----------------------------------------------------------------------- */

  const escapeHtml = (s) =>
    String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

  function showToast(message, o = {}) {
    if (!message) return;
    const el = document.createElement("div");
    el.className = "toast" + (o.tone ? ` ${o.tone}` : "");
    el.innerHTML = `
      <span class="t-ic"><svg class="icon"><use href="#i-${o.icon || "check"}"/></svg></span>
      <span class="t-copy">
        <span class="t-title">${escapeHtml(o.title || "Spacetime")}</span>
        <span class="t-sub">${escapeHtml(message)}</span>
      </span>`;
    toasts.appendChild(el);
    setTimeout(() => {
      el.classList.add("out");
      el.addEventListener("animationend", () => el.remove(), { once: true });
      setTimeout(() => el.remove(), 400);
    }, o.duration || 3800);
  }

  /* -----------------------------------------------------------------------
     Sidebar
     ----------------------------------------------------------------------- */

  const collapseBtn = $("#collapseBtn");

  function setSidebar(collapsed) {
    app.classList.toggle("is-collapsed", collapsed);
    collapseBtn.setAttribute("aria-expanded", String(!collapsed));
    collapseBtn.setAttribute("aria-label", collapsed ? "Expand sidebar" : "Collapse sidebar");
    collapseBtn.title = `${collapsed ? "Expand" : "Collapse"} sidebar (Ctrl+B)`;
  }

  collapseBtn.addEventListener("click", () => setSidebar(!app.classList.contains("is-collapsed")));

  /* -----------------------------------------------------------------------
     Director / drawer
     ----------------------------------------------------------------------- */

  const directorBtn = $("#directorBtn");
  const drawerBtn = $("#drawerBtn");

  function setDirector(open) {
    director.classList.toggle("is-open", open);
    director.inert = !open;
    directorBtn.classList.toggle("has-dot", false);
    if (open) {
      closeDrawer();
      setTimeout(() => $("#directorInput").focus({ preventScroll: true }), 220);
    }
  }

  function setDrawer(open) {
    drawer.classList.toggle("is-open", open);
    drawer.inert = !open;
    scrim.classList.toggle("is-on", open);
    if (open) setDirector(false);
  }

  const closeDirector = () => setDirector(false);
  const closeDrawer = () => setDrawer(false);

  directorBtn.addEventListener("click", () => setDirector(!director.classList.contains("is-open")));
  drawerBtn.addEventListener("click", () => setDrawer(!drawer.classList.contains("is-open")));
  $("#directorClose").addEventListener("click", closeDirector);
  $("#drawerClose").addEventListener("click", closeDrawer);
  scrim.addEventListener("click", closeDrawer);

  const autonomy = $("#autonomySwitch");
  autonomy.addEventListener("click", () => {
    const on = autonomy.getAttribute("aria-pressed") === "true";
    autonomy.setAttribute("aria-pressed", String(!on));
    showToast(
      on ? "Heartbeat paused — the Director will finish its current atomic task and hold." : "Heartbeat resumed — next cycle in 12s.",
      { title: on ? "Autonomy paused" : "Autonomy resumed", icon: on ? "pause" : "sparkles", tone: on ? "warn" : "" }
    );
  });

  /* -----------------------------------------------------------------------
     Director input
     ----------------------------------------------------------------------- */

  const directorInput = $("#directorInput");
  const stream = $("#stream");

  function appendStream(text, tone = "hl") {
    const now = new Date();
    const hh = String(now.getHours()).padStart(2, "0");
    const mm = String(now.getMinutes()).padStart(2, "0");
    const ss = String(now.getSeconds()).padStart(2, "0");
    const line = document.createElement("div");
    line.className = "stream-line";
    line.innerHTML = `<span class="t">${hh}:${mm}:${ss}</span><span class="a">${text}</span>`;
    stream.appendChild(line);
    stream.scrollTop = stream.scrollHeight;
    while (stream.children.length > 14) stream.removeChild(stream.firstElementChild);
  }

  function sendToDirector() {
    const text = directorInput.value.trim();
    if (!text) return;
    appendStream(`<span class="hl">you</span> ${escapeHtml(text)}`);
    directorInput.value = "";
    showToast("Queued as [RING -3 INTERRUPT] at the top of the next heartbeat — the loop is not restarted.", {
      title: "Director steered",
      icon: "send",
    });
  }

  $("#directorSend").addEventListener("click", sendToDirector);
  directorInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      sendToDirector();
    }
  });

  /* -----------------------------------------------------------------------
     Command palette
     ----------------------------------------------------------------------- */

  const TABS_META = tabs.map((t) => ({
    label: t.querySelector(".tab-title").textContent.trim(),
    sub: (t.querySelector(".tab-meta")?.textContent || "").replace(/\s+/g, " ").trim(),
    icon: (t.querySelector(".tab-ic .icon use")?.getAttribute("href") || "#i-grid").replace("#i-", ""),
    color: t.querySelector(".tab-ic")?.style.getPropertyValue("--ic") || "#7dd3fc",
    view: t.dataset.view,
    group: "Tabs",
  }));

  const COMMANDS = [
    { label: "Open Director", sub: "Ring -3 supervisor · heartbeat 318", icon: "sparkles", color: "#a78bfa", hint: "⌘⇧D", group: "Actions", run: () => setDirector(true) },
    { label: "Toggle network drawer", sub: "Honey-Duct · egress flows for tab-3a1f", icon: "network", color: "#5eead4", hint: "⌘⇧J", group: "Actions", run: () => setDrawer(true) },
    { label: "Branch this world-line", sub: "reflink clone + git commit in .space", icon: "branch", color: "#7dd3fc", group: "Actions", run: () => branchWorldLine() },
    { label: "Freeze tab to cryo", sub: "0 MB RSS · snapshot kept on disk", icon: "snow", color: "#bae6fd", group: "Actions", run: () => showToast("tab-3a1f frozen at heartbeat 318 — 0 MB RSS, resume from snapshot in ≈ 8 ms", { title: "Cryo-sleep", icon: "snow" }) },
    { label: "Run autopsy on tab-3a1f", sub: "diff · egress · canary report", icon: "scan", color: "#f87171", group: "Actions", run: () => openView("autopsy") },
    { label: "Open telemetry console", sub: "operators · verifiers · ledger · gates", icon: "activity", color: "#a78bfa", group: "Actions", run: () => openView("console") },
    { label: "Verify ledger chain", sub: "1,284 entries · hash-chained", icon: "shield", color: "#4ade80", group: "Actions", run: () => showToast("Ledger verified: 1,284 entries, 0 gaps, chain head #1a2f.", { title: "Ledger verified", icon: "check" }) },
    { label: "Teleport .space bundle", sub: "manifest + delta + vault + memory/ + skills/", icon: "download", color: "#5eead4", group: "Actions", run: () => showToast("Bundle ready: 46.2 MB zstd. Persona seed travels with it, so the device identity survives.", { title: "Export .space", icon: "download" }) },
    { label: "New tab", sub: "restore a warm, paused microVM", icon: "plus", color: "#fbbf24", hint: "⌘T", group: "Actions", run: () => openView("launcher") },
    { label: "Toggle sidebar", sub: "vertical tabs / compact rail", icon: "panel", color: "#a3aabd", hint: "⌘B", group: "Actions", run: () => setSidebar(!app.classList.contains("is-collapsed")) },
  ];

  let paletteItems = [];
  let paletteIndex = 0;

  function paletteData() {
    return [
      ...TABS_META.map((t) => ({
        ...t,
        hint: t.view === currentView ? "open" : "",
        run: () => openView(t.view),
      })),
      ...COMMANDS,
    ];
  }

  function renderPalette(query = "") {
    const q = query.trim().replace(/^>/, "").toLowerCase();
    const data = paletteData().filter((item) => {
      if (!q) return true;
      return `${item.label} ${item.sub}`.toLowerCase().includes(q);
    });

    paletteItems = data;
    paletteIndex = 0;

    if (!data.length) {
      paletteList.innerHTML = `<div class="palette-empty">No match. Try “console”, “cryo”, “branch”…</div>`;
      return;
    }

    let html = "";
    let lastGroup = "";
    data.forEach((item, i) => {
      if (item.group !== lastGroup) {
        html += `<div class="palette-group">${item.group}</div>`;
        lastGroup = item.group;
      }
      html += `
        <button class="palette-item${i === 0 ? " is-sel" : ""}" type="button" data-idx="${i}">
          <span class="p-ic" style="--ic:${item.color}"><svg class="icon"><use href="#i-${item.icon}"/></svg></span>
          <span class="p-copy">
            <span class="p-label">${escapeHtml(item.label)}</span>
            <span class="p-sub">${escapeHtml(item.sub || "")}</span>
          </span>
          ${item.hint ? `<span class="p-hint">${item.hint}</span>` : ""}
        </button>`;
    });
    paletteList.innerHTML = html;
    paletteList.scrollTop = 0;
  }

  function movePalette(delta) {
    if (!paletteItems.length) return;
    paletteIndex = (paletteIndex + delta + paletteItems.length) % paletteItems.length;
    $$(".palette-item", paletteList).forEach((el, i) => {
      el.classList.toggle("is-sel", i === paletteIndex);
      if (i === paletteIndex) el.scrollIntoView?.({ block: "nearest" });
    });
  }

  function runPalette(index = paletteIndex) {
    const item = paletteItems[index];
    if (!item) return;
    closePalette();
    item.run();
  }

  function openPalette() {
    closeDrawer();
    palette.classList.add("is-open");
    palette.inert = false;
    paletteInput.value = "";
    $(".mode", palette).textContent = "go";
    renderPalette("");
    setTimeout(() => paletteInput.focus(), 30);
  }

  function closePalette() {
    palette.classList.remove("is-open");
    paletteInput.blur();
    palette.inert = true;
  }

  paletteInput.addEventListener("input", () => {
    const raw = paletteInput.value;
    $(".mode", palette).textContent = raw.trim().startsWith(">") ? "act" : raw.trim() ? "find" : "go";
    renderPalette(raw);
  });

  paletteInput.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      movePalette(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      movePalette(-1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      runPalette();
    }
  });

  paletteList.addEventListener("click", (e) => {
    const btn = e.target.closest(".palette-item");
    if (!btn) return;
    runPalette(Number(btn.dataset.idx));
  });

  const omnibox = $("#omnibox");
  omnibox.addEventListener("click", openPalette);
  omnibox.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      openPalette();
    }
  });
  $("#menuBtn").addEventListener("click", openPalette);

  /* -----------------------------------------------------------------------
     Actions
     ----------------------------------------------------------------------- */

  function openView(name) {
    if (!VIEWS[name]) {
      showToast("That surface is not part of the prototype yet.", { title: "Not built", icon: "alert", tone: "warn" });
      return;
    }
    closePalette();
    setView(name);
  }

  function branchWorldLine() {
    const hashes = ["e09c", "b7f2", "5d41"];
    const hash = hashes[Math.floor(Math.random() * hashes.length)];
    showToast(`Forked → snapshot ${hash}: reflink clone in 42 ms, git commit in .space, 0 bytes of RAM copied.`, {
      title: "World-line branched",
      icon: "branch",
    });
    appendStream(`<span class="hl">branch</span> risky-render → snapshot ${hash} · reflink 42 ms`);
  }

  function resolveGate(el, approved) {
    const gate = el.closest(".blocker, .brow");
    if (!gate) return;
    const title = gate.querySelector("b")?.textContent?.trim() || "gate";
    gate.style.transition = "opacity 160ms linear, transform 160ms ease";
    gate.style.opacity = "0";
    gate.style.transform = "translateX(8px)";
    setTimeout(() => {
      gate.remove();
      const remaining = $$(".blocker").length;
      const count = $("#blockerCount");
      if (count) count.textContent = String(remaining);
      const status = $("#blockerStatus");
      if (status && remaining === 0) status.innerHTML = `<svg class="icon"><use href="#i-check"/></svg> no blockers`;
    }, 170);
    showToast(
      approved
        ? `${title} approved. Action executed by the supervisor and appended to the ledger.`
        : `${title} denied. The Director is told why at the next heartbeat; nothing was executed.`,
      { title: approved ? "Approved" : "Denied", icon: approved ? "check" : "x", tone: approved ? "" : "warn" }
    );
    appendStream(
      approved
        ? `<span class="hl">gate</span> human approved · ${escapeHtml(title)} · <span class="ok">executed</span>`
        : `<span class="bad">gate</span> human denied · ${escapeHtml(title)}`
    );
  }

  /* -----------------------------------------------------------------------
     Delegated clicks
     ----------------------------------------------------------------------- */

  document.addEventListener("click", (e) => {
    const el = e.target.closest(
      "[data-open],[data-open-director],[data-open-drawer],[data-open-palette],[data-approve],[data-deny],[data-branch],[data-filter],[data-toast]"
    );
    if (!el) return;

    if (el.dataset.open) {
      e.preventDefault();
      openView(el.dataset.open);
      return;
    }
    if (el.dataset.openDirector) {
      setDirector(true);
      return;
    }
    if (el.dataset.openDrawer) {
      setDrawer(true);
      return;
    }
    if (el.dataset.openPalette) {
      openPalette();
      return;
    }
    if (el.dataset.approve) {
      resolveGate(el, true);
      return;
    }
    if (el.hasAttribute("data-deny")) {
      resolveGate(el, false);
      return;
    }
    if (el.hasAttribute("data-branch")) {
      branchWorldLine();
      return;
    }
    if (el.dataset.filter) {
      const filter = el.dataset.filter;
      $$(".fchip", drawer).forEach((c) => c.classList.toggle("is-on", c === el));
      $$(".net-row[data-kind]", drawer).forEach((row) => {
        const show = filter === "all" || row.dataset.kind === filter || (filter === "blocked" && row.dataset.kind === "blocked");
        row.style.display = show ? "" : "none";
      });
      return;
    }
    if (el.dataset.toast) {
      e.preventDefault();
      showToast(el.dataset.toast, {
        icon: el.dataset.icon,
        title: el.dataset.toastTitle,
        tone: el.dataset.tone,
      });
    }
  });

  /* tabs ------------------------------------------------------------------ */

  tabs.forEach((tab) => {
    tab.addEventListener("click", (e) => {
      if (e.target.closest(".tab-close")) {
        e.stopPropagation();
        showToast("Closing this tab would vapourize the guest and keep only its tombstone. Nothing deleted in the prototype.", {
          title: "Close tab?",
          icon: "scan",
          tone: "warn",
        });
        return;
      }
      openView(tab.dataset.view);
    });
  });

  pins.forEach((pin) => {
    pin.addEventListener("click", () => openView(pin.dataset.open));
  });

  /* miscellaneous --------------------------------------------------------- */

  $("#spaceSwitch").addEventListener("click", () => openView("launcher"));
  $("#pinAdd").addEventListener("click", () =>
    showToast("Pin an app to the rail. In the real shell this is a virtio-fs share + a policy profile.", {
      title: "Pin an app",
      icon: "plus",
    })
  );
  $("#branchBtn").addEventListener("click", branchWorldLine);

  /* -----------------------------------------------------------------------
     Keyboard
     ----------------------------------------------------------------------- */

  document.addEventListener("keydown", (e) => {
    const mod = e.metaKey || e.ctrlKey;

    if (mod && (e.key === "k" || e.key === "l")) {
      e.preventDefault();
      palette.classList.contains("is-open") ? closePalette() : openPalette();
      return;
    }
    if (mod && e.key === "b") {
      e.preventDefault();
      setSidebar(!app.classList.contains("is-collapsed"));
      return;
    }
    if (mod && e.key === "t") {
      e.preventDefault();
      openView("launcher");
      return;
    }
    if (mod && e.shiftKey && e.key.toLowerCase() === "d") {
      e.preventDefault();
      setDirector(!director.classList.contains("is-open"));
      return;
    }
    if (mod && e.shiftKey && e.key.toLowerCase() === "j") {
      e.preventDefault();
      setDrawer(!drawer.classList.contains("is-open"));
      return;
    }
    if (e.key === "Escape") {
      if (palette.classList.contains("is-open")) {
        closePalette();
        omnibox.focus();
      } else if (director.classList.contains("is-open")) {
        closeDirector();
      } else if (drawer.classList.contains("is-open")) {
        closeDrawer();
      }
    }
  });

  /* -----------------------------------------------------------------------
     A small amount of simulated host life (subtle, pausable)
     ----------------------------------------------------------------------- */

  const STREAM_SCRIPT = [
    '<span class="hl">heartbeat</span> #319 · frontier: publish short 2/3',
    '<span class="hl">dom</span> at-spi2 · 38 nodes · no screenshot',
    '<span class="hl">verifier</span> mutation score 82% → <span class="ok">pass</span>',
    '<span class="warn">egress</span> blocked 45.9.148.12:8443 · policy c2-asn',
    '<span class="hl">skill</span> render_short(clip=47) → <span class="ok">ok</span> · 0 LLM turns',
    '<span class="hl">vault</span> @vault:gh_token injected · guest blind',
    '<span class="hl">memory</span> 03_WORKING_MEMORY.md → 148 / 150 lines',
    '<span class="hl">ledger</span> appended #1a30 · hash 7d1c…e08a',
    '<span class="warn">gate</span> spend $5.00 still waiting on you',
    '<span class="hl">cryo</span> YT_Channel_02 frozen · 0 MB RSS',
  ];
  let streamIdx = 0;

  setInterval(() => {
    if (document.hidden) return;

    if (currentView === "web") {
      const cpu = 8 + Math.round(Math.random() * 9);
      const mem = 146 + Math.round(Math.random() * 7);
      tabCpu.textContent = `${cpu}%`;
      tabMem.textContent = `${mem} MB`;
    }

    if (streamIdx < STREAM_SCRIPT.length) {
      appendStream(STREAM_SCRIPT[streamIdx]);
      streamIdx += 1;
    }
  }, 7000);

  /* -----------------------------------------------------------------------
     Boot
     ----------------------------------------------------------------------- */

  renderPalette("");
  setView("web", { scroll: false });
  document.documentElement.dataset.ready = "true";
})();
