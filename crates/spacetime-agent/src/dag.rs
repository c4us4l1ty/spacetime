//! Externalised goal state: the Goal DAG.
//!
//! A long-running mission ("build a crypto tracker, publish the site, cut a promo
//! video") is decomposed into a DAG of atomic tasks. Only the DAG is durable;
//! model context is disposable. Each heartbeat pulls exactly one ready node,
//! acts, verifies, and writes the result back. If the model forgets everything
//! between ticks, the mission still progresses.

use std::collections::{BTreeMap, BTreeSet};

use spacetime_core::json::Value;

/// Node identifier (stable, display-friendly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "n{}", self.0)
    }
}

/// Lifecycle of one task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
    Pending,
    Ready,
    Active,
    Done,
    Failed,
    /// Skipped after a human steering directive or three failures.
    Abandoned,
}

impl NodeState {
    pub fn label(self) -> &'static str {
        match self {
            NodeState::Pending => "pending",
            NodeState::Ready => "ready",
            NodeState::Active => "active",
            NodeState::Done => "done",
            NodeState::Failed => "failed",
            NodeState::Abandoned => "abandoned",
        }
    }
}

/// One atomic task.
#[derive(Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    /// Imperative description, e.g. "Import clips from /storage/X".
    pub title: String,
    pub state: NodeState,
    /// Nodes that must be `Done` first.
    pub deps: Vec<NodeId>,
    /// Child nodes (sub-goals). The mission tree the UI draws.
    pub children: Vec<NodeId>,
    pub attempts: u32,
    /// Notes written by the agent after acting (verification evidence).
    pub note: String,
    /// Number of heartbeat ticks spent on this node.
    pub ticks: u32,
}

/// Errors from DAG edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DagError {
    Unknown(NodeId),
    Cycle,
    NotReady(NodeId),
}

impl std::fmt::Display for DagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DagError::Unknown(id) => write!(f, "unknown node {id}"),
            DagError::Cycle => write!(f, "edit would create a cycle"),
            DagError::NotReady(id) => write!(f, "node {id} is not ready"),
        }
    }
}

impl std::error::Error for DagError {}

/// The mission graph.
#[derive(Debug, Clone)]
pub struct GoalDag {
    mission: String,
    nodes: BTreeMap<NodeId, Node>,
    roots: Vec<NodeId>,
    next_id: u64,
    /// Index of the node currently being executed.
    active: Option<NodeId>,
}

impl GoalDag {
    pub fn new(mission: &str) -> GoalDag {
        GoalDag {
            mission: mission.to_string(),
            nodes: BTreeMap::new(),
            roots: Vec::new(),
            next_id: 1,
            active: None,
        }
    }

    pub fn mission(&self) -> &str { &self.mission }

    pub fn active(&self) -> Option<NodeId> { self.active }

    /// Add a root task.
    pub fn add_root(&mut self, title: &str) -> NodeId {
        let id = self.insert(title, Vec::new());
        self.roots.push(id);
        id
    }

    /// Add a child task under `parent`, depending on `deps`.
    pub fn add_child(
        &mut self,
        parent: NodeId,
        title: &str,
        deps: Vec<NodeId>,
    ) -> Result<NodeId, DagError> {
        if !self.nodes.contains_key(&parent) {
            return Err(DagError::Unknown(parent));
        }
        for dep in &deps {
            if !self.nodes.contains_key(dep) {
                return Err(DagError::Unknown(*dep));
            }
        }
        let id = self.insert(title, deps);
        self.nodes.get_mut(&parent).unwrap().children.push(id);
        Ok(id)
    }

    fn insert(&mut self, title: &str, deps: Vec<NodeId>) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        let state = if deps.is_empty() {
            NodeState::Ready
        } else {
            NodeState::Pending
        };
        self.nodes.insert(
            id,
            Node {
                id,
                title: title.to_string(),
                state,
                deps,
                children: Vec::new(),
                attempts: 0,
                note: String::new(),
                ticks: 0,
            },
        );
        id
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn len(&self) -> usize { self.nodes.len() }

    pub fn is_empty(&self) -> bool { self.nodes.is_empty() }

    /// Recompute `Pending → Ready` for every node whose deps are done.
    /// Also demotes `Ready → Pending` if a dependency was re-opened by a rewind.
    pub fn refresh(&mut self) {
        // Snapshot the completed set first: `values_mut()` holds a mutable borrow
        // of `self.nodes`, so dependency lookups must not touch it in the loop.
        let done: BTreeSet<NodeId> = self
            .nodes
            .iter()
            .filter(|(_, node)| node.state == NodeState::Done)
            .map(|(id, _)| *id)
            .collect();
        for node in self.nodes.values_mut() {
            if matches!(
                node.state,
                NodeState::Done | NodeState::Failed | NodeState::Abandoned | NodeState::Active
            ) {
                continue;
            }
            let deps_done = node.deps.iter().all(|dep| done.contains(dep));
            node.state = if deps_done {
                NodeState::Ready
            } else {
                NodeState::Pending
            };
        }
    }

