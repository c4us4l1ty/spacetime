//! Mixed-workspace surface, snapshot, governor, CAS, and quarantine autopsy types.

use crate::{
    EgressClass, LivenessState, PreviewMode, SurfaceId, SurfaceKind, TrustTier, WorkspaceId,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticField {
    pub name: String,
    pub field_type: String,
    pub value: String,
    pub is_secret: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticView {
    pub workspace_id: WorkspaceId,
    pub surface_id: SurfaceId,
    pub fields: Vec<SemanticField>,
    pub contains_secret_fields: bool,
    pub raw_screenshot_allowed_for_cortex: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotFrame {
    pub seq: u64,
    pub workspace_id: WorkspaceId,
    pub surface_id: SurfaceId,
    pub preview_mode: PreviewMode,
    pub caption: String,
    pub width: usize,
    pub height: usize,
    /// RAM-only RGBA8 buffer for human display only; never written to disk or given to Cortex.
    pub rgba: Vec<u8>,
    pub fields: BTreeMap<String, String>,
    pub ledger_event_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentSummary {
    pub files_read: u64,
    pub files_written: u64,
    pub dns_queries: u64,
    pub fake_spend_credits: u64,
    pub risk_label: String,
}

impl IntentSummary {
    pub fn format_ribbon(&self) -> String {
        format!(
            "read {} files · wrote {} · DNS {} · will spend {} fake_credits · risk: {}",
            self.files_read,
            self.files_written,
            self.dns_queries,
            self.fake_spend_credits,
            self.risk_label
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSurfaceDescriptor {
    pub workspace_id: WorkspaceId,
    pub surface_id: SurfaceId,
    pub surface_kind: SurfaceKind,
    pub trust_tier: TrustTier,
    pub egress_class: EgressClass,
    pub preview_mode: PreviewMode,
    pub liveness: LivenessState,
    pub shared_ark_kernel: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GovernorAction {
    Allow,
    DowngradePreview(PreviewMode),
    SuspendCell,
    QueueWork,
    KillRunaway,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CasProvenanceTier {
    UpstreamSigned,
    ReproBuilt,
    TabLocal,
    Quarantined,
}

impl CasProvenanceTier {
    pub fn globally_shareable(&self) -> bool {
        matches!(self, Self::UpstreamSigned | Self::ReproBuilt)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CasLayer {
    pub digest: String,
    pub name: String,
    pub tier: CasProvenanceTier,
    pub owner_workspace: Option<WorkspaceId>,
    pub verifier_proof_digest: Option<String>,
    pub build_log_digest: Option<String>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutopsyReport {
    pub workspace_id: String,
    pub trust_tier: TrustTier,
    pub added_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub deleted_files: Vec<String>,
    pub whiteouts: Vec<String>,
    pub blocked_egress: Vec<String>,
    pub peak_rss_kb: u64,
    pub canary_reads: u64,
    pub blast_radius_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TombstoneRecord {
    pub schema_version: u32,
    pub workspace_id: String,
    pub trust_tier: TrustTier,
    pub blast_radius_hash: String,
    pub summary: String,
    pub upperdir_removed: bool,
}
