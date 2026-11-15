//! Capability-limited scripted runner for Spacetime Blocks 1-2.
//!
//! This is not a kernel sandbox. It exposes no network capability and only five scripted
//! operations. Real namespaces, cgroups, Ark cells, clock/RNG/socket shims, and quarantine
//! microVMs arrive in later blocks.

use spacetime_types::{sha256_hex, Action, ActionOp, Observation, SpacetimeError, Taint};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeterminismProfile {
    pub clock: &'static str,
    pub rng_seed: u64,
    pub locale: &'static str,
    pub temp_root: &'static str,
    pub environment: &'static str,
    pub network: &'static str,
}

pub const FIXED_DETERMINISM_PROFILE: DeterminismProfile = DeterminismProfile {
    clock: "fixed_scripted_clock",
    rng_seed: 0,
    locale: "C",
    temp_root: "runner_owned_pid_scoped",
    environment: "cleared",
    network: "none_by_construction",
};

pub const FIXED_CLOCK_EPOCH_MILLIS: u64 = 0;
const LCG_MULTIPLIER: u64 = 6_364_136_223_846_793_005;
const LCG_INCREMENT: u64 = 1_442_695_040_888_963_407;

pub fn fixed_clock_millis() -> u64 {
    FIXED_CLOCK_EPOCH_MILLIS
}

pub fn fixed_rng_sequence(seed: u64, count: usize) -> Vec<u64> {
    let mut state = seed;
    (0..count)
        .map(|_| {
            state = state
                .wrapping_mul(LCG_MULTIPLIER)
                .wrapping_add(LCG_INCREMENT);
            state
        })
        .collect()
}

impl DeterminismProfile {
    pub fn digest(&self) -> String {
        let mut material = String::new();
        let _ = writeln!(material, "clock={}", self.clock);
        let _ = writeln!(material, "rng_seed={}", self.rng_seed);
        let _ = writeln!(material, "locale={}", self.locale);
        let _ = writeln!(material, "temp_root={}", self.temp_root);
        let _ = writeln!(material, "environment={}", self.environment);
        let _ = writeln!(material, "network={}", self.network);
        sha256_hex(material.as_bytes())
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlProbe {
    pub clock_fixed: bool,
    pub rng_seed_fixed: bool,
    pub locale_c: bool,
    pub temp_root_runner_owned: bool,
    pub environment_cleared: bool,
    pub network_none_by_construction: bool,
    pub stdin_null: bool,
    pub profile_digest: String,
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
    flaky_counter: AtomicU64,
    profile_digest: String,
}

impl ScriptedRunner {
    pub fn new() -> Result<Self, SpacetimeError> {
        Ok(Self {
            work_dir: TempDir::new("spacetime-block1")?,
            flaky_counter: AtomicU64::new(0),
            profile_digest: FIXED_DETERMINISM_PROFILE.digest(),
        })
    }

    pub fn determinism_profile() -> DeterminismProfile {
        FIXED_DETERMINISM_PROFILE
    }

    pub fn determinism_profile_digest(&self) -> &str {
        &self.profile_digest
    }

    fn run_flaky(&self, action: &Action, token: &str) -> RunResult {
        let count = self.flaky_counter.fetch_add(1, Ordering::Relaxed);
        let parity = if count % 2 == 0 { "even" } else { "odd" };
        completed_result(action, format!("{token}-{parity}"), &self.profile_digest)
    }

    fn run_sleep(&self, action: &Action, milliseconds: u64) -> RunResult {
        let budget_millis = action.budget_millis;
        let timeout = Duration::from_millis(budget_millis.max(1));
        let seconds = milliseconds.div_ceil(1000);
        let mut child = match Command::new("/bin/sleep")
            .arg(seconds.to_string())
            .current_dir(self.work_dir.path())
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                return error_result(action, error.to_string(), &self.profile_digest);
            }
        };

        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => {
                    let _ = child.wait();
                    return completed_result(action, String::new(), &self.profile_digest);
                }
                Ok(None) => {
                    if started.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return error_result(
                            action,
                            format!("timeout after {budget_millis}ms"),
                            &self.profile_digest,
                        );
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => {
                    return error_result(action, error.to_string(), &self.profile_digest);
                }
            }
        }
    }
}

