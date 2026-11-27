//! Ring -3 policy, capability token, taint lattice, vault handle, and egress types.

use crate::{EgressClass, GoalId, Result, SpacetimeError, Taint, TrustTier};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub fn taint_is_untrusted(taint: Taint) -> bool {
    matches!(taint, Taint::UntrustedWeb | Taint::UntrustedToolOutput)
}

pub fn taint_rank(taint: Taint) -> u8 {
    match taint {
        Taint::TrustedConstitution => 0,
        Taint::TrustedUser => 1,
        Taint::TrustedTool => 2,
        Taint::UntrustedToolOutput => 3,
        Taint::UntrustedWeb => 4,
    }
}

pub fn join_taint(left: Taint, right: Taint) -> Taint {
    if taint_rank(left) >= taint_rank(right) {
        left
    } else {
        right
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityToken {
    pub token_id: String,
    pub goal_id: GoalId,
    pub allowed_ops: BTreeSet<String>,
    pub max_spend_fake_credits_per_day: u64,
    pub max_posts_per_day: u32,
    pub allow_irreversible_delete: bool,
    pub allow_publish: bool,
    pub expires_at_turn: u64,
    pub trust_tier: TrustTier,
    pub egress_class: EgressClass,
}

impl CapabilityToken {
    pub fn is_expired(&self, current_turn: u64) -> bool {
        current_turn > self.expires_at_turn
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VaultHandle {
    pub name: String,
    pub digest: String,
}

impl VaultHandle {
    pub fn parse_symbolic(token: &str) -> Option<&str> {
        let name = token.strip_prefix("@vault:")?;
        if name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        {
            return None;
        }
        Some(name)
    }

    pub fn format_symbolic(name: &str) -> Result<String> {
        if name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        {
            return Err(SpacetimeError::new("invalid vault handle name"));
        }
        Ok(format!("@vault:{name}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanaryAlarm {
    pub canary_id: String,
    pub source_channel: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleEffect {
    Permit,
    Forbid,
    QueueBlocker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstitutionPolicy {
    pub schema_version: u32,
    pub max_daily_fake_spend: u64,
    pub max_daily_posts: u32,
    pub forbid_untrusted_spend: bool,
    pub forbid_untrusted_publish: bool,
    pub forbid_untrusted_delete: bool,
    pub forbid_host_local_egress: bool,
    pub require_expected_verifier: bool,
    pub allowed_egress_classes: BTreeSet<String>,
}

impl Default for ConstitutionPolicy {
    fn default() -> Self {
        let mut allowed_egress_classes = BTreeSet::new();
        allowed_egress_classes.insert("none".to_string());
        Self {
            schema_version: 1,
            max_daily_fake_spend: 100,
            max_daily_posts: 1,
            forbid_untrusted_spend: true,
            forbid_untrusted_publish: true,
            forbid_untrusted_delete: true,
            forbid_host_local_egress: true,
            require_expected_verifier: true,
            allowed_egress_classes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EgressFlowRecord {
    pub sequence: u64,
    pub channel_credential_digest: String,
    pub workspace_id: String,
    pub egress_class: EgressClass,
    pub destination: String,
    pub port: u16,
    pub allowed: bool,
    pub reason: String,
}
