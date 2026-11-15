//! Capability-limited scripted runner for Spacetime Block 1.
//!
//! This is not a kernel sandbox. It exposes no network capability and only four scripted
//! operations. Real namespaces, cgroups, Ark cells, and quarantine microVMs arrive in later blocks.

use spacetime_types::{Action, ActionOp, Observation, SpacetimeError, Taint};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    Completed,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    pub observation: Observation,
    pub status: RunStatus,
}

pub trait Runner {
    fn run(&self, action: &Action) -> RunResult;
}

#[derive(Debug)]
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Result<Self, SpacetimeError> {
        let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("{prefix}-{pid}-{id}"));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[derive(Debug)]
pub struct ScriptedRunner {
    work_dir: TempDir,
}

impl ScriptedRunner {
    pub fn new() -> Result<Self, SpacetimeError> {
        Ok(Self {
            work_dir: TempDir::new("spacetime-block1")?,
        })
    }

    fn run_sleep(&self, action: &Action, milliseconds: u64) -> RunResult {
        let budget_millis = action.budget_millis;
        let timeout = Duration::from_millis(budget_millis.max(1));
        let seconds = milliseconds.div_ceil(1000);
        let mut child = match Command::new("/bin/sleep")
            .arg(seconds.to_string())
            .current_dir(self.work_dir.path())
            .env_clear()
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => return error_result(action, error.to_string()),
        };

        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => {
                    let _ = child.wait();
                    return completed_result(action, String::new());
                }
                Ok(None) => {
                    if started.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return error_result(action, format!("timeout after {budget_millis}ms"));
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => return error_result(action, error.to_string()),
            }
        }
    }
}

impl Runner for ScriptedRunner {
    fn run(&self, action: &Action) -> RunResult {
        match &action.op {
            ActionOp::ProduceEvidence { evidence } => completed_result(action, evidence.clone()),
            ActionOp::Echo { text } => completed_result(action, text.clone()),
            ActionOp::ClaimDone { .. } => completed_result(action, "claim_done".to_string()),
            ActionOp::Sleep { milliseconds } => self.run_sleep(action, *milliseconds),
        }
    }
}

fn completed_result(action: &Action, bytes: String) -> RunResult {
    RunResult {
        observation: observation(action, bytes),
        status: RunStatus::Completed,
    }
}

fn error_result(action: &Action, reason: String) -> RunResult {
    RunResult {
        observation: observation(action, reason.clone()),
        status: RunStatus::Error(reason),
    }
}

fn observation(action: &Action, bytes: String) -> Observation {
    Observation {
        action_id: action.id.clone(),
        digest: spacetime_types::sha256_hex(bytes.as_bytes()),
        bytes,
        provenance: Taint::UntrustedToolOutput,
        redactions: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetime_types::{
        EventSeq, GoalId, LivenessState, PreviewMode, SpaceId, SurfaceId, SurfaceKind,
        WorkspaceBinding, WorkspaceId,
    };

    fn action(op: ActionOp, budget_millis: u64) -> Action {
        Action::new(
            EventSeq(0),
            SpaceId::new("space-test"),
            GoalId::new("goal-test"),
            WorkspaceBinding {
                workspace_id: WorkspaceId::new("ws-test"),
                surface_id: SurfaceId::new("surface-test"),
                surface_kind: SurfaceKind::Terminal,
                preview_mode: PreviewMode::TerminalBuffer,
                liveness: LivenessState::Running,
            },
            op,
            budget_millis,
            None,
        )
        .expect("test action")
    }

    #[test]
    fn echo_is_deterministic() {
        let runner = ScriptedRunner::new().expect("runner");
        let first = runner.run(&action(
            ActionOp::Echo {
                text: "hello".to_string(),
            },
            1_000,
        ));
        let second = runner.run(&action(
            ActionOp::Echo {
                text: "hello".to_string(),
            },
            1_000,
        ));
        assert_eq!(first, second);
        assert_eq!(first.status, RunStatus::Completed);
    }

    #[test]
    fn timeout_is_an_error_not_a_pass() {
        let runner = ScriptedRunner::new().expect("runner");
        let result = runner.run(&action(
            ActionOp::Sleep {
                milliseconds: 2_000,
            },
            20,
        ));
        assert!(matches!(result.status, RunStatus::Error(_)));
    }
}
