//! Supervisor-owned egress broker and DNS boundary checks.
//!
//! Cells are identified solely by supervisor-assigned channel credentials, never by a guest-
//! supplied `workspace_id`. Host-local, link-local, metadata, LAN, and UNIX socket destinations
//! are denied unconditionally.

use spacetime_types::policy::EgressFlowRecord;
use spacetime_types::{sha256_hex, EgressClass, Result, SpacetimeError};
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CellEgressGrant {
    workspace_id: String,
    egress_class: EgressClass,
    pinned_proxy: Option<String>,
    pinned_dns: Option<String>,
}

#[derive(Debug, Default)]
pub struct EgressBroker {
    grants: BTreeMap<String, CellEgressGrant>,
    audit_log: Vec<EgressFlowRecord>,
}

impl EgressBroker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_cell(
        &mut self,
        channel_credential: &str,
        workspace_id: &str,
        egress_class: EgressClass,
        pinned_proxy: Option<String>,
        pinned_dns: Option<String>,
    ) -> Result<()> {
        if channel_credential.len() < 16 {
            let msg = "egress channel credential is too short";
            return Err(SpacetimeError::new(msg));
        }
        let needs_proxy = matches!(egress_class, EgressClass::Proxy | EgressClass::Tor);
        if needs_proxy && pinned_proxy.is_none() {
            return Err(SpacetimeError::new(
                "proxy and tor egress classes require a pinned proxy endpoint",
            ));
        }
        if egress_class != EgressClass::None && pinned_dns.is_none() {
            let msg = "networked cell requires a pinned tunnel DNS resolver";
            return Err(SpacetimeError::new(msg));
        }
        self.grants.insert(
            channel_credential.to_string(),
            CellEgressGrant {
                workspace_id: workspace_id.to_string(),
                egress_class,
                pinned_proxy,
                pinned_dns,
            },
        );
        Ok(())
    }

    pub fn audit_log(&self) -> &[EgressFlowRecord] {
        &self.audit_log
    }

    /// Evaluate an outbound connect attempt authenticated strictly by channel credential.
    pub fn evaluate_connect(
        &mut self,
        channel_credential: &str,
        claimed_workspace_id: Option<&str>,
        destination: &str,
        port: u16,
    ) -> EgressFlowRecord {
        let cred_digest = sha256_hex(channel_credential.as_bytes());
        let sequence = self.audit_log.len() as u64;

        let Some(grant) = self.grants.get(channel_credential) else {
            let record = EgressFlowRecord {
                sequence,
                channel_credential_digest: cred_digest,
                workspace_id: "unauthenticated".to_string(),
                egress_class: EgressClass::None,
                destination: destination.to_string(),
                port,
                allowed: false,
                reason: "unknown supervisor channel credential".to_string(),
            };
            self.audit_log.push(record.clone());
            return record;
        };

        if let Some(claimed) = claimed_workspace_id {
            if claimed != grant.workspace_id {
                let record = EgressFlowRecord {
                    sequence,
                    channel_credential_digest: cred_digest,
                    workspace_id: grant.workspace_id.clone(),
                    egress_class: grant.egress_class,
                    destination: destination.to_string(),
                    port,
                    allowed: false,
                    reason: "guest workspace_id does not match channel credential".to_string(),
                };
                self.audit_log.push(record.clone());
                return record;
            }
        }

        let (allowed, reason) = evaluate_destination(grant, destination, port);
        let record = EgressFlowRecord {
            sequence,
            channel_credential_digest: cred_digest,
            workspace_id: grant.workspace_id.clone(),
            egress_class: grant.egress_class,
            destination: destination.to_string(),
            port,
            allowed,
            reason,
        };
        self.audit_log.push(record.clone());
        record
    }

    /// Ensure guest DNS config never inherits the host resolver or localhost stub.
    pub fn validate_guest_resolver(
        &self,
        channel_credential: &str,
        resolver_spec: &str,
    ) -> Result<()> {
        let grant = self
            .grants
            .get(channel_credential)
            .ok_or_else(|| SpacetimeError::new("unknown channel credential"))?;
        if grant.egress_class == EgressClass::None {
            if resolver_spec != "none" {
                return Err(SpacetimeError::new(
                    "net=none cell cannot configure a DNS resolver",
                ));
            }
            return Ok(());
        }
        if resolver_spec.contains("/etc/resolv.conf")
            || is_forbidden_host_target(resolver_spec.trim())
        {
            return Err(SpacetimeError::new(
                "guest DNS must use pinned tunnel resolver, never host resolv.conf or localhost",
            ));
        }
        let expected = grant.pinned_dns.as_deref().unwrap_or("");
        if resolver_spec != expected {
            return Err(SpacetimeError::new(
                "guest DNS does not match pinned per-tab resolver",
            ));
        }
        Ok(())
    }
}

