//! Guest-side control plane.
//!
//! This crate is compiled for the guest (`x86_64-unknown-linux-musl`,
//! `aarch64-unknown-linux-musl`) into a static binary that becomes PID 1 of each
//! microVM. It has to be tiny, dependency-free and boring: a crashing `init`
//! means a dead tab.
//!
//! The modules below contain the *framing and policy* of what PID 1 does, kept
//! host-buildable and unit-tested. The syscall plumbing (`mount`, `pivot_root`,
//! `openpty`, `fork`, `execve`) lives behind `#[cfg(target_os = "linux")]` in the
//! eventual binary and is deliberately not faked here.
//!
//! Wire protocol (one request/response per line over `/dev/hvc1`):
//!
//! ```text
//! guest → host   READY <version>
//! host  → guest  EXEC <argv0> <arg1> ...      start a process on a new PTY
//! host  → guest  RESIZE <pid> <cols> <rows>   window resize
//! host  → guest  CLIP <base64>                 sanitized clipboard paste
//! host  → guest  POWEROFF                      clean shutdown
//! guest → host   PID <pid> EXIT <code>        process lifecycle
//! ```

/// Version of the control protocol; bumped on breaking changes.
pub const PROTOCOL_VERSION: u32 = 1;

/// A message from the host to the guest agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostCommand {
    /// Start `argv` on a fresh PTY.
    Exec { argv: Vec<String> },
    /// Resize the PTY of a running process.
    Resize { pid: u32, cols: u16, rows: u16 },
    /// Deliver sanitized clipboard text (UTF-8 only, already stripped of HTML
    /// and control sequences by the host).
    Clip { text: String },
    /// Shut the VM down cleanly.
    Poweroff,
}

/// A message from the guest agent to the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuestEvent {
    Ready { version: u32 },
    Pid { pid: u32 },
    Exit { pid: u32, code: i32 },
    Error { message: String },
}

/// Encode a host command as one wire line (`\n` terminated by the writer).
pub fn encode(command: &HostCommand) -> String {
    match command {
        HostCommand::Exec { argv } => {
            let mut line = String::from("EXEC");
            for arg in argv {
                line.push(' ');
                line.push_str(&escape(arg));
            }
            line
        }
        HostCommand::Resize { pid, cols, rows } => format!("RESIZE {pid} {cols} {rows}"),
        HostCommand::Clip { text } => format!("CLIP {}", escape(text)),
        HostCommand::Poweroff => "POWEROFF".to_string(),
    }
}

/// Parse one wire line into a host command.
pub fn decode(line: &str) -> Result<HostCommand, String> {
    let mut parts = line.trim_end_matches(['\r', '\n']).splitn(2, ' ');
    let verb = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("").trim();
    match verb {
        "" => Err("empty line".to_string()),
        "EXEC" => {
            let argv: Vec<String> = rest.split(' ').map(unescape).collect();
            if argv.first().map(|s| s.is_empty()).unwrap_or(true) {
                return Err("EXEC needs a program".to_string());
            }
            Ok(HostCommand::Exec { argv })
        }
        "RESIZE" => {
            let mut fields = rest.split_whitespace();
            let pid = parse(fields.next(), "pid")?;
            let cols = parse(fields.next(), "cols")?;
            let rows = parse(fields.next(), "rows")?;
            Ok(HostCommand::Resize { pid, cols, rows })
        }
        "CLIP" => Ok(HostCommand::Clip {
            text: unescape(rest),
        }),
        "POWEROFF" => Ok(HostCommand::Poweroff),
        other => Err(format!("unknown verb '{other}'")),
    }
}

/// Encode a guest event.
pub fn encode_event(event: &GuestEvent) -> String {
    match event {
        GuestEvent::Ready { version } => format!("READY {version}"),
        GuestEvent::Pid { pid } => format!("PID {pid}"),
        GuestEvent::Exit { pid, code } => format!("EXIT {pid} {code}"),
        GuestEvent::Error { message } => format!("ERROR {}", escape(message)),
    }
}

