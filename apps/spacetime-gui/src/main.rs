//! Native Rust mixed-workspace shell (`eframe` / `egui`) for Spacetime.
//!
//! Security & resource invariants enforced here:
//! - Terminal tab uses `vt100::Parser` over piped streams; terminal escape sequences never reach
//!   the host terminal.
//! - AI surface tab displays RAM-only `SnapshotOnAction` textures from `SurfaceBroker` (read-only
//!   `SurfaceReader` path; zero screenshots written to disk).
//! - Hidden AI tabs remain `LivenessState::Running` without `Rendering`.
//! - Irreversible actions (e.g. `publish_video`) enter the Ring -3 human blocker queue instead of
//!   executing autonomously.

use eframe::egui;
use spacetime_ctl::security::{HostVault, RingMinus3Policy};
use spacetime_ctl::spacetime_exec::governor::{GovernorPolicy, GovernorSample, ResourceGovernor};
use spacetime_ctl::spacetime_exec::surface::{default_mixed_workspace_descriptors, SurfaceBroker};
use spacetime_ctl::spacetime_types::policy::{CapabilityToken, RuleEffect};
use spacetime_ctl::spacetime_types::supervisor::{BlockerItem, BlockerStatus, SteeringMessage};
use spacetime_ctl::spacetime_types::surface::{GovernorAction, SemanticField};
use spacetime_ctl::spacetime_types::{
    Action, ActionOp, EgressClass, EventSeq, GoalId, LivenessState, PreviewMode, SpaceId,
    SurfaceId, SurfaceKind, Taint, TrustTier, WorkspaceBinding, WorkspaceId,
};
use spacetime_ctl::workspace::{default_busybox, default_kernel, prepare_guest, PreparedGuest};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Duration;

struct Session {
    child: Child,
    output: Receiver<Vec<u8>>,
    input: SyncSender<Vec<u8>>,
    // The image remains available until the VM exits or is stopped.
    _image: PreparedGuest,
    exited: Option<std::process::ExitStatus>,
}

