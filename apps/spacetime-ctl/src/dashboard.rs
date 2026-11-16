//! Small, read-only Rust terminal interface for inspecting a verified ledger.

use crate::console::default_ledger_path;
use spacetime_ledger::Ledger;
use spacetime_types::{ActionOp, Event, Result, SpacetimeError};
use std::fmt::Write;
use std::path::Path;

fn action_name(action: &ActionOp) -> &'static str {
    match action {
        ActionOp::ProduceEvidence { .. } => "produce evidence",
        ActionOp::Echo { .. } => "echo",
        ActionOp::Sleep { .. } => "sleep",
        ActionOp::ClaimDone { .. } => "claim done",
        ActionOp::Flaky { .. } => "flaky fixture",
    }
}

fn render(events: &[Event]) -> String {
    let mut output = String::from(
        "SPACETIME — offline prototype\n\
         =============================\n\
         Read-only activity from a verified local ledger.\n\
         This is a scripted demo, not a running AI or isolated app workspace.\n\
         Security isolation and 4 GB performance have not been established.\n\n",
    );
    let _ = writeln!(output, "Verified events: {}", events.len());
    output.push_str("\nRECENT ACTIVITY (newest first)\n");
    if events.is_empty() {
        output.push_str("No events.\n");
    }
    for event in events.iter().rev().take(10) {
        let _ = writeln!(
            output,
            "  #{}  {}  [{}]",
            event.seq.0,
            action_name(&event.action.op),
            event.verifier_proof.verdict.as_str()
        );
    }
    output
}

pub fn cmd_dashboard(args: &[String]) -> Result<()> {
    if args.len() > 1 || args.first().is_some_and(|arg| arg.starts_with('-')) {
        return Err(SpacetimeError::new("usage: dashboard [ledger-file]"));
    }
    let default_path = default_ledger_path();
    let path = args.first().map_or(default_path.as_path(), Path::new);
    // Do not render unverified events, including on verification failure.
    let events = Ledger::verify(path)?;
    print!("{}", render(&events));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_ledger_is_verified_before_rendering() {
        let events = Ledger::verify(&default_ledger_path()).unwrap();
        let text = render(&events);
        assert!(text.contains("Verified events: 4"));
        assert!(text.contains("scripted demo"));
        assert!(text.contains("#2"));
    }

    #[test]
    fn empty_history_is_clear() {
        assert!(render(&[]).contains("No events."));
    }

    #[test]
    fn invalid_arguments_and_missing_ledgers_are_rejected() {
        assert!(cmd_dashboard(&["--unknown".to_string()]).is_err());
        assert!(cmd_dashboard(&["a".to_string(), "b".to_string()]).is_err());
        assert!(cmd_dashboard(&["this-ledger-does-not-exist.jsonl".to_string()]).is_err());
    }
}
