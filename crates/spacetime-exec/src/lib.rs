//! Capability-limited scripted runner for Spacetime Blocks 1-2.
//!
//! This is not a kernel sandbox. Block 2 adds a narrow `LD_PRELOAD` shim for a fixed
//! `clock_gettime`, deterministic `getrandom`/`getentropy`, and socket denial in dynamically
//! linked local probes. Static binaries, direct syscalls, inherited descriptors, and direct
//! `/dev/urandom` reads remain outside this boundary. Real namespaces, cgroups, Ark cells, and
//! quarantine microVMs arrive in later blocks.

use spacetime_types::{
    canonical_json_bytes, sha256_hex, Action, ActionOp, CapabilityScope, Observation,
    SpacetimeError, Taint, SCHEMA_VERSION,
};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const SHIM_PATH: &str = env!("SPACETIME_SHIM_PATH");
const SHIM_PROBE_PATH: &str = env!("SPACETIME_SHIM_PROBE_PATH");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeterminismProfile {
    pub clock: &'static str,
    pub rng_seed: u64,
    pub locale: &'static str,
    pub temp_root: &'static str,
    pub environment: &'static str,
    pub network: &'static str,
    pub shim: &'static str,
    pub capability: &'static str,
}

pub const FIXED_DETERMINISM_PROFILE: DeterminismProfile = DeterminismProfile {
    clock: "fixed_clock_gettime_shim",
    rng_seed: 0,
    locale: "C",
    temp_root: "runner_owned_pid_scoped",
    environment: "cleared",
    network: "ld_preload_socket_denied",
    shim: "ld_preload_not_complete_boundary",
    capability: "local_fixture_allow_list",
};

impl DeterminismProfile {
    pub fn digest(&self) -> String {
        let mut material = String::new();
        let _ = writeln!(material, "clock={}", self.clock);
        let _ = writeln!(material, "rng_seed={}", self.rng_seed);
        let _ = writeln!(material, "locale={}", self.locale);
        let _ = writeln!(material, "temp_root={}", self.temp_root);
        let _ = writeln!(material, "environment={}", self.environment);
        let _ = writeln!(material, "network={}", self.network);
        let _ = writeln!(material, "shim={}", self.shim);
        let _ = writeln!(material, "capability={}", self.capability);
        sha256_hex(material.as_bytes())
    }
}

pub const FIXED_CLOCK_EPOCH_MILLIS: u64 = 0;
const LCG_MULTIPLIER: u64 = 6_364_136_223_846_793_005;
const LCG_INCREMENT: u64 = 1_442_695_040_888_963_407;
const RNG_MIX_OFFSET: u64 = 0x9E37_79B9_7F4A_7C15;
const RNG_MIX1: u64 = 0xBF58_476D_1CE4_E5B9;
const RNG_MIX2: u64 = 0x94D0_49BB_1331_11EB;

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

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(RNG_MIX1);
    value ^= value >> 27;
    value = value.wrapping_mul(RNG_MIX2);
    value ^= value >> 31;
    value
}

fn fixed_rng_byte(seed: u64, index: usize) -> u8 {
    let value = mix64(seed.wrapping_add(index as u64).wrapping_add(RNG_MIX_OFFSET));
    ((value >> 24) & 0xff) as u8
}

