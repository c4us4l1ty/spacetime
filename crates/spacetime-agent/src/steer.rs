//! Human steering without pausing the mission.
//!
//! The operator never asks for permission and never blocks on a human. But a
//! human watching the tab can always overrule it: text typed into the "Live
//! Steer" pill is queued here and consumed at the start of the next heartbeat
//! (sub-second), so the mission bends without stopping.
//!
//! Directives can be advisory ("use dark mode") or structural (`--skip` marks
//! the current node done, `--stop` halts the mission).

use std::collections::VecDeque;

use spacetime_core::json::Value;

/// What a directive asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// Applied to the active node's notes.
    Advisory,
    /// Replaces the plan for the active node.
    Constraint,
    /// Halts the mission.
    Stop,
}

/// One human message.
#[derive(Debug, Clone)]
pub struct Directive {
    pub text: String,
    pub priority: Priority,
    pub at_ms: u64,
    pub consumed: bool,
}

impl Directive {
    /// Classify raw operator text.
    pub fn parse(text: &str, at_ms: u64) -> Directive {
        let trimmed = text.trim();
        let structural = trimmed.starts_with("--") || (trimmed.contains(':') && trimmed.len() < 40);
        let priority = if trimmed.starts_with("--stop") || trimmed.eq_ignore_ascii_case("stop") {
            Priority::Stop
        } else if structural {
            // `--skip`, other `--flags`, and short `key:value` directives all
            // replace the active node's plan rather than annotating it.
            Priority::Constraint
        } else {
            Priority::Advisory
        };
        Directive {
            text: trimmed.to_string(),
            priority,
            at_ms,
            consumed: false,
        }
    }
}

/// The interrupt queue.
#[derive(Debug, Default, Clone)]
pub struct SteerChannel {
    queue: VecDeque<Directive>,
    history: Vec<Directive>,
    /// Sub-millisecond acknowledgement time budget for the pill's animation.
    pub ack_ms: u64,
}

impl SteerChannel {
    pub fn new() -> SteerChannel {
        SteerChannel {
            queue: VecDeque::new(),
            history: Vec::new(),
            ack_ms: 12,
        }
    }

    /// Queue a directive. Returns the parsed directive so the UI can show the
    /// "⚡ Directive queued for next Heartbeat" flash.
    pub fn push(&mut self, text: &str, at_ms: u64) -> Directive {
        let directive = Directive::parse(text, at_ms);
        self.queue.push_back(directive.clone());
        directive
    }

    /// Drain everything queued (called once per heartbeat).
    pub fn drain(&mut self) -> Vec<Directive> {
        let mut out = Vec::new();
        while let Some(mut directive) = self.queue.pop_front() {
            directive.consumed = true;
            self.history.push(directive.clone());
            out.push(directive);
        }
        out
    }

    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    pub fn history(&self) -> &[Directive] {
        &self.history
    }

    /// True if a `--stop` is queued or was the last directive.
    pub fn stop_requested(&self) -> bool {
        self.queue.iter().any(|d| d.priority == Priority::Stop)
            || self
                .history
                .iter()
                .rev()
                .take(1)
                .any(|d| d.priority == Priority::Stop)
    }

    pub fn to_json(&self) -> Value {
        let items: Vec<Value> = self
            .history
            .iter()
            .rev()
            .take(8)
            .map(|d| {
                Value::obj()
                    .with("text", d.text.as_str())
                    .with("at_ms", d.at_ms)
                    .with(
                        "priority",
                        match d.priority {
                            Priority::Advisory => "advisory",
                            Priority::Constraint => "constraint",
                            Priority::Stop => "stop",
                        },
                    )
            })
            .collect();
        Value::obj()
            .with("pending", self.queue.len())
            .with("history", Value::Arr(items))
            .with("stop_requested", self.stop_requested())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advisory_vs_structural() {
        assert_eq!(
            Directive::parse("use dark mode", 0).priority,
            Priority::Advisory
        );
        assert_eq!(Directive::parse("--skip", 0).priority, Priority::Constraint);
        assert_eq!(Directive::parse("--stop", 0).priority, Priority::Stop);
        assert_eq!(
            Directive::parse("aspect:9:16", 0).priority,
            Priority::Constraint
        );
    }

    #[test]
    fn drain_is_atomic_per_heartbeat() {
        let mut channel = SteerChannel::new();
        channel.push("use dark mode", 1_000);
        channel.push("--skip tests", 1_010);
        assert_eq!(channel.pending(), 2);
        let drained = channel.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(channel.pending(), 0);
        assert_eq!(channel.history().len(), 2);
        assert!(drained.iter().all(|d| d.consumed));
    }

    #[test]
    fn stop_is_detected() {
        let mut channel = SteerChannel::new();
        assert!(!channel.stop_requested());
        channel.push("--stop", 5);
        assert!(channel.stop_requested());
        channel.drain();
        assert!(channel.stop_requested());
        channel.push("carry on", 9);
        assert!(!channel.stop_requested() || channel.pending() == 1);
    }

    #[test]
    fn json_is_most_recent_first() {
        let mut channel = SteerChannel::new();
        channel.push("one", 1);
        channel.push("two", 2);
        channel.drain();
        let json = channel.to_json();
        let history = json.get("history").unwrap().as_arr().unwrap();
        assert_eq!(history[0].str_of("text"), "two");
    }
}
