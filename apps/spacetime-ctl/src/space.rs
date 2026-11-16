//! Experimental, offline QEMU boot spike. This is not a hardened malware sandbox.

use spacetime_types::{Result, SpacetimeError};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;

const USAGE: &str = "usage: space boot --experimental --kernel <file> --initramfs <file>";

#[derive(Debug, PartialEq, Eq)]
struct BootArgs {
    kernel: PathBuf,
    initramfs: PathBuf,
}

fn parse_args(args: &[String]) -> Result<BootArgs> {
    if args.first().map(String::as_str) != Some("boot") {
        return Err(SpacetimeError::new(USAGE));
    }
    let mut kernel = None;
    let mut initramfs = None;
    let mut experimental = false;
    let mut iter = args[1..].iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--experimental" if !experimental => experimental = true,
            "--kernel" | "--initramfs" => {
                let value = iter.next().filter(|v| !v.starts_with('-')).ok_or_else(|| {
                    SpacetimeError::new(format!("missing value for {arg}\n{USAGE}"))
                })?;
                let slot = if arg == "--kernel" {
                    &mut kernel
                } else {
                    &mut initramfs
                };
                if slot.replace(PathBuf::from(value)).is_some() {
                    return Err(SpacetimeError::new(format!("duplicate {arg}")));
                }
            }
            _ => {
                return Err(SpacetimeError::new(format!(
                    "unexpected argument: {arg}\n{USAGE}"
                )))
            }
        }
    }
    if !experimental {
        return Err(SpacetimeError::new(format!(
            "this is an experimental VM boot spike, not a security sandbox; acknowledge with --experimental\n{USAGE}"
        )));
    }
    Ok(BootArgs {
        kernel: kernel.ok_or_else(|| SpacetimeError::new(format!("missing --kernel\n{USAGE}")))?,
        initramfs: initramfs
            .ok_or_else(|| SpacetimeError::new(format!("missing --initramfs\n{USAGE}")))?,
    })
}

fn regular_file(path: &Path, name: &str) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| SpacetimeError::new(format!("cannot open {name}: {error}")))?;
    if !fs::metadata(&canonical)?.is_file() {
        return Err(SpacetimeError::new(format!(
            "{name} must be a regular file"
        )));
    }
    Ok(canonical)
}

fn qemu_command(kernel: &Path, initramfs: &Path, kvm: bool) -> Command {
    let mut command = Command::new("qemu-system-x86_64");
    // No disk, host shares, NIC, graphical display, monitor, or QEMU config files.
    // The guest is provided entirely by the supplied kernel and initramfs.
    command
        .env_clear()
        .args([
            "-no-user-config",
            "-nodefaults",
            "-machine",
            if kvm {
                "microvm,accel=kvm"
            } else {
                "microvm,accel=tcg"
            },
            "-cpu",
            if kvm { "host" } else { "max" },
            "-m",
            "512",
            "-smp",
            "1",
            "-display",
            "none",
            "-monitor",
            "none",
            "-serial",
            "stdio",
            "-nic",
            "none",
            "-no-reboot",
            "-sandbox",
            "on,obsolete=deny,elevateprivileges=deny,spawn=deny,resourcecontrol=deny",
            "-kernel",
        ])
        .arg(kernel)
        .arg("-initrd")
        .arg(initramfs)
        .args(["-append", "console=ttyS0 panic=-1"]);
    command
}

// The VM monitor itself runs with a private network and mount namespace. We deliberately do
// not fall back to an unconfined QEMU if bubblewrap or user namespaces are unavailable.
fn sandbox_command(kernel: &Path, initramfs: &Path, kvm: bool) -> Command {
    let mut command = Command::new("/usr/bin/prlimit");
    command.env_clear().args([
        "--core=0:0",
        "--",
        "/usr/bin/bwrap",
        "--die-with-parent",
        "--new-session",
        "--unshare-all",
        "--clearenv",
        "--setenv",
        "PATH",
        "/usr/bin:/bin",
        "--setenv",
        "HOME",
        "/nonexistent",
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
    ]);
    if kvm {
        command.args(["--dev-bind", "/dev/kvm", "/dev/kvm"]);
    }
    command
        .args(["--tmpfs", "/tmp", "--dir", "/inputs", "--ro-bind"])
        .arg(kernel)
        .arg("/inputs/kernel")
        .arg("--ro-bind")
        .arg(initramfs)
        .args([
            "/inputs/initramfs",
            "--chdir",
            "/",
            "--",
            "/usr/bin/qemu-system-x86_64",
        ]);
    let qemu = qemu_command(
        Path::new("/inputs/kernel"),
        Path::new("/inputs/initramfs"),
        kvm,
    );
    command.args(qemu.get_args());
    command
}

fn require_program(path: &str, package: &str) -> Result<()> {
    let metadata = fs::metadata(path).map_err(|error| {
        SpacetimeError::new(format!("{package} is required at {path}: {error}"))
    })?;
    if !metadata.is_file() {
        return Err(SpacetimeError::new(format!("{path} is not a regular file")));
    }
    Ok(())
}

