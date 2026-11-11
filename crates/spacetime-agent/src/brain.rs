//! The model interface, plus a deterministic brain for demos and tests.
//!
//! [`Brain`] is intentionally tiny: an operator only ever needs `observe →
//! propose → act`. That keeps the door open for *any* intelligence behind it —
//! an API model, a local Ollama instance, or the controversial browser bridge —
//! without the rest of the system caring which one is in use.
//!
//! `SimulatedBrain` is scripted rather than clever: it walks the marks on the
//! screen, types what the task implies, and occasionally proposes a deliberately
//! risky action so the heartbeat's snapshot/rollback path is exercised in demos
//! and in CI.

use spacetime_core::json::Value;

use crate::vision::Screen;

/// What the operator can see this tick.
#[derive(Debug, Clone)]
pub struct Observation {
    pub tick: u64,
    pub tab: u64,
    pub mission: String,
    pub active_node: String,
    pub screen: Screen,
}

/// What the operator wants to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposedAction {
    Click {
        mark: u32,
        why: String,
    },
    Type {
        mark: u32,
        text: String,
        why: String,
    },
    Key {
        combo: String,
        why: String,
    },
    Scroll {
        dy: i32,
        why: String,
    },
    /// Node finished; carry a one-line result for the DAG.
    Done {
        result: String,
    },
    /// Node cannot proceed; carry the reason.
    Fail {
        reason: String,
    },
}

impl ProposedAction {
    /// Risky actions are bracketed by a memory snapshot before they run.
    pub fn is_risky(&self) -> bool {
        match self {
            ProposedAction::Click { why, .. } => {
                let lowered = why.to_lowercase();
                lowered.contains("delete")
                    || lowered.contains("format")
                    || lowered.contains("install")
                    || lowered.contains("run")
                    || lowered.contains("danger")
            }
            ProposedAction::Key { combo, .. } => {
                let lowered = combo.to_lowercase();
                lowered.contains("ctrl+alt") || lowered.contains("delete")
            }
            ProposedAction::Type { text, .. } => {
                text.contains("rm -rf") || text.contains("curl | sh")
            }
            _ => false,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ProposedAction::Click { .. } => "click",
            ProposedAction::Type { .. } => "type",
            ProposedAction::Key { .. } => "key",
            ProposedAction::Scroll { .. } => "scroll",
            ProposedAction::Done { .. } => "done",
            ProposedAction::Fail { .. } => "fail",
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            ProposedAction::Click { mark, why } => Value::obj()
                .with("action", "click")
                .with("mark", *mark as u64)
                .with("why", why.as_str()),
            ProposedAction::Type { mark, text, why } => Value::obj()
                .with("action", "type")
                .with("mark", *mark as u64)
                .with("text", text.as_str())
                .with("why", why.as_str()),
            ProposedAction::Key { combo, why } => Value::obj()
                .with("action", "key")
                .with("combo", combo.as_str())
                .with("why", why.as_str()),
            ProposedAction::Scroll { dy, why } => Value::obj()
                .with("action", "scroll")
                .with("dy", *dy as i64)
                .with("why", why.as_str()),
            ProposedAction::Done { result } => Value::obj()
                .with("action", "done")
                .with("result", result.as_str()),
            ProposedAction::Fail { reason } => Value::obj()
                .with("action", "fail")
                .with("reason", reason.as_str()),
        }
    }
}

/// The only capability the agent engine needs from a model.
/// Brains live inside the host engine, which is shared behind `Arc<Mutex<..>>`
/// across the dashboard's connection threads — so every implementation must be
/// `Send`. Real brains (HTTP clients, ONNX sessions) are all `Send`.
pub trait Brain: Send {
    /// Human label shown in the brain-pool UI.
    fn name(&self) -> &'static str;
    /// Propose the next action for this observation.
    fn act(&mut self, observation: &Observation) -> ProposedAction;
    /// Called when the action's outcome is known (for self-correction).
    fn feedback(&mut self, _action: &ProposedAction, _outcome: &str) {}
}

/// Scripted brain: no network, no keys, deterministic.
#[derive(Debug, Default)]
pub struct SimulatedBrain {
    /// Rotates through the screen's marks.
    cursor: usize,
    /// Every 7th tick proposes a risky action (snapshot/rollback demo).
    ticks: u64,
}

