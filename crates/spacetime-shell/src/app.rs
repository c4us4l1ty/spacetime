//! The zero-dependency dashboard.
//!
//! `spacetime serve` starts this: a std-only HTTP server that exposes the engine
//! as JSON and serves a single-page shell. The page is deliberately a *client* of
//! the same `/api/state` contract a native iced/egui shell would use, so it doubles
//! as a living specification for the desktop UI and as the demo you can open in
//! any browser — no toolchain, no GPU, no hypervisor required.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use spacetime_core::tab::{TabKind, Trust};

use crate::engine::{EngineConfig, LogLevel, Spacetime};
use crate::http::{self, Request, Response};

/// Server state.
pub struct Dashboard {
    pub engine: Spacetime,
    pub playing: bool,
    /// Ticks advanced per poll while playing.
    pub ticks_per_poll: u64,
    pub polls: u64,
}

impl Dashboard {
    pub fn new(config: EngineConfig) -> Dashboard {
        Dashboard {
            engine: Spacetime::demo(config),
            playing: true,
            ticks_per_poll: 1,
            polls: 0,
        }
    }

    pub fn state_json(&self) -> String {
        self.engine.to_json().to_string()
    }

    /// Handle one request. Pure besides the engine mutation, which is what makes
    /// it unit-testable without sockets.
    pub fn handle(&mut self, request: &Request) -> Response {
        let path = request.path.as_str();
        match (request.method.as_str(), path) {
            ("GET", "/") | ("GET", "/index.html") => Response::html(DASHBOARD_HTML.to_string()),
            ("GET", "/api/state") => {
                self.polls += 1;
                if self.playing {
                    self.engine.run(self.ticks_per_poll);
                }
                Response::json(&self.state_json())
            }
            ("GET", "/api/tick") => {
                let n = request.u64_param("n").unwrap_or(1).min(2_000);
                self.engine.run(n);
                Response::json(&self.state_json())
            }
            ("GET", "/api/play") => {
                self.playing = request.param("on").map(|v| v != "0").unwrap_or(true);
                if let Some(n) = request.u64_param("speed") {
                    self.ticks_per_poll = n.clamp(1, 10);
                }
                Response::json(&format!(
                    "{{\"playing\":{},\"speed\":{}}}",
                    self.playing, self.ticks_per_poll
                ))
            }
            ("GET", "/api/open") => {
                let name = request.param("name").unwrap_or("new tab").to_string();
                let kind = request
                    .param("kind")
                    .and_then(TabKind::from_label)
                    .unwrap_or(TabKind::Web);
                let trust = request
                    .param("trust")
                    .and_then(Trust::from_label)
                    .unwrap_or(match kind {
                        TabKind::File => Trust::Disposable,
                        TabKind::Terminal => Trust::Airgapped,
                        _ => Trust::Chameleon,
                    });
                let id = self.engine.open_tab(&name, kind, trust);
                self.engine.log(
                    LogLevel::Good,
                    "shell",
                    &format!("opened tab #{id:02} ({}, {})", kind.label(), trust.label()),
                );
                Response::json(&self.state_json())
            }
            ("GET", "/api/branch") => {
                let parent = request
                    .u64_param("tab")
                    .or_else(|| self.engine.tabs.focus().map(|f| f.0))
                    .unwrap_or(0);
                match self.engine.branch(parent) {
                    Some(child) => Response::json(&format!(
                        "{{\"child\":{child},\"state\":{}}}",
                        self.state_json()
                    )),
                    None => Response::text(400, "cannot branch that tab"),
                }
            }
            ("GET", "/api/focus") => {
                if let Some(tab) = request.u64_param("tab") {
                    self.engine.focus(tab);
                    Response::json(&self.state_json())
                } else {
                    Response::text(400, "missing tab")
                }
            }
            ("GET", "/api/close") => {
                if let Some(tab) = request.u64_param("tab") {
                    self.engine.close(tab);
                    Response::json(&self.state_json())
                } else {
                    Response::text(400, "missing tab")
                }
            }
            ("GET", "/api/steer") => {
                if let Some(text) = request.param("text") {
                    self.engine.steer(text);
                    Response::json(&self.state_json())
                } else {
                    Response::text(400, "missing text")
                }
            }
            ("GET", "/api/rewind") => {
                let tab = request
                    .u64_param("tab")
                    .or_else(|| self.engine.tabs.focus().map(|f| f.0))
                    .unwrap_or(0);
                let at = request.u64_param("at").unwrap_or(0);
                let (start, end) = self.engine.travel.window();
                let at = at.clamp(start, end);
                self.engine.rewind(tab, at);
                Response::json(&self.state_json())
            }
            ("GET", "/api/trap") => {
                if let Some(endpoint) = request.param("endpoint") {
                    self.engine.net.trap(endpoint);
                    Response::json(&self.state_json())
                } else {
                    Response::text(400, "missing endpoint")
                }
            }
            ("POST", "/api/steer") => {
                let text = request.body.trim();
                if text.is_empty() {
                    Response::text(400, "empty directive")
                } else {
                    self.engine.steer(text);
                    Response::json(&self.state_json())
                }
            }
            _ => Response::text(404, "not found"),
        }
    }
}