    /// Pull the next task: first ready node in insertion order.
    pub fn pull_ready(&mut self) -> Option<NodeId> {
        self.refresh();
        let next = self
            .nodes
            .values()
            .find(|n| n.state == NodeState::Ready)
            .map(|n| n.id);
        if let Some(id) = next {
            self.nodes.get_mut(&id).unwrap().state = NodeState::Active;
            self.nodes.get_mut(&id).unwrap().attempts += 1;
            self.nodes.get_mut(&id).unwrap().ticks += 1;
            self.active = Some(id);
        }
        next
    }

    /// Count one tick against the active node.
    pub fn tick_active(&mut self) {
        if let Some(id) = self.active {
            if let Some(node) = self.nodes.get_mut(&id) {
                node.ticks += 1;
            }
        }
    }

    /// Mark the active node complete.
    pub fn complete_active(&mut self, note: &str) -> Option<NodeId> {
        self.finish(self.active?, NodeState::Done, note)
    }

    /// Mark the active node failed (it will retry up to `MAX_ATTEMPTS`).
    pub fn fail_active(&mut self, note: &str) -> Option<NodeId> {
        let id = self.active?;
        let attempts = self.nodes.get(&id)?.attempts;
        let state = if attempts >= MAX_ATTEMPTS {
            NodeState::Abandoned
        } else {
            NodeState::Pending
        };
        self.finish(id, state, note)
    }

    fn finish(&mut self, id: NodeId, state: NodeState, note: &str) -> Option<NodeId> {
        let node = self.nodes.get_mut(&id)?;
        node.state = state;
        node.note = note.to_string();
        if self.active == Some(id) {
            self.active = None;
        }
        // A retryable failure must stay `Pending` until the next `pull_ready`
        // call; refreshing here would immediately promote it back to `Ready`
        // (its dependency set is empty) and hide the failed attempt.
        if state != NodeState::Pending {
            self.refresh();
        }
        Some(id)
    }

    /// Rewind every node touched since a snapshot: reopen the active/done ones.
    ///
    /// Used by the heartbeat's auto-rollback: if an action broke the environment,
    /// the DAG must not believe the step succeeded.
    pub fn rollback_active(&mut self, reason: &str) {
        if let Some(id) = self.active.take() {
            if let Some(node) = self.nodes.get_mut(&id) {
                node.state = NodeState::Pending;
                node.note = format!("auto-rewound: {reason}");
            }
        }
        self.refresh();
    }

    /// Human steering: replace the active node's title, or add a constraint node.
    pub fn steer_active(&mut self, directive: &str) {
        match self.active {
            Some(id) => {
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.note = format!("steer: {directive}");
                }
            }
            None => {
                let id = self.add_root(directive);
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.state = NodeState::Ready;
                }
            }
        }
    }

    /// Delete a planned (not yet started) node — the UI lets users prune the tree.
    pub fn remove(&mut self, id: NodeId) -> Result<(), DagError> {
        match self.nodes.get(&id) {
            None => Err(DagError::Unknown(id)),
            Some(node) if matches!(node.state, NodeState::Active | NodeState::Done) => {
                Err(DagError::NotReady(id))
            }
            Some(_) => {
                self.nodes.remove(&id);
                self.roots.retain(|r| *r != id);
                for other in self.nodes.values_mut() {
                    other.children.retain(|c| *c != id);
                    other.deps.retain(|d| *d != id);
                }
                Ok(())
            }
        }
    }

    pub fn is_complete(&self) -> bool {
        self.nodes
            .values()
            .all(|n| matches!(n.state, NodeState::Done | NodeState::Abandoned))
    }

    /// Progress for the "context freshness" gauge: fraction of nodes done.
    pub fn progress(&self) -> f64 {
        if self.nodes.is_empty() {
            return 0.0;
        }
        let done = self
            .nodes
            .values()
            .filter(|n| n.state == NodeState::Done)
            .count();
        done as f64 / self.nodes.len() as f64
    }

    /// One-line history: `[x] build site · [>] deploy · [ ] announce`.
    pub fn summary(&self, limit: usize) -> String {
        let mut parts = Vec::new();
        for id in self.roots.iter().take(limit) {
            if let Some(node) = self.nodes.get(id) {
                let mark = match node.state {
                    NodeState::Done => "[x]",
                    NodeState::Active | NodeState::Ready => "[>]",
                    NodeState::Failed | NodeState::Abandoned => "[!]",
                    _ => "[ ]",
                };
                parts.push(format!("{mark} {}", node.title));
            }
        }
        parts.join(" · ")
    }

    /// JSON tree for the DAG visualiser.
    pub fn to_json(&self) -> Value {
        let mut roots = Vec::new();
        for id in &self.roots {
            roots.push(self.node_json(*id));
        }
        Value::obj()
            .with("mission", self.mission.as_str())
            .with("active", self.active.map(|a| a.0).unwrap_or(0))
            .with("progress", self.progress())
            .with("complete", self.is_complete())
            .with("nodes", Value::Arr(roots))
    }

    fn node_json(&self, id: NodeId) -> Value {
        let node = self.nodes.get(&id).cloned().unwrap_or(Node {
            id,
            title: String::new(),
            state: NodeState::Pending,
            deps: Vec::new(),
            children: Vec::new(),
            attempts: 0,
            note: String::new(),
            ticks: 0,
        });
        let children: Vec<Value> = node.children.iter().map(|c| self.node_json(*c)).collect();
        Value::obj()
            .with("id", id.0)
            .with("title", node.title.as_str())
            .with("state", node.state.label())
            .with("note", node.note.as_str())
            .with("attempts", node.attempts as u64)
            .with("ticks", node.ticks as u64)
            .with("children", Value::Arr(children))
    }
}