pub fn cmd_space(args: &[String]) -> Result<()> {
    if args.first().map(String::as_str) == Some("run") {
        return crate::workspace::cmd_run(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("initramfs") {
        return crate::initramfs::cmd_initramfs(&args[1..]);
    }
    let parsed = parse_args(args)?;
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return Err(SpacetimeError::new(
            "the experimental VM requires Linux on x86-64 with KVM",
        ));
    }
    let kernel = regular_file(&parsed.kernel, "kernel")?;
    let initramfs = regular_file(&parsed.initramfs, "initramfs")?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .map_err(|error| SpacetimeError::new(format!("KVM is unavailable: {error}")))?;
    require_program("/usr/bin/prlimit", "util-linux prlimit")?;
    require_program("/usr/bin/bwrap", "bubblewrap")?;
    require_program("/usr/bin/qemu-system-x86_64", "QEMU")?;
    eprintln!(
        "Experimental offline VM: trusted images only. This is not a hardened sandbox.\n\
         No guest NIC, disk, or host file share is configured. Guest RAM and terminal output\n\
         can still leave traces on the host. Exit from the guest to stop QEMU."
    );
    let status = sandbox_command(&kernel, &initramfs, true)
        .status()
        .map_err(|error| SpacetimeError::new(format!("could not start the VM sandbox: {error}")))?;
    if !status.success() {
        return Err(SpacetimeError::new(format!("VM sandbox or QEMU exited with {status}; check bubblewrap, user namespaces and KVM permissions")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn requires_explicit_acknowledgement_and_both_images() {
        assert!(parse_args(&strings(&["boot", "--kernel", "k", "--initramfs", "i"])).is_err());
        assert!(parse_args(&strings(&["boot", "--experimental", "--kernel", "k"])).is_err());
        assert!(parse_args(&strings(&[
            "boot",
            "--experimental",
            "--kernel",
            "k",
            "--kernel",
            "other",
            "--initramfs",
            "i"
        ]))
        .is_err());
        assert!(parse_args(&strings(&[
            "boot",
            "--experimental",
            "--kernel",
            "--initramfs",
            "i"
        ]))
        .is_err());
        let parsed = parse_args(&strings(&[
            "boot",
            "--experimental",
            "--kernel",
            "k",
            "--initramfs",
            "i",
        ]))
        .unwrap();
        assert_eq!(parsed.kernel, PathBuf::from("k"));
        assert_eq!(parsed.initramfs, PathBuf::from("i"));
    }

    #[test]
    fn qemu_configuration_is_offline_and_has_no_host_share() {
        let command = qemu_command(Path::new("kernel"), Path::new("initramfs"), true);
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect();
        assert!(args.windows(2).any(|a| a == ["-nic", "none"]));
        assert!(args
            .windows(2)
            .any(|a| a == ["-machine", "microvm,accel=kvm"]));
        assert!(args.contains(&"-nodefaults".into()));
        assert!(args.contains(&"-sandbox".into()));
        for flag in ["-drive", "-blockdev", "-netdev", "-virtfs", "-fsdev"] {
            assert!(!args.iter().any(|arg| arg == flag));
        }
    }

    #[test]
    fn monitor_sandbox_is_mandatory_and_offline() {
        let command = sandbox_command(Path::new("/host/kernel"), Path::new("/host/guest"), true);
        assert_eq!(command.get_program(), "/usr/bin/prlimit");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect();
        assert!(args.contains(&"--core=0:0".into()));
        assert!(args.contains(&"/usr/bin/bwrap".into()));
        assert!(args.contains(&"--unshare-all".into()));
        assert!(args.contains(&"--die-with-parent".into()));
        assert!(args
            .windows(3)
            .any(|a| a == ["--dev-bind", "/dev/kvm", "/dev/kvm"]));
        assert!(args
            .windows(3)
            .any(|a| a == ["--ro-bind", "/host/kernel", "/inputs/kernel"]));
        assert!(args
            .windows(3)
            .any(|a| a == ["--ro-bind", "/host/guest", "/inputs/initramfs"]));
        assert!(args.windows(2).any(|a| a == ["-nic", "none"]));
        assert!(!args.iter().any(|arg| arg == "/home"));
    }

    // The production command always requires KVM. This opt-in CI test uses software emulation
    // solely to exercise the *same* bubblewrap and guest boot path on a runner without /dev/kvm.
    #[test]
    #[ignore = "requires QEMU, bubblewrap, user namespaces and CI-supplied guest images"]
    fn sandboxed_guest_boots_without_network() {
        use std::io::Write;
        use std::process::Stdio;
        use std::thread;
        use std::time::Duration;

        let kernel = std::env::var("SPACETIME_GUEST_KERNEL").unwrap();
        let initramfs = std::env::var("SPACETIME_GUEST_INITRAMFS").unwrap();
        let mut child = sandbox_command(Path::new(&kernel), Path::new(&initramfs), false)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let writer = thread::spawn(move || {
            thread::sleep(Duration::from_secs(25));
            stdin.write_all(b"app\nif [ -z \"$(ls /sys/class/net | grep -v '^lo$')\" ]; then echo SPACETIME_OFFLINE_OK; fi\nexit\n")
        });
        let output = child.wait_with_output().unwrap();
        let _ = writer.join();
        let stdout = String::from_utf8_lossy(&output.stdout).replace('\r', "");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "stdout: {stdout}\nstderr: {stderr}"
        );
        assert!(
            stdout.lines().any(|line| line == "SPACETIME_APP_OK"),
            "{stdout}"
        );
        assert!(
            stdout.lines().any(|line| line == "SPACETIME_OFFLINE_OK"),
            "{stdout}"
        );
    }

    #[test]
    fn refuses_non_files() {
        assert!(regular_file(Path::new("."), "kernel").is_err());
        assert!(regular_file(Path::new("missing-spacetime-kernel"), "kernel").is_err());
    }
}