impl SimulatedBrain {
    pub fn new() -> SimulatedBrain {
        SimulatedBrain::default()
    }
}

impl Brain for SimulatedBrain {
    fn name(&self) -> &'static str {
        "simulated"
    }

    fn act(&mut self, observation: &Observation) -> ProposedAction {
        self.ticks += 1;
        let screen = &observation.screen;
        if screen.elements.is_empty() {
            return ProposedAction::Done {
                result: "screen has no interactive elements; nothing to do".to_string(),
            };
        }
        let index = self.cursor % screen.elements.len();
        self.cursor += 1;
        let element = &screen.elements[index];

        // Demo the auto-rollback path once every seven ticks.
        if self.ticks % 7 == 0 {
            return ProposedAction::Click {
                mark: element.mark,
                why: format!("install {}", element.label),
            };
        }

        let node = observation.active_node.to_lowercase();
        if node.contains("type") || node.contains("name") || node.contains("search") {
            return ProposedAction::Type {
                mark: element.mark,
                text: observation.mission.clone(),
                why: format!("enter the mission text into \"{}\"", element.label),
            };
        }
        if node.contains("cut") || node.contains("trim") || node.contains("silence") {
            return ProposedAction::Key {
                combo: "ctrl+k".to_string(),
                why: "open the split command".to_string(),
            };
        }
        ProposedAction::Click {
            mark: element.mark,
            why: format!(
                "advance {} via \"{}\"",
                observation.active_node, element.label
            ),
        }
    }
}

impl Brain for Box<dyn Brain> {
    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
    fn act(&mut self, observation: &Observation) -> ProposedAction {
        self.as_mut().act(observation)
    }
    fn feedback(&mut self, action: &ProposedAction, outcome: &str) {
        self.as_mut().feedback(action, outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::{ElementKind, MarkedElement};

    fn observation() -> Observation {
        Observation {
            tick: 1,
            tab: 1,
            mission: "edit promo video".to_string(),
            active_node: "import clips".to_string(),
            screen: Screen::from_elements(
                1,
                1280,
                720,
                vec![
                    MarkedElement {
                        mark: 0,
                        kind: ElementKind::Button,
                        label: "Import".to_string(),
                        rect: (0, 0, 80, 30),
                        enabled: true,
                    },
                    MarkedElement {
                        mark: 0,
                        kind: ElementKind::MenuItem,
                        label: "File".to_string(),
                        rect: (0, 40, 40, 20),
                        enabled: true,
                    },
                ],
            ),
        }
    }

    #[test]
    fn simulated_brain_cycles_marks() {
        let mut brain = SimulatedBrain::new();
        let obs = observation();
        match brain.act(&obs) {
            ProposedAction::Click { mark, .. } => assert_eq!(mark, 1),
            other => panic!("unexpected {other:?}"),
        }
        match brain.act(&obs) {
            ProposedAction::Click { mark, .. } => assert_eq!(mark, 2),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn typing_nodes_type() {
        let mut brain = SimulatedBrain::new();
        let mut obs = observation();
        obs.active_node = "type repo name".to_string();
        match brain.act(&obs) {
            ProposedAction::Type { text, .. } => assert_eq!(text, "edit promo video"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn risky_actions_are_classified() {
        assert!(ProposedAction::Click {
            mark: 3,
            why: "delete the project".into()
        }
        .is_risky(),);
        assert!(ProposedAction::Type {
            mark: 1,
            text: "rm -rf ~".into(),
            why: "x".into()
        }
        .is_risky(),);
        assert!(!ProposedAction::Click {
            mark: 3,
            why: "open the file menu".into()
        }
        .is_risky(),);
        assert!(!ProposedAction::Done {
            result: "ok".into()
        }
        .is_risky(),);
    }

    #[test]
    fn empty_screen_completes() {
        let mut brain = SimulatedBrain::new();
        let mut obs = observation();
        obs.screen = Screen::from_elements(1, 10, 10, vec![]);
        assert!(matches!(brain.act(&obs), ProposedAction::Done { .. }));
    }
}
