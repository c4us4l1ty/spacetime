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
            _ => return Err(SpacetimeError::new(format!("unexpected argument: {arg}\n{USAGE}"))),
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
        return Err(SpacetimeError::new(format!("{name} must be a regular file")));
    }
    Ok(canonical)
}

fn qemu_command(kernel: &Path, initramfs: &Path) -> Command {
    let mut command = Command::new("qemu-system-x86_64");
    // No disk, host shares, NIC, graphical display, monitor, or QEMU config files.
    // The guest is provided entirely by the supplied kernel and initramfs.
    command
        .env_clear()
        .args([
            "-no-user-config",
            "-nodefaults",
            "-machine",
            "microvm,accel=kvm",
            "-cpu",
            "host",
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

pub fn cmd_space(args: &[String]) -> Result<()> {
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
    eprintln!(
        "Experimental offline VM: trusted images only. This is not a hardened sandbox.\n\
         No guest NIC, disk, or host file share is configured. Guest RAM and terminal output\n\
         can still leave traces on the host. Exit from the guest to stop QEMU."
    );
    let status = qemu_command(&kernel, &initramfs)
        .status()
        .map_err(|error| SpacetimeError::new(format!("could not start QEMU: {error}")))?;
    if !status.success() {
        return Err(SpacetimeError::new(format!("QEMU exited with {status}")));
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
            "boot", "--experimental", "--kernel", "k", "--kernel", "other", "--initramfs", "i"
        ]))
        .is_err());
        assert!(parse_args(&strings(&[
            "boot", "--experimental", "--kernel", "--initramfs", "i"
        ]))
        .is_err());
        let parsed = parse_args(&strings(&[
            "boot", "--experimental", "--kernel", "k", "--initramfs", "i"
        ]))
        .unwrap();
        assert_eq!(parsed.kernel, PathBuf::from("k"));
        assert_eq!(parsed.initramfs, PathBuf::from("i"));
    }

    #[test]
    fn qemu_configuration_is_offline_and_has_no_host_share() {
        let command = qemu_command(Path::new("kernel"), Path::new("initramfs"));
        let args: Vec<_> = command.get_args().map(|arg| arg.to_string_lossy()).collect();
        assert!(args.windows(2).any(|a| a == ["-nic", "none"]));
        assert!(args.windows(2).any(|a| a == ["-machine", "microvm,accel=kvm"]));
        assert!(args.contains(&"-nodefaults".into()));
        assert!(args.contains(&"-sandbox".into()));
        for flag in ["-drive", "-blockdev", "-netdev", "-virtfs", "-fsdev"] {
            assert!(!args.iter().any(|arg| arg == flag));
        }
    }

    #[test]
    fn refuses_non_files() {
        assert!(regular_file(Path::new("."), "kernel").is_err());
        assert!(regular_file(Path::new("missing-spacetime-kernel"), "kernel").is_err());
    }
}
