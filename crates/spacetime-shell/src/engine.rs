//! The engine: one struct that owns the whole machine.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use spacetime_agent::brain::{Brain, ProposedAction, SimulatedBrain};
use spacetime_agent::heartbeat::{AgentEvent, Heartbeat, TickOutcome};
use spacetime_agent::vision::{ElementKind, MarkedElement, Screen};
use spacetime_core::json::Value;
use spacetime_core::tab::{TabId, TabKind, TabTree, Trust};
use spacetime_core::time_travel::TimeTravel;
use spacetime_core::NetMode;
use spacetime_net::vswitch::{Flow, Proto, Switch};
use spacetime_net::Verdict;
use spacetime_vmm::zygote::{Zygote, ZygoteState};
use spacetime_vmm::{SimulatedBackend, SpacetimeVmm, VmConfig, VmHandle};
use spacetime_warden::Warden;

/// Engine knobs.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Milliseconds of simulated time per tick (one heartbeat ≈ one tick).
    pub tick_ms: u64,
    /// Capture a dirty-page snapshot every N ticks.
    pub snapshot_every: u64,
    /// Seconds of history retained by the time-travel ring.
    pub history_s: u64,
    /// Let the demo inject risky actions that fail (auto-rollback showcase).
    pub demo_rollbacks: bool,
    /// Rotate focus every N ticks so the choke badge is visible in the demo.
    pub demo_focus_every: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            tick_ms: 400,
            snapshot_every: 2,
            history_s: 300,
            demo_rollbacks: true,
            demo_focus_every: 9,
        }
    }
}

/// A side effect requested by the engine, for the shell/CLI to execute.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Boot { name: String, kind: TabKind, trust: Trust },
    Branch { parent: u64 },
    Focus(u64),
    Close(u64),
    Steer(String),
    Rewind { tab: u64, at_ms: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Good,
    Warn,
    Alert,
}

impl LogLevel {
    pub fn label(self) -> &'static str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Good => "good",
            LogLevel::Warn => "warn",
            LogLevel::Alert => "alert",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub at_ms: u64,
    pub level: LogLevel,
    pub channel: &'static str,
    pub text: String,
}

/// Live position of the AI's virtual pointer inside a tab.
#[derive(Debug, Clone)]
pub struct GhostCursor {
    pub tab: u64,
    pub x: f64,
    pub y: f64,
    pub target_x: f64,
    pub target_y: f64,
    /// 0..1 progress of the current movement.
    pub progress: f64,
    pub label: String,
    pub moving: bool,
}

impl GhostCursor {
    pub fn new(tab: u64) -> GhostCursor {
        GhostCursor {
            tab,
            x: 640.0,
            y: 360.0,
            target_x: 640.0,
            target_y: 360.0,
            progress: 1.0,
            label: String::new(),
            moving: false,
        }
    }

    /// Advance along an ease-out curve (reads as a human hand, not teleporting).
    pub fn advance(&mut self, dt_ms: u64) {
        if !self.moving {
            return;
        }
        self.progress = (self.progress + dt_ms as f64 / 220.0).min(1.0);
        let eased = 1.0 - (1.0 - self.progress).powi(3);
        self.x += (self.target_x - self.x) * eased * 0.55;
        self.y += (self.target_y - self.y) * eased * 0.55;
        if self.progress >= 1.0 {
            self.x = self.target_x;
            self.y = self.target_y;
            self.moving = false;
        }
    }

    pub fn move_to(&mut self, x: f64, y: f64, label: &str) {
        self.target_x = x;
        self.target_y = y;
        self.progress = 0.0;
        self.moving = true;
        self.label = label.to_string();
    }
}

/// The engine.
pub struct Spacetime {
    pub config: EngineConfig,
    pub tabs: TabTree,
    pub vmm: SimulatedBackend,
    pub zygote: Zygote,
    pub warden: Warden,
    pub net: Switch,
    /// History of the currently focused tab (one ring per workspace, following
    /// focus; the production engine keeps one ring per tab).
    pub travel: TimeTravel,
    pub agent: Heartbeat<Box<dyn Brain>>,
    handles: Vec<VmHandle>,
    last_balloon: BTreeMap<u64, u64>,
    pub cursor: GhostCursor,
    pub log: Vec<LogLine>,
    pub now_ms: u64,
    ticks: u64,
    demo_switch_in: u64,
}

