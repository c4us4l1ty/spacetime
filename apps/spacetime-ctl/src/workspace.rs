//! A small Rust terminal interface and one-shot experimental offline workspace.

use spacetime_types::{Result, SpacetimeError};
use std::fs::{self, DirBuilder};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);
const USAGE: &str =
    "usage: space run --experimental --kernel <file> --busybox <file> [--app <file>]";

#[derive(Debug, PartialEq, Eq)]
struct RunArgs {
    kernel: String,
    busybox: String,
    app: Option<String>,
}

fn parse_args(args: &[String]) -> Result<RunArgs> {
    let mut kernel = None;
    let mut busybox = None;
    let mut app = None;
    let mut experimental = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--experimental" && !experimental {
            experimental = true;
            continue;
        }
        let slot = match arg.as_str() {
            "--kernel" => &mut kernel,
            "--busybox" => &mut busybox,
            "--app" => &mut app,
            _ => {
                return Err(SpacetimeError::new(format!(
                    "unexpected argument: {arg}\n{USAGE}"
                )))
            }
        };
        let value = iter
            .next()
            .filter(|v| !v.starts_with('-'))
            .ok_or_else(|| SpacetimeError::new(format!("missing value for {arg}\n{USAGE}")))?;
        if slot.replace(value.clone()).is_some() {
            return Err(SpacetimeError::new(format!("duplicate {arg}")));
        }
    }
    if !experimental {
        return Err(SpacetimeError::new(format!(
            "experimental acknowledgement required\n{USAGE}"
        )));
    }
    Ok(RunArgs {
        kernel: kernel.ok_or_else(|| SpacetimeError::new(format!("missing --kernel\n{USAGE}")))?,
        busybox: busybox
            .ok_or_else(|| SpacetimeError::new(format!("missing --busybox\n{USAGE}")))?,
        app,
    })
}

// Use a private RAM-backed directory for the generated guest image. A tmpfs can still be
// swapped, and an abrupt termination can leave the file there until manually removed or reboot.
fn has_tmpfs_shm(mountinfo: &str) -> bool {
    mountinfo.lines().any(|line| {
        let Some((mount, details)) = line.split_once(" - ") else {
            return false;
        };
        mount.split_whitespace().nth(4) == Some("/dev/shm")
            && details.split_whitespace().next() == Some("tmpfs")
    })
}

fn temporary_base() -> Result<PathBuf> {
    if !cfg!(target_os = "linux") {
        return Err(SpacetimeError::new(
            "the experimental workspace requires Linux",
        ));
    }
    let mountinfo = fs::read_to_string("/proc/self/mountinfo")?;
    if !has_tmpfs_shm(&mountinfo) {
        return Err(SpacetimeError::new(
            "/dev/shm must be a mounted tmpfs for the temporary guest image",
        ));
    }
    Ok(PathBuf::from("/dev/shm"))
}

struct TemporaryImage(PathBuf);

impl TemporaryImage {
    fn create() -> Result<Self> {
        let base = temporary_base()?;
        for _ in 0..100 {
            let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
            let path = base.join(format!("spacetime-{}-{id}", std::process::id()));
            let mut builder = DirBuilder::new();
            #[cfg(unix)]
            builder.mode(0o700);
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(SpacetimeError::new(
            "could not create a private temporary workspace",
        ))
    }

    fn image_path(&self) -> PathBuf {
        self.0.join("guest.cpio")
    }
}

impl Drop for TemporaryImage {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!(
                "warning: could not remove temporary workspace {}: {error}",
                self.0.display()
            );
        }
    }
}

pub fn cmd_run(args: &[String]) -> Result<()> {
    let parsed = parse_args(args)?;
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return Err(SpacetimeError::new(
            "the experimental VM requires Linux x86-64",
        ));
    }
    let temporary = TemporaryImage::create()?;
    let image = temporary.image_path();
    let image_text = image
        .to_str()
        .ok_or_else(|| SpacetimeError::new("temporary path is not UTF-8"))?;
    let mut image_args = vec![
        "--busybox".to_string(),
        parsed.busybox,
        "--out".to_string(),
        image_text.to_string(),
    ];
    if let Some(app) = parsed.app {
        image_args.extend(["--app".to_string(), app]);
    }
    crate::initramfs::cmd_initramfs(&image_args)?;
    let boot_args = vec![
        "boot".to_string(),
        "--experimental".to_string(),
        "--kernel".to_string(),
        parsed.kernel,
        "--initramfs".to_string(),
        image_text.to_string(),
    ];
    crate::space::cmd_space(&boot_args)
}

