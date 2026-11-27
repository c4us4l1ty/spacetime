//! Read-only SurfaceBroker and RAM-only snapshot rendering.
//!
//! Security boundary:
//! - A surface consumer (`SurfaceReader`) can ONLY read snapshots and semantic previews; it cannot
//!   inject input into a cell or VM. Input injection happens strictly through the supervisor.
//! - Human display receives rendered RAM-only stills; Cortex receives only `SemanticView` where
//!   `is_secret == true` fields are masked by construction.
//! - Screenshots are kept strictly in RAM ring buffers and wiped on tab close.

use spacetime_types::surface::{
    SemanticField, SemanticView, SnapshotFrame, WorkspaceSurfaceDescriptor,
};
use spacetime_types::{
    sha256_hex, EgressClass, LivenessState, PreviewMode, Result, SpacetimeError, SurfaceId,
    SurfaceKind, TrustTier, WorkspaceId,
};
use std::collections::{BTreeMap, VecDeque};

pub const SNAPSHOT_WIDTH: usize = 360;
pub const SNAPSHOT_HEIGHT: usize = 160;
const MAX_FRAMES_PER_SURFACE: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SurfaceState {
    descriptor: WorkspaceSurfaceDescriptor,
    fields: Vec<SemanticField>,
    frames: VecDeque<SnapshotFrame>,
    next_seq: u64,
    last_caption: String,
    disk_writes_performed: u64,
}

/// Read-only handle passed to UI/console consumers. It exposes no input-injection methods.
#[derive(Debug, Clone)]
pub struct SurfaceReader<'a> {
    broker: &'a SurfaceBroker,
}

impl<'a> SurfaceReader<'a> {
    pub fn workspaces(&self) -> Vec<WorkspaceSurfaceDescriptor> {
        self.broker
            .surfaces
            .values()
            .map(|state| state.descriptor.clone())
            .collect()
    }

    pub fn descriptor(&self, workspace_id: &str) -> Option<&'a WorkspaceSurfaceDescriptor> {
        self.broker
            .surfaces
            .get(workspace_id)
            .map(|state| &state.descriptor)
    }

    pub fn active_frame(&self, workspace_id: &str) -> Option<&'a SnapshotFrame> {
        self.broker
            .surfaces
            .get(workspace_id)
            .and_then(|state| state.frames.back())
    }

    pub fn snapshot_strip(&self, workspace_id: &str) -> Vec<&'a SnapshotFrame> {
        self.broker
            .surfaces
            .get(workspace_id)
            .map(|state| state.frames.iter().collect())
            .unwrap_or_default()
    }

    pub fn active_workspace_id(&self) -> Option<&'a str> {
        self.broker.active_workspace.as_deref()
    }

    pub fn rendering_count(&self) -> usize {
        self.broker
            .surfaces
            .values()
            .filter(|state| state.descriptor.liveness == LivenessState::Rendering)
            .count()
    }

    pub fn disk_writes_performed(&self) -> u64 {
        self.broker
            .surfaces
            .values()
            .map(|state| state.disk_writes_performed)
            .sum()
    }
}

#[derive(Debug, Default)]
pub struct SurfaceBroker {
    surfaces: BTreeMap<String, SurfaceState>,
    active_workspace: Option<String>,
}

