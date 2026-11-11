//! The Spacetime workspace engine.
//!
//! This is the process a future desktop shell (iced/egui/wgpu) talks to, and it
//! is also the process the CLI drives. It owns every subsystem and advances them
//! on a fixed tick:
//!
//! ```text
//! advance(dt):
//!   zygote ─┐
//!   vmm.tick ├─▶ page deltas ─▶ time travel ring
//!   warden  ─┘                  │
//!   switch.judge(flows)  ───────┤
//!   heartbeat.tick(screen) ─────┴─▶ ghost cursor + DAG + events
//! ```
//!
//! Everything is deterministic given the tick sequence, which is what makes the
//! demo reproducible and the tests meaningful.

pub mod app;
pub mod engine;
pub mod http;

pub use engine::{Command, EngineConfig, GhostCursor, LogLevel, LogLine, Spacetime};
