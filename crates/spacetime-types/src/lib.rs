//! Core versioned types for Spacetime Block 1.
//!
//! This crate is deliberately small: no async runtime, no GUI, no network client, and no model SDK.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fmt::Write as _;
use std::str::FromStr;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpacetimeError(String);

impl SpacetimeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for SpacetimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SpacetimeError {}

impl From<serde_json::Error> for SpacetimeError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error.to_string())
    }
}

impl From<std::io::Error> for SpacetimeError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

impl From<std::num::ParseIntError> for SpacetimeError {
    fn from(error: std::num::ParseIntError) -> Self {
        Self::new(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, SpacetimeError>;

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest.iter() {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value)?;
    Ok(serde_json::to_vec(&value)?)
}

pub fn canonical_json_string<T: Serialize>(value: &T) -> Result<String> {
    let value = serde_json::to_value(value)?;
    Ok(serde_json::to_string(&value)?)
}

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

string_id!(SpaceId);
string_id!(WorkspaceId);
string_id!(SurfaceId);
string_id!(GoalId);
string_id!(ActionId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventSeq(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustTier {
    HostTrusted,
    WasiWorker,
    ArkCell,
    QuarantineMicroVm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EgressClass {
    None,
    Proxy,
    Direct,
    Tor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Taint {
    TrustedUser,
    TrustedConstitution,
    TrustedTool,
    UntrustedWeb,
    UntrustedToolOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityScope {
    LocalFixture,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceKind {
    Terminal,
    Framebuffer,
    BrowserCdp,
}

impl FromStr for SurfaceKind {
    type Err = SpacetimeError;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "terminal" => Ok(Self::Terminal),
            "framebuffer" => Ok(Self::Framebuffer),
            "browser_cdp" => Ok(Self::BrowserCdp),
            _ => Err(SpacetimeError::new(format!(
                "unknown surface_kind: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewMode {
    LiveDamage,
    SnapshotOnAction,
    SnapshotOnSwitch,
    Headless,
    TerminalBuffer,
}

impl FromStr for PreviewMode {
    type Err = SpacetimeError;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "live_damage" => Ok(Self::LiveDamage),
            "snapshot_on_action" => Ok(Self::SnapshotOnAction),
            "snapshot_on_switch" => Ok(Self::SnapshotOnSwitch),
            "headless" => Ok(Self::Headless),
            "terminal_buffer" => Ok(Self::TerminalBuffer),
            _ => Err(SpacetimeError::new(format!(
                "unknown preview_mode: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LivenessState {
    Running,
    Rendering,
    Suspended,
    Dead,
}

impl FromStr for LivenessState {
    type Err = SpacetimeError;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "running" => Ok(Self::Running),
            "rendering" => Ok(Self::Rendering),
            "suspended" => Ok(Self::Suspended),
            "dead" => Ok(Self::Dead),
            _ => Err(SpacetimeError::new(format!("unknown liveness: {value}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    InProgress,
    Verified,
    Unverified,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    Fail,
    Error,
    Unverified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    Allow,
    Deny,
    Queue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    pub scope: CapabilityScope,
    pub limit_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceBinding {
    pub workspace_id: WorkspaceId,
    pub surface_id: SurfaceId,
    pub surface_kind: SurfaceKind,
    pub preview_mode: PreviewMode,
    pub liveness: LivenessState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionOp {
    ProduceEvidence { evidence: String },
    Echo { text: String },
    Sleep { milliseconds: u64 },
    ClaimDone { expected_digest: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub schema_version: u32,
    pub id: ActionId,
    pub space_id: SpaceId,
    pub goal_id: GoalId,
    pub binding: WorkspaceBinding,
    pub op: ActionOp,
    pub args_digest: String,
    pub taint: Taint,
    pub capability: Capability,
    pub budget_millis: u64,
    pub expected_verifier: Option<String>,
}

impl Action {
    pub fn new(
        seq: EventSeq,
        space_id: SpaceId,
        goal_id: GoalId,
        binding: WorkspaceBinding,
        op: ActionOp,
        budget_millis: u64,
        expected_verifier: Option<String>,
    ) -> Result<Self> {
        let sequence = seq.0;
        let args_digest = sha256_hex(canonical_json_bytes(&op)?.as_slice());
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            id: ActionId::new(format!("action-{sequence:06}")),
            space_id,
            goal_id,
            binding,
            op,
            args_digest,
            taint: Taint::TrustedTool,
            capability: Capability {
                scope: CapabilityScope::LocalFixture,
                limit_millis: budget_millis,
            },
            budget_millis,
            expected_verifier,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub action_id: ActionId,
    pub bytes: String,
    pub digest: String,
    pub provenance: Taint,
    pub redactions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierProof {
    pub verifier_id: String,
    pub verdict: Verdict,
    pub evidence_digest: String,
    pub determinism_runs: u32,
    pub mutation_score_bps: u16,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyDecision {
    pub decision: DecisionKind,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub schema_version: u32,
    pub seq: EventSeq,
    pub prev_hash: String,
    pub hash: String,
    pub action: Action,
    pub observation: Observation,
    pub verifier_proof: VerifierProof,
    pub policy_decision: PolicyDecision,
}

impl Event {
    pub fn new(
        seq: EventSeq,
        prev_hash: String,
        action: Action,
        observation: Observation,
        verifier_proof: VerifierProof,
        policy_decision: PolicyDecision,
    ) -> Result<Self> {
        let mut event = Self {
            schema_version: SCHEMA_VERSION,
            seq,
            prev_hash,
            hash: String::new(),
            action,
            observation,
            verifier_proof,
            policy_decision,
        };
        event.hash = event.compute_hash()?;
        Ok(event)
    }

    pub fn to_json_string(&self) -> Result<String> {
        canonical_json_string(self)
    }

    pub fn from_json_str(line: &str) -> Result<Self> {
        Ok(serde_json::from_str(line)?)
    }

    pub fn verify_hash(&self) -> Result<bool> {
        Ok(self.compute_hash()? == self.hash)
    }

    fn compute_hash(&self) -> Result<String> {
        let mut value = serde_json::to_value(self)?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| SpacetimeError::new("event must serialize to a JSON object"))?;
        object.remove("hash");
        let canonical = serde_json::to_vec(&value)?;
        let mut bytes = self.prev_hash.as_bytes().to_vec();
        bytes.extend_from_slice(&canonical);
        Ok(sha256_hex(&bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn preview_mode_is_data_not_a_hardcoded_ui_decision() -> Result<()> {
        assert_eq!(
            PreviewMode::from_str("snapshot_on_action")?,
            PreviewMode::SnapshotOnAction
        );
        assert_eq!(PreviewMode::from_str("headless")?, PreviewMode::Headless);
        assert!(PreviewMode::from_str("cinematic_4k").is_err());
        Ok(())
    }

    #[test]
    fn workspace_binding_round_trips_with_stable_ids() -> Result<()> {
        let binding = WorkspaceBinding {
            workspace_id: WorkspaceId::new("ws-1"),
            surface_id: SurfaceId::new("surface-1"),
            surface_kind: SurfaceKind::Framebuffer,
            preview_mode: PreviewMode::SnapshotOnSwitch,
            liveness: LivenessState::Rendering,
        };
        let json = serde_json::to_string(&binding)?;
        let decoded: WorkspaceBinding = serde_json::from_str(&json)?;
        assert_eq!(binding, decoded);
        assert_eq!(decoded.workspace_id.as_str(), "ws-1");
        assert_eq!(decoded.surface_id.as_str(), "surface-1");
        Ok(())
    }

    #[test]
    fn canonical_json_sorts_object_keys() -> Result<()> {
        #[derive(Serialize)]
        struct Example {
            zebra: u8,
            alpha: u8,
        }

        let bytes = canonical_json_bytes(&Example { zebra: 1, alpha: 2 })?;
        let text = String::from_utf8(bytes).expect("canonical JSON is UTF-8");
        assert_eq!(text, r#"{"alpha":2,"zebra":1}"#);
        Ok(())
    }
}
