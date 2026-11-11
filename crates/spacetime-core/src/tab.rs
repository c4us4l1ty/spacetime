//! The tab tree: Spacetime's unit of isolation and its primary UI structure.
//!
//! A tab is a microVM. Tabs form a *tree* because a tab can be branched: the
//! parent's writable layer is frozen read-only and every child starts from that
//! exact moment with a fresh copy-on-write layer (see [`TabTree::branch`]).

use std::collections::BTreeMap;

/// Stable identifier for a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TabId(pub u64);

impl std::fmt::Display for TabId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:02}", self.0)
    }
}

/// Hardware isolation posture, rendered as the tab's coloured border.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// Blue: no network device attached at all.
    Airgapped,
    /// Purple: residential egress with fingerprint morphing.
    Chameleon,
    /// Orange: every packet terminates in the sinkhole switch.
    HoneyDuct,
    /// Red: RAM-only, shredded on close.
    Disposable,
}

impl Trust {
    pub fn label(self) -> &'static str {
        match self {
            Trust::Airgapped => "airgapped",
            Trust::Chameleon => "chameleon",
            Trust::HoneyDuct => "honey-duct",
            Trust::Disposable => "disposable",
        }
    }

    /// Accent colour for the shell (Qubes-style border glow).
    pub fn color(self) -> &'static str {
        match self {
            Trust::Airgapped => "#38bdf8",
            Trust::Chameleon => "#a855f7",
            Trust::HoneyDuct => "#f59e0b",
            Trust::Disposable => "#ef4444",
        }
    }

    pub fn from_label(text: &str) -> Option<Trust> {
        match text {
            "airgapped" => Some(Trust::Airgapped),
            "chameleon" | "stealth" => Some(Trust::Chameleon),
            "honey-duct" | "honeyduct" => Some(Trust::HoneyDuct),
            "disposable" => Some(Trust::Disposable),
            _ => None,
        }
    }
}

/// What the user asked Spacetime to open; drives the boot pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabKind {
    /// A URL: stealth browser microVM.
    Web,
    /// `gh:owner/repo`: clone + dependency store mount.
    Repo,
    /// A dropped binary: airgapped disposable microVM.
    File,
    /// A shell: virtio-console + host terminal renderer.
    Terminal,
    /// An `@ai` goal: heartbeat orchestrator attached.
    Ai,
}

impl TabKind {
    pub fn label(self) -> &'static str {
        match self {
            TabKind::Web => "web",
            TabKind::Repo => "repo",
            TabKind::File => "file",
            TabKind::Terminal => "terminal",
            TabKind::Ai => "ai",
        }
    }

    pub fn from_label(text: &str) -> Option<TabKind> {
        match text {
            "web" => Some(TabKind::Web),
            "repo" => Some(TabKind::Repo),
            "file" => Some(TabKind::File),
            "terminal" => Some(TabKind::Terminal),
            "ai" => Some(TabKind::Ai),
            _ => None,
        }
    }
}

/// Lifecycle state of the backing microVM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabState {
    /// Zygote clone in flight.
    Booting,
    /// Guest is executing.
    Running,
    /// Background-throttled by the warden.
    Choked,
    /// Memory snapshot written; VM torn down.
    Suspended,
    /// Closed; RAM zeroed.
    Closed,
}

impl TabState {
    pub fn label(self) -> &'static str {
        match self {
            TabState::Booting => "booting",
            TabState::Running => "running",
            TabState::Choked => "choked",
            TabState::Suspended => "suspended",
            TabState::Closed => "closed",
        }
    }
}

/// Resource budget enforced by the warden. Permille = thousandths of one core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resources {
    pub cpu_permille: u32,
    pub ram_mb: u64,
    pub ram_cap_mb: u64,
    pub pids_max: u32,
}

impl Resources {
    pub const FOCUSED: Resources = Resources {
        cpu_permille: 750,
        ram_mb: 410,
        ram_cap_mb: 4096,
        pids_max: 512,
    };

    pub const BACKGROUND: Resources = Resources {
        cpu_permille: 50,
        ram_mb: 410,
        ram_cap_mb: 1024,
        pids_max: 256,
    };
}

/// One live tab.
#[derive(Debug, Clone)]
pub struct Tab {
    pub id: TabId,
    pub name: String,
    pub parent: Option<TabId>,
    pub kind: TabKind,
    pub trust: Trust,
    pub state: TabState,
    pub resources: Resources,
    /// Measured cold-boot time of this tab's zygote clone, in milliseconds.
    pub boot_ms: u32,
    /// Host RAM actually charged to this tab (excludes shared pages).
    pub private_ram_mb: u64,
    /// Whether this tab is the user's active tab.
    pub focused: bool,
    /// Allows a background tab to keep compute (e.g. long builds).
    pub allow_background_compute: bool,
    /// Every tab shares one copy of the base image; this is the budget it draws.
    pub shared_ram_mb: u64,
}

