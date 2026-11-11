//! The Warden: keeps a laptop silent, cool and responsive while dozens of
//! untrusted VMs run.
//!
//! Two mechanisms, deliberately modelled before the platform code exists:
//!
//! * **Focus-aware choking.** The focused tab gets up to 75 % of a core; the
//!   instant focus moves, the old tab drops to 5 % of one core. A `pids.max`
//!   ceiling makes a fork bomb fail with `ENOMEM` instead of taking the host
//!   down. Tabs can opt out with "Allow Background Compute".
//! * **Balloon reclaim.** When the guest frees pages, virtio-balloon
//!   free-page-reporting hands them straight back to the host.
//!
//! The Linux implementation writes `cgroup v2` files; the Windows one sets
//! `Job Object` limits; the macOS one toggles `task_policy`. All three satisfy
//! the [`ChokeSink`] contract, so the policy below is identical everywhere.

use std::collections::BTreeMap;

use spacetime_core::json::Value;
use spacetime_core::tab::{Resources, TabId};

/// Where throttle decisions land. Real implementations write cgroups; the
/// default [`RecordingSink`] keeps them in memory for tests and the CLI.
pub trait ChokeSink {
    fn set_limits(&mut self, tab: TabId, limits: &Resources);
    fn freeze(&mut self, tab: TabId);
    fn thaw(&mut self, tab: TabId);
}

/// In-memory sink: records the latest decision per tab.
#[derive(Debug, Default)]
pub struct RecordingSink {
    pub limits: BTreeMap<TabId, Resources>,
    pub frozen: BTreeMap<TabId, bool>,
    pub transitions: u64,
}

impl ChokeSink for RecordingSink {
    fn set_limits(&mut self, tab: TabId, limits: &Resources) {
        let changed = self.limits.get(&tab) != Some(limits);
        if changed {
            self.transitions += 1;
        }
        self.limits.insert(tab, *limits);
    }
    fn freeze(&mut self, tab: TabId) {
        self.frozen.insert(tab, true);
    }
    fn thaw(&mut self, tab: TabId) {
        self.frozen.insert(tab, false);
    }
}

/// Per-tab balloon accounting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Balloon {
    /// RSS charged to the host.
    pub resident_mb: u64,
    /// Pages handed back to the host this session.
    pub reclaimed_mb: u64,
}

/// The warden.
#[derive(Debug)]
pub struct Warden<S: ChokeSink = RecordingSink> {
    sink: S,
    balloons: BTreeMap<TabId, Balloon>,
    focus: Option<TabId>,
    pub throttle_events: u64,
    pub reclaimed_mb: u64,
}

impl Default for Warden<RecordingSink> {
    fn default() -> Self {
        Warden::new(RecordingSink::default())
    }
}

impl<S: ChokeSink> Warden<S> {
    pub fn new(sink: S) -> Warden<S> {
        Warden {
            sink,
            balloons: BTreeMap::new(),
            focus: None,
            throttle_events: 0,
            reclaimed_mb: 0,
        }
    }

    pub fn sink(&self) -> &S { &self.sink }

    pub fn sink_mut(&mut self) -> &mut S {
        &mut self.sink
    }

    /// Apply the focus policy. `tabs` is `(id, allow_background_compute)`.
    pub fn apply_focus(&mut self, focus: TabId, tabs: &[(TabId, bool)]) {
        self.focus = Some(focus);
        for (tab, allow_bg) in tabs {
            if *tab == focus || *allow_bg {
                self.sink.thaw(*tab);
                self.sink.set_limits(*tab, &Resources::FOCUSED);
            } else {
                self.sink.set_limits(*tab, &Resources::BACKGROUND);
                self.throttle_events += 1;
            }
        }
    }

    /// Fully freeze a tab (e.g. while a memory snapshot is written).
    pub fn freeze(&mut self, tab: TabId) {
        self.sink.freeze(tab);
    }

    pub fn thaw(&mut self, tab: TabId) {
        self.sink.thaw(tab);
    }

    /// Report a balloon event from the guest.
    pub fn report_balloon(&mut self, tab: TabId, resident_mb: u64, reclaimed_mb: u64) {
        let entry = self.balloons.entry(tab).or_default();
        entry.resident_mb = resident_mb;
        entry.reclaimed_mb += reclaimed_mb;
        self.reclaimed_mb += reclaimed_mb;
    }

    pub fn balloon(&self, tab: TabId) -> Balloon {
        self.balloons.get(&tab).copied().unwrap_or_default()
    }

    /// Total host RAM charged across tabs.
    pub fn host_ram_mb(&self) -> u64 { self.balloons.values().map(|b| b.resident_mb).sum() }

    /// PID ceiling the host enforces for a tab (fork-bomb guard).
    pub fn pids_ceiling(&self, tab: TabId) -> u32 {
        let _ = tab;
        Resources::BACKGROUND.pids_max
    }

    pub fn to_json(&self) -> Value {
        let mut tabs = Vec::new();
        for (tab, balloon) in &self.balloons {
            tabs.push(
                Value::obj()
                    .with("tab", tab.0)
                    .with("resident_mb", balloon.resident_mb)
                    .with("reclaimed_mb", balloon.reclaimed_mb),
            );
        }
        Value::obj()
            .with("focus", self.focus.map(|f| f.0).unwrap_or(0))
            .with("throttle_events", self.throttle_events)
            .with("reclaimed_mb", self.reclaimed_mb)
            .with("tabs", Value::Arr(tabs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_tabs_get_five_percent() {
        let mut warden = Warden::default();
        warden.apply_focus(TabId(1), &[(TabId(1), false), (TabId(2), false)]);
        let sink = warden.sink();
        assert_eq(sink.limits[&TabId(1)].cpu_permille, 750);        assert_eq(sink.limits[&TabId(2)].cpu_permille, 50);        assert_eq(warden.throttle_events, 1);    }

    #[test]
    fn background_compute_opt_out_keeps_full_cpu() {
        let mut warden = Warden::default();
        warden.apply_focus(TabId(1), &[(TabId(1), false), (TabId(2), true)]);
        assert_eq(warden.sink().limits[&TabId(2)].cpu_permille, 750);        assert_eq(warden.throttle_events, 0);    }

    #[test]
    fn freeze_thaw_is_recorded() {
        let mut warden = Warden::default();
        warden.freeze(TabId(3));
        assert(warden.sink().frozen[&TabId(3)]);        warden.thaw(TabId(3));
        assert(!warden.sink().frozen[&TabId(3)]);    }

    #[test]
    fn balloon_reclaim_is_summed() {
        let mut warden = Warden::default();
        warden.report_balloon(TabId(1), 900, 64);
        warden.report_balloon(TabId(1), 700, 32);
        warden.report_balloon(TabId(2), 400, 16);
        assert_eq(warden.balloon(TabId(1)).reclaimed_mb, 96);        assert_eq(warden.reclaimed_mb, 112);        assert_eq(warden.balloon(TabId(2)).resident_mb, 400);    }

    #[test]
    fn fork_bomb_ceiling_is_finite() {
        let warden = Warden::default();
        assert(warden.pids_ceiling(TabId(9)) <= 256);        assert_eq(warden.host_ram_mb(), 0);    }
}