impl Spacetime {
    /// Build the demo workspace from the wireframe: a stealth browser tab with an
    /// AI mission, an airgapped terminal, and a disposable malware lab tab.
    pub fn demo(config: EngineConfig) -> Spacetime {
        let mut engine = Spacetime::new(config.clone());
        engine.log(LogLevel::Info, "core", "engine starting");
        engine.log(
            LogLevel::Info,
            "vmm",
            "backend: simulated (real hypervisor backends land in phase 3)",
        );
        match engine.zygote.boot() {
            Ok(ms) => engine.log(
                LogLevel::Good,
                "vmm",
                &format!("zygote ready in {ms} ms — golden memory snapshot taken"),
            ),
            Err(err) => engine.log(LogLevel::Alert, "vmm", &err.to_string()),
        }

        let mission = "edit the promo video in /storage/X and export a 9:16 cut";
        engine.agent = Heartbeat::new(mission, 1, Box::new(SimulatedBrain::new()) as Box<dyn Brain>);
        engine.agent.fail_every = if config.demo_rollbacks { 5 } else { 0 };

        let opencut = engine.open_tab("opencut.app", TabKind::Web, Trust::Chameleon);
        engine.open_tab("ffmpeg-cli", TabKind::Terminal, Trust::Airgapped);
        engine.open_tab("wannacry.exe", TabKind::File, Trust::Disposable);
        engine.focus(opencut);
        engine.log(
            LogLevel::Info,
            "shell",
            "workspace restored: opencut.app (AI mission), ffmpeg-cli, wannacry.exe",
        );
        engine
    }

    /// An empty engine (tests, `spacetime run --empty`).
    pub fn new(config: EngineConfig) -> Spacetime {
        let history_ms = config.history_s * 1_000;
        Spacetime {
            config,
            tabs: TabTree::new(),
            vmm: SimulatedBackend::new(),
            zygote: Zygote::new(),
            warden: Warden::default(),
            net: Switch::new(NetMode::HoneyDuct),
            travel: TimeTravel::new(history_ms),
            agent: Heartbeat::new("idle", 0, Box::new(SimulatedBrain::new()) as Box<dyn Brain>),
            handles: Vec::new(),
            last_balloon: BTreeMap::new(),
            cursor: GhostCursor::new(0),
            log: Vec::new(),
            now_ms: 0,
            ticks: 0,
            demo_switch_in: 9,
        }
    }

    pub fn log(&mut self, level: LogLevel, channel: &'static str, text: &str) {
        self.log.push(LogLine {
            at_ms: self.now_ms,
            level,
            channel,
            text: text.to_string(),
        });
        if self.log.len() > 400 {
            self.log.drain(0..100);
        }
    }

    fn net_mode_for(trust: Trust) -> NetMode {
        match trust {
            Trust::Airgapped | Trust::Disposable => NetMode::Airgapped,
            Trust::HoneyDuct => NetMode::HoneyDuct,
            Trust::Chameleon => NetMode::Stealth,
        }
    }

