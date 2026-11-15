//! Deterministic scripted Cortex for Spacetime Block 1.
//!
//! No API keys, browser, web chat, or network calls exist in this adapter.

use spacetime_ledger::LedgerState;
use spacetime_types::{
    Action, ActionOp, EventSeq, GoalId, GoalStatus, LivenessState, PreviewMode, Result, SpaceId,
    SpacetimeError, SurfaceId, SurfaceKind, WorkspaceBinding, WorkspaceId, SCHEMA_VERSION,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;

const MAX_TASK_BYTES: usize = 64 * 1024;
const DEFAULT_BUDGET_MILLIS: u64 = 1_000;

pub trait Cortex {
    fn next_action(&mut self, state: &LedgerState) -> Result<Option<Action>>;
}

#[derive(Debug, Clone)]
pub struct ScriptedCortex {
    actions: Vec<Action>,
    cursor: usize,
    metadata: BTreeMap<String, String>,
}

impl ScriptedCortex {
    pub fn from_file(path: &Path) -> Result<Self> {
        let metadata = fs::metadata(path)?;
        if metadata.len() > MAX_TASK_BYTES as u64 {
            return Err(SpacetimeError::new("task file exceeds 64 KiB resource cap"));
        }
        let bytes = fs::read(path)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| SpacetimeError::new("task file is not valid UTF-8"))?;

        let mut metadata: BTreeMap<String, String> = BTreeMap::new();
        let mut action_lines: Vec<String> = Vec::new();

        for raw_line in text.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("action ") {
                action_lines.push(rest.trim().to_string());
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| SpacetimeError::new(format!("invalid task line: {line}")))?;
            metadata.insert(key.trim().to_string(), value.trim().to_string());
        }

        let binding = binding_from_metadata(&metadata)?;
        let space_id = SpaceId::new(value_or(&metadata, "space_id", "space-default"));
        let goal_id = GoalId::new(value_or(&metadata, "goal_id", "goal-default"));
        let mut actions = Vec::with_capacity(action_lines.len());

        for (index, line) in action_lines.iter().enumerate() {
            let seq = EventSeq(
                u64::try_from(index)
                    .map_err(|_| SpacetimeError::new("action index does not fit in u64"))?,
            );
            actions.push(parse_action_line(
                line,
                seq,
                space_id.clone(),
                goal_id.clone(),
                binding.clone(),
            )?);
        }

        Ok(Self {
            actions,
            cursor: 0,
            metadata,
        })
    }

    pub fn first_action(&self) -> Result<Action> {
        self.actions
            .first()
            .cloned()
            .ok_or_else(|| SpacetimeError::new("task has no actions"))
    }

    pub fn expected_digest(&self) -> Option<String> {
        let raw = self.metadata.get("expected_digest")?;
        if raw == "none" {
            return None;
        }
        if let Some(preimage) = raw.strip_prefix("sha256:") {
            Some(spacetime_types::sha256_hex(preimage.as_bytes()))
        } else {
            Some(raw.clone())
        }
    }

    pub fn verifier_source(&self) -> Result<spacetime_types::VerifierSource> {
        match self.metadata.get("verifier_source") {
            Some(raw) => raw.parse::<spacetime_types::VerifierSource>(),
            None => Ok(spacetime_types::VerifierSource::FixtureSuite),
        }
    }

    pub fn expected_verdict(&self) -> Result<spacetime_types::Verdict> {
        match self.metadata.get("expected_verdict") {
            Some(raw) => raw.parse::<spacetime_types::Verdict>(),
            None => Ok(spacetime_types::Verdict::Pass),
        }
    }

    pub fn determinism_runs(&self) -> Result<u32> {
        metadata_number(&self.metadata, "determinism_runs", 2)
    }

    pub fn mutation_threshold_bps(&self) -> Result<u16> {
        metadata_number(&self.metadata, "mutation_threshold_bps", 8_000)
    }

    pub fn candidate_mutation_score_bps(&self) -> Result<u16> {
        metadata_number(&self.metadata, "candidate_mutation_score_bps", 10_000)
    }
}

impl Cortex for ScriptedCortex {
    fn next_action(&mut self, state: &LedgerState) -> Result<Option<Action>> {
        if state.goal_status != GoalStatus::InProgress {
            return Ok(None);
        }
        if self.cursor >= self.actions.len() {
            return Ok(None);
        }
        let action = self.actions[self.cursor].clone();
        self.cursor += 1;
        Ok(Some(action))
    }
}

fn value_or(metadata: &BTreeMap<String, String>, key: &str, default: &str) -> String {
    metadata
        .get(key)
        .cloned()
        .unwrap_or_else(|| default.to_string())
}

fn metadata_number<T>(
    metadata: &BTreeMap<String, String>,
    key: &str,
    default: T,
) -> Result<T>
where
    T: FromStr,
    SpacetimeError: From<T::Err>,
{
    match metadata.get(key) {
        Some(raw) => Ok(raw.parse::<T>()?),
        None => Ok(default),
    }
}