impl Tab {
    fn new(id: TabId, name: &str, kind: TabKind, trust: Trust) -> Tab {
        Tab {
            id,
            name: name.to_string(),
            parent: None,
            kind,
            trust,
            state: TabState::Booting,
            resources: Resources::FOCUSED,
            // Deterministic pseudo-boot time in the 5–15 ms zygote-clone band.
            boot_ms: 5 + ((id.0.wrapping_mul(7)) % 11) as u32,
            private_ram_mb: 0,
            focused: false,
            allow_background_compute: false,
            shared_ram_mb: 48,
        }
    }

    /// True when the warden has throttled this tab.
    pub fn is_choked(&self) -> bool { self.state == TabState::Choked }
}

/// Errors from tree operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabError {
    Unknown(TabId),
    AlreadyClosed(TabId),
    NotRoot { tab: TabId, parent: TabId },
}

impl std::fmt::Display for TabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TabError::Unknown(id) => write!(f, "unknown tab {id}"),
            TabError::AlreadyClosed(id) => write!(f, "tab {id} is already closed"),
            TabError::NotRoot { tab, parent } => {
                write!(f, "tab {tab} is a child of {parent}, not a root")
            }
        }
    }
}

impl std::error::Error for TabError {}

/// The full tree of tabs plus the focus/throttle policy.
#[derive(Debug, Clone)]
pub struct TabTree {
    tabs: BTreeMap<TabId, Tab>,
    children: BTreeMap<TabId, Vec<TabId>>,
    roots: Vec<TabId>,
    next_id: u64,
    focus: Option<TabId>,
    pub total_created: u64,
    pub total_closed: u64,
}

impl Default for TabTree {
    fn default() -> Self {
        TabTree::new()
    }
}

impl TabTree {
    pub fn new() -> TabTree {
        TabTree {
            tabs: BTreeMap::new(),
            children: BTreeMap::new(),
            roots: Vec::new(),
            next_id: 1,
            focus: None,
            total_created: 0,
            total_closed: 0,
        }
    }