pub fn fixed_rng_bytes(seed: u64, count: usize) -> Vec<u8> {
    (0..count)
        .map(|index| fixed_rng_byte(seed, index))
        .collect()
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
    pub shim_clock_fixed: bool,
    pub shim_rng_fixed: bool,
    pub shim_socket_denied: bool,
    pub shim_connect_denied: bool,
    pub shim_locale_c: bool,
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
            .env("LD_PRELOAD", SHIM_PATH)
            .env("LD_BIND_NOW", "1")
            .env("LC_ALL", "C")
            .env("TZ", "UTC")
            .env("SOURCE_DATE_EPOCH", "0")
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

    pub fn probe_controls(&self) -> spacetime_types::Result<ControlProbe> {
        let script = concat!(
            "unset PATH\n",
            "probe_line=\n",
            "if IFS= read -r probe_line; then :; fi\n",
            "printf 'env=<%s|%s|%s> cwd=<%s> stdin=<%s> locale=<%s>' ",
            "\"$PATH\" \"$HOME\" \"$CARGO_TERM_COLOR\" \"$(pwd)\" \"$probe_line\" \"$LC_ALL\""
        );
        let expected_shell = format!(
            "env=<||> cwd=<{}> stdin=<> locale=<C>",
            self.work_dir.path().display()
        );
        let shell_output = Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .current_dir(self.work_dir.path())
            .env_clear()
            .env("LC_ALL", "C")
            .env("TZ", "UTC")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()?;
        let shell_actual = String::from_utf8_lossy(&shell_output.stdout)
            .trim()
            .to_string();
        let shell_ok = shell_output.status.success() && shell_actual == expected_shell;

        let shim = Path::new(SHIM_PATH);
        let probe = Path::new(SHIM_PROBE_PATH);
        if !shim.is_file() || !probe.is_file() {
            return Err(SpacetimeError::new(
                "LD_PRELOAD shim or probe was not built",
            ));
        }
        let probe_output = Command::new(probe)
            .current_dir(self.work_dir.path())
            .env_clear()
            .env("LD_PRELOAD", shim)
            .env("LD_BIND_NOW", "1")
            .env("LC_ALL", "C")
            .env("TZ", "UTC")
            .env("SOURCE_DATE_EPOCH", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()?;
        let probe_actual = String::from_utf8_lossy(&probe_output.stdout)
            .trim()
            .to_string();
        let mut expected_rng = String::with_capacity(32);
        for byte in fixed_rng_bytes(FIXED_DETERMINISM_PROFILE.rng_seed, 16) {
            let _ = write!(expected_rng, "{byte:02x}");
        }
        let expected_probe = format!(
            "clock=0.000000000 rng=16 rng_hex={expected_rng} socket=-1 socket_errno=13 connect=-1 connect_errno=13 locale=C"
        );
        let shim_ok = probe_output.status.success() && probe_actual == expected_probe;

        let work_dir = self.work_dir.path().display().to_string();
        let seed = FIXED_DETERMINISM_PROFILE.rng_seed;
        let first_rng = fixed_rng_sequence(seed, 8);
        let second_rng = fixed_rng_sequence(seed, 8);
        let first_bytes = fixed_rng_bytes(seed, 16);
        let second_bytes = fixed_rng_bytes(seed, 16);
        let control = ControlProbe {
            clock_fixed: fixed_clock_millis() == FIXED_CLOCK_EPOCH_MILLIS,
            rng_seed_fixed: first_rng == second_rng
                && first_bytes == second_bytes
                && first_rng[0] != seed,
            locale_c: shell_ok && FIXED_DETERMINISM_PROFILE.locale == "C",
            temp_root_runner_owned: shell_ok && work_dir.contains("spacetime-block1"),
            environment_cleared: shell_ok,
            network_none_by_construction: FIXED_DETERMINISM_PROFILE.network
                == "ld_preload_socket_denied",
            stdin_null: shell_ok,
            shim_clock_fixed: shim_ok,
            shim_rng_fixed: shim_ok,
            shim_socket_denied: shim_ok,
            shim_connect_denied: shim_ok,
            shim_locale_c: shim_ok,
            profile_digest: self.profile_digest.clone(),
        };
        let checks = [
            control.clock_fixed,
            control.rng_seed_fixed,
            control.locale_c,
            control.temp_root_runner_owned,
            control.environment_cleared,
            control.network_none_by_construction,
            control.stdin_null,
            control.shim_clock_fixed,
            control.shim_rng_fixed,
            control.shim_socket_denied,
            control.shim_connect_denied,
            control.shim_locale_c,
        ];
        if checks.contains(&false) {
            return Err(SpacetimeError::new(format!(
                "deterministic control probe failed: shell expected {expected_shell:?}, got {shell_actual:?}; shim expected {expected_probe:?}, got {probe_actual:?}"
            )));
        }
        Ok(control)
    }
}

impl Runner for ScriptedRunner {
    fn run(&self, action: &Action) -> RunResult {
        if let Some(reason) = capability_error(action) {
            return error_result(
                action,
                format!("capability denied: {reason}"),
                &self.profile_digest,
            );
        }
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

fn capability_error(action: &Action) -> Option<String> {
    if action.schema_version != SCHEMA_VERSION {
        return Some("unsupported action schema version".to_string());
    }
    if action.capability.scope != CapabilityScope::LocalFixture {
        return Some("capability scope is not local_fixture".to_string());
    }
    if action.budget_millis == 0 || action.capability.limit_millis == 0 {
        return Some("capability has no time budget".to_string());
    }
    if action.capability.limit_millis > action.budget_millis {
        return Some("capability limit exceeds action budget".to_string());
    }
    if matches!(
        action.taint,
        Taint::UntrustedWeb | Taint::UntrustedToolOutput
    ) {
        return Some("untrusted action taint is denied".to_string());
    }
    let expected_args_digest = match canonical_json_bytes(&action.op) {
        Ok(bytes) => sha256_hex(bytes.as_slice()),
        Err(error) => {
            return Some(format!(
                "capability args could not be canonicalized: {error}"
            ));
        }
    };
    if action.args_digest.as_str() != expected_args_digest.as_str() {
        return Some("action args digest does not match its operation".to_string());
    }
    if action.expected_verifier.as_deref().unwrap_or("").is_empty() {
        return Some("action has no expected verifier".to_string());
    }
    None
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
        digest: sha256_hex(bytes.as_bytes()),
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
            Some("fixture-verifier-v1".to_string()),
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
        assert!(probe.shim_clock_fixed);
        assert!(probe.shim_rng_fixed);
        assert!(probe.shim_socket_denied);
        assert!(probe.shim_connect_denied);
        assert!(probe.shim_locale_c);
        let expected = FIXED_DETERMINISM_PROFILE.digest();
        assert_eq!(probe.profile_digest.as_str(), expected.as_str());
        let first = fixed_rng_sequence(FIXED_DETERMINISM_PROFILE.rng_seed, 8);
        let second = fixed_rng_sequence(FIXED_DETERMINISM_PROFILE.rng_seed, 8);
        assert_eq!(first, second);
        assert_ne!(first[0], FIXED_DETERMINISM_PROFILE.rng_seed);
        let first_bytes = fixed_rng_bytes(FIXED_DETERMINISM_PROFILE.rng_seed, 16);
        let second_bytes = fixed_rng_bytes(FIXED_DETERMINISM_PROFILE.rng_seed, 16);
        assert_eq!(first_bytes, second_bytes);
        Ok(())
    }

    #[test]
    fn capability_allow_list_denies_tampered_actions() {
        let runner = ScriptedRunner::new().expect("runner");
        let valid = action(
            ActionOp::Echo {
                text: "stable".to_string(),
            },
            1_000,
        );
        assert_eq!(runner.run(&valid).status, RunStatus::Completed);

        let mut no_scope = valid.clone();
        no_scope.capability.scope = CapabilityScope::None;
        let mut no_budget = valid.clone();
        no_budget.capability.limit_millis = 0;
        let mut bad_digest = valid.clone();
        bad_digest.args_digest = "tampered".to_string();
        let mut untrusted = valid.clone();
        untrusted.taint = Taint::UntrustedWeb;
        let mut no_verifier = valid.clone();
        no_verifier.expected_verifier = None;
        let mut bad_schema = valid.clone();
        bad_schema.schema_version = SCHEMA_VERSION + 1;

        let denied = [
            no_scope,
            no_budget,
            bad_digest,
            untrusted,
            no_verifier,
            bad_schema,
        ];
        for action in denied {
            let result = runner.run(&action);
            match result.status {
                RunStatus::Error(reason) => assert!(reason.contains("capability denied")),
                RunStatus::Completed => panic!("tampered action was executed"),
            }
        }
    }
}
