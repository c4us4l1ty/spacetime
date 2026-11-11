//! Copy-on-write memory accounting, snapshots and the time-travel ring.
//!
//! Real backends hand Spacetime a [`MemoryImage`] — the guest's dirty-page set —
//! and Spacetime decides *what* to keep. The policy lives here so it can be
//! tested without a hypervisor:
//!
//! * every capture only stores pages that differ from the previous capture
//!   (copy-on-write diffs, never full RAM dumps);
//! * captures live in a bounded ring, so memory cost is predictable;
//! * rewinding composes diffs backwards until the requested moment.
//!
//! The real implementation swaps `BTreeMap<u64, Vec<u8>>` for a `memfd`/userfaultfd
//! backed store and an `mmap(MAP_PRIVATE)` view per tab. The policy, eviction and
//! accounting stay identical.

use std::collections::BTreeMap;

/// One page of guest physical memory. 4 KiB is what every x86-64/arm64 guest
/// hands us through the virtio-balloon free-page-reporting channel.
pub const PAGE_SIZE: u64 = 4096;

/// A sparse view of guest physical memory: page index → 4 KiB page contents.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryImage {
    pages: BTreeMap<u64, Vec<u8>>,
}

impl MemoryImage {
    pub fn new() -> MemoryImage {
        MemoryImage {
            pages: BTreeMap::new(),
        }
    }

    /// Set a page (truncated/zero-padded to [`PAGE_SIZE`]).
    pub fn set_page(&mut self, index: u64, mut data: Vec<u8>) {
        data.resize(PAGE_SIZE as usize, 0);
        data.truncate(PAGE_SIZE as usize);
        self.pages.insert(index, data);
    }

    /// Fill `count` pages from `start` using `pattern`, handy for tests.
    pub fn fill(&mut self, start: u64, count: u64, pattern: u8) {
        for index in start..start + count {
            self.set_page(index, vec![pattern; PAGE_SIZE as usize]);
        }
    }

    pub fn get_page(&self, index: u64) -> Option<&[u8]> {
        self.pages.get(&index).map(|v| v.as_slice())
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    pub fn byte_len(&self) -> u64 {
        self.pages.len() as u64 * PAGE_SIZE
    }

    pub fn iter(&self) -> impl Iterator<Item = (&u64, &Vec<u8>)> {
        self.pages.iter()
    }

    /// Pages present in `self` but absent or different in `base`.
    pub fn diff_against(&self, base: &MemoryImage) -> MemoryImage {
        let mut out = MemoryImage::new();
        for (index, page) in &self.pages {
            if base.pages.get(index) != Some(page) {
                out.pages.insert(*index, page.clone());
            }
        }
        out
    }

    /// Apply a sparse diff on top of `self` (later pages win).
    pub fn apply(&mut self, diff: &MemoryImage) {
        for (index, page) in &diff.pages {
            self.pages.insert(*index, page.clone());
        }
    }

    /// Reconstruct `self + diff` without mutating `self`.
    pub fn overlay(&self, diff: &MemoryImage) -> MemoryImage {
        let mut out = self.clone();
        out.apply(diff);
        out
    }
}

/// A lightweight dirty-page capture.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub seq: u64,
    /// Milliseconds since the tab (or the program) started.
    pub at_ms: u64,
    /// Pages changed since the previous snapshot.
    pub diff: MemoryImage,
    /// Human label for the scrubber pin ("File created", "AI risky action").
    pub marker: Option<String>,
}

impl Snapshot {
    /// Host bytes charged for this snapshot.
    pub fn cost_bytes(&self) -> u64 {
        self.diff.byte_len()
    }
}

/// Default capture cadence from the design doc: a snapshot every 2 s, keeping
/// five minutes of history.
pub const DEFAULT_INTERVAL_MS: u64 = 2_000;
pub const DEFAULT_HORIZON_MS: u64 = 300_000;

/// The per-tab ring of dirty-page diffs.
#[derive(Debug, Clone)]
pub struct TimeTravel {
    /// Full image at `base_ms`; diffs are layered on top.
    base: MemoryImage,
    base_ms: u64,
    snapshots: Vec<Snapshot>,
    horizon_ms: u64,
    max_snapshots: usize,
    next_seq: u64,
    /// Highest timestamp captured so far.
    pub now_ms: u64,
    /// Guards against capturing when nothing changed.
    last: MemoryImage,
    /// Whether the golden base image has been installed. Callers must seed
    /// exactly once per ring; re-seeding on every empty-snapshot tick would
    /// reset `last` to the live image and make every diff empty forever.
    seeded: bool,
}

