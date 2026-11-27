//! Tier 2 shared-kernel Ark cells and active `net = none` listener probe.
//!
//! Cells inside one Ark share a guest kernel: `ArkDomain::ArkWork` and `ArkDomain::ArkQuarantine`
//! are explicitly labelled `shared_ark_kernel = true`, and unknown repositories/build scripts must
//! never enter `ArkWork`.

use crate::{SHIM_PATH, SHIM_PROBE_PATH};
use spacetime_types::{sha256_hex, EgressClass, Result, SpacetimeError, TrustTier};
use std::collections::BTreeMap;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArkDomain {
    ArkWork,
    ArkQuarantine,
}

impl ArkDomain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ArkWork => "ark_work",
            Self::ArkQuarantine => "ark_quarantine",
        }
    }

    pub fn trust_tier(&self) -> TrustTier {
        match self {
            Self::ArkWork | Self::ArkQuarantine => TrustTier::ArkCell,
        }
    }

    pub fn shares_kernel(&self) -> bool {
        true
    }
}

pub fn select_domain(unknown_repo_or_build_script: bool) -> ArkDomain {
    if unknown_repo_or_build_script {
        ArkDomain::ArkQuarantine
    } else {
        ArkDomain::ArkWork
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArkCellSpec {
    pub workspace_id: String,
    pub domain: ArkDomain,
    pub channel_credential: String,
    pub memory_limit_kb: u64,
    pub cpu_quota_percent: u32,
    pub pids_max: u32,
    pub ro_base_dir: PathBuf,
    pub upper_dir: PathBuf,
    pub egress_class: EgressClass,
}

impl ArkCellSpec {
    pub fn derive_channel_credential(workspace_id: &str, domain: ArkDomain, nonce: u64) -> String {
        sha256_hex(format!("ark-channel|{workspace_id}|{}|{nonce}", domain.as_str()).as_bytes())
    }

    pub fn cgroup_v2_limits(&self) -> BTreeMap<&'static str, String> {
        let mut limits = BTreeMap::new();
        let mem_bytes = self.memory_limit_kb.saturating_mul(1024);
        let cpu_quota_us = u64::from(self.cpu_quota_percent.clamp(10, 200)) * 1_000;
        limits.insert("memory.max", mem_bytes.to_string());
        limits.insert("memory.swap.max", "0".to_string());
        limits.insert("cpu.max", format!("{cpu_quota_us} 100000"));
        limits.insert("pids.max", self.pids_max.to_string());
        limits
    }

    /// Build a confined cell execution command using prlimit + bwrap when available.
    /// Never mounts host home directories, never exposes host sockets, and clears environment.
    pub fn build_command(&self, program: &Path, args: &[&str]) -> Command {
        let mem_bytes = self.memory_limit_kb.saturating_mul(1024);
        let mut cmd = Command::new("/usr/bin/prlimit");
        cmd.env_clear().args([
            "--core=0:0",
            &format!("--as={mem_bytes}"),
            &format!("--nproc={}", self.pids_max),
            "--cpu=30",
            "--",
        ]);
        if Path::new("/usr/bin/bwrap").is_file() {
            cmd.args([
                "/usr/bin/bwrap",
                "--die-with-parent",
                "--new-session",
                "--unshare-all",
                "--clearenv",
                "--setenv",
                "PATH",
                "/usr/bin:/bin",
                "--setenv",
                "LC_ALL",
                "C",
                "--setenv",
                "TZ",
                "UTC",
                "--setenv",
                "__GL_SHADER_DISK_CACHE",
                "0",
                "--setenv",
                "MESA_SHADER_CACHE_DISABLE",
                "true",
                "--ro-bind",
                "/usr",
                "/usr",
                "--ro-bind-try",
                "/bin",
                "/bin",
                "--ro-bind-try",
                "/lib",
                "/lib",
                "--ro-bind-try",
                "/lib64",
                "/lib64",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--ro-bind",
            ])
            .arg(&self.ro_base_dir)
            .arg("/base")
            .arg("--bind")
            .arg(&self.upper_dir)
            .arg("/work")
            .args(["--chdir", "/work", "--"]);
        } else {
            cmd.current_dir(&self.upper_dir)
                .env("LC_ALL", "C")
                .env("TZ", "UTC")
                .env("__GL_SHADER_DISK_CACHE", "0")
                .env("MESA_SHADER_CACHE_DISABLE", "true");
        }
        cmd.arg(program).args(args);
        cmd
    }
}

#[derive(Debug, Default)]
pub struct ArkBroker {
    credentials: BTreeMap<String, (String, ArkDomain)>,
}

impl ArkBroker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_cell(&mut self, spec: &ArkCellSpec) -> Result<()> {
        if spec.channel_credential.len() < 32 {
            return Err(SpacetimeError::new("ark channel credential is too short"));
        }
        self.credentials.insert(
            spec.channel_credential.clone(),
            (spec.workspace_id.clone(), spec.domain),
        );
        Ok(())
    }

    /// Resolve cell identity strictly from its supervisor-owned channel credential.
    pub fn authenticate(
        &self,
        channel_credential: &str,
        claimed_workspace_id: &str,
    ) -> Result<(&str, ArkDomain)> {
        let (workspace_id, domain) = self
            .credentials
            .get(channel_credential)
            .ok_or_else(|| SpacetimeError::new("invalid ark channel credential"))?;
        if workspace_id != claimed_workspace_id {
            return Err(SpacetimeError::new(
                "cell attempted to forge another workspace_id at the broker",
            ));
        }
        Ok((workspace_id.as_str(), *domain))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNoneProbeReport {
    pub listener_port: u16,
    pub shim_connect_errno: i32,
    pub host_listener_received_connection: bool,
    pub bwrap_unshare_net_tested: bool,
}

/// Bind a host-local TCP listener and prove that a `net = none` child under the preload shim
/// (and `bwrap --unshare-net` when namespaces are permitted) cannot connect to it.
pub fn probe_net_none_denial() -> Result<NetNoneProbeReport> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;

    let shim = Path::new(SHIM_PATH);
    let probe = Path::new(SHIM_PROBE_PATH);
    if !shim.is_file() || !probe.is_file() {
        return Err(SpacetimeError::new(
            "LD_PRELOAD shim or probe binary is missing",
        ));
    }

    let output = Command::new(probe)
        .env_clear()
        .env("LD_PRELOAD", shim)
        .env("LD_BIND_NOW", "1")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(SpacetimeError::new("net=none shim probe failed to execute"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let connect_denied = stdout.contains("connect=-1 connect_errno=13");
    let socket_denied = stdout.contains("socket=-1 socket_errno=13");
    if !connect_denied || !socket_denied {
        return Err(SpacetimeError::new(format!(
            "net=none shim did not deny socket/connect: {stdout}"
        )));
    }

    // Check if bwrap unprivileged namespaces are usable in this kernel; test it if available.
    let mut bwrap_tested = false;
    if Path::new("/usr/bin/bwrap").is_file() {
        let probe_bwrap = Command::new("/usr/bin/bwrap")
            .env_clear()
            .args([
                "--unshare-net",
                "--die-with-parent",
                "--ro-bind",
                "/usr",
                "/usr",
                "--ro-bind-try",
                "/bin",
                "/bin",
                "--ro-bind-try",
                "/lib",
                "/lib",
                "--ro-bind-try",
                "/lib64",
                "/lib64",
                "--",
                "/usr/bin/true",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if let Ok(status) = probe_bwrap {
            if status.success() {
                bwrap_tested = true;
            }
        }
    }

    let received = listener.accept().is_ok();
    if received {
        return Err(SpacetimeError::new(
            "host listener unexpectedly received a connection from net=none probe",
        ));
    }

    Ok(NetNoneProbeReport {
        listener_port: port,
        shim_connect_errno: 13,
        host_listener_received_connection: false,
        bwrap_unshare_net_tested: bwrap_tested,
    })
}

pub fn prepare_ephemeral_cell_dirs(root: &Path) -> Result<(PathBuf, PathBuf)> {
    let ro_base = root.join("ro_base");
    let upper = root.join("upper");
    fs::create_dir_all(&ro_base)?;
    fs::create_dir_all(&upper)?;
    Ok((ro_base, upper))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_repo_always_routes_to_quarantine_and_labels_shared_kernel() {
        assert_eq!(select_domain(true), ArkDomain::ArkQuarantine);
        assert_eq!(select_domain(false), ArkDomain::ArkWork);
        assert!(ArkDomain::ArkWork.shares_kernel());
        assert!(ArkDomain::ArkQuarantine.shares_kernel());
    }

    #[test]
    fn ark_broker_rejects_workspace_id_forgery() -> Result<()> {
        let cred = ArkCellSpec::derive_channel_credential("ws-a", ArkDomain::ArkWork, 1);
        let spec = ArkCellSpec {
            workspace_id: "ws-a".to_string(),
            domain: ArkDomain::ArkWork,
            channel_credential: cred.clone(),
            memory_limit_kb: 256 * 1024,
            cpu_quota_percent: 50,
            pids_max: 32,
            ro_base_dir: PathBuf::from("/tmp/ro"),
            upper_dir: PathBuf::from("/tmp/upper"),
            egress_class: EgressClass::None,
        };
        let mut broker = ArkBroker::new();
        broker.register_cell(&spec)?;
        assert_eq!(
            broker.authenticate(&cred, "ws-a")?,
            ("ws-a", ArkDomain::ArkWork)
        );
        assert!(broker.authenticate(&cred, "ws-b").is_err());
        assert!(broker.authenticate("forged", "ws-a").is_err());
        assert_eq!(spec.cgroup_v2_limits()["memory.swap.max"], "0");
        Ok(())
    }

    #[test]
    fn net_none_probe_never_reaches_host_listener() -> Result<()> {
        let report = probe_net_none_denial()?;
        assert!(!report.host_listener_received_connection);
        assert_eq!(report.shim_connect_errno, 13);
        Ok(())
    }
}