    fn image_for(kind: TabKind) -> &'static str {
        match kind {
            TabKind::Web => "browser.wavefunction",
            TabKind::Repo | TabKind::Ai => "debian-trixie.wavefunction",
            TabKind::File => "sandbox.wavefunction",
            TabKind::Terminal => "alpine-minimal.wavefunction",
        }
    }

    /// Open a tab: zygote CoW clone + resource policy + network posture.
    pub fn open_tab(&mut self, name: &str, kind: TabKind, trust: Trust) -> u64 {
        let id = self.tabs.open(name, kind, trust);
        let tab_id = TabId(id);
        let mut config = VmConfig::new(tab_id, name, Self::image_for(kind), Self::net_mode_for(trust));
        config.ram_mb = self
            .tabs
            .get(tab_id)
            .map(|t| t.resources.ram_cap_mb)
            .unwrap_or(1024);

        let mut handle = self.vmm.clone_from_zygote(&mut self.zygote, &config).ok();
        if handle.is_none() && self.zygote.state() == ZygoteState::Retired {
            let _ = self.zygote.boot();
            self.log(LogLevel::Warn, "vmm", "zygote recycled; golden VM rebooted");
            handle = self.vmm.clone_from_zygote(&mut self.zygote, &config).ok();
        }
        let handle = handle.or_else(|| self.vmm.boot(&config).ok());

        match handle {
            Some(handle) => {
                self.handles.push(handle);
                self.log(
                    LogLevel::Good,
                    "vmm",
                    &format!(
                        "{name}: cloned zygote in {} ms ({} MB shared, {} MB private)",
                        handle.boot_ms,
                        self.tabs.get(tab_id).map(|t| t.shared_ram_mb).unwrap_or(0),
                        self.tabs.get(tab_id).map(|t| t.private_ram_mb).unwrap_or(0),
                    ),
                );
            }
            None => self.log(LogLevel::Alert, "vmm", &format!("{name}: VM failed to boot")),
        }
        self.focus(id);
        id
    }

    /// Branch a tab: its overlay is frozen read-only and the child starts from
    /// that exact moment with a fresh CoW layer.
    pub fn branch(&mut self, parent: u64) -> Option<u64> {
        let child = self.tabs.branch(TabId(parent)).ok()?;
        let tab = self.tabs.get(child)?.clone();
        let config = VmConfig::new(
            child,
            &tab.name,
            "debian-trixie.wavefunction",
            Self::net_mode_for(tab.trust),
        );
        if let Ok(handle) = self.vmm.clone_from_zygote(&mut self.zygote, &config) {
            self.handles.push(handle);
        }
        self.focus(child.0);
        self.log(
            LogLevel::Good,
            "vmm",
            &format!(
                "branched #{parent:02} → {} (parent overlay frozen, child CoW clone)",
                tab.name
            ),
        );
        Some(child.0)
    }

    /// Make a tab active. The warden chokes the previous one immediately.
    pub fn focus(&mut self, id: u64) {
        if self.tabs.set_focus(TabId(id)).is_err() {
            return;
        }
        if let Some(tab) = self.tabs.get(TabId(id)) {
            self.net.mode = Self::net_mode_for(tab.trust);
        }
        self.sync_warden();
        self.cursor.tab = id;
        self.log(LogLevel::Info, "warden", &format!("focus → tab #{id:02}"));
    }

    pub fn close(&mut self, id: u64) {
        let Ok(removed) = self.tabs.close(TabId(id)) else {
            return;
        };
        for handle in self.handles.clone() {
            if removed.contains(&handle.tab) {
                let _ = self.vmm.stop(handle);
                self.handles.retain(|h| h.tab != handle.tab);
                self.last_balloon.remove(&handle.tab.0);
                self.travel = TimeTravel::new(self.config.history_s * 1_000);
            }
        }
        self.sync_warden();
        self.log(
            LogLevel::Warn,
            "vmm",
            &format!(
                "closed tab #{id:02}: {} VM(s) stopped, RAM zeroed, overlay dropped",
                removed.len()
            ),
        );
    }

    /// Human steering from the pill.
    pub fn steer(&mut self, text: &str) {
        let directive = self.agent.steer.push(text, self.now_ms);
        self.log(
            LogLevel::Good,
            "agent",
            &format!(
                "steer queued [{}]: {text}",
                match directive.priority {
                    spacetime_agent::steer::Priority::Advisory => "advisory",
                    spacetime_agent::steer::Priority::Constraint => "constraint",
                    spacetime_agent::steer::Priority::Stop => "stop",
                }
            ),
        );
    }

    /// Rewind the focused tab to `at_ms`.
    pub fn rewind(&mut self, tab: u64, at_ms: u64) {
        match self.travel.rewind_to(at_ms) {
            Some(image) => {
                if let Some(handle) = self.handles.iter().find(|h| h.tab.0 == tab).copied() {
                    let _ = self.vmm.restore(handle, &image);
                    let back = (self.now_ms.saturating_sub(at_ms) / 1000).max(1);
                    self.log(
                        LogLevel::Warn,
                        "travel",
                        &format!("tab #{tab:02} rewound {back}s (RAM + registers restored)"),
                    );
                }
            }
            None => self.log(
                LogLevel::Alert,
                "travel",
                "that moment predates the oldest snapshot; cannot restore",
            ),
        }
    }

    fn sync_warden(&mut self) {
        let tabs: Vec<(TabId, bool)> = self
            .tabs
            .walk()
            .into_iter()
            .filter_map(|(_, id)| self.tabs.get(id).map(|t| (id, t.allow_background_compute)))
            .collect();
        if let Some(focus) = self.tabs.focus() {
            self.warden.apply_focus(focus, &tabs);
        }
    }

    /// Advance the whole machine by one tick.
    pub fn tick(&mut self) {
        self.ticks += 1;
        self.now_ms += self.config.tick_ms;

        self.vmm.tick();
        self.capture_history();
        self.simulate_flows();
        self.agent_tick();
        self.cursor.advance(self.config.tick_ms);
        self.demo_focus_rotation();
    }

    fn capture_history(&mut self) {
        if self.config.snapshot_every == 0 || self.ticks % self.config.snapshot_every != 0 {
            return;
        }
        let Some(focus) = self.tabs.focus() else {
            return;
        };
        let Some(handle) = self.handles.iter().find(|h| h.tab == focus).copied() else {
            return;
        };
        if let Ok(image) = self.vmm.snapshot(handle) {
            if self.travel.snapshots().is_empty() {
                self.travel.seed(image.clone());
            }
            if self.travel.capture(&image, self.now_ms).is_some()
                && self.ticks % (self.config.snapshot_every * 5) == 0
            {
                let marker = if self.ticks % (self.config.snapshot_every * 10) == 0 {
                    "File created"
                } else {
                    "Outbound connection"
                };
                self.travel.mark(marker);
            }
        }
        let stats = self.vmm.stats(handle).unwrap_or_default();
        let previous = self.last_balloon.get(&focus.0).copied().unwrap_or(0);
        let reclaimed = stats.balloon_reclaimed_mb.saturating_sub(previous);
        self.last_balloon.insert(focus.0, stats.balloon_reclaimed_mb);
        self.warden
            .report_balloon(focus, stats.resident_mb, reclaimed);
    }

    fn simulate_flows(&mut self) {
        if self.ticks % 3 != 0 {
            return;
        }
        let Some(focus) = self.tabs.focus() else {
            return;
        };
        let tab = focus.0;
        let flow = match self.ticks % 12 {
            0 => Flow {
                dst: Ipv4Addr::new(185, 220, 101, 1),
                port: 4444,
                proto: Proto::Tcp,
                process: "/tmp/.hidden_miner".to_string(),
                hostname: Some("c2.unknown-host.ru".to_string()),
                bytes: 0,
            },
            3 => Flow::tcp(Ipv4Addr::new(104, 18, 1, 1), 443, "chrome").with_host("opencut.app"),
            6 => Flow::tcp(Ipv4Addr::new(52, 10, 0, 1), 443, "chrome").with_host("cdn.example"),
            _ => Flow {
                dst: Ipv4Addr::new(10, 0, 0, 2),
                port: 53,
                proto: Proto::Dns,
                process: "systemd-resolved".to_string(),
                hostname: Some("svc.local".to_string()),
                bytes: 120,
            },
        };
        let verdict = self.net.judge(tab, self.now_ms, &flow);
        match verdict {
            Verdict::Sinkhole { .. } => self.log(
                LogLevel::Alert,
                "honey-duct",
                &format!(
                    "tab #{tab:02} → {} [{}] trapped in sinkhole",
                    flow.key(),
                    spacetime_net::GeoIp::label(flow.dst)
                ),
            ),
            Verdict::Block { reason } => self.log(
                LogLevel::Warn,
                "net",
                &format!("tab #{tab:02} → {} blocked: {reason}", flow.key()),
            ),
            Verdict::Allow { .. } => {}
        }
    }

    fn agent_tick(&mut self) {
        let Some(focus) = self.tabs.focus() else {
            return;
        };
        let tab = focus.0;
        let screen = self.capture_screen(tab);
        let outcome = self.agent.tick(screen, self.now_ms);
        for event in self.agent.drain_events() {
            match event {
                AgentEvent::SnapshotRequested { reason, .. } => {
                    self.log(LogLevel::Warn, "agent", &format!("CoW snapshot before {reason}"));
                }
                AgentEvent::RollbackRequested { reason, .. } => {
                    self.travel.mark("AI risky action");
                    self.log(LogLevel::Alert, "agent", &format!("auto-rewind: {reason}"));
                    let at = self.now_ms.saturating_sub(4_000);
                    self.rewind(tab, at);
                }
                AgentEvent::InputInjected { action, .. } => self.inject(action, tab),
                AgentEvent::ContextFlushed { session } => self.log(
                    LogLevel::Good,
                    "agent",
                    &format!("context flushed → session #{session} (0 tokens carried over)"),
                ),
            }
        }
        match outcome {
            TickOutcome::Completed { node, next } => self.log(
                LogLevel::Good,
                "agent",
                &format!(
                    "node {node} done{}",
                    next.map(|n| format!(" → next {n}")).unwrap_or_default()
                ),
            ),
            TickOutcome::Finished => self.log(LogLevel::Good, "agent", "mission complete"),
            TickOutcome::Stopped => self.log(LogLevel::Warn, "agent", "stopped by operator"),
            _ => {}
        }
    }

    /// Build a Set-of-Mark screen for a tab (stands in for framebuffer capture
    /// plus OmniParser in the real engine).
    pub fn capture_screen(&self, tab: u64) -> Screen {
        let trust = self
            .tabs
            .get(TabId(tab))
            .map(|t| t.trust)
            .unwrap_or(Trust::Chameleon);
        let mut elements = vec![
            MarkedElement {
                mark: 0,
                kind: ElementKind::MenuItem,
                label: "File".to_string(),
                rect: (16, 12, 48, 22),
                enabled: true,
            },
            MarkedElement {
                mark: 0,
                kind: ElementKind::Button,
                label: "Import".to_string(),
                rect: (120, 64, 96, 32),
                enabled: true,
            },
            MarkedElement {
                mark: 0,
                kind: ElementKind::Button,
                label: "Split".to_string(),
                rect: (860, 520, 80, 30),
                enabled: true,
            },
            MarkedElement {
                mark: 0,
                kind: ElementKind::Button,
                label: "Delete".to_string(),
                rect: (960, 520, 80, 30),
                enabled: trust == Trust::Chameleon,
            },
            MarkedElement {
                mark: 0,
                kind: ElementKind::TextField,
                label: "Search".to_string(),
                rect: (320, 12, 240, 22),
                enabled: true,
            },
            MarkedElement {
                mark: 0,
                kind: ElementKind::Button,
                label: "Export MP4".to_string(),
                rect: (1100, 660, 120, 34),
                enabled: true,
            },
        ];
        if trust == Trust::Disposable {
            elements.push(MarkedElement {
                mark: 0,
                kind: ElementKind::Button,
                label: "Run (untrusted)".to_string(),
                rect: (640, 360, 140, 34),
                enabled: true,
            });
        }
        Screen::from_elements(tab, 1280, 720, elements)
    }

    /// Apply an action through virtio-input and move the ghost cursor.
    fn inject(&mut self, action: ProposedAction, tab: u64) {
        match &action {
            ProposedAction::Click { mark, why } | ProposedAction::Type { mark, why, .. } => {
                let screen = self.capture_screen(tab);
                if let Some(element) = screen.find(*mark) {
                    let (cx, cy) = element.center();
                    self.cursor.tab = tab;
                    self.cursor.move_to(cx as f64, cy as f64, why);
                    self.log(
                        LogLevel::Debug,
                        "virtio-input",
                        &format!("click #{} \"{}\" — {}", mark, element.label, why),
                    );
                }
            }
            ProposedAction::Key { combo, .. } => {
                self.log(LogLevel::Debug, "virtio-input", &format!("key {combo}"));
            }
            _ => {}
        }
    }

    fn demo_focus_rotation(&mut self) {
        let every = self.config.demo_focus_every;
        if every == 0 {
            return;
        }
        if self.demo_switch_in > 0 {
            self.demo_switch_in -= 1;
            return;
        }
        self.demo_switch_in = every;
        let order = self.tabs.walk();
        if order.len() <= 1 {
            return;
        }
        let focus = self.tabs.focus();
        let index = focus
            .and_then(|f| order.iter().position(|(_, id)| *id == f))
            .map(|i| (i + 1) % order.len())
            .unwrap_or(0);
        self.focus(order[index].1 .0);
    }

    /// Run `n` ticks.
    pub fn run(&mut self, n: u64) {
        for _ in 0..n {
            self.tick();
        }
    }

    /// Number of live VMs (equals live tab count when healthy).
    pub fn vm_count(&self) -> usize { self.handles.len() }

    pub fn last_tab_id(&self) -> Option<u64> { self.tabs.walk().last().map(|(_, id)| id.0) }

    /// Snapshot of the whole workspace as JSON (wire format for any UI).
    pub fn to_json(&self) -> Value {
        let logs: Vec<Value> = self
            .log
            .iter()
            .rev()
            .take(40)
            .map(|line| {
                Value::obj()
                    .with("at_ms", line.at_ms)
                    .with("level", line.level.label())
                    .with("channel", line.channel)
                    .with("text", line.text.as_str())
            })
            .collect();
        Value::obj()
            .with("now_ms", self.now_ms)
            .with("tick", self.ticks)
            .with("tabs", self.tabs.to_json())
            .with("travel", self.travel.to_json())
            .with("net", self.net.to_json())
            .with("warden", self.warden.to_json())
            .with("agent", self.agent.to_json(self.now_ms))
            .with(
                "cursor",
                Value::obj()
                    .with("tab", self.cursor.tab)
                    .with("x", self.cursor.x.round())
                    .with("y", self.cursor.y.round())
                    .with("label", self.cursor.label.as_str())
                    .with("moving", self.cursor.moving),
            )
            .with(
                "zygote",
                Value::obj()
                    .with("state", self.zygote.state().label())
                    .with("boot_ms", self.zygote.boot_ms() as u64)
                    .with("clones", self.zygote.clones_served())
                    .with("shared_ram_mb", self.zygote.shared_ram_mb()),
            )
            .with("log", Value::Arr(logs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_scene_boots_tabs_from_zygote() {
        let engine = Spacetime::demo(EngineConfig::default());
        assert_eq(engine.tabs.live_count(), 3);        assert_eq(engine.vm_count(), 3);        assert_eq(engine.zygote.clones_served(), 3);        assert(engine.log.iter().any(|l| l.text.contains("zygote ready")));    }

    #[test]
    fn focus_switches_throttle_the_old_tab() {
        let mut engine = Spacetime::demo(EngineConfig::default());
        let first = engine.tabs.focus().unwrap().0;
        let other = engine
            .tabs
            .walk()
            .into_iter()
            .map(|(_, id)| id.0)
            .find(|id| *id != first)
            .unwrap();
        engine.focus(other);
        assert_eq(engine.tabs.get(TabId(other)).unwrap().state, spacetime_core::TabState::Running);        assert_eq(engine.warden.sink().limits[&TabId(first)].cpu_permille, 50);    }

    #[test]
    fn ticking_produces_snapshots_and_net_events() {
        let mut engine = Spacetime::demo(EngineConfig::default());
        engine.run(20);
        assert(engine.travel.len() > 0);        assert(engine.travel.window().1 > 0);        assert(engine.net.stats().entries() > 0);    }

    #[test]
    fn ghost_cursor_moves_toward_target() {
        let mut engine = Spacetime::demo(EngineConfig::default());
        engine.run(6);
        engine.cursor.move_to(900.0, 400.0, "test");
        engine.cursor.advance(400);
        assert(engine.cursor.x > 640.0);    }

    #[test]
    fn branch_creates_child_tab_and_vm() {
        let mut engine = Spacetime::demo(EngineConfig::default());
        let parent = engine.tabs.focus().unwrap().0;
        let child = engine.branch(parent).unwrap();
        assert_eq(engine.tabs.get(TabId(child)).unwrap().parent, Some(TabId(parent)));        assert_eq(engine.vm_count(), 4);    }

    #[test]
    fn close_stops_vms() {
        let mut engine = Spacetime::demo(EngineConfig::default());
        let target = engine.tabs.walk()[0].1 .0;
        engine.close(target);
        assert_eq(engine.vm_count(), 2);        assert_eq(engine.tabs.live_count(), 2);    }

    #[test]
    fn steering_reaches_the_agent() {
        let mut engine = Spacetime::demo(EngineConfig::default());
        engine.steer("use 9:16");
        assert_eq(engine.agent.steer.pending(), 1);        engine.tick();
        assert_eq(engine.agent.steer.pending(), 0);    }

    #[test]
    fn rewind_restores_focused_tab() {
        let mut engine = Spacetime::demo(EngineConfig::default());
        engine.run(10);
        let target = engine.now_ms.saturating_sub(2_000);
        let focus = engine.tabs.focus().unwrap().0;
        engine.rewind(focus, target);
        assert(engine.log.iter().any(|l| l.text.contains("rewound")));    }

    #[test]
    fn json_snapshot_has_all_sections() {
        let mut engine = Spacetime::demo(EngineConfig::default());
        engine.run(5);
        let json = engine.to_json();
        for key in ["tabs", "travel", "net", "warden", "agent", "cursor", "zygote", "log"] {
            assert(json.get(key).is_some(), "missing {key}");        }
        assert(json.u64_of("now_ms") > 0);        // Must round-trip through the JSON writer used by the daemon.
        let text = json.to_string();
        assert(Value::parse(&text).is_ok());    }

    #[test]
    fn long_session_does_not_panic() {
        let mut engine = Spacetime::demo(EngineConfig::default());
        engine.run(300);
        assert(engine.now_ms >= 120_000);        assert(engine.travel.len() > 5);    }
}
