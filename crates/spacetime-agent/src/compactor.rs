//! Context compaction: keeping a model sharp for 12-hour missions.
//!
//! Models rot when their context fills with stale screenshots. Spacetime never
//! fights that; it exploits it:
//!
//! * durable state lives in [`Scratchpad`] (a markdown-ish notebook) and the
//!   Goal DAG — both on the host, both outside any model;
//! * each [`Session`] carries a hard token budget;
//! * when the budget is spent, or a sub-goal completes, the session is **flushed**:
//!   zero tokens of history survive, and the next session is hydrated from the
//!   master goal + active node + scratchpad + one fresh screenshot.
//!
//! [`Session::hydrate`] produces exactly the string a web UI would receive as
//! message #1 or an API would receive as the system prompt.

use std::collections::BTreeMap;

use spacetime_core::json::Value;

/// Rough token estimate. Real counts come from the model's tokenizer; 4 chars
/// per token is the standard planning heuristic and is all the compactor needs.
pub fn estimate_tokens(text: &str) -> u32 {
    (text.chars().count() as u32 / 4).max(1)
}

/// The durable notebook.
#[derive(Debug, Clone, Default)]
pub struct Scratchpad {
    facts: BTreeMap<String, String>,
    notes: Vec<String>,
}

impl Scratchpad {
    pub fn new() -> Scratchpad {
        Scratchpad::default()
    }

    /// Set or update a durable fact (e.g. `repo_url`).
    pub fn set(&mut self, key: &str, value: &str) {
        self.facts.insert(key.to_string(), value.to_string());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.facts.get(key).map(|s| s.as_str())
    }

    pub fn remove(&mut self, key: &str) {
        self.facts.remove(key);
    }

    /// Append a chronological note ("learned: OpenCut exports via File menu").
    pub fn note(&mut self, text: &str) {
        self.notes.push(text.to_string());
        if self.notes.len() > 200 {
            self.notes.remove(0);
        }
    }

    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    pub fn facts(&self) -> &BTreeMap<String, String> {
        &self.facts
    }

    /// Render for hydration, facts first then the last `keep` notes.
    pub fn render(&self, keep: usize) -> String {
        let mut out = String::new();
        if !self.facts.is_empty() {
            out.push_str("Facts:\n");
            for (key, value) in &self.facts {
                out.push_str(&format!("- {key}: {value}\n"));
            }
        }
        if !self.notes.is_empty() {
            out.push_str("Recent notes:\n");
            for note in self.notes.iter().rev().take(keep).rev() {
                out.push_str(&format!("- {note}\n"));
            }
        }
        out
    }

    pub fn to_json(&self) -> Value {
        let notes: Vec<Value> = self.notes.iter().map(|n| Value::from(n.as_str())).collect();
        let facts: BTreeMap<String, Value> = self
            .facts
            .iter()
            .map(|(k, v)| (k.clone(), Value::from(v.as_str())))
            .collect();
        Value::obj()
            .with("facts", Value::Obj(facts))
            .with("notes", Value::Arr(notes))
    }
}

/// One disposable conversation with a model.
#[derive(Debug, Clone)]
pub struct Session {
    /// Monotonic session number; the UI shows "reset 3 min ago".
    pub number: u64,
    pub token_budget: u32,
    pub tokens_used: u32,
    pub turns: u32,
    pub hydrated: bool,
    /// Why the previous session ended.
    pub ended_because: String,
}

impl Session {
    pub fn new(number: u64, token_budget: u32) -> Session {
        Session {
            number,
            token_budget,
            tokens_used: 0,
            turns: 0,
            hydrated: false,
            ended_because: "mission start".to_string(),
        }
    }

    /// Charge a turn against the budget; returns `false` if this turn would
    /// exceed it (the caller should flush first).
    pub fn charge(&mut self, text: &str) -> bool {
        let cost = estimate_tokens(text);
        if self.tokens_used + cost > self.token_budget {
            return false;
        }
        self.tokens_used += cost;
        self.turns += 1;
        true
    }

    pub fn usage(&self) -> f64 {
        if self.token_budget == 0 {
            return 1.0;
        }
        self.tokens_used as f64 / self.token_budget as f64
    }

    pub fn should_flush(&self, max_turns: u32) -> bool {
        self.usage() >= 0.85 || self.turns >= max_turns
    }

    /// The hydration message: everything the model needs and nothing it doesn't.
    ///
    /// `screenshot` is a one-line description of the current frame (the real
    /// implementation passes a Set-of-Mark annotated image instead).
    pub fn hydrate(
        &mut self,
        master_goal: &str,
        active_node: &str,
        dag_summary: &str,
        scratchpad: &Scratchpad,
        screenshot: &str,
    ) -> String {
        self.hydrated = true;
        self.tokens_used = 0;
        self.turns = 0;
        let message = format!(
            "You are Spacetime's operator. You act only through virtual mouse/keyboard.\n\
             MASTER GOAL: {master_goal}\n\
             CURRENT TASK: {active_node}\n\
             PROGRESS: {dag_summary}\n\
             \n{pad}\nSCREEN: {screenshot}\n\
             Reply with a single JSON action: {{\"action\":\"click|type|key|done|fail\",\"target\":\"...\",\"text\":\"...\",\"why\":\"...\"}}",
            pad = scratchpad.render(6)
        );
        self.tokens_used += estimate_tokens(&message);
        self.turns = 1;
        message
    }
}