/// Run the dashboard until the process is killed.
pub fn run(port: u16, config: EngineConfig) -> std::io::Result<()> {
    let listener = http::bind(port)?;
    let shared = Arc::new(Mutex::new(Dashboard::new(config)));
    println!("spacetime dashboard → http://0.0.0.0:{port}");
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let shared = Arc::clone(&shared);
        thread::spawn(move || {
            let mut handler = |request: &Request| -> Response {
                match shared.lock() {
                    Ok(mut dashboard) => dashboard.handle(request),
                    Err(_) => Response::text(500, "engine poisoned"),
                }
            };
            let _ = http::serve_one(stream, &mut handler);
        });
    }
    Ok(())
}

/// Run `ticks` engine steps while printing the log, then exit (CLI `run`).
pub fn run_headless(ticks: u64, config: EngineConfig) -> Spacetime {
    let mut engine = Spacetime::demo(config);
    let mut printed = 0;
    for _ in 0..ticks {
        engine.tick();
        while printed < engine.log.len() {
            let line = &engine.log[printed];
            println!(
                "[{:>7.1}s] {:<6} {:<10} {}",
                line.at_ms as f64 / 1000.0,
                line.level.label(),
                line.channel,
                line.text
            );
            printed += 1;
        }
        thread::sleep(Duration::from_millis(5));
    }
    engine
}

/// The single-page shell.
pub const DASHBOARD_HTML: &str = include_str!("dashboard.html");

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn get(path: &str) -> Request {
        let (path, query) = match path.split_once('?') {
            Some((p, q)) => {
                let mut map = BTreeMap::new();
                for pair in q.split('&') {
                    if let Some((k, v)) = pair.split_once('=') {
                        map.insert(k.to_string(), v.to_string());
                    }
                }
                (p.to_string(), map)
            }
            None => (path.to_string(), BTreeMap::new()),
        };
        Request {
            method: "GET".to_string(),
            path,
            query,
            body: String::new(),
        }
    }

    #[test]
    fn serves_the_page() {
        let mut dash = Dashboard::new(EngineConfig::default());
        let response = dash.handle(&get("/"));
        assert_eq!(response.status, 200);
        assert!(String::from_utf8_lossy(&response.body).contains("SPACETIME"));
    }

    #[test]
    fn state_advances_while_playing() {
        let mut dash = Dashboard::new(EngineConfig::default());
        let before = dash.engine.to_json().u64_of("tick");
        let response = dash.handle(&get("/api/state"));
        assert_eq!(response.status, 200);
        assert!(dash.engine.to_json().u64_of("tick") > before);
        assert!(String::from_utf8_lossy(&response.body).contains("\"tabs\""));
    }

    #[test]
    fn pause_halts_progress() {
        let mut dash = Dashboard::new(EngineConfig::default());
        dash.handle(&get("/api/play?on=0"));
        let before = dash.engine.to_json().u64_of("tick");
        dash.handle(&get("/api/state"));
        assert_eq!(dash.engine.to_json().u64_of("tick"), before);
    }

    #[test]
    fn commands_mutate_the_engine() {
        let mut dash = Dashboard::new(EngineConfig::default());
        let tabs = dash.engine.tabs.live_count();
        dash.handle(&get("/api/open?name=test&kind=terminal&trust=airgapped"));
        assert_eq!(dash.engine.tabs.live_count(), tabs + 1);
        dash.handle(&get("/api/branch?tab=1"));
        assert_eq!(dash.engine.tabs.live_count(), tabs + 2);
        dash.handle(&get("/api/steer?text=use+9:16"));
        assert_eq!(dash.engine.agent.steer.pending(), 1);
    }

    #[test]
    fn unknown_route_is_404() {
        let mut dash = Dashboard::new(EngineConfig::default());
        assert_eq!(dash.handle(&get("/nope")).status, 404);
    }
}