impl SurfaceBroker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reader(&self) -> SurfaceReader<'_> {
        SurfaceReader { broker: self }
    }

    pub fn register_workspace(&mut self, descriptor: WorkspaceSurfaceDescriptor) -> Result<()> {
        let key = descriptor.workspace_id.as_str().to_string();
        if self.surfaces.contains_key(&key) {
            return Err(SpacetimeError::new(format!(
                "workspace already registered: {key}"
            )));
        }
        let is_first = self.active_workspace.is_none();
        let mut descriptor = descriptor;
        if is_first {
            self.active_workspace = Some(key.clone());
            descriptor.liveness = LivenessState::Rendering;
        }
        self.surfaces.insert(
            key,
            SurfaceState {
                descriptor,
                fields: Vec::new(),
                frames: VecDeque::with_capacity(MAX_FRAMES_PER_SURFACE),
                next_seq: 0,
                last_caption: "initialized".to_string(),
                disk_writes_performed: 0,
            },
        );
        Ok(())
    }

    /// Switch which tab is actively `Rendering` while keeping background workspaces `Running`.
    pub fn switch_active_tab(&mut self, workspace_id: &str) -> Result<Option<&SnapshotFrame>> {
        if !self.surfaces.contains_key(workspace_id) {
            return Err(SpacetimeError::new(format!(
                "unknown workspace: {workspace_id}"
            )));
        }
        for (id, state) in &mut self.surfaces {
            if id == workspace_id {
                if state.descriptor.liveness != LivenessState::Dead {
                    state.descriptor.liveness = LivenessState::Rendering;
                }
            } else if state.descriptor.liveness == LivenessState::Rendering {
                state.descriptor.liveness = LivenessState::Running;
            }
        }
        self.active_workspace = Some(workspace_id.to_string());
        // Refresh RAM snapshot immediately upon switching to an active non-terminal tab.
        let needs_refresh = self
            .surfaces
            .get(workspace_id)
            .is_some_and(|state| state.descriptor.surface_kind != SurfaceKind::Terminal);
        if needs_refresh {
            self.capture_ram_snapshot(workspace_id, "tab switch refresh", "tab-switch")?;
        }
        Ok(self
            .surfaces
            .get(workspace_id)
            .and_then(|state| state.frames.back()))
    }

    /// Supervisor-only channel: apply a semantic field update inside the isolated app and capture
    /// a fresh RAM-only snapshot if `SnapshotOnAction` / `Rendering`.
    pub fn apply_supervisor_field_update(
        &mut self,
        workspace_id: &str,
        field: SemanticField,
        caption: &str,
        ledger_event_hash: &str,
    ) -> Result<()> {
        let state = self
            .surfaces
            .get_mut(workspace_id)
            .ok_or_else(|| SpacetimeError::new(format!("unknown workspace: {workspace_id}")))?;
        if let Some(existing) = state.fields.iter_mut().find(|f| f.name == field.name) {
            *existing = field;
        } else {
            state.fields.push(field);
        }
        state.last_caption = caption.to_string();
        let mode = state.descriptor.preview_mode;
        if preview_captures_snapshot(mode) {
            self.capture_ram_snapshot(workspace_id, caption, ledger_event_hash)?;
        }
        Ok(())
    }

    pub fn set_preview_mode(&mut self, workspace_id: &str, mode: PreviewMode) -> Result<()> {
        let state = self
            .surfaces
            .get_mut(workspace_id)
            .ok_or_else(|| SpacetimeError::new(format!("unknown workspace: {workspace_id}")))?;
        state.descriptor.preview_mode = mode;
        Ok(())
    }

    /// Sanitized observation channel for Cortex: masks all `is_secret` / `password` fields and
    /// refuses raw screenshot access whenever secrets are present.
    pub fn cortex_semantic_view(&self, workspace_id: &str) -> Result<SemanticView> {
        let state = self
            .surfaces
            .get(workspace_id)
            .ok_or_else(|| SpacetimeError::new(format!("unknown workspace: {workspace_id}")))?;
        let mut contains_secret_fields = false;
        let sanitized_fields = state
            .fields
            .iter()
            .map(|field| {
                let secret_like = field.is_secret
                    || field.field_type.eq_ignore_ascii_case("password")
                    || field.name.to_ascii_lowercase().contains("password")
                    || field.name.to_ascii_lowercase().contains("token");
                if secret_like {
                    contains_secret_fields = true;
                    SemanticField {
                        name: field.name.clone(),
                        field_type: field.field_type.clone(),
                        value: "[MASKED]".to_string(),
                        is_secret: true,
                    }
                } else {
                    field.clone()
                }
            })
            .collect();
        Ok(SemanticView {
            workspace_id: state.descriptor.workspace_id.clone(),
            surface_id: state.descriptor.surface_id.clone(),
            fields: sanitized_fields,
            contains_secret_fields,
            raw_screenshot_allowed_for_cortex: false,
        })
    }

    /// Wipe and drop all RAM-only frames when closing a workspace.
    pub fn close_workspace(&mut self, workspace_id: &str) -> Result<()> {
        let mut state = self
            .surfaces
            .remove(workspace_id)
            .ok_or_else(|| SpacetimeError::new(format!("unknown workspace: {workspace_id}")))?;
        for frame in &mut state.frames {
            frame.rgba.fill(0);
        }
        state.frames.clear();
        if self.active_workspace.as_deref() == Some(workspace_id) {
            self.active_workspace = self.surfaces.keys().next().cloned();
        }
        Ok(())
    }

    fn capture_ram_snapshot(
        &mut self,
        workspace_id: &str,
        caption: &str,
        ledger_event_hash: &str,
    ) -> Result<()> {
        let state = self
            .surfaces
            .get_mut(workspace_id)
            .ok_or_else(|| SpacetimeError::new(format!("unknown workspace: {workspace_id}")))?;
        let seq = state.next_seq;
        state.next_seq = state.next_seq.saturating_add(1);
        let mut field_map = BTreeMap::new();
        for field in &state.fields {
            let shown = if field.is_secret || field.field_type.eq_ignore_ascii_case("password") {
                "********".to_string()
            } else {
                field.value.clone()
            };
            field_map.insert(field.name.clone(), shown);
        }
        let rgba = render_snapshot_rgba(
            SNAPSHOT_WIDTH,
            SNAPSHOT_HEIGHT,
            workspace_id,
            caption,
            &field_map,
            seq,
        );
        if state.frames.len() >= MAX_FRAMES_PER_SURFACE {
            if let Some(mut evicted) = state.frames.pop_front() {
                evicted.rgba.fill(0);
            }
        }
        state.frames.push_back(SnapshotFrame {
            seq,
            workspace_id: WorkspaceId::new(workspace_id),
            surface_id: state.descriptor.surface_id.clone(),
            preview_mode: state.descriptor.preview_mode,
            caption: caption.to_string(),
            width: SNAPSHOT_WIDTH,
            height: SNAPSHOT_HEIGHT,
            rgba,
            fields: field_map,
            ledger_event_hash: ledger_event_hash.to_string(),
        });
        Ok(())
    }
}