/// Counters the status pill shows.
#[derive(Debug, Clone, Default)]
pub struct CompactorStats {
    pub flushes: u64,
    pub total_turns: u64,
    pub last_flush_ms: u64,
}

/// The policy object: decides when a session must die.
#[derive(Debug, Clone)]
pub struct Compactor {
    pub max_turns: u32,
    pub token_budget: u32,
    stats: CompactorStats,
    next_session: u64,
}

impl Default for Compactor {
    fn default() -> Self {
        Compactor {
            max_turns: 10,
            token_budget: 8_000,
            stats: CompactorStats::default(),
            next_session: 1,
        }
    }
}

impl Compactor {
    pub fn new(max_turns: u32, token_budget: u32) -> Compactor {
        Compactor {
            max_turns,
            token_budget,
            stats: CompactorStats::default(),
            next_session: 1,
        }
    }

    /// Start a fresh session.
    pub fn open(&mut self) -> Session {
        let session = Session::new(self.next_session, self.token_budget);
        self.next_session += 1;
        session
    }

    /// Flush the current session; returns the replacement (unhydrated).
    pub fn flush(&mut self, current: &mut Session, at_ms: u64, reason: &str) -> Session {
        current.ended_because = reason.to_string();
        self.stats.flushes += 1;
        self.stats.total_turns += current.turns as u64;
        self.stats.last_flush_ms = at_ms;
        self.open()
    }

    pub fn stats(&self) -> &CompactorStats {
        &self.stats
    }

    pub fn to_json(&self, session: &Session, elapsed_ms: u64) -> Value {
        Value::obj()
            .with("session", session.number)
            .with("tokens_used", session.tokens_used as u64)
            .with("token_budget", session.token_budget as u64)
            .with("usage", session.usage())
            .with("turns", session.turns as u64)
            .with("max_turns", self.max_turns as u64)
            .with("flushes", self.stats.flushes)
            .with("last_flush_ms", self.stats.last_flush_ms)
            .with(
                "since_flush_ms",
                elapsed_ms.saturating_sub(self.stats.last_flush_ms),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_estimate_is_rough_but_sane() {
        assert_eq!(estimate_tokens(""), 1);
        assert_eq!(estimate_tokens("12345678"), 2);
        assert!(estimate_tokens(&"a".repeat(4_000)) >= 900);
    }

    #[test]
    fn session_charges_and_flushes() {
        let mut compactor = Compactor::new(10, 100);
        let mut session = compactor.open();
        assert!(session.charge(&"x".repeat(200))); // ~50 tokens
        assert!(!session.charge(&"x".repeat(400))); // would blow the budget
        let fresh = compactor.flush(&mut session, 5_000, "budget");
        assert_eq!(session.ended_because, "budget");
        assert_eq!(fresh.number, 2);
        assert_eq!(compactor.stats().flushes, 1);
        assert_eq!(fresh.tokens_used, 0);
    }

    #[test]
    fn turn_cap_triggers_flush() {
        let mut session = Session::new(1, 100_000);
        for _ in 0..10 {
            session.charge("tiny");
        }
        assert!(session.should_flush(10));
        assert_eq!(session.turns, 10);
    }

    #[test]
    fn hydration_contains_external_state_and_nothing_else() {
        let mut scratch = Scratchpad::new();
        scratch.set("repo", "https://github.com/me/crypto-tracker");
        scratch.note("learned: export lives under File > Export");
        scratch.note("stale: 40 screenshots of the home screen");

        let mut session = Session::new(4, 8_000);
        let prompt = session.hydrate(
            "build a crypto tracker",
            "export mp4",
            "[x] build · [>] export · [ ] announce",
            &scratch,
            "[1] File  [2] Export  [3] Cancel",
        );

        assert!(prompt.contains("build a crypto tracker"));
        assert!(prompt.contains("export mp4"));
        assert!(prompt.contains("github.com/me/crypto-tracker"));
        assert!(prompt.contains("[2] Export"));
        // Only the most recent notes are carried, and no screenshot history.
        assert!(!prompt.contains("home screen"));
        assert!(session.hydrated);
        assert!(session.tokens_used > 0);
    }

    #[test]
    fn scratchpad_renders_bounded_notes() {
        let mut scratch = Scratchpad::new();
        for i in 0..50 {
            scratch.note(&format!("note {i}"));
        }
        let text = scratch.render(3);
        assert!(text.contains("note 49"));
        assert!(!text.contains("note 40"));
    }
}
