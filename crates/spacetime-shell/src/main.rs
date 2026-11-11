//! `spacetime` — the workspace engine's command line.
//!
//! ```text
//! spacetime serve [--port 7420]      dashboard + JSON API (the demo)
//! spacetime run [--ticks 120]        headless tick loop, prints the event log
//! spacetime state [--pretty]         one JSON snapshot of a simulated workspace
//! spacetime tabs                     the tab tree as a table
//! spacetime branch --tab 1          branch a tab and show the result
//! spacetime store <file>...          insert files into the CAS, show dedup savings
//! spacetime versions                 engine + schema versions
//! ```

use std::process::ExitCode;

use spacetime_core::cas::{CasStore, MemBackend};
use spacetime_core::json::Value;
use spacetime_core::tab::{TabKind, TabTree, Trust};
use spacetime_shell::app;
use spacetime_shell::engine::{EngineConfig, Spacetime};

const DEFAULT_PORT: u16 = 7420;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(|s| s.as_str()).unwrap_or("help");
    let rest = &args[args.len().min(1)..];

    match command {
        "serve" => {
            let port = flag(rest, "--port")
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_PORT);
            match app::run(port, EngineConfig::default()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("spacetime: cannot bind port {port}: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        "run" => {
            let ticks = flag(rest, "--ticks")
                .and_then(|v| v.parse().ok())
                .unwrap_or(120);
            let engine = app::run_headless(ticks, EngineConfig::default());
            println!(
                "\n{} ticks · {} tabs · {} snapshots · {} net events · {:.0}% mission",
                engine.to_json().u64_of("tick"),
                engine.tabs.live_count(),
                engine.travel.len(),
                engine.net.stats().entries(),
                engine.agent.dag.progress() * 100.0
            );
            ExitCode::SUCCESS
        }
        "state" => {
            let mut engine = Spacetime::demo(EngineConfig::default());
            engine.run(12);
            let json = engine.to_json();
            println!("{}", if has(rest, "--pretty") { json.pretty() } else { json.to_string() });
            ExitCode::SUCCESS
        }
        "tabs" => {
            let mut engine = Spacetime::demo(EngineConfig::default());
            engine.run(4);
            print_tabs(&engine.tabs);
            ExitCode::SUCCESS
        }
        "branch" => {
            let mut engine = Spacetime::demo(EngineConfig::default());
            let parent = flag(rest, "--tab")
                .and_then(|v| v.parse().ok())
                .or_else(|| engine.tabs.focus().map(|f| f.0))
                .unwrap_or(1);
            match engine.branch(parent) {
                Some(child) => {
                    println!("branched tab #{parent:02} → #{child:02}");
                    print_tabs(&engine.tabs);
                    ExitCode::SUCCESS
                }
                None => {
                    eprintln!("spacetime: cannot branch tab #{parent:02}");
                    ExitCode::FAILURE
                }
            }
        }
        "store" => {
            let files: Vec<&String> = rest.iter().filter(|a| !a.starts_with("--")).collect();
            if files.is_empty() {
                eprintln!("usage: spacetime store <file>... [--duplicate]");
                return ExitCode::FAILURE;
            }
            let mut store = CasStore::new(MemBackend::default());
            for file in files {
                match std::fs::read(file) {
                    Ok(data) => {
                        let insert = match store.insert(&data) {
                            Ok(v) => v,
                            Err(err) => {
                                eprintln!("spacetime: {file}: {err}");
                                continue;
                            }
                        };
                        println!(
                            "{}  {}  {}",
                            insert.hash.short(16),
                            if insert.deduplicated { "dedup " } else { "stored" },
                            file
                        );
                    }
                    Err(err) => eprintln!("spacetime: {file}: {err}"),
                }
            }
            println!(
                "\nobjects {} · stored {} B · saved {} B by dedup",
                store.object_count(),
                store.stored_bytes(),
                store.saved_bytes()
            );
            ExitCode::SUCCESS
        }
        "versions" => {
            let json = Value::obj()
                .with("spacetime", env!("CARGO_PKG_VERSION"))
                .with("schema", 1u64)
                .with("engine", "simulated-vmm")
                .with("state_format", "json v1");
            println!("{}", json.pretty());
            ExitCode::SUCCESS
        }
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("spacetime: unknown command '{other}'\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "\
spacetime — hardware-isolated workspace shell

USAGE:
  spacetime serve   [--port 7420]   start the dashboard + JSON API
  spacetime run     [--ticks 120]   headless heartbeat loop, prints the log
  spacetime state   [--pretty]      print one workspace snapshot as JSON
  spacetime tabs                    print the tab tree
  spacetime branch  [--tab N]       branch a tab and print the resulting tree
  spacetime store   <file>...       content-address a set of files (dedup demo)
  spacetime versions                engine and schema versions";

fn has(args: &[String], needle: &str) -> bool {
    args.iter().any(|a| a == needle)
}

fn flag(args: &[String], name: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == name {
            return iter.next().cloned();
        }
        if let Some(value) = arg.strip_prefix(&format!("{name}=")) {
            return Some(value.to_string());
        }
    }
    None
}

fn print_tabs(tree: &TabTree) {
    for (depth, id) in tree.walk() {
        if let Some(tab) = tree.get(id) {
            let branch = if depth == 0 { "" } else { "└─ " };
            let indent = "   ".repeat(depth.saturating_sub(1));
            let focus = if tab.focused { "▸" } else { " " };
            println!(
                "{focus} {indent}{branch}#{:02} {:<22} {:<9} {:<10} boot {:>2}ms  cpu {:>3}‰  ram {}MB+{}sh",
                id.0,
                tab.name,
                tab.kind.label(),
                tab.trust.label(),
                tab.boot_ms,
                tab.resources.cpu_permille,
                tab.private_ram_mb,
                tab.shared_ram_mb
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_parsing() {
        let args: Vec<String> = ["--port", "9000", "--ticks=12"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(flag(&args, "--port").as_deref(), Some("9000"));
        assert_eq!(flag(&args, "--ticks").as_deref(), Some("12"));
        assert_eq!(flag(&args, "--nope"), None);
        assert!(has(&args, "--port"));
    }

    #[test]
    fn tab_kind_round_trips() {
        for kind in [TabKind::Web, TabKind::Repo, TabKind::File, TabKind::Terminal, TabKind::Ai] {
            assert_eq!(TabKind::from_label(kind.label()), Some(kind));
        }
        for trust in [Trust::Airgapped, Trust::Chameleon, Trust::HoneyDuct, Trust::Disposable] {
            assert_eq!(Trust::from_label(trust.label()), Some(trust));
        }
    }
}