fn forward_output(mut reader: impl Read + Send + 'static, sender: SyncSender<Vec<u8>>) {
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if sender.send(buffer[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

fn queue_input(sender: &SyncSender<Vec<u8>>, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > 4096 {
        return Err("Input is limited to 4096 bytes at a time".into());
    }
    sender
        .try_send(bytes.to_vec())
        .map_err(|error| match error {
            mpsc::TrySendError::Full(_) => "Guest input queue is full".to_string(),
            mpsc::TrySendError::Disconnected(_) => "Guest input is closed".to_string(),
        })
}

impl Session {
    fn start(kernel: &str, busybox: &str, app: &str) -> Result<Self, String> {
        if kernel.is_empty() || busybox.is_empty() {
            return Err("Choose a Linux kernel and static BusyBox first.".into());
        }
        let image = prepare_guest(busybox, (!app.is_empty()).then_some(app))
            .map_err(|error| error.to_string())?;
        let image_path = image.path();
        let child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--worker")
            .args([
                "space",
                "boot",
                "--experimental",
                "--kernel",
                kernel,
                "--initramfs",
            ])
            .arg(image_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Could not start workspace: {error}"))?;
        Ok(Self::from_child(child, image))
    }

    fn from_child(mut child: Child, image: PreparedGuest) -> Self {
        let (input, input_rx) = mpsc::sync_channel::<Vec<u8>>(16);
        if let Some(mut stdin) = child.stdin.take() {
            thread::spawn(move || {
                while let Ok(bytes) = input_rx.recv() {
                    if stdin
                        .write_all(&bytes)
                        .and_then(|()| stdin.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        let (sender, output) = mpsc::sync_channel(64);
        // Both streams are piped by construction. Reader threads never touch the UI or execute
        // terminal escape sequences; bytes are parsed into text by vt100 in the UI thread.
        if let Some(stdout) = child.stdout.take() {
            forward_output(stdout, sender.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            forward_output(stderr, sender);
        }
        Self {
            child,
            output,
            input,
            _image: image,
            exited: None,
        }
    }

    fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        queue_input(&self.input, bytes)
    }

    fn send_line(&mut self, text: &str) -> Result<(), String> {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(b'\n');
        self.send_bytes(&bytes)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None) | Err(_)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

struct WorkspaceApp {
    kernel: String,
    busybox: String,
    app: String,
    input: String,
    status: String,
    terminal: vt100::Parser,
    session: Option<Session>,
    active_tab: String,
    surface_broker: SurfaceBroker,
    governor: ResourceGovernor,
    policy: RingMinus3Policy,
    vault: HostVault,
    capability: CapabilityToken,
    blockers: Vec<BlockerItem>,
    steering: Vec<SteeringMessage>,
    steering_input: String,
    turn_counter: u64,
    ai_step: u8,
    snapshot_texture: Option<egui::TextureHandle>,
    last_snapshot_seq: Option<u64>,
    last_governor_action: GovernorAction,
}

impl Default for WorkspaceApp {
    fn default() -> Self {
        let mut surface_broker = SurfaceBroker::new();
        for descriptor in default_mixed_workspace_descriptors() {
            let _ = surface_broker.register_workspace(descriptor);
        }
        let _ = surface_broker.apply_supervisor_field_update(
            "ws-ai-upload",
            SemanticField {
                name: "status".to_string(),
                field_type: "text".to_string(),
                value: "IDLE_READY".to_string(),
                is_secret: false,
            },
            "AI surface ready (mock dry-run)",
            "init-0",
        );

        let mut vault = HostVault::new();
        let _ = vault.insert_secret("upload_token", "MOCK_HOST_ONLY_TOKEN_999");
        let _ = vault.plant_canary("gui_canary", "SPACETIME_GUI_CANARY_01");

        let mut allowed_ops = BTreeSet::new();
        allowed_ops.insert("ui_set_field".to_string());
        allowed_ops.insert("irreversible_effect".to_string());
        let capability = CapabilityToken {
            token_id: "tok-gui-upload".to_string(),
            goal_id: GoalId::new("goal-upload-demo"),
            allowed_ops,
            max_spend_fake_credits_per_day: 100,
            max_posts_per_day: 1,
            allow_irreversible_delete: false,
            allow_publish: true,
            expires_at_turn: 500,
            trust_tier: TrustTier::ArkCell,
            egress_class: EgressClass::None,
        };

        Self {
            kernel: default_kernel().unwrap_or_default(),
            busybox: default_busybox().unwrap_or_default(),
            app: String::new(),
            input: String::new(),
            status: "Not running".into(),
            terminal: vt100::Parser::new(30, 100, 0),
            session: None,
            active_tab: "ws-human-terminal".to_string(),
            surface_broker,
            governor: ResourceGovernor::new(GovernorPolicy::baseline_4gb_potato()),
            policy: RingMinus3Policy::default(),
            vault,
            capability,
            blockers: Vec::new(),
            steering: Vec::new(),
            steering_input: String::new(),
            turn_counter: 1,
            ai_step: 0,
            snapshot_texture: None,
            last_snapshot_seq: None,
            last_governor_action: GovernorAction::Allow,
        }
    }
}

impl WorkspaceApp {
    fn switch_tab(&mut self, workspace_id: &str) {
        if self.active_tab == workspace_id {
            return;
        }
        if self.surface_broker.switch_active_tab(workspace_id).is_ok() {
            self.active_tab = workspace_id.to_string();
            self.last_snapshot_seq = None;
            self.refresh_governor();
        }
    }

    fn refresh_governor(&mut self) {
        let rss_kb = ResourceGovernor::sample_current_process_rss_kb().unwrap_or(64_000);
        if let Some(desc) = self.surface_broker.reader().descriptor(&self.active_tab) {
            let eval = self.governor.evaluate_workspace(
                desc,
                &GovernorSample {
                    total_rss_kb: rss_kb,
                    active_heavy_rendering: self.surface_broker.reader().rendering_count(),
                    running_cells: 2,
                },
                true,
            );
            self.last_governor_action = eval.action;
        }
    }

    fn advance_ai_upload_step(&mut self) {
        self.turn_counter = self.turn_counter.saturating_add(1);
        match self.ai_step {
            0 => {
                let _ = self.surface_broker.apply_supervisor_field_update(
                    "ws-ai-upload",
                    SemanticField {
                        name: "title".to_string(),
                        field_type: "text".to_string(),
                        value: "Spacetime Offline Demo".to_string(),
                        is_secret: false,
                    },
                    "AI filled title & description",
                    "ev-title",
                );
                let _ = self.surface_broker.apply_supervisor_field_update(
                    "ws-ai-upload",
                    SemanticField {
                        name: "thumbnail".to_string(),
                        field_type: "swatch".to_string(),
                        value: "slate-emerald-01".to_string(),
                        is_secret: false,
                    },
                    "AI attached local thumbnail swatch",
                    "ev-thumb",
                );
                self.ai_step = 1;
            }
            1 => {
                let resolved = self
                    .vault
                    .resolve_at_boundary("@vault:upload_token")
                    .unwrap_or("FALLBACK_SECRET")
                    .to_string();
                let _ = self.surface_broker.apply_supervisor_field_update(
                    "ws-ai-upload",
                    SemanticField {
                        name: "upload_token".to_string(),
                        field_type: "password".to_string(),
                        value: resolved,
                        is_secret: true,
                    },
                    "Host vault injected @vault:upload_token (masked)",
                    "ev-secret",
                );
                self.ai_step = 2;
            }
            _ => {
                let binding = WorkspaceBinding {
                    workspace_id: WorkspaceId::new("ws-ai-upload"),
                    surface_id: SurfaceId::new("surface-ai-upload"),
                    surface_kind: SurfaceKind::Framebuffer,
                    preview_mode: PreviewMode::SnapshotOnAction,
                    liveness: LivenessState::Running,
                };
                if let Ok(action) = Action::new(
                    EventSeq(self.turn_counter),
                    SpaceId::new("space-gui"),
                    GoalId::new("goal-upload-demo"),
                    binding,
                    ActionOp::IrreversibleEffect {
                        effect: "publish_video".to_string(),
                        target: "mock-channel".to_string(),
                    },
                    100,
                    Some("spec-oracle-v1".to_string()),
                ) {
                    let decision = self.policy.evaluate_action(
                        &action,
                        &self.capability,
                        self.turn_counter,
                        Taint::UntrustedToolOutput,
                        false,
                    );
                    if decision.effect == RuleEffect::QueueBlocker {
                        let blocker_id = format!("blk-{:03}", self.blockers.len() + 1);
                        self.blockers.push(BlockerItem {
                            id: blocker_id,
                            goal_id: "goal-upload-demo".to_string(),
                            action_op_kind: "irreversible_effect".to_string(),
                            effect: "publish_video".to_string(),
                            target: "mock-channel".to_string(),
                            reason: decision.reason,
                            taint: decision.action_taint,
                            created_turn: self.turn_counter,
                            status: BlockerStatus::Pending,
                        });
                        let _ = self.surface_broker.apply_supervisor_field_update(
                            "ws-ai-upload",
                            SemanticField {
                                name: "status".to_string(),
                                field_type: "text".to_string(),
                                value: "QUEUED_HUMAN_BLOCKER".to_string(),
                                is_secret: false,
                            },
                            "Publish queued behind Ring -3 human blocker",
                            "ev-blocker",
                        );
                    }
                }
            }
        }
        self.last_snapshot_seq = None;
        self.refresh_governor();
    }

    fn resolve_blocker(&mut self, idx: usize, approve: bool) {
        if let Some(blocker) = self.blockers.get_mut(idx) {
            blocker.status = if approve {
                BlockerStatus::Approved
            } else {
                BlockerStatus::Denied
            };
            let caption = if approve {
                "Human approved mock publish"
            } else {
                "Human denied mock publish"
            };
            let value = if approve {
                "PUBLISHED_MOCK_ONLY"
            } else {
                "DENIED_BY_HUMAN"
            };
            let _ = self.surface_broker.apply_supervisor_field_update(
                "ws-ai-upload",
                SemanticField {
                    name: "status".to_string(),
                    field_type: "text".to_string(),
                    value: value.to_string(),
                    is_secret: false,
                },
                caption,
                "ev-resolve",
            );
            self.last_snapshot_seq = None;
        }
    }

    fn sync_snapshot_texture(&mut self, context: &egui::Context) {
        let Some(frame) = self.surface_broker.reader().active_frame("ws-ai-upload") else {
            return;
        };
        if self.last_snapshot_seq == Some(frame.seq) {
            return;
        }
        let image =
            egui::ColorImage::from_rgba_unmultiplied([frame.width, frame.height], &frame.rgba);
        match &mut self.snapshot_texture {
            Some(handle) => handle.set(image, egui::TextureOptions::NEAREST),
            None => {
                self.snapshot_texture = Some(context.load_texture(
                    "ws-ai-upload-ram-snapshot",
                    image,
                    egui::TextureOptions::NEAREST,
                ));
            }
        }
        self.last_snapshot_seq = Some(frame.seq);
    }
}

impl eframe::App for WorkspaceApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        let mut finished = false;
        if let Some(session) = &mut self.session {
            let mut disconnected = false;
            for _ in 0..64 {
                match session.output.try_recv() {
                    Ok(bytes) => self.terminal.process(&bytes),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
            if session.exited.is_none() {
                match session.child.try_wait() {
                    Ok(status) => session.exited = status,
                    Err(error) => {
                        self.status = format!("Could not check workspace: {error}");
                        finished = true;
                    }
                }
            }
            if let Some(status) = session.exited {
                if disconnected {
                    self.status = format!("Workspace exited: {status}");
                    finished = true;
                }
            }
            if !finished {
                context.request_repaint_after(Duration::from_millis(100));
            }
        }
        if finished {
            self.session = None;
        }

        if self.active_tab == "ws-ai-upload" {
            self.sync_snapshot_texture(context);
        }

        let mut switch_target: Option<String> = None;
        egui::TopBottomPanel::top("mixed_workspace_tabs").show(context, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Spacetime");
                ui.separator();
                let descriptors = self.surface_broker.reader().workspaces();
                for desc in descriptors {
                    let id = desc.workspace_id.as_str();
                    let label = format!(
                        "{} [{:?} · {:?}]",
                        id, desc.liveness, desc.preview_mode
                    );
                    if ui
                        .selectable_label(self.active_tab == id, label)
                        .clicked()
                    {
                        switch_target = Some(id.to_string());
                    }
                }
            });
            let rss_text = ResourceGovernor::sample_current_process_rss_kb()
                .map(|kb| format!("{kb} KiB"))
                .unwrap_or_else(|| "unavailable".to_string());
            ui.horizontal(|ui| {
                ui.colored_label(
                    egui::Color32::LIGHT_GREEN,
                    ResourceGovernor::intent_ribbon(
                        2,
                        1,
                        0,
                        self.policy.daily_fake_spend_used,
                        "mock_offline",
                    ),
                );
                ui.separator();
                ui.small(format!(
                    "resource_gate: unmeasured_target_4gb · performance_claims: unmeasured · rss: {rss_text} · governor: {:?}",
                    self.last_governor_action
                ));
            });
        });

        if let Some(target) = switch_target {
            self.switch_tab(&target);
            if self.active_tab == "ws-ai-upload" {
                self.sync_snapshot_texture(context);
            }
        }

        egui::SidePanel::right("supervisor_telemetry")
            .default_width(320.0)
            .show(context, |ui| {
                ui.heading("Supervisor & Ring -3");
                ui.small(format!(
                    "Disk thumbnail writes: {} · Rendering tabs: {} · Canary alarms: {}",
                    self.surface_broker.reader().disk_writes_performed(),
                    self.surface_broker.reader().rendering_count(),
                    self.vault.alarms().len()
                ));
                ui.separator();

                ui.label("Non-blocking steering (TTL 4 turns)");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.steering_input);
                    if ui.button("Queue").clicked() && !self.steering_input.trim().is_empty() {
                        let text = self.steering_input.trim().to_string();
                        self.steering.push(SteeringMessage {
                            id: format!("steer-{}", self.steering.len() + 1),
                            text,
                            created_turn: self.turn_counter,
                            ttl_turns: 4,
                        });
                        self.steering_input.clear();
                    }
                });
                for msg in self.steering.iter().rev().take(3) {
                    ui.small(format!("- [t{}] {}", msg.created_turn, msg.text));
                }
                ui.separator();

                ui.label("Human Blocker Queue");
                if self.blockers.is_empty() {
                    ui.small("No pending irreversible actions.");
                } else {
                    let mut decision: Option<(usize, bool)> = None;
                    for (idx, blocker) in self.blockers.iter().enumerate() {
                        ui.group(|ui| {
                            ui.label(format!(
                                "{}: {} -> {} [{:?}]",
                                blocker.id, blocker.effect, blocker.target, blocker.status
                            ));
                            ui.small(&blocker.reason);
                            if blocker.status == BlockerStatus::Pending {
                                ui.horizontal(|ui| {
                                    if ui.button("Approve").clicked() {
                                        decision = Some((idx, true));
                                    }
                                    if ui.button("Deny").clicked() {
                                        decision = Some((idx, false));
                                    }
                                });
                            }
                        });
                    }
                    if let Some((idx, approve)) = decision {
                        self.resolve_blocker(idx, approve);
                    }
                }
                ui.separator();

                ui.label("Cortex SemanticView (secrets masked)");
                if let Ok(view) = self.surface_broker.cortex_semantic_view("ws-ai-upload") {
                    for field in &view.fields {
                        ui.monospace(format!(
                            "{} ({}) = {}",
                            field.name, field.field_type, field.value
                        ));
                    }
                }
            });

        egui::CentralPanel::default().show(context, |ui| {
            if self.active_tab == "ws-ai-upload" {
                ui.heading("AI Upload Workspace (RAM-only SnapshotOnAction)");
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "Untrusted Ark cell · net=none · @vault:upload_token resolved only on host boundary",
                );
                ui.horizontal(|ui| {
                    if ui.button("Advance AI upload step").clicked() {
                        self.advance_ai_upload_step();
                        self.sync_snapshot_texture(context);
                    }
                    if ui.button("Switch to Human Terminal (AI keeps RUNNING)").clicked() {
                        self.switch_tab("ws-human-terminal");
                    }
                });
                ui.separator();
                if let Some(texture) = &self.snapshot_texture {
                    ui.image(egui::load::SizedTexture {
                        id: texture.id(),
                        size: texture.size_vec2(),
                    });
                } else {
                    ui.label("No RAM snapshot captured yet.");
                }
                if let Some(frame) = self.surface_broker.reader().active_frame("ws-ai-upload") {
                    ui.small(format!(
                        "Snapshot #{} · mode={:?} · caption={}",
                        frame.seq, frame.preview_mode, frame.caption
                    ));
                }
                return;
            }

            ui.label("Experimental offline VM terminal · trusted software only");
            ui.colored_label(
                egui::Color32::YELLOW,
                "Not a hardened malware sandbox. Host terminal escape sequences are blocked via vt100.",
            );
            ui.separator();
            ui.add_enabled_ui(self.session.is_none(), |ui| {
                egui::Grid::new("inputs").num_columns(2).show(ui, |ui| {
                    ui.label("Linux kernel");
                    ui.text_edit_singleline(&mut self.kernel);
                    ui.end_row();
                    ui.label("Static BusyBox");
                    ui.text_edit_singleline(&mut self.busybox);
                    ui.end_row();
                    ui.label("Static terminal app (optional)");
                    ui.text_edit_singleline(&mut self.app);
                    ui.end_row();
                });
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.session.is_none(), egui::Button::new("Start workspace"))
                    .clicked()
                {
                    self.terminal = vt100::Parser::new(30, 100, 0);
                    match Session::start(self.kernel.trim(), self.busybox.trim(), self.app.trim()) {
                        Ok(session) => {
                            self.status = "Starting offline workspace…".into();
                            self.session = Some(session);
                        }
                        Err(error) => self.status = error,
                    }
                }
                if ui
                    .add_enabled(self.session.is_some(), egui::Button::new("Stop workspace"))
                    .clicked()
                {
                    self.session = None;
                    self.status = "Workspace stopped".into();
                }
                ui.label(&self.status);
            });
            ui.separator();
            ui.label("Console (line input only; interactive full-screen apps are not supported)");
            egui::ScrollArea::both()
                .stick_to_bottom(true)
                .max_height(420.0)
                .show(ui, |ui| {
                    ui.monospace(self.terminal.screen().contents());
                });
            ui.horizontal(|ui| {
                let response = ui.add_enabled(
                    self.session.is_some(),
                    egui::TextEdit::singleline(&mut self.input).hint_text("Enter a command"),
                );
                let send = ui
                    .add_enabled(self.session.is_some(), egui::Button::new("Send"))
                    .clicked()
                    || (response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter)));
                if send {
                    if let Some(session) = &mut self.session {
                        if let Err(error) = session.send_line(&self.input) {
                            self.status = format!("Input failed: {error}");
                        }
                        self.input.clear();
                        response.request_focus();
                    }
                }
            });
            ui.horizontal(|ui| {
                for (label, bytes) in [
                    ("Interrupt (Ctrl+C)", &b"\x03"[..]),
                    ("EOF (Ctrl+D)", &b"\x04"[..]),
                ] {
                    if ui
                        .add_enabled(self.session.is_some(), egui::Button::new(label))
                        .clicked()
                    {
                        if let Some(session) = &mut self.session {
                            if let Err(error) = session.send_bytes(bytes) {
                                self.status = format!("Input failed: {error}");
                            }
                        }
                    }
                }
            });
            ui.small("Use 'app' to run the supplied program, or 'exit' to shut down the guest.");
        });
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--worker") {
        if let Err(error) = spacetime_ctl::run(&args[1..]) {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    eframe::run_native(
        "Spacetime",
        eframe::NativeOptions::default(),
        Box::new(|_creation_context| Box::<WorkspaceApp>::default()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_guest_inputs_before_starting() {
        assert!(Session::start("", "busybox", "").is_err());
        assert!(Session::start("kernel", "", "").is_err());
    }

    #[test]
    fn input_is_bounded_and_never_waits_for_a_busy_guest() {
        let (sender, receiver) = mpsc::sync_channel(1);
        assert!(queue_input(&sender, &[0; 4097]).is_err());
        queue_input(&sender, b"hello").unwrap();
        assert!(queue_input(&sender, b"again").is_err());
        assert_eq!(receiver.recv().unwrap(), b"hello");
        drop(receiver);
        assert!(queue_input(&sender, b"closed").is_err());
    }

    #[test]
    fn output_reader_forwards_data_and_closes() {
        let (sender, receiver) = mpsc::sync_channel(2);
        forward_output(std::io::Cursor::new(b"hello".to_vec()), sender);
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            b"hello"
        );
        assert!(receiver.recv_timeout(Duration::from_secs(2)).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn session_exchanges_input_and_cleans_its_guest_image() {
        let dir = std::env::temp_dir().join(format!("spacetime-gui-test-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let source = dir.join("static-elf");
        let mut elf = vec![0u8; 120];
        elf[..6].copy_from_slice(b"\x7fELF\x02\x01");
        elf[18..20].copy_from_slice(&62u16.to_le_bytes());
        elf[32..40].copy_from_slice(&64u64.to_le_bytes());
        elf[54..56].copy_from_slice(&56u16.to_le_bytes());
        elf[56..58].copy_from_slice(&1u16.to_le_bytes());
        elf[64..68].copy_from_slice(&1u32.to_le_bytes());
        std::fs::write(&source, elf).unwrap();
        let image = prepare_guest(source.to_str().unwrap(), None).unwrap();
        let image_path = image.path();
        let child = Command::new("/bin/sh")
            .args(["-c", "IFS= read -r text; printf 'REPLY:%s\\n' \"$text\""])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut session = Session::from_child(child, image);
        session.send_line("hello").unwrap();
        let output = session.output.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(output, b"REPLY:hello\n");
        drop(session);
        assert!(!image_path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn guest_terminal_escape_sequences_are_not_passed_to_the_host_terminal() {
        let mut parser = vt100::Parser::new(4, 40, 0);
        parser.process(b"before\x1b]52;c;YWJj\x07after");
        let screen = parser.screen().contents();
        assert!(screen.contains("beforeafter"));
        assert!(!screen.contains('\x1b'));
    }

    #[test]
    fn mixed_workspace_app_runs_ai_background_masks_secrets_and_queues_blockers() {
        let mut app = WorkspaceApp::default();
        assert_eq!(app.active_tab, "ws-human-terminal");
        app.advance_ai_upload_step();
        app.advance_ai_upload_step();
        app.advance_ai_upload_step();
        assert_eq!(app.blockers.len(), 1);
        assert_eq!(app.blockers[0].status, BlockerStatus::Pending);

        // Background AI stays RUNNING while Human Terminal is RENDERING.
        assert_eq!(
            app.surface_broker
                .reader()
                .descriptor("ws-ai-upload")
                .map(|d| d.liveness),
            Some(LivenessState::Running)
        );
        // Cortex view masks the vault-injected secret.
        let view = app
            .surface_broker
            .cortex_semantic_view("ws-ai-upload")
            .unwrap();
        assert!(view
            .fields
            .iter()
            .any(|f| f.name == "upload_token" && f.value == "[MASKED]"));

        // Switching tab makes AI tab RENDERING and Human Terminal RUNNING with 0 disk writes.
        app.switch_tab("ws-ai-upload");
        assert_eq!(app.surface_broker.reader().rendering_count(), 1);
        assert_eq!(app.surface_broker.reader().disk_writes_performed(), 0);
        app.resolve_blocker(0, true);
        assert_eq!(app.blockers[0].status, BlockerStatus::Approved);
    }
}
