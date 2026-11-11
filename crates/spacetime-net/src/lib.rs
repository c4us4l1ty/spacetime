//! The user-space network switch.
//!
//! Every tab's `virtio-net` terminates here, in the host process, never in a
//! host kernel firewall rule. That gives Spacetime three properties at once:
//!
//! 1. **Airgap** — a tab with [`NetMode::Airgapped`] was never given a NIC, so
//!    this switch is never reached.
//! 2. **Attribution** — each attempt is matched against an ASN/GeoIP table and
//!    the guest process that made it, producing the Honey-Duct console feed.
//! 3. **Deception** — blocked C2 traffic can be answered by a sinkhole so the
//!    malware believes it is talking to its operator (see [`sinkhole`]).
//!
//! The module is deliberately transport-agnostic: [`Switch::judge`] takes a
//! [`Flow`] and returns a [`Verdict`], which is everything a real `smoltcp`
//! integration needs to do its job.

pub mod geoip;
pub mod sinkhole;
pub mod vswitch;

pub use geoip::{AsnKind, AsnRecord, GeoIp};
pub use vswitch::{Flow, Proto, Switch, SwitchStats, Verdict};
