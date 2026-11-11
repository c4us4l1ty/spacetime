//! The autonomous operator.
//!
//! The design contract, taken straight from the project brief:
//!
//! * the AI lives in the **host**, not in the guest, and acts only through
//!   virtual hardware (`virtio-input`) and the framebuffer;
//! * progress lives **outside** the model, in a [`dag::GoalDag`] and a
//!   [`compactor::Scratchpad`], so any model session can be discarded at will;
//! * the loop never blocks on a human, but a [`steer::SteerChannel`] can redirect
//!   it between heartbeats;
//! * risky actions are bracketed by memory snapshots so a broken environment is
//!   rewound automatically instead of needing permission prompts.
//!
//! [`brain`] contains the [`brain::Brain`] trait. `SimulatedBrain` ships in-tree
//! so the whole loop is testable and demoable with no API keys or network; real
//! brains (OpenAI-compatible endpoints, local Ollama, or a browser bridge) are
//! drop-in implementations of the same trait.

pub mod brain;
pub mod compactor;
pub mod dag;
pub mod heartbeat;
pub mod steer;
pub mod vision;

pub use brain::{Brain, Observation, ProposedAction, SimulatedBrain};
pub use compactor::{Scratchpad, Session};
pub use dag::{GoalDag, Node, NodeId, NodeState};
pub use heartbeat::{Heartbeat, TickOutcome};
pub use steer::{Directive, SteerChannel};
pub use vision::{ElementKind, MarkedElement, Screen};