pub fn default_mixed_workspace_descriptors() -> [WorkspaceSurfaceDescriptor; 2] {
    [
        WorkspaceSurfaceDescriptor {
            workspace_id: WorkspaceId::new("ws-human-terminal"),
            surface_id: SurfaceId::new("surface-human-terminal"),
            surface_kind: SurfaceKind::Terminal,
            trust_tier: TrustTier::HostTrusted,
            egress_class: EgressClass::None,
            preview_mode: PreviewMode::TerminalBuffer,
            liveness: LivenessState::Rendering,
            shared_ark_kernel: false,
        },
        WorkspaceSurfaceDescriptor {
            workspace_id: WorkspaceId::new("ws-ai-upload"),
            surface_id: SurfaceId::new("surface-ai-upload"),
            surface_kind: SurfaceKind::Framebuffer,
            trust_tier: TrustTier::ArkCell,
            egress_class: EgressClass::None,
            preview_mode: PreviewMode::SnapshotOnAction,
            liveness: LivenessState::Running,
            shared_ark_kernel: true,
        },
    ]
}

fn preview_captures_snapshot(mode: PreviewMode) -> bool {
    matches!(
        mode,
        PreviewMode::SnapshotOnAction | PreviewMode::SnapshotOnSwitch | PreviewMode::LiveDamage
    )
}

fn render_snapshot_rgba(
    width: usize,
    height: usize,
    workspace_id: &str,
    caption: &str,
    fields: &BTreeMap<String, String>,
    seq: u64,
) -> Vec<u8> {
    let mut rgba = vec![0_u8; width * height * 4];
    let bg = [20, 25, 34, 255];
    let top_bar = [35, 45, 62, 255];
    let fg_head = [220, 232, 248, 255];
    let fg_cap = [145, 224, 178, 255];
    let fg_name = [180, 195, 215, 255];
    let fg_val = [242, 246, 252, 255];

    fill_rect(&mut rgba, width, height, 0, 0, width, height, bg);
    fill_rect(&mut rgba, width, height, 0, 0, width, 24, top_bar);
    // Header indicator bars derived deterministically from caption/workspace
    let accent = color_from_seed(sha256_hex(caption.as_bytes()).as_bytes());
    fill_rect(&mut rgba, width, height, 10, 6, 12, 12, accent);
    draw_text_blocks(&mut rgba, width, height, 28, 8, workspace_id, fg_head);
    draw_text_blocks(&mut rgba, width, height, 10, 34, caption, fg_cap);

    // Render deterministic visual form boxes for title, description, thumbnail, secret, publish
    let mut y = 52;
    for (name, value) in fields.iter().take(5) {
        fill_rect(
            &mut rgba,
            width,
            height,
            10,
            y,
            width.saturating_sub(20),
            16,
            [30, 38, 50, 255],
        );
        let label_color = if name == "thumbnail" {
            color_from_seed(value.as_bytes())
        } else {
            [110, 168, 254, 255]
        };
        fill_rect(&mut rgba, width, height, 12, y + 3, 8, 10, label_color);
        draw_text_blocks(&mut rgba, width, height, 26, y + 4, name, fg_name);
        draw_text_blocks(&mut rgba, width, height, 118, y + 4, value, fg_val);
        y += 20;
    }

    // Footer frame sequence marker
    let seq_color = if seq % 2 == 0 {
        [64, 160, 110, 255]
    } else {
        [95, 145, 220, 255]
    };
    fill_rect(
        &mut rgba,
        width,
        height,
        width.saturating_sub(28),
        height.saturating_sub(14),
        18,
        8,
        seq_color,
    );
    rgba
}