fn parse<T: std::str::FromStr>(field: Option<&str>, name: &str) -> Result<T, String> {
    field
        .ok_or_else(|| format!("missing {name}"))?
        .parse()
        .map_err(|_| format!("bad {name}"))
}

/// Percent-encode whitespace and `%` so arguments survive the line protocol.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '%' => out.push_str("%25"),
            ' ' => out.push_str("%20"),
            '\n' => out.push_str("%0A"),
            '\r' => out.push_str("%0D"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let hex: String = chars.clone().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    chars.next();
                    chars.next();
                    // `escape` only emits ASCII escapes (%XX), so this is safe.
                    out.push(byte as char);
                    continue;
                }
            }
        }
        out.push(ch);
    }
    out
}

/// Bootstrap policy: what PID 1 must set up before anything can run.
///
/// This is the checklist the real binary walks on boot; keeping it as data makes
/// it testable and lets the host render "where did boot stop" when a tab fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootStep {
    MountProc,
    MountSys,
    MountDev,
    MountOverlayUpper,
    PivotRoot,
    StartConsoleAgent,
    SignalReady,
}

/// The ordered boot plan.
pub const BOOT_PLAN: &[BootStep] = &[
    BootStep::MountProc,
    BootStep::MountSys,
    BootStep::MountDev,
    BootStep::MountOverlayUpper,
    BootStep::PivotRoot,
    BootStep::StartConsoleAgent,
    BootStep::SignalReady,
];

impl BootStep {
    pub fn label(self) -> &'static str {
        match self {
            BootStep::MountProc => "mount /proc",
            BootStep::MountSys => "mount /sys",
            BootStep::MountDev => "mount /dev",
            BootStep::MountOverlayUpper => "tmpfs overlay upper",
            BootStep::PivotRoot => "pivot_root to squashfs",
            BootStep::StartConsoleAgent => "start hvc0 console agent",
            BootStep::SignalReady => "signal READY",
        }
    }
}

/// Render the plan for `spacetime run --verbose` / boot diagnostics.
pub fn boot_plan_text() -> String {
    BOOT_PLAN
        .iter()
        .enumerate()
        .map(|(i, step)| format!("{:>2}. {}", i + 1, step.label()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_round_trips_with_spaces() {
        let command = HostCommand::Exec {
            argv: vec!["/bin/sh".into(), "-c".into(), "echo hello world".into()],
        };
        let line = encode(&command);
        assert_eq(line, "EXEC /bin/sh -c echo%20hello%20world");        assert_eq(decode(&line).unwrap(), command);    }

    #[test]
    fn clipboard_is_sanitized_text() {
        let command = HostCommand::Clip {
            text: "hello world 100%".into(),
        };
        assert_eq(decode(&encode(&command)).unwrap(), command);    }

    #[test]
    fn resize_parses_numbers() {
        let command = decode("RESIZE 12 120 30").unwrap();
        assert_eq(
            command,
            HostCommand::Resize {
                pid: 12,
                cols: 120,
                rows: 30
            },
        );        assert(decode("RESIZE x 1 2").is_err());        assert(decode("RESIZE").is_err());    }

    #[test]
    fn poweroff_and_events() {
        assert_eq(decode("POWEROFF").unwrap(), HostCommand::Poweroff);        assert_eq(encode_event(&GuestEvent::Ready { version: PROTOCOL_VERSION }), "READY 1");        assert_eq(encode_event(&GuestEvent::Exit { pid: 7, code: 1 }), "EXIT 7 1");    }

    #[test]
    fn bad_input_is_rejected() {
        assert(decode("").is_err());        assert(decode("FLY away").is_err());        assert(decode("EXEC").is_err());    }

    #[test]
    fn boot_plan_is_ordered_and_complete() {
        assert_eq(BOOT_PLAN.len(), 7);        assert_eq(BOOT_PLAN[0], BootStep::MountProc);        assert_eq(*BOOT_PLAN.last().unwrap(), BootStep::SignalReady);        assert(boot_plan_text().contains("signal READY"));    }
}