impl Default for TimeTravel {
    fn default() -> Self {
        TimeTravel::new(DEFAULT_HORIZON_MS)
    }
}

impl TimeTravel {
    pub fn new(horizon_ms: u64) -> TimeTravel {
        TimeTravel {
            base: MemoryImage::new(),
            base_ms: 0,
            snapshots: Vec::new(),
            horizon_ms,
            // ~1 snapshot per second at the default cadence, plus a safety cap.
            max_snapshots: 600,
            next_seq: 1,
            now_ms: 0,
            last: MemoryImage::new(),
            seeded: false,
        }
    }

    /// Install the initial memory image (the paused zygote snapshot).
    pub fn seed(&mut self, image: MemoryImage) {
        self.last = image.clone();
        self.base = image;
        self.base_ms = 0;
        self.seeded = true;
    }

    /// Whether [`TimeTravel::seed`] has been called on this ring.
    pub fn seeded(&self) -> bool {
        self.seeded
    }

    /// Capture the current image at time `at_ms`. Returns the new snapshot, or
    /// `None` if nothing changed.
    pub fn capture(&mut self, image: &MemoryImage, at_ms: u64) -> Option<&Snapshot> {
        self.now_ms = self.now_ms.max(at_ms);
        let diff = image.diff_against(&self.last);
        if diff.is_empty() {
            return None;
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.snapshots.push(Snapshot {
            seq,
            at_ms,
            diff: diff.clone(),
            marker: None,
        });
        self.last = image.clone();
        self.evict();
        self.snapshots.last()
    }

    /// Attach a scrubber marker to the most recent snapshot.
    pub fn mark(&mut self, marker: &str) -> bool {
        match self.snapshots.last_mut() {
            Some(last) => {
                last.marker = Some(marker.to_string());
                true
            }
            None => false,
        }
    }

    /// Drop snapshots older than the horizon (or beyond the count cap).
    fn evict(&mut self) {
        let newest = self.now_ms;
        while let Some(first) = self.snapshots.first() {
            let too_old = newest.saturating_sub(first.at_ms) > self.horizon_ms;
            let too_many = self.snapshots.len() > self.max_snapshots;
            if too_old || too_many {
                // Fold the evicted diff into the base so rewinds stay correct.
                let first = self.snapshots.remove(0);
                self.base.apply(&first.diff);
                self.base_ms = first.at_ms;
            } else {
                break;
            }
        }
    }

    /// Reconstruct the full memory image as of `at_ms`.
    ///
    /// Returns `None` if the requested moment predates the oldest snapshot.
    pub fn rewind_to(&self, at_ms: u64) -> Option<MemoryImage> {
        let oldest = self
            .snapshots
            .first()
            .map(|s| s.at_ms)
            .unwrap_or(self.now_ms);
        if at_ms < oldest {
            return None;
        }
        let mut image = self.base.clone();
        for snapshot in &self.snapshots {
            if snapshot.at_ms <= at_ms {
                image.apply(&snapshot.diff);
            } else {
                break;
            }
        }
        Some(image)
    }

    /// Snapshot nearest to (and not after) `at_ms`, if any.
    pub fn nearest_before(&self, at_ms: u64) -> Option<&Snapshot> {
        self.snapshots.iter().rev().find(|s| s.at_ms <= at_ms)
    }

    /// All snapshots currently retained, oldest first.
    pub fn snapshots(&self) -> &[Snapshot] {
        &self.snapshots
    }

    /// Total host bytes charged for retained diffs.
    pub fn cost_bytes(&self) -> u64 {
        self.snapshots.iter().map(Snapshot::cost_bytes).sum()
    }

    /// Window currently covered by the scrubber.
    pub fn window(&self) -> (u64, u64) {
        let start = self
            .snapshots
            .first()
            .map(|s| s.at_ms)
            .unwrap_or(self.now_ms);
        (start, self.now_ms)
    }

    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// JSON view for the scrubber UI.
    pub fn to_json(&self) -> crate::json::Value {
        use crate::json::Value;
        let mut pins = Vec::new();
        for snapshot in &self.snapshots {
            pins.push(
                Value::obj()
                    .with("seq", snapshot.seq)
                    .with("at_ms", snapshot.at_ms)
                    .with("kb", snapshot.cost_bytes() / 1024)
                    .with("marker", snapshot.marker.clone().unwrap_or_default()),
            );
        }
        let (start, end) = self.window();
        Value::obj()
            .with("start_ms", start)
            .with("end_ms", end)
            .with("count", self.snapshots.len())
            .with("cost_kb", self.cost_bytes() / 1024)
            .with("pins", Value::Arr(pins))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(pages: &[(u64, u8)]) -> MemoryImage {
        let mut img = MemoryImage::new();
        for (index, byte) in pages {
            img.set_page(*index, vec![*byte; PAGE_SIZE as usize]);
        }
        img
    }

    #[test]
    fn only_dirty_pages_are_stored() {
        let mut tt = TimeTravel::new(DEFAULT_HORIZON_MS);
        tt.seed(image(&[(1, 0xaa), (2, 0xbb)]));
        let snap = tt
            .capture(&image(&[(1, 0xaa), (2, 0xcc), (3, 0xdd)]), 1_000)
            .unwrap();
        assert_eq!(snap.diff.page_count(), 2); // page 1 unchanged, 2 and 3 are new
    }

    #[test]
    fn no_change_means_no_snapshot() {
        let mut tt = TimeTravel::new(DEFAULT_HORIZON_MS);
        let img = image(&[(1, 1)]);
        tt.seed(img.clone());
        assert!(tt.capture(&img, 500).is_none());
        assert!(tt.is_empty());
    }

    #[test]
    fn rewind_restores_earlier_state() {
        let mut tt = TimeTravel::new(DEFAULT_HORIZON_MS);
        tt.seed(image(&[(1, 0x01)]));
        tt.capture(&image(&[(1, 0x02)]), 1_000).unwrap();
        tt.capture(&image(&[(1, 0x03), (9, 0x09)]), 2_000).unwrap();

        let at_1500 = tt.rewind_to(1_500).unwrap();
        assert_eq!(at_1500.get_page(1), Some(&[0x02u8; PAGE_SIZE as usize][..]));
        assert!(at_1500.get_page(9).is_none());
        let at_2500 = tt.rewind_to(2_500).unwrap();
        assert_eq!(at_2500.get_page(9), Some(&[0x09u8; PAGE_SIZE as usize][..]));
    }

    #[test]
    fn horizon_eviction_refolds_base() {
        let mut tt = TimeTravel::new(10_000);
        tt.seed(image(&[(1, 0x01)]));
        for step in 1..=20u64 {
            tt.capture(&image(&[(1, (step % 250) as u8 + 1)]), step * 1_000)
                .unwrap();
        }
        // Only snapshots within the 10 s horizon survive.
        let (start, end) = tt.window();
        assert!(end - start <= 10_000);
        // Rewinding after eviction still returns something coherent.
        assert!(tt.rewind_to(end).is_some());
    }

    #[test]
    fn markers_are_attached() {
        let mut tt = TimeTravel::new(DEFAULT_HORIZON_MS);
        tt.seed(image(&[(1, 0)]));
        tt.capture(&image(&[(1, 1)]), 1_000).unwrap();
        assert!(tt.mark("file created"));
        assert_eq!(tt.snapshots()[0].marker.as_deref(), Some("file created"));
    }

    #[test]
    fn full_image_rebuild_matches_live_state() {
        let mut tt = TimeTravel::new(DEFAULT_HORIZON_MS);
        tt.seed(image(&[(1, 0x10)]));
        let live = image(&[(1, 0x11), (2, 0x22), (3, 0x33)]);
        tt.capture(&live, 1_000).unwrap();
        let rebuilt = tt.rewind_to(1_000).unwrap();
        assert_eq!(rebuilt.page_count(), live.page_count());
        assert_eq!(rebuilt.get_page(2), live.get_page(2));
    }

    #[test]
    fn json_includes_pins() {
        let mut tt = TimeTravel::new(DEFAULT_HORIZON_MS);
        tt.seed(image(&[(1, 0)]));
        tt.capture(&image(&[(1, 5)]), 1_000).unwrap();
        tt.mark("risky action");
        let json = tt.to_json();
        assert_eq!(json.u64_of("count"), 1);
        let pins = json.get("pins").unwrap().as_arr().unwrap();
        assert_eq!(pins[0].str_of("marker"), "risky action");
    }
}