fn color_from_seed(seed: &[u8]) -> [u8; 4] {
    let r = seed.first().copied().unwrap_or(100).saturating_add(60);
    let g = seed.get(1).copied().unwrap_or(140).saturating_add(60);
    let b = seed.get(2).copied().unwrap_or(180).saturating_add(40);
    [r, g, b, 255]
}

#[allow(clippy::too_many_arguments)]
fn fill_rect(
    rgba: &mut [u8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    rect_w: usize,
    rect_h: usize,
    color: [u8; 4],
) {
    let max_y = y.saturating_add(rect_h).min(height);
    let max_x = x.saturating_add(rect_w).min(width);
    for py in y..max_y {
        for px in x..max_x {
            let offset = (py * width + px) * 4;
            rgba[offset..offset + 4].copy_from_slice(&color);
        }
    }
}

fn draw_text_blocks(
    rgba: &mut [u8],
    width: usize,
    height: usize,
    start_x: usize,
    start_y: usize,
    text: &str,
    color: [u8; 4],
) {
    let mut x = start_x;
    for ch in text.bytes().take(42) {
        if x + 4 >= width {
            break;
        }
        if ch != b' ' {
            let top_h = if ch.is_ascii_uppercase() || ch.is_ascii_digit() {
                7
            } else {
                5
            };
            let y_off = if top_h == 7 { 0 } else { 2 };
            fill_rect(rgba, width, height, x, start_y + y_off, 3, top_h, color);
        }
        x += 5;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_tabs_preserves_background_running_and_masks_cortex_secrets() -> Result<()> {
        let mut broker = SurfaceBroker::new();
        for descriptor in default_mixed_workspace_descriptors() {
            broker.register_workspace(descriptor)?;
        }

        // Background AI tab updates fields while human terminal is active.
        broker.apply_supervisor_field_update(
            "ws-ai-upload",
            SemanticField {
                name: "title".to_string(),
                field_type: "text".to_string(),
                value: "Spacetime Demo".to_string(),
                is_secret: false,
            },
            "typed title",
            "hash-1",
        )?;
        broker.apply_supervisor_field_update(
            "ws-ai-upload",
            SemanticField {
                name: "upload_password".to_string(),
                field_type: "password".to_string(),
                value: "super-secret-token-123".to_string(),
                is_secret: true,
            },
            "masked token",
            "hash-2",
        )?;

        let reader = broker.reader();
        let active_id = reader.active_workspace_id();
        assert_eq!(active_id, Some("ws-human-terminal"));
        let ai_live = reader.descriptor("ws-ai-upload").map(|d| d.liveness);
        assert_eq!(ai_live, Some(LivenessState::Running));

        // Cortex channel masks secret fields and forbids raw screenshot access.
        let semantic = broker.cortex_semantic_view("ws-ai-upload")?;
        assert!(semantic.contains_secret_fields);
        assert!(!semantic.raw_screenshot_allowed_for_cortex);
        assert!(semantic
            .fields
            .iter()
            .any(|f| f.name == "upload_password" && f.value == "[MASKED]"));
        assert!(!semantic
            .fields
            .iter()
            .any(|f| f.value.contains("super-secret-token-123")));

        // Switching to AI tab switches RENDERING without disk writes.
        let frame = broker
            .switch_active_tab("ws-ai-upload")?
            .expect("AI tab snapshot");
        let title = frame.fields.get("title").map(String::as_str);
        assert_eq!(title, Some("Spacetime Demo"));
        let reader = broker.reader();
        let ai_live_after = reader.descriptor("ws-ai-upload").map(|d| d.liveness);
        assert_eq!(ai_live_after, Some(LivenessState::Rendering));
        let human_live = reader.descriptor("ws-human-terminal").map(|d| d.liveness);
        assert_eq!(human_live, Some(LivenessState::Running));
        assert_eq!(broker.reader().rendering_count(), 1);
        assert_eq!(broker.reader().disk_writes_performed(), 0);

        broker.close_workspace("ws-ai-upload")?;
        assert!(broker.reader().descriptor("ws-ai-upload").is_none());
        Ok(())
    }
}
