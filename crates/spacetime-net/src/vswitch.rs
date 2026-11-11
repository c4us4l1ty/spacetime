//! The in-memory switch: policy, attribution, telemetry and fingerprint morphing.
//!
//! A [`Flow`] is what a real `smoltcp` stack would hand the switch after parsing
//! a guest packet. The switch answers with a [`Verdict`]: allow (optionally
//! rewritten, optionally tunneled), block, or sinkhole. It also appends to the
//! Honey-Duct console feed, which is the tab's live network ledger.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use spacetime_core::{json::Value, NetMode};

use crate::geoip::{AsnKind, GeoIp};
use crate::sinkhole::{Sinkhole, SinkholeProfile};

/// Transport protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proto {
    Tcp,
    Udp,
    /// DNS over UDP 53 or DoT.
    Dns,
}

/// One connection attempt.
#[derive(Debug, Clone)]
pub struct Flow {
    pub dst: Ipv4Addr,
    pub port: u16,
    pub proto: Proto,
    /// Guest process that opened the socket, as reported by the guest agent.
    pub process: String,
    /// Requested hostname, if the guest resolved one (from the DNS flow).
    pub hostname: Option<String>,
    pub bytes: u64,
}

impl Flow {
    pub fn tcp(dst: Ipv4Addr, port: u16, process: &str) -> Flow {
        Flow {
            dst,
            port,
            proto: Proto::Tcp,
            process: process.to_string(),
            hostname: None,
            bytes: 0,
        }
    }

    pub fn with_host(mut self, host: &str) -> Flow {
        self.hostname = Some(host.to_string());
        self
    }

    pub fn key(&self) -> String {
        format!("{}:{}", self.dst, self.port)
    }
}

/// What happens to a flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Pass, possibly with a fingerprint-morphed packet.
    Allow {
        /// Rewritten TTL (Windows 128 vs Linux 64) when morphing is on.
        ttl: Option<u8>,
        /// Egress description shown in the console.
        egress: String,
    },
    /// Hard block. The guest sees a connection reset.
    Block { reason: String },
    /// Block, but answer convincingly from the sinkhole.
    Sinkhole { profile: SinkholeProfile, reply: Vec<u8> },
}

impl Verdict {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Verdict::Allow { .. })
    }
}

/// One line in the console feed.
#[derive(Debug, Clone)]
pub struct FeedEntry {
    pub at_ms: u64,
    pub tab: u64,
    pub endpoint: String,
    pub hostname: Option<String>,
    pub verdict: Verdict,
    pub asn: String,
    pub process: String,
}

impl FeedEntry {
    /// Icon + text exactly as the shell renders it.
    pub fn render(&self) -> String {
        let icon = match self.verdict {
            Verdict::Allow { .. } => "🟢 ALLOWED",
            Verdict::Block { .. } => "🛑 BLOCKED",
            Verdict::Sinkhole { .. } => "🕳️ TRAPPED",
        };
        let host = self
            .hostname
            .as_deref()
            .map(|h| format!(" ({h})"))
            .unwrap_or_default();
        let detail = match &self.verdict {
            Verdict::Allow { egress, ttl } => {
                format!("via {egress}{}", ttl.map(|t| format!(" [TTL {t}]")).unwrap_or_default())
            }
            Verdict::Block { reason } => reason.clone(),
            Verdict::Sinkhole { .. } => "fed sinkhole response".to_string(),
        };
        format!(
            "[{}] {icon} tab {} → {}{host} · {} · {} · {detail}",
            self.at_ms, self.tab, self.endpoint, self.asn, self.process
        )
    }
}

/// Aggregate counters for the status bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SwitchStats {
    pub allowed: u64,
    pub blocked: u64,
    pub sinkholed: u64,
    pub bytes: u64,
}

impl SwitchStats {
    pub fn entries(&self) -> u64 { self.allowed + self.blocked + self.sinkholed }
}

/// The switch itself.
#[derive(Debug)]
pub struct Switch {
    pub mode: NetMode,
    sinkhole: Sinkhole,
    /// Endpoints explicitly fed to the sinkhole operator console.
    trapped: BTreeMap<String, String>,
    feed: Vec<FeedEntry>,
    stats: SwitchStats,
    /// Egress label for allowed stealth traffic ("Comcast Residential", ...).
    egress: String,
    /// Rewrite TCP/IP headers so packets look like Windows 11 Chrome.
    spoof_fingerprint: bool,
    max_feed: usize,
}

