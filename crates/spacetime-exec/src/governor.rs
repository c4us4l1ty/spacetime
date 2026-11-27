//! Authoritative ResourceGovernor enforcing the First Law.
//!
//! The governor is not advisory: it downgrades PreviewMode, suspends hidden heavy surfaces,
//! queues work, or kills runaways when budgets or concurrency caps are exceeded.

use spacetime_types::surface::{GovernorAction, IntentSummary, WorkspaceSurfaceDescriptor};
use spacetime_types::{LivenessState, PreviewMode, SurfaceKind};
use std::fs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernorPolicy {
    pub profile_id: String,
    pub memory_budget_kb: u64,
    pub max_concurrent_heavy_rendering: usize,
    pub max_running_cells: usize,
    pub prefer_snapshot_first: bool,
}

impl GovernorPolicy {
    pub fn baseline_4gb_potato() -> Self {
        Self {
            profile_id: "4gb_potato".to_string(),
            memory_budget_kb: 4 * 1024 * 1024,
            max_concurrent_heavy_rendering: 1,
            max_running_cells: 4,
            prefer_snapshot_first: true,
        }
    }

    pub fn for_profile(profile_id: &str, memory_budget_kb: u64) -> Self {
        Self {
            profile_id: profile_id.to_string(),
            memory_budget_kb,
            max_concurrent_heavy_rendering: 1,
            max_running_cells: if memory_budget_kb <= 4 * 1024 * 1024 {
                4
            } else {
                8
            },
            prefer_snapshot_first: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernorSample {
    pub total_rss_kb: u64,
    pub active_heavy_rendering: usize,
    pub running_cells: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernorEvaluation {
    pub action: GovernorAction,
    pub chosen_preview_mode: PreviewMode,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct ResourceGovernor {
    policy: GovernorPolicy,
}

impl ResourceGovernor {
    pub fn new(policy: GovernorPolicy) -> Self {
        Self { policy }
    }

    pub fn policy(&self) -> &GovernorPolicy {
        &self.policy
    }

    /// Read current process RSS from Linux `/proc/self/status` when available.
    pub fn sample_current_process_rss_kb() -> Option<u64> {
        let status = fs::read_to_string("/proc/self/status").ok()?;
        status.lines().find_map(|line| {
            let rest = line.strip_prefix("VmRSS:")?;
            rest.split_whitespace().next()?.parse::<u64>().ok()
        })
    }

    pub fn evaluate_workspace(
        &self,
        descriptor: &WorkspaceSurfaceDescriptor,
        sample: &GovernorSample,
        is_active_tab: bool,
    ) -> GovernorEvaluation {
        let hard_rss_limit = self.policy.memory_budget_kb;
        let high_pressure_limit = hard_rss_limit.saturating_mul(85) / 100;

        if sample.total_rss_kb > hard_rss_limit {
            return GovernorEvaluation {
                action: GovernorAction::KillRunaway,
                chosen_preview_mode: PreviewMode::Headless,
                reason: format!(
                    "sampled RSS {} KiB exceeds {} budget {} KiB",
                    sample.total_rss_kb, self.policy.profile_id, hard_rss_limit
                ),
            };
        }

        if sample.running_cells > self.policy.max_running_cells && !is_active_tab {
            return GovernorEvaluation {
                action: GovernorAction::QueueWork,
                chosen_preview_mode: PreviewMode::Headless,
                reason: format!(
                    "running cells ({}) exceed policy limit ({})",
                    sample.running_cells, self.policy.max_running_cells
                ),
            };
        }

        if descriptor.surface_kind == SurfaceKind::Terminal {
            let mode = if is_active_tab {
                PreviewMode::TerminalBuffer
            } else {
                PreviewMode::Headless
            };
            return GovernorEvaluation {
                action: GovernorAction::Allow,
                chosen_preview_mode: mode,
                reason: "terminal text buffer stays within low-cost budget".to_string(),
            };
        }

        if !is_active_tab {
            let mode = if sample.total_rss_kb >= high_pressure_limit {
                PreviewMode::Headless
            } else {
                PreviewMode::SnapshotOnSwitch
            };
            return GovernorEvaluation {
                action: GovernorAction::DowngradePreview(mode),
                chosen_preview_mode: mode,
                reason: "background tab stops rendering while AI continues RUNNING".to_string(),
            };
        }

        let is_heavy = matches!(
            descriptor.surface_kind,
            SurfaceKind::Framebuffer | SurfaceKind::BrowserCdp
        );
        if is_heavy && sample.active_heavy_rendering > self.policy.max_concurrent_heavy_rendering {
            return GovernorEvaluation {
                action: GovernorAction::SuspendCell,
                chosen_preview_mode: PreviewMode::SnapshotOnSwitch,
                reason: "at most one heavy rendering surface is allowed simultaneously".to_string(),
            };
        }

        if sample.total_rss_kb >= high_pressure_limit {
            return GovernorEvaluation {
                action: GovernorAction::DowngradePreview(PreviewMode::Headless),
                chosen_preview_mode: PreviewMode::Headless,
                reason: "memory pressure triggered headless preview downgrade".to_string(),
            };
        }

        let is_live = descriptor.preview_mode == PreviewMode::LiveDamage;
        if self.policy.prefer_snapshot_first && is_live {
            return GovernorEvaluation {
                action: GovernorAction::DowngradePreview(PreviewMode::SnapshotOnAction),
                chosen_preview_mode: PreviewMode::SnapshotOnAction,
                reason: "snapshot-first default downgraded LiveDamage to SnapshotOnAction"
                    .to_string(),
            };
        }

        let chosen = if descriptor.liveness == LivenessState::Suspended {
            PreviewMode::SnapshotOnSwitch
        } else {
            descriptor.preview_mode
        };
        GovernorEvaluation {
            action: GovernorAction::Allow,
            chosen_preview_mode: chosen,
            reason: "surface allowed under resource governor".to_string(),
        }
    }

    pub fn intent_ribbon(
        files_read: u64,
        files_written: u64,
        dns_queries: u64,
        fake_spend_credits: u64,
        risk_label: &str,
    ) -> String {
        IntentSummary {
            files_read,
            files_written,
            dns_queries,
            fake_spend_credits,
            risk_label: risk_label.to_string(),
        }
        .format_ribbon()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetime_types::{EgressClass, SurfaceId, TrustTier, WorkspaceId};

    fn descriptor(kind: SurfaceKind, preview_mode: PreviewMode) -> WorkspaceSurfaceDescriptor {
        WorkspaceSurfaceDescriptor {
            workspace_id: WorkspaceId::new("ws-ai"),
            surface_id: SurfaceId::new("surface-ai"),
            surface_kind: kind,
            trust_tier: TrustTier::ArkCell,
            egress_class: EgressClass::None,
            preview_mode,
            liveness: LivenessState::Rendering,
            shared_ark_kernel: true,
        }
    }

    #[test]
    fn governor_downgrades_live_damage_and_enforces_single_heavy_surface() {
        let governor = ResourceGovernor::new(GovernorPolicy::baseline_4gb_potato());
        let sample = GovernorSample {
            total_rss_kb: 900_000,
            active_heavy_rendering: 1,
            running_cells: 2,
        };
        let eval = governor.evaluate_workspace(
            &descriptor(SurfaceKind::Framebuffer, PreviewMode::LiveDamage),
            &sample,
            true,
        );
        assert_eq!(
            eval.action,
            GovernorAction::DowngradePreview(PreviewMode::SnapshotOnAction)
        );
        assert_eq!(eval.chosen_preview_mode, PreviewMode::SnapshotOnAction);

        let too_many_heavy = GovernorSample {
            total_rss_kb: 900_000,
            active_heavy_rendering: 2,
            running_cells: 2,
        };
        let suspended = governor.evaluate_workspace(
            &descriptor(SurfaceKind::BrowserCdp, PreviewMode::SnapshotOnAction),
            &too_many_heavy,
            true,
        );
        assert_eq!(suspended.action, GovernorAction::SuspendCell);
    }

    #[test]
    fn governor_kills_runaway_over_budget_and_pauses_only_rendering_on_switch() {
        let governor = ResourceGovernor::new(GovernorPolicy::baseline_4gb_potato());
        let background = governor.evaluate_workspace(
            &descriptor(SurfaceKind::Framebuffer, PreviewMode::SnapshotOnAction),
            &GovernorSample {
                total_rss_kb: 500_000,
                active_heavy_rendering: 1,
                running_cells: 2,
            },
            false,
        );
        assert_eq!(
            background.action,
            GovernorAction::DowngradePreview(PreviewMode::SnapshotOnSwitch)
        );

        let runaway = governor.evaluate_workspace(
            &descriptor(SurfaceKind::Framebuffer, PreviewMode::SnapshotOnAction),
            &GovernorSample {
                total_rss_kb: 5_000_000,
                active_heavy_rendering: 1,
                running_cells: 2,
            },
            true,
        );
        assert_eq!(runaway.action, GovernorAction::KillRunaway);
    }

    #[test]
    fn intent_ribbon_formats_concise_summary() {
        let ribbon = ResourceGovernor::intent_ribbon(3, 1, 0, 25, "low");
        assert_eq!(
            ribbon,
            "read 3 files · wrote 1 · DNS 0 · will spend 25 fake_credits · risk: low"
        );
    }
}