fn binding_from_metadata(metadata: &BTreeMap<String, String>) -> Result<WorkspaceBinding> {
    let schema_version = match metadata.get("schema_version") {
        Some(raw) => raw.parse::<u32>()?,
        None => SCHEMA_VERSION,
    };
    if schema_version != SCHEMA_VERSION {
        return Err(SpacetimeError::new("unsupported task schema version"));
    }

    let surface_kind = SurfaceKind::from_str(&value_or(metadata, "surface_kind", "framebuffer"))?;
    let preview_mode =
        PreviewMode::from_str(&value_or(metadata, "preview_mode", "snapshot_on_action"))?;
    let liveness = LivenessState::from_str(&value_or(metadata, "liveness", "running"))?;

    Ok(WorkspaceBinding {
        workspace_id: WorkspaceId::new(value_or(metadata, "workspace_id", "workspace-default")),
        surface_id: SurfaceId::new(value_or(metadata, "surface_id", "surface-default")),
        surface_kind,
        preview_mode,
        liveness,
    })
}

fn parse_action_line(
    line: &str,
    seq: EventSeq,
    space_id: SpaceId,
    goal_id: GoalId,
    binding: WorkspaceBinding,
) -> Result<Action> {
    let mut parts = line.split_whitespace();
    let name = parts
        .next()
        .ok_or_else(|| SpacetimeError::new("empty action line"))?;
    let mut fields: BTreeMap<&str, &str> = BTreeMap::new();
    for part in parts {
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| SpacetimeError::new(format!("invalid action field: {part}")))?;
        fields.insert(key, value);
    }

    let budget_millis = optional_u64(fields.get("budget_ms"), DEFAULT_BUDGET_MILLIS)?;
    let op = match name {
        "produce_evidence" => ActionOp::ProduceEvidence {
            evidence: required(&fields, "evidence")?.to_string(),
        },
        "echo" => ActionOp::Echo {
            text: required(&fields, "text")?.to_string(),
        },
        "sleep" => ActionOp::Sleep {
            milliseconds: required(&fields, "ms")?.parse::<u64>()?,
        },
        "claim_done" => ActionOp::ClaimDone {
            expected_digest: parse_expected_digest(fields.get("expected_digest"))?,
        },
        "flaky" => ActionOp::Flaky {
            token: required(&fields, "token")?.to_string(),
        },
        _ => return Err(SpacetimeError::new(format!("unknown action: {name}"))),
    };

    Action::new(
        seq,
        space_id,
        goal_id,
        binding,
        op,
        budget_millis,
        Some("fixture-verifier-v1".to_string()),
    )
}

fn required<'a>(fields: &BTreeMap<&str, &'a str>, key: &str) -> Result<&'a str> {
    fields
        .get(key)
        .copied()
        .ok_or_else(|| SpacetimeError::new(format!("missing action field: {key}")))
}

fn optional_u64(value: Option<&&str>, default: u64) -> Result<u64> {
    match value {
        Some(raw) => Ok((*raw).parse::<u64>()?),
        None => Ok(default),
    }
}

fn parse_expected_digest(value: Option<&&str>) -> Result<Option<String>> {
    match value {
        None => Ok(None),
        Some(raw) => {
            let text = *raw;
            if text == "none" {
                Ok(None)
            } else if let Some(preimage) = text.strip_prefix("sha256:") {
                Ok(Some(spacetime_types::sha256_hex(preimage.as_bytes())))
            } else {
                Ok(Some(text.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_happy_path_fixture() -> Result<()> {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/tasks/block1_hello.task");
        let mut cortex = ScriptedCortex::from_file(&path)?;
        let state = LedgerState::new();
        let first = cortex.next_action(&state)?.expect("first action");
        assert!(matches!(&first.op, ActionOp::ProduceEvidence { .. }));
        assert_eq!(first.id, spacetime_types::ActionId::new("action-000000"));
        Ok(())
    }

    #[test]
    fn oversized_task_is_rejected_before_reading() -> Result<()> {
        let pid = std::process::id();
        let suffix = MAX_TASK_BYTES;
        let path = std::env::temp_dir().join(format!("spacetime-cortex-oversize-{pid}-{suffix}"));
        let data = vec![b'#'; MAX_TASK_BYTES + 1];
        fs::write(&path, data)?;
        let result = ScriptedCortex::from_file(&path);
        let _ = fs::remove_file(&path);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn parses_block2_expected_verdict() -> Result<()> {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/tasks/block2_flaky.task");
        let cortex = ScriptedCortex::from_file(&path)?;
        assert_eq!(cortex.expected_verdict()?, spacetime_types::Verdict::Error);
        assert_eq!(
            cortex.verifier_source()?,
            spacetime_types::VerifierSource::FixtureSuite
        );
        Ok(())
    }

    #[test]
    fn parses_block2_metadata_controls() -> Result<()> {
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("spacetime-cortex-metadata-{pid}"));
        fs::write(
            &path,
            "schema_version=1\ndeterminism_runs=3\nmutation_threshold_bps=9000\ncandidate_mutation_score_bps=7500\nexpected_verdict=fail\nverifier_source=adversarial_model\naction echo text=x\n",
        )?;
        let cortex = ScriptedCortex::from_file(&path)?;
        let result = (
            cortex.determinism_runs()?,
            cortex.mutation_threshold_bps()?,
            cortex.candidate_mutation_score_bps()?,
            cortex.expected_verdict()?,
            cortex.verifier_source()?,
        );
        let _ = fs::remove_file(&path);
        assert_eq!(result.0, 3);
        assert_eq!(result.1, 9_000);
        assert_eq!(result.2, 7_500);
        assert_eq!(result.3, spacetime_types::Verdict::Fail);
        assert_eq!(
            result.4,
            spacetime_types::VerifierSource::AdversarialModel
        );
        Ok(())
    }
}