impl Switch {
    pub fn new(mode: NetMode) -> Switch {
        Switch {
            mode,
            sinkhole: Sinkhole::new(SinkholeProfile::C2Standby),
            trapped: BTreeMap::new(),
            feed: Vec::new(),
            stats: SwitchStats::default(),
            egress: match mode {
                NetMode::Stealth => "Residential Proxy #1 (Comcast)".to_string(),
                NetMode::HoneyDuct => "sinkhole".to_string(),
                NetMode::Airgapped => "none".to_string(),
            },
            spoof_fingerprint: matches!(mode, NetMode::Stealth),
            max_feed: 512,
        }
    }

    pub fn set_egress(&mut self, label: &str) {
        self.egress = label.to_string();
    }

    pub fn set_fingerprint_spoofing(&mut self, on: bool) {
        self.spoof_fingerprint = on;
    }

    pub fn sinkhole(&mut self) -> &mut Sinkhole {
        &mut self.sinkhole
    }

    pub fn feed(&self) -> &[FeedEntry] { &self.feed }

    pub fn stats(&self) -> SwitchStats { self.stats }

    /// Decide a flow's fate and record it.
    pub fn judge(&mut self, tab: u64, at_ms: u64, flow: &Flow) -> Verdict {
        let record = GeoIp::lookup(flow.dst);
        let asn = GeoIp::label(flow.dst);

        let verdict = match self.mode {
            NetMode::Airgapped => Verdict::Block {
                reason: "no NIC attached (airgap)".to_string(),
            },
            NetMode::HoneyDuct => {
                // Everything that is not obviously legitimate is trapped.
                if record.kind == AsnKind::Private {
                    Verdict::Allow {
                        ttl: None,
                        egress: "loopback".to_string(),
                    }
                } else {
                    let endpoint = flow.key();
                    let reply = self.sinkhole.respond(&endpoint, b"PING");
                    self.trapped.insert(endpoint, asn.clone());
                    Verdict::Sinkhole {
                        profile: self.sinkhole.profile(),
                        reply,
                    }
                }
            }
            NetMode::Stealth => {
                if record.kind.is_flagged_by_waf() {
                    Verdict::Block {
                        reason: format!(
                            "{} egress would trip WAF detection; rotate proxy",
                            record.kind.label()
                        ),
                    }
                } else {
                    Verdict::Allow {
                        ttl: if self.spoof_fingerprint { Some(128) } else { None },
                        egress: self.egress.clone(),
                    }
                }
            }
        };

        match &verdict {
            Verdict::Allow { .. } => {
                self.stats.allowed += 1;
                self.stats.bytes += flow.bytes;
            }
            Verdict::Block { .. } => self.stats.blocked += 1,
            Verdict::Sinkhole { .. } => self.stats.sinkholed += 1,
        }

        self.feed.push(FeedEntry {
            at_ms,
            tab,
            endpoint: flow.key(),
            hostname: flow.hostname.clone(),
            verdict: verdict.clone(),
            asn,
            process: flow.process.clone(),
        });
        if self.feed.len() > self.max_feed {
            let overflow = self.feed.len() - self.max_feed;
            self.feed.drain(0..overflow);
        }
        verdict
    }

    /// Endpoints currently being deceived, with their attribution.
    pub fn trapped(&self) -> &BTreeMap<String, String> { &self.trapped }

    /// Route an already-allowed endpoint into the sinkhole from the UI toggle.
    pub fn trap(&mut self, endpoint: &str) {
        let asn = self
            .feed
            .iter()
            .rev()
            .find(|e| e.endpoint == endpoint)
            .map(|e| e.asn.clone())
            .unwrap_or_else(|| "manual".to_string());
        self.trapped.insert(endpoint.to_string(), asn);
    }

    pub fn untrap(&mut self, endpoint: &str) {
        self.trapped.remove(endpoint);
    }

