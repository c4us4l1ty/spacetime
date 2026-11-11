//! The heartbeat loop: `Wake → Pull → Act → Verify → Persist → Wipe`.
//!
//! The loop is a state machine so that the shell can drive it frame-by-frame,
//! tests can drive it tick-by-tick, and a real engine can drive it from a tokio
//! task. Each [`Heartbeat::tick`] does exactly one unit of work, which is what
//! makes "steer on the next beat" and "auto-rollback after a bad action" cheap.

use std::collections::BTreeMap;

use spacetime_core::json::Value;

use crate::brain::{Brain, Observation, ProposedAction};
use crate::compactor::{Compactor, Scratchpad, Session};
use crate::dag::{GoalDag, NodeId};
use crate::steer::{Priority, SteerChannel};
use crate::vision::Screen;

/// What the operator is doing right now (shown in the floating pill).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Planning,
    Acting,
    Verifying,
    Flushing,
    Complete,
    Stopped,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Planning => "planning",
            Phase::Acting => "acting",
            Phase::Verifying => "verifying",
            Phase::Flushing => "flushing",
            Phase::Complete => "complete",
            Phase::Stopped => "stopped",
        }
    }
}

/// Result of one heartbeat tick.
#[derive(Debug, Clone, PartialEq)]
pub enum TickOutcome {
    /// An action was proposed and applied to the (virtual) input device.
    Acted {
        node: NodeId,
        action: ProposedAction,
        /// A snapshot was taken before this action because it was risky.
        guarded: bool,
    },
    /// The node finished and its result went into the DAG.
    Completed { node: NodeId, next: Option<NodeId> },
    /// The environment broke; state was rewound and the node retried.
    RolledBack { node: NodeId, reason: String },
    /// The session was flushed and rehydrated (context wipe).
    Flushed { session: u64, reason: String },
    /// A human directive was applied before acting.
    Steered { directives: usize },
    /// Nothing left to do.
    Finished,
    /// A human said stop.
    Stopped,
}

/// Events other subsystems care about (they persist snapshots, restore VMs…).
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// Take a CoW memory snapshot now; returns a token the caller can use.
    SnapshotRequested { tab: u64, reason: String },
    /// Restore the last snapshot of this tab.
    RollbackRequested { tab: u64, reason: String },
    /// Inject a synthetic keyboard/mouse event into the tab.
    InputInjected { tab: u64, action: ProposedAction },
    /// Session boundary reached; UI shows the "New Chat" animation.
    ContextFlushed { session: u64 },
}

/// The orchestrator.
pub struct Heartbeat<B: Brain> {
    pub dag: GoalDag,
    pub scratchpad: Scratchpad,
    pub compactor: Compactor,
    pub session: Session,
    pub steer: SteerChannel,
    brain: B,
    tab: u64,
    phase: Phase,
    tick_count: u64,
    events: Vec<AgentEvent>,
    /// Simulated failure injection for the rollback path (every Nth risky action).
    pub fail_every: u64,
    risky_actions: u64,
    /// Screens captured this session (context bloat counter).
    screenshots_this_session: u32,
    /// Total screenshots sent over the mission (for the "context rot" story).
    total_screenshots: u64,
    /// Last result per node.
    results: BTreeMap<NodeId, String>,
}