/// A node is abandoned after this many failed attempts.
pub const MAX_ATTEMPTS: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulls_in_dependency_order() {
        let mut dag = GoalDag::new("ship a video");
        let a = dag.add_root("import clips");
        let b = dag.add_child(a, "cut silence", vec![a]).unwrap();
        let _ = dag.add_child(a, "export mp4", vec![b]).unwrap();

        assert_eq!(dag.pull_ready(), Some(a));
        dag.complete_active("3 clips imported");
        assert_eq!(dag.pull_ready(), Some(b));
        dag.complete_active("removed 12 s of silence");
        let third = dag.pull_ready().expect("export should be ready");
        assert_eq!(dag.get(third).unwrap().title, "export mp4");
    }

    #[test]
    fn failures_retry_then_abandon() {
        let mut dag = GoalDag::new("x");
        let a = dag.add_root("flaky step");
        dag.pull_ready();
        dag.fail_active("crashed");
        assert_eq!(dag.get(a).unwrap().state, NodeState::Pending);
        dag.pull_ready();
        dag.fail_active("crashed");
        dag.pull_ready();
        dag.fail_active("crashed");
        assert_eq!(dag.get(a).unwrap().state, NodeState::Abandoned);
    }

    #[test]
    fn rollback_reopens_active_node() {
        let mut dag = GoalDag::new("x");
        let a = dag.add_root("risky step");
        dag.pull_ready();
        dag.rollback_active("kernel panic");
        assert_eq!(dag.get(a).unwrap().state, NodeState::Ready);
        assert!(dag.get(a).unwrap().note.contains("auto-rewound"));
    }

    #[test]
    fn steer_appends_or_annotates() {
        // With no active node, a directive becomes a new root task.
        let mut dag = GoalDag::new("x");
        dag.steer_active("use dark mode");
        assert!(dag.summary(4).contains("use dark mode"));

        // With an active node, a directive annotates that node instead. Use a
        // fresh graph so `a` really is the node that gets pulled.
        let mut dag = GoalDag::new("x");
        let a = dag.add_root("active task");
        dag.pull_ready();
        dag.steer_active("9:16 vertical");
        assert!(dag.get(a).unwrap().note.contains("9:16"));
    }

    #[test]
    fn progress_and_completion() {
        let mut dag = GoalDag::new("x");
        dag.add_root("one");
        dag.add_root("two");
        assert_eq!(dag.progress(), 0.0);
        dag.pull_ready();
        dag.complete_active("done");
        assert!((dag.progress() - 0.5).abs() < f64::EPSILON);
        assert!(!dag.is_complete());
        dag.pull_ready();
        dag.complete_active("done");
        assert!(dag.is_complete());
    }

    #[test]
    fn remove_prunes_planned_nodes() {
        let mut dag = GoalDag::new("x");
        let a = dag.add_root("keep");
        let b = dag.add_root("drop");
        // `a` is the active node now; running nodes cannot be pruned.
        dag.pull_ready();
        assert_eq!(dag.remove(a), Err(DagError::NotReady(a)));
        // `b` is still only planned, so the UI may delete it.
        dag.remove(b).unwrap();
        assert_eq!(dag.len(), 1);
    }

    #[test]
    fn json_tree_nests_children() {
        let mut dag = GoalDag::new("m");
        let a = dag.add_root("root");
        dag.add_child(a, "kid", vec![]).unwrap();
        let json = dag.to_json();
        let roots = json.get("nodes").unwrap().as_arr().unwrap();
        assert_eq!(roots[0].get("children").unwrap().as_arr().unwrap().len(), 1);
    }
}