fn prompt(label: &str) -> Result<Option<String>> {
    print!("{label}");
    io::stdout().flush()?;
    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        return Ok(None);
    }
    Ok(Some(line.trim().to_string()))
}

fn default_kernel() -> Option<String> {
    let release = fs::read_to_string("/proc/sys/kernel/osrelease").ok()?;
    let release = release.trim();
    if release.is_empty()
        || !release
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
    {
        return None;
    }
    let path = PathBuf::from("/boot").join(format!("vmlinuz-{release}"));
    if path.is_file() && fs::File::open(&path).is_ok() {
        Some(path.to_string_lossy().into_owned())
    } else {
        None
    }
}

fn default_busybox() -> Option<String> {
    ["/bin/busybox", "/usr/bin/busybox"]
        .iter()
        .find(|path| std::path::Path::new(path).is_file() && fs::File::open(path).is_ok())
        .map(|path| (*path).to_string())
}

fn prompt_path(label: &str, default: Option<String>) -> Result<Option<String>> {
    let message = match &default {
        Some(value) => format!("{label} [{value}]: "),
        None => format!("{label}: "),
    };
    Ok(prompt(&message)?.map(|value| {
        if value.is_empty() {
            default.unwrap_or_default()
        } else {
            value
        }
    }))
}

pub fn cmd_ui(args: &[String]) -> Result<()> {
    if !args.is_empty() {
        return Err(SpacetimeError::new("usage: workspace"));
    }
    println!(
        "SPACETIME | experimental offline workspace\n\
        Only use trusted software. This is not yet a hardened malware sandbox.\n\
        It runs one static terminal app in a temporary VM, without guest networking.\n\
        No graphical apps, AI, or verified 4 GB performance yet.\n"
    );
    loop {
        println!("1. Start a workspace\n2. Quit");
        let Some(choice) = prompt("> ")? else {
            return Ok(());
        };
        match choice.as_str() {
            "2" | "q" | "quit" => return Ok(()),
            "1" => {
                let Some(kernel) = prompt_path("Linux kernel path", default_kernel())? else {
                    return Ok(());
                };
                let Some(busybox) = prompt_path("Static BusyBox path", default_busybox())? else {
                    return Ok(());
                };
                let Some(app) = prompt("Static terminal app path (blank for shell): ")? else {
                    return Ok(());
                };
                let mut arguments = vec![
                    "--experimental".to_string(),
                    "--kernel".to_string(),
                    kernel,
                    "--busybox".to_string(),
                    busybox,
                ];
                if !app.is_empty() {
                    arguments.extend(["--app".to_string(), app]);
                }
                if let Err(error) = cmd_run(&arguments) {
                    eprintln!("Could not run workspace: {error}");
                }
            }
            _ => println!("Choose 1 or 2."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_requires_explicit_experimental_and_unique_inputs() {
        let args = |s: &[&str]| s.iter().map(|v| v.to_string()).collect::<Vec<_>>();
        assert!(parse_args(&args(&["--kernel", "k", "--busybox", "b"])).is_err());
        assert!(parse_args(&args(&["--experimental", "--kernel", "k"])).is_err());
        assert!(parse_args(&args(&[
            "--experimental",
            "--kernel",
            "k",
            "--kernel",
            "k",
            "--busybox",
            "b"
        ]))
        .is_err());
        assert_eq!(
            parse_args(&args(&[
                "--experimental",
                "--kernel",
                "k",
                "--busybox",
                "b",
                "--app",
                "a"
            ]))
            .unwrap()
            .app
            .as_deref(),
            Some("a")
        );
    }

    #[test]
    fn detects_only_actual_shm_tmpfs_mounts() {
        assert!(has_tmpfs_shm("36 25 0:32 / /dev/shm rw - tmpfs tmpfs rw"));
        assert!(!has_tmpfs_shm(
            "36 25 0:32 / /dev/shm rw - ext4 /dev/sda rw"
        ));
        assert!(!has_tmpfs_shm("36 25 0:32 / /tmp rw - tmpfs tmpfs rw"));
        assert!(!has_tmpfs_shm("not a mount entry"));
    }

    #[test]
    fn temporary_workspace_removed_on_drop() {
        let temp = TemporaryImage::create().unwrap();
        let dir = temp.0.clone();
        fs::write(temp.image_path(), b"data").unwrap();
        assert!(dir.is_dir());
        drop(temp);
        assert!(!dir.exists());
    }
}