impl<B: Brain> Heartbeat<B> {
    pub fn new(mission: &str, tab: u64, brain: B) -> Heartbeat<B> {
        let compactor = Compactor::default();
        let session = Compactor::default().open();
        let mut dag = GoalDag::new(mission);
        // A default decomposition any operator would recognise.
        let _plan = dag.add_root("inspect the tab");
        let name = dag.add_root("perform the mission");
        let _verify = dag.add_root("verify the result");
        let _report = dag.add_root("report back");
        let _ = name;
        Heartbeat {
            dag,
            scratchpad: Scratchpad::new(),
            compactor,
            session,
            steer: SteerChannel::new(),
            brain,
            tab,
            phase: Phase::Idle,
            tick_count: 0,
            events: Vec::new(),
            fail_every: 0,
            risky_actions: 0,
            screenshots_this_session: 0,
            total_screenshots: 0,
            results: BTreeMap::new(),
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    pub fn results(&self) -> &BTreeMap<NodeId, String> {
        &self.results
    }

    /// Drain queued side effects.
    pub fn drain_events(&mut self) -> Vec<AgentEvent> {
        std::mem::take(&mut self.events)
    }

    /// Run one heartbeat.
    ///
    /// `screen` is the current Set-of-Mark view of the tab. In the real engine it
    /// comes from framebuffer capture + OmniParser; here the caller builds it.
    pub fn tick(&mut self, screen: Screen, at_ms: u64) -> TickOutcome {
        self.tick_count += 1;

        // 0. Stop is respected before anything else.
        let directives = self.steer.drain();
        if directives.iter().any(|d| d.priority == Priority::Stop) {
            self.phase = Phase::Stopped;
            return TickOutcome::Stopped;
        }
        for directive in &directives {
            self.dag.steer_active(&directive.text);
            self.scratchpad.note(&format!("human: {}", directive.text));
        }
        if !directives.is_empty() {
            self.phase = Phase::Planning;
            return TickOutcome::Steered {
                directives: directives.len(),
            };
        }

        // 1. Flush the context if the session is spent. State survives; the
        //    conversation does not.
        if self.session.should_flush(self.compactor.max_turns) {
            let reason = if self.session.turns >= self.compactor.max_turns {
                "turn cap"
            } else {
                "token budget"
            };
            let number = self.session.number + 1;
            self.session = self.compactor.flush(&mut self.session, at_ms, reason);
            self.phase = Phase::Flushing;
            self.events
                .push(AgentEvent::ContextFlushed { session: number });
            self.screenshots_this_session = 0;
            return TickOutcome::Flushed {
                session: number,
                reason: reason.to_string(),
            };
        }

        // 2. Pick (or keep) a task.
        let node = match self.dag.active() {
            Some(id) => id,
            None => match self.dag.pull_ready() {
                Some(id) => id,
                None => {
                    if self.dag.is_complete() {
                        self.phase = Phase::Complete;
                        return TickOutcome::Finished;
                    }
                    // Nothing ready and not complete: dependency deadlock.
                    self.phase = Phase::Idle;
                    return TickOutcome::Finished;
                }
            },
        };

        // 3. Observe (compressed vision).
        let node_title = self
            .dag
            .get(node)
            .map(|n| n.title.clone())
            .unwrap_or_default();
        self.total_screenshots += 1;
        self.screenshots_this_session += 1;
        if !self.session.hydrated {
            let prompt = self.session.hydrate(
                self.dag.mission(),
                &node_title,
                &self.dag.summary(4),
                &self.scratchpad,
                &screen.markup(),
            );
            let _ = prompt;
        }

        // 4. Ask the brain.
        self.phase = Phase::Planning;
        let observation = Observation {
            tick: self.tick_count,
            tab: self.tab,
            mission: self.dag.mission().to_string(),
            active_node: node_title.clone(),
            screen,
        };
        let action = self.brain.act(&observation);

        // 5. Terminal actions finalise the node.
        match &action {
            ProposedAction::Done { result } => {
                let result = result.clone();
                self.results.insert(node, result.clone());
                self.scratchpad
                    .set(&format!("done:{}", node_title), &result);
                let completed = self.dag.complete_active(&result);
                self.phase = Phase::Idle;
                // Pulling the next node also arms it as active, so the following
                // tick acts immediately instead of wasting a heartbeat.
                let next = self.dag.pull_ready();
                return TickOutcome::Completed {
                    node: completed.unwrap_or(node),
                    next,
                };
            }
            ProposedAction::Fail { reason } => {
                let reason = reason.clone();
                self.dag.fail_active(&reason);
                self.scratchpad
                    .note(&format!("failed {}: {reason}", node_title));
                self.phase = Phase::Idle;
                return TickOutcome::RolledBack { node, reason };
            }
            _ => {}
        }

        // 6. Risky actions are bracketed by a snapshot, and may be simulated to
        //    fail, which exercises auto-rollback.
        let risky = action.is_risky();
        let mut guarded = false;
        if risky {
            self.risky_actions += 1;
            guarded = true;
            self.events.push(AgentEvent::SnapshotRequested {
                tab: self.tab,
                reason: format!("before {}", action.label()),
            });
            if self.fail_every > 0 && self.risky_actions % self.fail_every == 0 {
                let reason = "the action crashed the guest; auto-rewound".to_string();
                self.events.push(AgentEvent::RollbackRequested {
                    tab: self.tab,
                    reason: reason.clone(),
                });
                self.dag.rollback_active(&reason);
                self.scratchpad
                    .note("auto-rollback: kernel panic after install");
                self.phase = Phase::Verifying;
                return TickOutcome::RolledBack { node, reason };
            }
        }

        // 7. Inject the input through virtio-input and charge the session.
        self.phase = Phase::Acting;
        self.session.charge(&format!("{action:?}"));
        self.events.push(AgentEvent::InputInjected {
            tab: self.tab,
            action: action.clone(),
        });
        self.dag.tick_active();
        self.phase = Phase::Verifying;
        TickOutcome::Acted {
            node,
            action,
            guarded,
        }
    }

    /// JSON for the AI drawer (DAG + session + scratchpad).
    pub fn to_json(&self, at_ms: u64) -> Value {
        Value::obj()
            .with("dag", self.dag.to_json())
            .with("phase", self.phase.label())
            .with("brain", self.brain.name())
            .with("tick", self.tick_count)
            .with("session", self.compactor.to_json(&self.session, at_ms))
            .with("scratchpad", self.scratchpad.to_json())
            .with("steer", self.steer.to_json())
            .with("screenshots_total", self.total_screenshots)
            .with("screenshots_session", self.screenshots_this_session as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::SimulatedBrain;
    use crate::vision::{ElementKind, MarkedElement};

    fn screen() -> Screen {
        Screen::from_elements(
            1,
            1280,
            720,
            (0..6)
                .map(|i| MarkedElement {
                    mark: 0,
                    kind: ElementKind::Button,
                    label: format!("Button {i}"),
                    rect: (i * 100, 0, 80, 30),
                    enabled: true,
                })
                .collect(),
        )
    }

    #[test]
    fn heartbeat_acts_guards_and_completes() {
        let mut hb = Heartbeat::new("edit a promo video", 1, SimulatedBrain::new());
        let mut saw_guard = false;
        let mut saw_complete = false;
        // The scripted brain only proposes `Done` on a screen with no marks, so
        // the last tick feeds an empty frame to exercise the completion path.
        let empty = Screen::from_elements(1, 10, 10, vec![]);
        for at in 0..12u64 {
            let view = if at == 11 { empty.clone() } else { screen() };
            match hb.tick(view, at * 500) {
                TickOutcome::Acted { guarded, .. } => saw_guard |= guarded,
                TickOutcome::Completed { .. } => saw_complete = true,
                TickOutcome::Flushed { .. } => {}
                _ => {}
            }
        }
        assert!(saw_guard, "expected at least one snapshot-guarded action");
        assert!(saw_complete, "expected the first node to complete");
        assert!(hb.dag.progress() > 0.0);
    }

    #[test]
    fn snapshots_are_requested_for_risky_actions() {
        let mut hb = Heartbeat::new("install dependencies", 7, SimulatedBrain::new());
        for at in 0..8u64 {
            let _ = hb.tick(screen(), at * 100);
        }
        let events = hb.drain_events();
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::SnapshotRequested { tab: 7, .. })),);
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::InputInjected { .. })),);
    }

    #[test]
    fn auto_rollback_on_injected_failure() {
        let mut hb = Heartbeat::new("run a sketchy installer", 3, SimulatedBrain::new());
        hb.fail_every = 1; // every risky action "fails"
        let mut rolled_back = false;
        for at in 0..10u64 {
            if let TickOutcome::RolledBack { reason, .. } = hb.tick(screen(), at * 100) {
                assert!(reason.contains("auto-rewound"));
                rolled_back = true;
                break;
            }
        }
        assert!(rolled_back);
        let events = hb.drain_events();
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::RollbackRequested { .. })),);
    }

    #[test]
    fn steering_bends_the_plan_without_stopping() {
        let mut hb = Heartbeat::new("make a video", 1, SimulatedBrain::new());
        let _ = hb.tick(screen(), 0);
        hb.steer.push("use 9:16 vertical", 100);
        match hb.tick(screen(), 200) {
            TickOutcome::Steered { directives } => assert_eq!(directives, 1),
            other => panic!("expected steer, got {other:?}"),
        }
        assert!(
            hb.dag.summary(8).contains("9:16")
                || hb.scratchpad.notes().iter().any(|n| n.contains("9:16")),
        );
        // Next tick continues acting.
        assert!(matches!(
            hb.tick(screen(), 300),
            TickOutcome::Acted { .. } | TickOutcome::Completed { .. }
        ),);
    }

    #[test]
    fn stop_directive_halts() {
        let mut hb = Heartbeat::new("x", 1, SimulatedBrain::new());
        hb.steer.push("--stop", 5);
        assert_eq!(hb.tick(screen(), 10), TickOutcome::Stopped);
        assert_eq!(hb.phase(), Phase::Stopped);
    }

    #[test]
    fn context_flushes_at_turn_cap_and_rehydrates() {
        let mut hb = Heartbeat::new("long mission", 1, SimulatedBrain::new());
        hb.compactor.max_turns = 3;
        let mut flushes = 0;
        let mut acted_after_flush = 0;
        for at in 0..20u64 {
            match hb.tick(screen(), at * 100) {
                TickOutcome::Flushed { .. } => {
                    flushes += 1;
                    assert_eq!(hb.session.tokens_used, 0);
                }
                TickOutcome::Acted { .. } if flushes > 0 => acted_after_flush += 1,
                _ => {}
            }
        }
        assert!(flushes >= 2, "expected several context resets");
        assert!(
            acted_after_flush > 0,
            "operator must keep working after a reset"
        );
        assert_eq!(hb.steer.pending(), 0);
    }

    #[test]
    fn mission_finishes() {
        let mut hb = Heartbeat::new("short mission", 1, SimulatedBrain::new());
        // The simulated brain completes a node when it has no screen elements.
        let empty = Screen::from_elements(1, 10, 10, vec![]);
        for at in 0..20u64 {
            let _ = hb.tick(empty.clone(), at * 100);
        }
        assert!(hb.dag.is_complete());
        assert_eq!(hb.phase(), Phase::Complete);
    }

    #[test]
    fn json_exposes_dag_and_session() {
        let mut hb = Heartbeat::new("mission", 1, SimulatedBrain::new());
        let _ = hb.tick(screen(), 0);
        let json = hb.to_json(1_000);
        assert_eq!(json.str_of("brain"), "simulated");
        assert!(json.get("dag").is_some());
        assert!(json.get("session").is_some());
    }
}
