//! Minimal native Rust front end for the experimental offline VM.
//! This is a line-oriented console, not a complete terminal emulator or graphical-app surface.

use eframe::egui;
use spacetime_ctl::workspace::{default_busybox, default_kernel, prepare_guest, PreparedGuest};
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Duration;

struct Session {
    child: Child,
    output: Receiver<Vec<u8>>,
    // The image remains available until the VM exits or is stopped.
    _image: PreparedGuest,
    exited: Option<std::process::ExitStatus>,
}

fn forward_output(mut reader: impl Read + Send + 'static, sender: SyncSender<Vec<u8>>) {
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if sender.send(buffer[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

impl Session {
    fn start(kernel: &str, busybox: &str, app: &str) -> Result<Self, String> {
        if kernel.is_empty() || busybox.is_empty() {
            return Err("Choose a Linux kernel and static BusyBox first.".into());
        }
        let image = prepare_guest(busybox, (!app.is_empty()).then_some(app))
            .map_err(|error| error.to_string())?;
        let image_path = image.path();
        let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--worker")
            .args([
                "space",
                "boot",
                "--experimental",
                "--kernel",
                kernel,
                "--initramfs",
            ])
            .arg(image_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Could not start workspace: {error}"))?;
        let (sender, output) = mpsc::sync_channel(64);
        // Both streams are piped by construction. Reader threads never touch the UI or execute
        // terminal escape sequences; bytes are parsed into text by vt100 in the UI thread.
        if let Some(stdout) = child.stdout.take() {
            forward_output(stdout, sender.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            forward_output(stderr, sender);
        }
        Ok(Self {
            child,
            output,
            _image: image,
            exited: None,
        })
    }

    fn send(&mut self, text: &str) -> Result<(), String> {
        let stdin = self.child.stdin.as_mut().ok_or("Guest input is closed")?;
        stdin
            .write_all(text.as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .and_then(|()| stdin.flush())
            .map_err(|error| error.to_string())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // The worker is the bubblewrap parent; --die-with-parent stops its VM. The image guard
        // is dropped after the child is reaped. Forced host termination can still leave tmpfs data.
        if matches!(self.child.try_wait(), Ok(None) | Err(_)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

struct WorkspaceApp {
    kernel: String,
    busybox: String,
    app: String,
    input: String,
    status: String,
    terminal: vt100::Parser,
    session: Option<Session>,
}

impl Default for WorkspaceApp {
    fn default() -> Self {
        Self {
            kernel: default_kernel().unwrap_or_default(),
            busybox: default_busybox().unwrap_or_default(),
            app: String::new(),
            input: String::new(),
            status: "Not running".into(),
            terminal: vt100::Parser::new(30, 100, 0),
            session: None,
        }
    }
}

impl eframe::App for WorkspaceApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        let mut finished = false;
        if let Some(session) = &mut self.session {
            let mut disconnected = false;
            for _ in 0..64 {
                match session.output.try_recv() {
                    Ok(bytes) => self.terminal.process(&bytes),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
            if session.exited.is_none() {
                match session.child.try_wait() {
                    Ok(status) => session.exited = status,
                    Err(error) => {
                        self.status = format!("Could not check workspace: {error}");
                        finished = true;
                    }
                }
            }
            if let Some(status) = session.exited {
                if disconnected {
                    self.status = format!("Workspace exited: {status}");
                    finished = true;
                }
            }
            if !finished {
                context.request_repaint_after(Duration::from_millis(100));
            }
        }
        if finished {
            self.session = None;
        }

        egui::CentralPanel::default().show(context, |ui| {
            ui.heading("Spacetime");
            ui.label("Experimental offline workspace · trusted software only");
            ui.colored_label(
                egui::Color32::YELLOW,
                "Not a hardened malware sandbox. Desktop apps and AI are not supported yet.",
            );
            ui.separator();
            ui.add_enabled_ui(self.session.is_none(), |ui| {
                egui::Grid::new("inputs").num_columns(2).show(ui, |ui| {
                    ui.label("Linux kernel");
                    ui.text_edit_singleline(&mut self.kernel);
                    ui.end_row();
                    ui.label("Static BusyBox");
                    ui.text_edit_singleline(&mut self.busybox);
                    ui.end_row();
                    ui.label("Static terminal app (optional)");
                    ui.text_edit_singleline(&mut self.app);
                    ui.end_row();
                });
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.session.is_none(), egui::Button::new("Start workspace"))
                    .clicked()
                {
                    self.terminal = vt100::Parser::new(30, 100, 0);
                    match Session::start(self.kernel.trim(), self.busybox.trim(), self.app.trim()) {
                        Ok(session) => {
                            self.status = "Starting offline workspace…".into();
                            self.session = Some(session);
                        }
                        Err(error) => self.status = error,
                    }
                }
                if ui
                    .add_enabled(self.session.is_some(), egui::Button::new("Stop workspace"))
                    .clicked()
                {
                    self.session = None;
                    self.status = "Workspace stopped".into();
                }
                ui.label(&self.status);
            });
            ui.separator();
            ui.label("Console (line input only; interactive full-screen apps are not supported)");
            egui::ScrollArea::both()
                .stick_to_bottom(true)
                .max_height(500.0)
                .show(ui, |ui| {
                    ui.monospace(self.terminal.screen().contents());
                });
            ui.horizontal(|ui| {
                let response = ui.add_enabled(
                    self.session.is_some(),
                    egui::TextEdit::singleline(&mut self.input).hint_text("Enter a command"),
                );
                let send = ui
                    .add_enabled(self.session.is_some(), egui::Button::new("Send"))
                    .clicked()
                    || (response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter)));
                if send {
                    if let Some(session) = &mut self.session {
                        if let Err(error) = session.send(&self.input) {
                            self.status = format!("Input failed: {error}");
                        }
                        self.input.clear();
                        response.request_focus();
                    }
                }
            });
            ui.small("Use 'app' to run the supplied program, or 'exit' to shut down the guest.");
        });
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--worker") {
        if let Err(error) = spacetime_ctl::run(&args[1..]) {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    eframe::run_native(
        "Spacetime",
        eframe::NativeOptions::default(),
        Box::new(|_creation_context| Box::<WorkspaceApp>::default()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_guest_inputs_before_starting() {
        assert!(Session::start("", "busybox", "").is_err());
        assert!(Session::start("kernel", "", "").is_err());
    }

    #[test]
    fn guest_terminal_escape_sequences_are_not_passed_to_the_host_terminal() {
        let mut parser = vt100::Parser::new(4, 40, 0);
        parser.process(b"before\x1b]52;c;YWJj\x07after");
        let screen = parser.screen().contents();
        assert!(screen.contains("beforeafter"));
        assert!(!screen.contains('\x1b'));
    }
}
