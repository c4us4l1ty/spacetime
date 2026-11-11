//! Offline ASN / GeoIP attribution.
//!
//! Production builds ship a compressed MaxMind-style table refreshed daily. The
//! workspace embeds a small, hand-verified slice of the IPv4 space covering the
//! ranges used in Spacetime's demos and tests, so results are deterministic and
//! the crate builds offline.

use std::net::Ipv4Addr;

/// How an address behaves from a proxy-detection point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsnKind {
    /// Comcast, Jio, China Telecom, … — trusted by Cloudflare/Akamai.
    Residential,
    /// AWS, Hetzner, OVH, … — flagged by every bot detector.
    Hosting,
    /// Carrier-grade NAT mobile network.
    Mobile,
    /// Bullet-proof / known-bad hosting used by malware C2.
    Bulletproof,
    /// Private, loopback or CGNAT space.
    Private,
    Unknown,
}

impl AsnKind {
    pub fn label(self) -> &'static str {
        match self {
            AsnKind::Residential => "residential",
            AsnKind::Hosting => "hosting",
            AsnKind::Mobile => "mobile",
            AsnKind::Bulletproof => "bulletproof",
            AsnKind::Private => "private",
            AsnKind::Unknown => "unknown",
        }
    }

    /// Whether an egress through this ASN is likely to be blocked by a WAF.
    pub fn is_flagged_by_waf(self) -> bool {
        matches!(self, AsnKind::Hosting | AsnKind::Bulletproof)
    }
}

/// One attribution record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsnRecord {
    pub asn: u32,
    /// Inclusive network start.
    pub start: Ipv4Addr,
    /// Exclusive network end.
    pub end: Ipv4Addr,
    pub org: &'static str,
    pub country: &'static str,
    pub kind: AsnKind,
}

impl AsnRecord {
    pub fn contains(&self, ip: Ipv4Addr) -> bool {
        let value = u32::from(ip);
        value >= u32::from(self.start) && value < u32::from(self.end)
    }
}

const fn quad(a: u8, b: u8, c: u8, d: u8) -> Ipv4Addr {
    Ipv4Addr::new(a, b, c, d)
}

/// Deterministic sample of the attribution table.
pub const TABLE: &[AsnRecord] = &[
    AsnRecord {
        asn: 7922,
        start: quad(73, 0, 0, 0),
        end: quad(73, 16, 0, 0),
        org: "Comcast Cable",
        country: "US",
        kind: AsnKind::Residential,
    },
    AsnRecord {
        asn: 55836,
        start: quad(49, 32, 0, 0),
        end: quad(49, 48, 0, 0),
        org: "Reliance Jio",
        country: "IN",
        kind: AsnKind::Mobile,
    },
    AsnRecord {
        asn: 16509,
        start: quad(52, 0, 0, 0),
        end: quad(54, 0, 0, 0),
        org: "Amazon AWS",
        country: "US",
        kind: AsnKind::Hosting,
    },
    AsnRecord {
        asn: 24940,
        start: quad(88, 198, 0, 0),
        end: quad(88, 199, 0, 0),
        org: "Hetzner Online",
        country: "DE",
        kind: AsnKind::Hosting,
    },
    AsnRecord {
        asn: 20473,
        start: quad(45, 32, 0, 0),
        end: quad(45, 36, 0, 0),
        org: "Vultr",
        country: "US",
        kind: AsnKind::Hosting,
    },
    AsnRecord {
        asn: 49505,
        start: quad(185, 220, 0, 0),
        end: quad(185, 224, 0, 0),
        org: "Selectel JSC",
        country: "RU",
        kind: AsnKind::Bulletproof,
    },
    AsnRecord {
        asn: 51852,
        start: quad(45, 137, 0, 0),
        end: quad(45, 139, 0, 0),
        org: "Private Layer",
        country: "CH",
        kind: AsnKind::Bulletproof,
    },
    AsnRecord {
        asn: 13335,
        start: quad(104, 16, 0, 0),
        end: quad(104, 32, 0, 0),
        org: "Cloudflare",
        country: "US",
        kind: AsnKind::Hosting,
    },
    AsnRecord {
        asn: 0,
        start: quad(10, 0, 0, 0),
        end: quad(11, 0, 0, 0),
        org: "Private",
        country: "--",
        kind: AsnKind::Private,
    },
    AsnRecord {
        asn: 0,
        start: quad(192, 168, 0, 0),
        end: quad(192, 169, 0, 0),
        org: "Private",
        country: "--",
        kind: AsnKind::Private,
    },
];

/// Lookup front-end.
#[derive(Debug, Default, Clone, Copy)]
pub struct GeoIp;

impl GeoIp {
    /// Attribute `ip`, falling back to an unknown record.
    pub fn lookup(ip: Ipv4Addr) -> AsnRecord {
        TABLE
            .iter()
            .find(|record| record.contains(ip))
            .copied()
            .unwrap_or(AsnRecord {
                asn: 0,
                start: Ipv4Addr::UNSPECIFIED,
                end: Ipv4Addr::BROADCAST,
                org: "Unattributed",
                country: "??",
                kind: AsnKind::Unknown,
            })
    }

    /// One-line feed label, e.g. `AS49505 Selectel JSC (RU, bulletproof)`.
    pub fn label(ip: Ipv4Addr) -> String {
        let record = Self::lookup(ip);
        format!(
            "AS{} {} ({}, {})",
            record.asn,
            record.org,
            record.country,
            record.kind.label()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn residential_and_hosting_are_distinguished() {
        assert_eq!(
            GeoIp::lookup(quad(73, 14, 3, 9)).kind,
            AsnKind::Residential
        );
        assert!(GeoIp::lookup(quad(52, 10, 0, 1)).kind.is_flagged_by_waf());
        assert!(!GeoIp::lookup(quad(73, 14, 3, 9)).kind.is_flagged_by_waf());
    }

    #[test]
    fn bulletproof_russian_range_matches_demo() {
        let record = GeoIp::lookup(quad(185, 220, 101, 1));
        assert_eq!(record.country, "RU");
        assert_eq!(record.kind, AsnKind::Bulletproof);
        assert!(GeoIp::label(quad(185, 220, 101, 1)).contains("AS49505"));
    }

    #[test]
    fn private_space() {
        assert_eq!(GeoIp::lookup(quad(10, 1, 2, 3)).kind, AsnKind::Private);
        assert_eq!(
            GeoIp::lookup(quad(192, 168, 4, 4)).kind,
            AsnKind::Private
        );
    }

    #[test]
    fn unknown_falls_back_cleanly() {
        let record = GeoIp::lookup(quad(8, 8, 8, 8));
        assert_eq!(record.kind, AsnKind::Unknown);
        assert_eq!(record.country, "??");
    }
}