    /// JSON view for the Honey-Duct drawer.
    pub fn to_json(&self) -> Value {
        let mut entries = Vec::new();
        for entry in &self.feed {
            let (kind, color) = match entry.verdict {
                Verdict::Allow { .. } => ("allow", "#22c55e"),
                Verdict::Block { .. } => ("block", "#ef4444"),
                Verdict::Sinkhole { .. } => ("sinkhole", "#f59e0b"),
            };
            entries.push(
                Value::obj()
                    .with("at_ms", entry.at_ms)
                    .with("tab", entry.tab)
                    .with("endpoint", entry.endpoint.as_str())
                    .with("hostname", entry.hostname.clone().unwrap_or_default())
                    .with("kind", kind)
                    .with("color", color)
                    .with("asn", entry.asn.as_str())
                    .with("process", entry.process.as_str())
                    .with("line", entry.render()),
            );
        }
        Value::obj()
            .with("mode", self.mode.label())
            .with("allowed", self.stats.allowed)
            .with("blocked", self.stats.blocked)
            .with("sinkholed", self.stats.sinkholed)
            .with("entries", Value::Arr(entries))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(a: u8, b: u8, c: u8, d: u8) -> Ipv4Addr {
        Ipv4Addr::new(a, b, c, d)
    }

    #[test]
    fn airgap_blocks_everything() {
        let mut switch = Switch::new(NetMode::Airgapped);
        let verdict = switch.judge(1, 0, &Flow::tcp(ip(1, 1, 1, 1), 443, "curl"));
        assert(!verdict.is_allowed());        assert_eq(switch.stats().blocked, 1);        assert_eq(switch.stats().allowed, 0);    }

    #[test]
    fn honeyduct_traps_public_and_allows_loopback() {
        let mut switch = Switch::new(NetMode::HoneyDuct);
        let public = switch.judge(2, 100, &Flow::tcp(ip(185, 220, 101, 1), 4444, "/tmp/.miner"));
        assert(matches!(public, Verdict::Sinkhole { .. }));        assert(switch.trapped().contains_key("185.220.101.1:4444"));        let lo = switch.judge(2, 101, &Flow::tcp(ip(10, 0, 0, 2), 8080, "init"));
        assert(lo.is_allowed());    }

    #[test]
    fn stealth_blocks_datacenter_and_morphs_ttl() {
        let mut switch = Switch::new(NetMode::Stealth);
        let ok = switch.judge(3, 0, &Flow::tcp(ip(73, 14, 3, 9), 443, "chrome").with_host("opencut.app"));
        match ok {
            Verdict::Allow { ttl, egress } => {
                assert_eq(ttl, Some(128));                assert(egress.contains("Residential"));            }
            other => panic!("expected allow, got {other:?}"),
        }
        let bad = switch.judge(3, 1, &Flow::tcp(ip(52, 10, 0, 1), 443, "chrome"));
        assert(!bad.is_allowed());    }

    #[test]
    fn feed_is_bounded() {
        let mut switch = Switch::new(NetMode::HoneyDuct);
        switch.max_feed = 10;
        for i in 0..50u8 {
            switch.judge(1, i as u64, &Flow::tcp(ip(185, 220, 0, i), 4444, "x"));
        }
        assert_eq(switch.feed().len(), 10);        assert_eq(switch.stats().entries(), 50);    }

    #[test]
    fn feed_renders_for_the_console() {
        let mut switch = Switch::new(NetMode::HoneyDuct);
        switch.judge(
            4,
            1_000,
            &Flow::tcp(ip(185, 220, 101, 1), 4444, "/tmp/.hidden_miner").with_host("c2.evil"),
        );
        let line = switch.feed()[0].render();
        assert(line.contains("TRAPPED"));        assert(line.contains("AS49505"));        assert(line.contains("c2.evil"));    }

    #[test]
    fn trap_and_untrap_from_ui() {
        let mut switch = Switch::new(NetMode::Stealth);
        switch.judge(1, 0, &Flow::tcp(ip(73, 14, 3, 9), 443, "chrome"));
        switch.trap("73.14.3.9:443");
        assert(switch.trapped().contains_key("73.14.3.9:443"));        switch.untrap("73.14.3.9:443");
        assert(switch.trapped().is_empty());    }

    #[test]
    fn json_contains_every_entry() {
        let mut switch = Switch::new(NetMode::HoneyDuct);
        switch.judge(1, 0, &Flow::tcp(ip(185, 220, 1, 1), 4444, "x"));
        switch.judge(1, 1, &Flow::tcp(ip(10, 0, 0, 2), 53, "systemd-resolved"));
        let json = switch.to_json();
        assert_eq(json.get("entries").unwrap().as_arr().unwrap().len(), 2);        assert_eq(json.u64_of("sinkholed"), 1);    }
}