impl ScriptedRunner {
    pub fn probe_controls(&self) -> spacetime_types::Result<ControlProbe> {
        let script = concat!(
            "unset PATH\n",
            "probe_line=\n",
            "if IFS= read -r probe_line; then :; fi\n",
            "printf 'env=<%s|%s|%s> cwd=<%s> stdin=<%s> locale=<%s>' ",
            "\"$PATH\" \"$HOME\" \"$CARGO_TERM_COLOR\" \"$(pwd)\" \"$probe_line\" \"$LC_ALL\""
        );
        let expected = format!(
            "env=<||> cwd=<{}> stdin=<> locale=<C>",
            self.work_dir.path().display()
        );
        let output = Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .current_dir(self.work_dir.path())
            .env_clear()
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()?;
        let actual = String::from_utf8_lossy(&output.stdout).into_owned();
        let probe_matches = output.status.success() && actual == expected;
        let work_dir = self.work_dir.path().display().to_string();
        let seed = FIXED_DETERMINISM_PROFILE.rng_seed;
        let first_rng = fixed_rng_sequence(seed, 8);
        let second_rng = fixed_rng_sequence(seed, 8);
        let probe = ControlProbe {
            clock_fixed: fixed_clock_millis() == FIXED_CLOCK_EPOCH_MILLIS,
            rng_seed_fixed: first_rng == second_rng && first_rng[0] != seed,
            locale_c: probe_matches && FIXED_DETERMINISM_PROFILE.locale == "C",
            temp_root_runner_owned: probe_matches && work_dir.contains("spacetime-block1"),
            environment_cleared: probe_matches,
            network_none_by_construction: FIXED_DETERMINISM_PROFILE.network
                == "none_by_construction",
            stdin_null: probe_matches,
            profile_digest: self.profile_digest.clone(),
        };
        let checks = [
            probe.clock_fixed,
            probe.rng_seed_fixed,
            probe.locale_c,
            probe.temp_root_runner_owned,
            probe.environment_cleared,
            probe.network_none_by_construction,
            probe.stdin_null,
        ];
        if checks.contains(&false) {
            return Err(SpacetimeError::new(format!(
                "deterministic control probe failed: expected {expected:?}, got {actual:?}"
            )));
        }
        Ok(probe)
    }
}

impl Runner for ScriptedRunner {
    fn run(&self, action: &Action) -> RunResult {
        match &action.op {
            ActionOp::ProduceEvidence { evidence } => {
                completed_result(action, evidence.clone(), &self.profile_digest)
            }
            ActionOp::Echo { text } => completed_result(action, text.clone(), &self.profile_digest),
            ActionOp::ClaimDone { .. } => {
                completed_result(action, "claim_done".to_string(), &self.profile_digest)
            }
            ActionOp::Sleep { milliseconds } => self.run_sleep(action, *milliseconds),
            ActionOp::Flaky { token } => self.run_flaky(action, token),
        }
    }
}

fn completed_result(action: &Action, bytes: String, profile_digest: &str) -> RunResult {
    RunResult {
        observation: observation(action, bytes, profile_digest),
        status: RunStatus::Completed,
    }
}

fn error_result(action: &Action, reason: String, profile_digest: &str) -> RunResult {
    RunResult {
        observation: observation(action, reason.clone(), profile_digest),
        status: RunStatus::Error(reason),
    }
}

fn observation(action: &Action, bytes: String, profile_digest: &str) -> Observation {
    Observation {
        action_id: action.id.clone(),
        digest: spacetime_types::sha256_hex(bytes.as_bytes()),
        determinism_profile_digest: profile_digest.to_string(),
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

    #[test]
    fn zero_sleep_completes() {
        let runner = ScriptedRunner::new().expect("runner");
        let result = runner.run(&action(ActionOp::Sleep { milliseconds: 0 }, 1_000));
        assert_eq!(result.status, RunStatus::Completed);
    }

    #[test]
    fn flaky_action_disagrees_on_two_calls() {
        let runner = ScriptedRunner::new().expect("runner");
        let action = action(
            ActionOp::Flaky {
                token: "flaky".to_string(),
            },
            1_000,
        );
        let first = runner.run(&action);
        let second = runner.run(&action);
        assert_ne!(first.observation.digest, second.observation.digest);
    }

    #[test]
    fn determinism_profile_is_fixed_and_recorded() {
        let runner = ScriptedRunner::new().expect("runner");
        assert_eq!(
            ScriptedRunner::determinism_profile(),
            FIXED_DETERMINISM_PROFILE
        );
        let result = runner.run(&action(
            ActionOp::Echo {
                text: "hello".to_string(),
            },
            1_000,
        ));
        assert_eq!(
            result.observation.determinism_profile_digest,
            FIXED_DETERMINISM_PROFILE.digest()
        );
        let expected = FIXED_DETERMINISM_PROFILE.digest();
        assert_eq!(runner.determinism_profile_digest(), expected.as_str());
    }

    #[test]
    fn control_probe_validates_cheap_controls() -> spacetime_types::Result<()> {
        let runner = ScriptedRunner::new()?;
        let probe = runner.probe_controls()?;
        assert!(probe.clock_fixed);
        assert!(probe.rng_seed_fixed);
        assert!(probe.locale_c);
        assert!(probe.temp_root_runner_owned);
        assert!(probe.environment_cleared);
        assert!(probe.network_none_by_construction);
        assert!(probe.stdin_null);
        let expected = FIXED_DETERMINISM_PROFILE.digest();
        assert_eq!(probe.profile_digest.as_str(), expected.as_str());
        let first = fixed_rng_sequence(FIXED_DETERMINISM_PROFILE.rng_seed, 8);
        let second = fixed_rng_sequence(FIXED_DETERMINISM_PROFILE.rng_seed, 8);
        assert_eq!(first, second);
        assert_ne!(first[0], FIXED_DETERMINISM_PROFILE.rng_seed);
        Ok(())
    }
}