fn evaluate_destination(grant: &CellEgressGrant, destination: &str, port: u16) -> (bool, String) {
    if grant.egress_class == EgressClass::None {
        return (false, "net=none cell denies all egress".to_string());
    }
    if port == 0 {
        return (false, "invalid port 0".to_string());
    }
    if is_forbidden_host_target(destination) {
        return (
            false,
            "destination is host-local, LAN, metadata, or UNIX socket (denied by invariant)"
                .to_string(),
        );
    }
    match grant.egress_class {
        EgressClass::None => (false, "net=none cell denies all egress".to_string()),
        EgressClass::Proxy | EgressClass::Tor => {
            if grant.pinned_proxy.as_deref() == Some(destination) {
                (true, "allowed via pinned proxy/tor tunnel".to_string())
            } else {
                (
                    false,
                    "proxy/tor cell cannot bypass pinned tunnel endpoint".to_string(),
                )
            }
        }
        EgressClass::Direct => (true, "allowed non-local public destination".to_string()),
    }
}

pub fn is_forbidden_host_target(raw: &str) -> bool {
    let trimmed = raw
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    if trimmed.is_empty()
        || trimmed.starts_with('/')
        || trimmed.starts_with("unix:")
        || Path::new(&trimmed).is_absolute()
        || trimmed == "localhost"
        || trimmed.ends_with(".localhost")
        || trimmed.ends_with(".local")
        || trimmed == "metadata.google.internal"
    {
        return true;
    }

    if let Ok(ip) = trimmed.parse::<Ipv4Addr>() {
        return is_forbidden_ipv4(ip);
    }
    if let Ok(ip) = trimmed.parse::<Ipv6Addr>() {
        return is_forbidden_ipv6(ip);
    }
    false
}

fn is_forbidden_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_unspecified()
        // 100.64.0.0/10 CGNAT / shared address space
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
}

fn is_forbidden_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    // IPv4-mapped (::ffff:a.b.c.d) or IPv4-compatible (::a.b.c.d) addresses
    if segments[..5] == [0, 0, 0, 0, 0] && (segments[5] == 0xffff || segments[5] == 0) {
        let mapped = Ipv4Addr::from((u32::from(segments[6]) << 16) | u32::from(segments[7]));
        return is_forbidden_ipv4(mapped);
    }
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        // Unique local addresses fc00::/7
        || (segments[0] & 0xfe00) == 0xfc00
        // Link-local unicast fe80::/10
        || (segments[0] & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broker_blocks_host_local_lan_metadata_and_forged_credentials() -> Result<()> {
        let mut broker = EgressBroker::new();
        let cred = "cred-ws-direct-00000001";
        broker.register_cell(
            cred,
            "ws-direct",
            EgressClass::Direct,
            None,
            Some("tunnel://dns-pin:53".to_string()),
        )?;

        for forbidden in [
            "127.0.0.1",
            "127.0.1.1",
            "::1",
            "::ffff:127.0.0.1",
            "169.254.169.254",
            "10.0.0.1",
            "172.16.0.5",
            "192.168.1.1",
            "fe80::1",
            "fc00::1",
            "localhost",
            "/var/run/docker.sock",
            "unix:/tmp/.X11-unix/X0",
        ] {
            let flow = broker.evaluate_connect(cred, Some("ws-direct"), forbidden, 443);
            assert!(!flow.allowed, "{forbidden} was unexpectedly allowed");
        }

        // Forged workspace_id claim with a valid credential must fail.
        let spoofed = broker.evaluate_connect(cred, Some("ws-other"), "93.184.216.34", 443);
        assert!(!spoofed.allowed);

        // Unknown credential must fail.
        let unknown = broker.evaluate_connect("bogus-credential-1234", None, "93.184.216.34", 443);
        assert!(!unknown.allowed);

        // Valid public IP on direct cell succeeds.
        let ok = broker.evaluate_connect(cred, Some("ws-direct"), "93.184.216.34", 443);
        assert!(ok.allowed);

        assert!(broker
            .validate_guest_resolver(cred, "/etc/resolv.conf")
            .is_err());
        assert!(broker.validate_guest_resolver(cred, "127.0.0.53").is_err());
        broker.validate_guest_resolver(cred, "tunnel://dns-pin:53")?;
        Ok(())
    }

    #[test]
    fn net_none_cell_denies_all_destinations() -> Result<()> {
        let mut broker = EgressBroker::new();
        let cred = "cred-ws-offline-0000001";
        broker.register_cell(cred, "ws-offline", EgressClass::None, None, None)?;
        let denied = broker.evaluate_connect(cred, None, "93.184.216.34", 443);
        assert!(!denied.allowed);
        assert!(broker.validate_guest_resolver(cred, "8.8.8.8").is_err());
        broker.validate_guest_resolver(cred, "none")?;
        Ok(())
    }
}