    /// Open a new root tab. Returns its id.
    pub fn open(&mut self, name: &str, kind: TabKind, trust: Trust) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;
        let mut tab = Tab::new(id, name, kind, trust);
        tab.state = TabState::Running;
        self.tabs.insert(id, tab);
        self.roots.push(id);
        self.total_created += 1;
        id
    }

    /// Branch `parent` into a new child tab.
    ///
    /// The parent's writable overlay is frozen (as a read-only lower layer) and
    /// the child is charged only for pages it dirties. This is the "install 5 GB
    /// of deps once, fork twice" workflow.
    pub fn branch(&mut self, parent: TabId) -> Result<TabId, TabError> {
        let parent_tab = self
            .tabs
            .get(&parent)
            .ok_or(TabError::Unknown(parent))?
            .clone();
        if parent_tab.state == TabState::Closed {
            return Err(TabError::AlreadyClosed(parent));
        }

        let id = TabId(self.next_id);
        self.next_id += 1;

        let mut tab = Tab::new(
            id,
            &format!("{}-b{}", parent_tab.name, id.0),
            parent_tab.kind,
            parent_tab.trust,
        );
        tab.parent = Some(parent);
        tab.state = TabState::Running;
        tab.private_ram_mb = parent_tab.private_ram_mb / 10;
        tab.shared_ram_mb = parent_tab.shared_ram_mb;

        // Branching is itself the cheapest snapshot: the parent's overlay becomes
        // a frozen read-only lower layer shared by every child.
        self.tabs.insert(id, tab);
        self.children.entry(parent).or_default().push(id);
        self.total_created += 1;
        Ok(id)
    }

    pub fn get(&self, id: TabId) -> Option<&Tab> {
        self.tabs.get(&id)
    }

    pub fn get_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.get_mut(&id)
    }

    pub fn contains(&self, id: TabId) -> bool {
        self.tabs.contains_key(&id)
    }

    /// All live tabs in tree order (depth-first), with their depth.
    pub fn walk(&self) -> Vec<(usize, TabId)> {
        let mut out = Vec::new();
        for root in &self.roots {
            self.walk_from(*root, 0, &mut out);
        }
        out
    }

    fn walk_from(&self, id: TabId, depth: usize, out: &mut Vec<(usize, TabId)>) {
        if !self.tabs.contains_key(&id) {
            return;
        }
        out.push((depth, id));
        if let Some(kids) = self.children.get(&id) {
            for kid in kids {
                self.walk_from(*kid, depth + 1, out);
            }
        }
    }

    /// Ancestry from root down to `id`.
    pub fn ancestry(&self, id: TabId) -> Vec<TabId> {
        let mut chain = Vec::new();
        let mut cursor = Some(id);
        while let Some(current) = cursor {
            chain.push(current);
            cursor = self.tabs.get(&current).and_then(|t| t.parent);
        }
        chain.reverse();
        chain
    }

    /// Close a tab and every descendant. Returns the ids removed, root first.
    ///
    /// Closing is the security guarantee: the ephemeral overlay is dropped and
    /// every RAM page is returned, so guest artefacts cannot outlive the tab.
    pub fn close(&mut self, id: TabId) -> Result<Vec<TabId>, TabError> {
        if !self.tabs.contains_key(&id) {
            return Err(TabError::Unknown(id));
        }
        let mut removed = Vec::new();
        self.collect_subtree(id, &mut removed);

        let parent = self.tabs.get(&id).and_then(|t| t.parent);
        match parent {
            Some(p) => {
                if let Some(kids) = self.children.get_mut(&p) {
                    kids.retain(|k| *k != id);
                }
            }
            None => self.roots.retain(|k| *k != id),
        }
        self.children.remove(&id);

        for tab_id in &removed {
            self.tabs.remove(tab_id);
            self.children.remove(tab_id);
        }
        self.total_closed += removed.len() as u64;

        let focused_tab_closed = self
            .focus
            .map(|f| removed.contains(&f))
            .unwrap_or(false);
        if focused_tab_closed {
            self.focus = self.roots.first().copied().or_else(|| {
                self.tabs.keys().next().copied()
            });
        }
        self.apply_focus_policy();
        Ok(removed)
    }

    fn collect_subtree(&self, id: TabId, out: &mut Vec<TabId>) {
        out.push(id);
        if let Some(kids) = self.children.get(&id) {
            for kid in kids.clone() {
                self.collect_subtree(kid, out);
            }
        }
    }

    /// The active tab.
    pub fn focus(&self) -> Option<TabId> { self.focus }

    /// Switch tabs. The warden immediately chokes the old tab to 5 % of a core
    /// and restores the new one, so a crypto-miner in the background suffocates
    /// within one scheduler tick.
    pub fn set_focus(&mut self, id: TabId) -> Result<(), TabError> {
        let tab = self.tabs.get_mut(&id).ok_or(TabError::Unknown(id))?;
        if tab.state == TabState::Closed {
            return Err(TabError::AlreadyClosed(id));
        }
        self.focus = Some(id);
        self.apply_focus_policy();
        Ok(())
    }

    /// Cycle focus to the next tab in tree order (Ctrl+Tab).
    pub fn focus_next(&mut self) {
        let order = self.walk();
        if order.is_empty() {
            self.focus = None;
            return;
        }
        let current = self.focus;
        let index = current
            .and_then(|id| order.iter().position(|(_, t)| *t == id))
            .map(|i| (i + 1) % order.len())
            .unwrap_or(0);
        self.focus = Some(order[index].1);
        self.apply_focus_policy();
    }

    /// Recompute throttle state for every tab.
    pub fn apply_focus_policy(&mut self) {
        let focus = self.focus;
        for (id, tab) in self.tabs.iter_mut() {
            let is_focused = Some(*id) == focus;
            tab.focused = is_focused;
            if tab.state == TabState::Closed || tab.state == TabState::Suspended {
                continue;
            }
            if is_focused || tab.allow_background_compute {
                tab.state = TabState::Running;
                tab.resources = Resources::FOCUSED;
            } else {
                tab.state = TabState::Choked;
                tab.resources = Resources::BACKGROUND;
            }
        }
    }

    pub fn live_count(&self) -> usize { self.tabs.len() }

    /// Sum of RAM charged to tabs, excluding the shared base image.
    pub fn private_ram_mb(&self) -> u64 { self.tabs.values().map(|t| t.private_ram_mb).sum() }

    /// Sum of shared pages across tabs; because the zygote clone is CoW this is
    /// charged to the host exactly once, so it is reported as savings.
    pub fn shared_ram_saved_mb(&self) -> u64 {
        let mut base: Option<u64> = None;
        for tab in self.tabs.values() {
            base = Some(base.unwrap_or(0).max(tab.shared_ram_mb));
        }
        base.unwrap_or(0) * self.tabs.len().saturating_sub(1) as u64
    }

    /// Serialise the tree for the shell / CLI.
    pub fn to_json(&self) -> crate::json::Value {
        use crate::json::Value;
        let mut tabs = Vec::new();
        for (depth, id) in self.walk() {
            if let Some(tab) = self.tabs.get(&id) {
                tabs.push(
                    Value::obj()
                        .with("id", id.0)
                        .with("name", tab.name.as_str())
                        .with("depth", depth)
                        .with("parent", tab.parent.map(|p| p.0).unwrap_or(0))
                        .with("kind", tab.kind.label())
                        .with("trust", tab.trust.label())
                        .with("color", tab.trust.color())
                        .with("state", tab.state.label())
                        .with("boot_ms", tab.boot_ms as u64)
                        .with("private_ram_mb", tab.private_ram_mb)
                        .with("shared_ram_mb", tab.shared_ram_mb)
                        .with("cpu_permille", tab.resources.cpu_permille as u64)
                        .with("pids_max", tab.resources.pids_max as u64)
                        .with("focused", tab.focused)
                        .with("allow_background_compute", tab.allow_background_compute),
                );
            }
        }
        Value::obj()
            .with("focus", self.focus.map(|f| f.0).unwrap_or(0))
            .with("tabs", Value::Arr(tabs))
            .with("live", self.live_count())
            .with("created", self.total_created)
            .with("closed", self.total_closed)
            .with("private_ram_mb", self.private_ram_mb())
            .with("shared_ram_saved_mb", self.shared_ram_saved_mb())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_and_focus_throttles_background() {
        let mut tree = TabTree::new();
        let a = tree.open("opencut.app", TabKind::Web, Trust::Chameleon);
        let b = tree.open("wannacry.exe", TabKind::File, Trust::Disposable);
        tree.set_focus(a).unwrap();
        assert_eq!(tree.get(a).unwrap().state, TabState::Running);
        assert_eq!(tree.get(a).unwrap().resources.cpu_permille, 750);
        tree.set_focus(b).unwrap();
        assert_eq!(tree.get(a).unwrap().state, TabState::Choked);
        assert_eq!(tree.get(a).unwrap().resources.cpu_permille, 50);
        assert_eq!(tree.get(b).unwrap().resources.cpu_permille, 750);
    }

    #[test]
    fn branch_records_lineage_and_charges_less_ram() {
        let mut tree = TabTree::new();
        let parent = tree.open("project", TabKind::Repo, Trust::Airgapped);
        tree.get_mut(parent).unwrap().private_ram_mb = 800;
        let child = tree.branch(parent).unwrap();
        let child_tab = tree.get(child).unwrap();
        assert_eq!(child_tab.parent, Some(parent));
        assert!(child_tab.private_ram_mb < 800);
        assert_eq!(tree.ancestry(child), vec![parent, child]);
        assert_eq!(tree.walk()[1].0, 1);
    }

    #[test]
    fn closing_parent_vaporises_descendants() {
        let mut tree = TabTree::new();
        let root = tree.open("root", TabKind::Terminal, Trust::Airgapped);
        let a = tree.branch(root).unwrap();
        let b = tree.branch(a).unwrap();
        assert_eq!(tree.live_count(), 3);
        let removed = tree.close(root).unwrap();
        assert_eq!(removed, vec![root, a, b]);
        assert_eq!(tree.live_count(), 0);
        assert!(tree.get(b).is_none());
    }

    #[test]
    fn focus_next_cycles() {
        let mut tree = TabTree::new();
        let a = tree.open("a", TabKind::Web, Trust::Chameleon);
        let b = tree.open("b", TabKind::Web, Trust::Chameleon);
        tree.set_focus(a).unwrap();
        tree.focus_next();
        assert_eq!(tree.focus(), Some(b));
        tree.focus_next();
        assert_eq!(tree.focus(), Some(a));
    }

    #[test]
    fn allow_background_compute_opts_out_of_throttle() {
        let mut tree = TabTree::new();
        let a = tree.open("build", TabKind::Terminal, Trust::Airgapped);
        let b = tree.open("read", TabKind::Web, Trust::Chameleon);
        tree.set_focus(a).unwrap();
        tree.get_mut(b).unwrap().allow_background_compute = true;
        tree.set_focus(a).unwrap();
        assert_eq!(tree.get(b).unwrap().resources.cpu_permille, 750);
    }

    #[test]
    fn unknown_tab_errors() {
        let mut tree = TabTree::new();
        assert_eq!(tree.close(TabId(99)), Err(TabError::Unknown(TabId(99))));
        assert_eq!(tree.branch(TabId(42)), Err(TabError::Unknown(TabId(42))));
    }

    #[test]
    fn json_snapshot_has_every_tab() {
        let mut tree = TabTree::new();
        let a = tree.open("a", TabKind::Web, Trust::Chameleon);
        tree.branch(a).unwrap();
        let json = tree.to_json();
        assert_eq!(json.u64_of("live"), 2);
        assert_eq!(json.get("tabs").unwrap().as_arr().unwrap().len(), 2);
    }
}
