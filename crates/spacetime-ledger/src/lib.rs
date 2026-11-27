//! Hash-chained append-only ledger and replay state for Spacetime Block 1.

use serde::{Deserialize, Serialize};
use spacetime_types::{
    ActionOp, Event, EventSeq, GoalStatus, LivenessState, PreviewMode, Result, SpacetimeError,
    SurfaceKind, Verdict, SCHEMA_VERSION,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static ATOMIC_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn genesis_hash() -> String {
    spacetime_types::sha256_hex(b"spacetime-ledger-v1")
}

pub fn head_path(path: &Path) -> PathBuf {
    let mut head = path.as_os_str().to_os_string();
    head.push(".head");
    PathBuf::from(head)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    let seq = ATOMIC_SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let mut name = path
        .file_name()
        .map(|os| os.to_os_string())
        .unwrap_or_else(|| OsString::from("ledger"));
    name.push(format!(".tmp.{pid}.{seq}"));
    let temporary = parent.join(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.flush()) {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Head {
    pub schema_version: u32,
    pub count: u64,
    pub last_hash: String,
}

impl Head {
    pub fn genesis() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            count: 0,
            last_hash: genesis_hash(),
        }
    }

    pub fn to_json_string(&self) -> Result<String> {
        spacetime_types::canonical_json_string(self)
    }

    pub fn read(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(head_path(path))?;
        Ok(serde_json::from_str(&text)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceState {
    pub workspace_id: String,
    pub surface_id: String,
    pub surface_kind: SurfaceKind,
    pub preview_mode: PreviewMode,
    pub liveness: LivenessState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerState {
    pub schema_version: u32,
    pub goal_status: GoalStatus,
    pub last_hash: String,
    pub event_count: u64,
    pub last_evidence_digest: Option<String>,
    pub workspaces: BTreeMap<String, WorkspaceState>,
}

impl LedgerState {
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            goal_status: GoalStatus::InProgress,
            last_hash: genesis_hash(),
            event_count: 0,
            last_evidence_digest: None,
            workspaces: BTreeMap::new(),
        }
    }

    pub fn to_json_string(&self) -> Result<String> {
        let mut out = spacetime_types::canonical_json_string(self)?;
        out.push('\n');
        Ok(out)
    }

    pub fn apply(&mut self, event: &Event) {
        self.last_hash = event.hash.clone();
        self.event_count = event.seq.0 + 1;

        let binding = &event.action.binding;
        self.workspaces.insert(
            binding.workspace_id.as_str().to_string(),
            WorkspaceState {
                workspace_id: binding.workspace_id.as_str().to_string(),
                surface_id: binding.surface_id.as_str().to_string(),
                surface_kind: binding.surface_kind,
                preview_mode: binding.preview_mode,
                liveness: binding.liveness,
            },
        );

        match event.verifier_proof.verdict {
            Verdict::Pass => match &event.action.op {
                ActionOp::ProduceEvidence { .. } => {
                    self.last_evidence_digest = Some(event.observation.digest.clone());
                }
                ActionOp::ClaimDone { .. } => self.goal_status = GoalStatus::Verified,
                ActionOp::Echo { .. }
                | ActionOp::Sleep { .. }
                | ActionOp::Flaky { .. }
                | ActionOp::UiSetField { .. }
                | ActionOp::IrreversibleEffect { .. }
                | ActionOp::SpendFakeCredits { .. }
                | ActionOp::RunOperator { .. } => {}
            },
            Verdict::Unverified => {
                if matches!(&event.action.op, ActionOp::ClaimDone { .. }) {
                    self.goal_status = GoalStatus::Unverified;
                }
            }
            Verdict::Error | Verdict::Fail => self.goal_status = GoalStatus::Failed,
        }
    }
}

impl Default for LedgerState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Ledger {
    path: PathBuf,
    head: PathBuf,
    prev_hash: String,
    next_seq: EventSeq,
}

impl Ledger {
    pub fn create(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(path, b"")?;
        let head = head_path(path);
        let genesis = Head::genesis();
        atomic_write(&head, genesis.to_json_string()?.as_bytes())?;
        Ok(Self {
            path: path.to_path_buf(),
            head,
            prev_hash: genesis.last_hash,
            next_seq: EventSeq(0),
        })
    }

    pub fn next_seq(&self) -> EventSeq {
        self.next_seq
    }

    pub fn prev_hash(&self) -> String {
        self.prev_hash.clone()
    }

    pub fn append(&mut self, event: &Event) -> Result<()> {
        if event.seq != self.next_seq {
            return Err(SpacetimeError::new("ledger append out of sequence"));
        }
        if event.prev_hash != self.prev_hash {
            return Err(SpacetimeError::new("ledger prev_hash mismatch"));
        }
        if !event.verify_hash()? {
            return Err(SpacetimeError::new("ledger event hash is invalid"));
        }

        let mut line = event.to_json_string()?;
        line.push('\n');
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        file.write_all(line.as_bytes())?;
        file.flush()?;

        self.prev_hash = event.hash.clone();
        self.next_seq = EventSeq(self.next_seq.0 + 1);
        let head = Head {
            schema_version: SCHEMA_VERSION,
            count: self.next_seq.0,
            last_hash: self.prev_hash.clone(),
        };
        atomic_write(&self.head, head.to_json_string()?.as_bytes())?;
        Ok(())
    }

    pub fn verify(path: &Path) -> Result<Vec<Event>> {
        let head = Head::read(path)?;
        if head.schema_version != SCHEMA_VERSION {
            return Err(SpacetimeError::new("unsupported ledger head schema"));
        }

        let file = fs::File::open(path)?;
        let mut reader = BufReader::with_capacity(64 * 1024, file);
        let mut prev = genesis_hash();
        let mut events = Vec::new();
        let mut line = String::new();
        let mut index = 0usize;

        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                break;
            }
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            if line.trim().is_empty() {
                return Err(SpacetimeError::new("blank ledger line"));
            }
            let event = Event::from_json_str(&line)?;
            let (is_canonical, hash_ok) = event.verify_canonical_and_hash(&line)?;
            if !is_canonical {
                return Err(SpacetimeError::new("non-canonical ledger line"));
            }
            if event.schema_version != SCHEMA_VERSION {
                return Err(SpacetimeError::new("unsupported event schema"));
            }
            let expected_seq = EventSeq(
                u64::try_from(index)
                    .map_err(|_| SpacetimeError::new("ledger index does not fit in u64"))?,
            );
            if event.seq != expected_seq {
                return Err(SpacetimeError::new("ledger sequence mismatch"));
            }
            if event.prev_hash != prev {
                return Err(SpacetimeError::new("ledger prev_hash chain mismatch"));
            }
            if !hash_ok {
                return Err(SpacetimeError::new("ledger event hash mismatch"));
            }
            prev = event.hash.clone();
            events.push(event);
            index += 1;
        }

        let count = u64::try_from(events.len())
            .map_err(|_| SpacetimeError::new("event count does not fit in u64"))?;
        if head.count != count {
            return Err(SpacetimeError::new("ledger truncation detected"));
        }
        if head.last_hash != prev {
            return Err(SpacetimeError::new("ledger head hash mismatch"));
        }
        Ok(events)
    }

    pub fn replay(path: &Path) -> Result<LedgerState> {
        let events = Self::verify(path)?;
        let mut state = LedgerState::new();
        for event in &events {
            state.apply(event);
        }
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_is_deterministic() {
        assert_eq!(genesis_hash(), genesis_hash());
        assert_eq!(Head::genesis().count, 0);
        assert_eq!(Head::genesis().last_hash, genesis_hash());
    }

    #[test]
    fn empty_state_serializes_compactly() -> Result<()> {
        let state = LedgerState::new();
        let json = state.to_json_string()?;
        assert!(json.ends_with('\n'));
        assert!(json.contains("\"goal_status\":\"in_progress\""));
        Ok(())
    }

    #[test]
    fn head_json_is_canonical() -> Result<()> {
        let head = Head::genesis();
        let json = head.to_json_string()?;
        assert!(json.starts_with("{\"count\":0"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn head_path_preserves_non_utf8_bytes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let path = PathBuf::from(OsString::from_vec(vec![b'f', b'o', b'o', 0xff]));
        let head = head_path(&path);
        let mut expected = path.into_os_string();
        expected.push(".head");
        assert_eq!(head.as_os_str(), expected.as_os_str());
    }
}
