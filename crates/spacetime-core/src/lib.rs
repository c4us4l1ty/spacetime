//! Core domain model for Spacetime.
//!
//! Everything here is pure, deterministic and dependency-free so that it can be
//! unit-tested on every host OS and reused by any front-end (shell, CLI, web).
//!
//! * [`tab`] — the browser-like tab tree, trust levels and lifecycle.
//! * [`time_travel`] — copy-on-write memory accounting and the snapshot ring.
//! * [`cas`] + [`hash`] — content-addressed storage with BLAKE3-style dedup.
//! * [`json`] — a tiny JSON reader/writer used for the engine/front-end wire format.

pub mod cas;
pub mod hash;
pub mod json;
pub mod tab;
pub mod time_travel;

pub use cas::{CasStore, Insert};
pub use hash::{sha256, Hash};
pub use tab::{Tab, TabId, TabKind, TabState, TabTree, Trust};
pub use time_travel::{MemoryImage, Snapshot, TimeTravel};

/// Network posture of a tab. Lives in core because it is a property of a tab,
/// not of any particular backend implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetMode {
    /// No `virtio-net` device is attached. Egress is physically impossible.
    Airgapped,
    /// Every packet terminates in the in-memory switch; unknown destinations are
    /// fed to a sinkhole responder.
    HoneyDuct,
    /// Routed through residential egress with p0f/JA4 fingerprint morphing.
    Stealth,
}

impl NetMode {
    /// Human label used by CLI output and the shell.
    pub fn label(self) -> &'static str {
        match self {
            NetMode::Airgapped => "airgapped",
            NetMode::HoneyDuct => "honey-duct",
            NetMode::Stealth => "stealth",
        }
    }

    /// Maps the posture to the trust colour shown in the shell.
    pub fn trust(self) -> Trust {
        match self {
            NetMode::Airgapped => Trust::Airgapped,
            NetMode::HoneyDuct => Trust::HoneyDuct,
            NetMode::Stealth => Trust::Chameleon,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_mode_maps_to_trust() {
        assert_eq!(NetMode::Airgapped.trust(), Trust::Airgapped);
        assert_eq!(NetMode::HoneyDuct.trust(), Trust::HoneyDuct);
        assert_eq!(NetMode::Stealth.trust(), Trust::Chameleon);
    }
}
