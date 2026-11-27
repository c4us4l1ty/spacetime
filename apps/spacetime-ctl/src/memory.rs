//! Block 5: Durable `.space` workspace supervisor and 3-layer memory with BM25 retrieval.
//!
//! Layer 1: Append-only immutable ledger + provenance pointers.
//! Layer 2: Derived pure-Rust BM25 keyword index (`k1 = 1.5, b = 0.75`), discardable and
//!          deterministically rebuildable from Layer 1 + `.space` records.
//! Layer 3: Structured `.space` workspace files (`goals.json`, `steering.json`, `facts.json`,
//!          `blockers.json`) written atomically via temp-file + rename so restarts resume cleanly.

use spacetime_types::supervisor::{
    BlockerItem, BlockerStatus, FactKind, FactRecord, GoalLifecycle, GoalNode, SteeringMessage,
};
use spacetime_types::{
    load_json_file_or_default, sha256_hex, to_json_pretty, Result, SpacetimeError, Taint,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryLayer {
    Layer1Ledger,
    Layer2Bm25Index,
    Layer3FactsAndGoals,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bm25Hit {
    pub doc_id: String,
    pub layer: MemoryLayer,
    pub kind: String,
    pub snippet: String,
    pub score_milli: u64,
}

#[derive(Debug, Clone)]
struct IndexedDoc {
    doc_id: String,
    kind: String,
    snippet: String,
    terms: BTreeMap<String, usize>,
    length: usize,
}

/// Pure-Rust deterministic BM25 index (Layer 2). Always rebuildable from `.space` Layer 1/3 data.
#[derive(Debug, Clone, Default)]
pub struct Bm25Index {
    docs: Vec<IndexedDoc>,
    doc_freq: BTreeMap<String, usize>,
    avg_len: f64,
}

impl Bm25Index {
    pub fn build(docs_input: &[(String, String, String)]) -> Self {
        let mut docs = Vec::with_capacity(docs_input.len());
        let mut doc_freq: BTreeMap<String, usize> = BTreeMap::new();
        let mut total_len = 0usize;

        for (doc_id, kind, text) in docs_input {
            let tokens = tokenize(text);
            let length = tokens.len();
            total_len += length;
            let mut terms: BTreeMap<String, usize> = BTreeMap::new();
            for token in tokens {
                *terms.entry(token).or_insert(0) += 1;
            }
            for term in terms.keys() {
                *doc_freq.entry(term.clone()).or_insert(0) += 1;
            }
            docs.push(IndexedDoc {
                doc_id: doc_id.clone(),
                kind: kind.clone(),
                snippet: text.clone(),
                terms,
                length,
            });
        }

        let avg_len = if docs.is_empty() {
            1.0
        } else {
            (total_len as f64) / (docs.len() as f64).max(1.0)
        };

        Self {
            docs,
            doc_freq,
            avg_len: avg_len.max(1.0),
        }
    }

    pub fn doc_count(&self) -> usize {
        self.docs.len()
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<Bm25Hit> {
        if self.docs.is_empty() || top_k == 0 {
            return Vec::new();
        }
        let query_terms: BTreeSet<String> = tokenize(query).into_iter().collect();
        if query_terms.is_empty() {
            return Vec::new();
        }

        let n = self.docs.len() as f64;
        let k1 = 1.5_f64;
        let b = 0.75_f64;
        let mut hits = Vec::new();

        for doc in &self.docs {
            let mut score = 0.0_f64;
            let dl = doc.length as f64;
            for term in &query_terms {
                let Some(&tf) = doc.terms.get(term) else {
                    continue;
                };
                let df = *self.doc_freq.get(term).unwrap_or(&0) as f64;
                let idf = (((n - df + 0.5) / (df + 0.5)) + 1.0).ln();
                let tf_f = tf as f64;
                let denom = tf_f + k1 * (1.0 - b + b * (dl / self.avg_len));
                if denom > 0.0 {
                    score += idf * ((tf_f * (k1 + 1.0)) / denom);
                }
            }
            if score > 0.0 {
                let score_milli = (score * 1_000.0).round() as u64;
                hits.push(Bm25Hit {
                    doc_id: doc.doc_id.clone(),
                    layer: MemoryLayer::Layer2Bm25Index,
                    kind: doc.kind.clone(),
                    snippet: doc.snippet.clone(),
                    score_milli,
                });
            }
        }

        hits.sort_by(|a, b_hit| {
            b_hit
                .score_milli
                .cmp(&a.score_milli)
                .then_with(|| a.doc_id.cmp(&b_hit.doc_id))
        });
        hits.truncate(top_k);
        hits
    }
}

fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[derive(Debug, Clone)]
pub struct SpaceSupervisor {
    root: PathBuf,
    current_turn: u64,
    goals: Vec<GoalNode>,
    steering: Vec<SteeringMessage>,
    facts: Vec<FactRecord>,
    blockers: Vec<BlockerItem>,
    bm25: Bm25Index,
}

impl SpaceSupervisor {
    pub fn open_or_init(space_dir: &Path) -> Result<Self> {
        fs::create_dir_all(space_dir)?;
        let turn_path = space_dir.join("turn.txt");
        let current_turn = if turn_path.is_file() {
            fs::read_to_string(&turn_path)?.trim().parse::<u64>()?
        } else {
            0
        };
        let goals = load_json_file_or_default(&space_dir.join("goals.json"))?;
        let steering = load_json_file_or_default(&space_dir.join("steering.json"))?;
        let facts = load_json_file_or_default(&space_dir.join("facts.json"))?;
        let blockers = load_json_file_or_default(&space_dir.join("blockers.json"))?;

        let mut supervisor = Self {
            root: space_dir.to_path_buf(),
            current_turn,
            goals,
            steering,
            facts,
            blockers,
            bm25: Bm25Index::default(),
        };
        supervisor.rebuild_bm25_index();
        supervisor.persist()?;
        Ok(supervisor)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn current_turn(&self) -> u64 {
        self.current_turn
    }

    pub fn advance_turn(&mut self) -> Result<u64> {
        self.current_turn = self.current_turn.saturating_add(1);
        for goal in &mut self.goals {
            if matches!(
                goal.status,
                GoalLifecycle::Open | GoalLifecycle::InProgress
            ) {
                goal.turns_elapsed = goal.turns_elapsed.saturating_add(1);
                if goal.turns_elapsed >= goal.deadline_turns
                    || goal.tokens_spent > goal.budget_tokens
                {
                    goal.status = GoalLifecycle::Dead;
                }
            }
        }
        // Expire TTL-bounded steering and facts cleanly on turn advance.
        let turn = self.current_turn;
        self.steering.retain(|item| !item.is_expired(turn));
        self.facts.retain(|fact| !fact.is_expired(turn));
        self.rebuild_bm25_index();
        self.persist()?;
        Ok(self.current_turn)
    }

    pub fn add_goal(
        &mut self,
        id: impl Into<String>,
        parent: Option<String>,
        title: impl Into<String>,
        budget_tokens: u64,
        kill_metric: impl Into<String>,
        deadline_turns: u64,
    ) -> Result<()> {
        let id = id.into();
        if self.goals.iter().any(|g| g.id == id) {
            return Err(SpacetimeError::new(format!("duplicate goal id: {id}")));
        }
        self.goals.push(GoalNode {
            id,
            parent,
            title: title.into(),
            budget_tokens,
            tokens_spent: 0,
            kill_metric: kill_metric.into(),
            deadline_turns,
            turns_elapsed: 0,
            status: GoalLifecycle::InProgress,
        });
        self.rebuild_bm25_index();
        self.persist()
    }

    pub fn set_goal_status(&mut self, goal_id: &str, status: GoalLifecycle) -> Result<()> {
        let goal = self
            .goals
            .iter_mut()
            .find(|g| g.id == goal_id)
            .ok_or_else(|| SpacetimeError::new(format!("unknown goal: {goal_id}")))?;
        goal.status = status;
        self.persist()
    }

    pub fn push_steering(
        &mut self,
        id: impl Into<String>,
        text: impl Into<String>,
        ttl_turns: u64,
    ) -> Result<()> {
        self.steering.push(SteeringMessage {
            id: id.into(),
            text: text.into(),
            created_turn: self.current_turn,
            ttl_turns: ttl_turns.max(1),
        });
        self.rebuild_bm25_index();
        self.persist()
    }

    pub fn active_steering(&self) -> Vec<&SteeringMessage> {
        self.steering
            .iter()
            .filter(|m| !m.is_expired(self.current_turn))
            .collect()
    }

    pub fn record_fact(
        &mut self,
        id: impl Into<String>,
        subject: impl Into<String>,
        kind: FactKind,
        reason: impl Into<String>,
        evidence_pointer: impl Into<String>,
        ttl_turns: u64,
    ) -> Result<()> {
        let id = id.into();
        self.facts.retain(|f| f.id != id);
        self.facts.push(FactRecord {
            id,
            subject: subject.into(),
            kind,
            reason: reason.into(),
            evidence_pointer: evidence_pointer.into(),
            created_turn: self.current_turn,
            ttl_turns: ttl_turns.max(1),
        });
        self.rebuild_bm25_index();
        self.persist()
    }

    pub fn queue_blocker(
        &mut self,
        id: impl Into<String>,
        goal_id: impl Into<String>,
        effect: impl Into<String>,
        target: impl Into<String>,
        reason: impl Into<String>,
        taint: Taint,
    ) -> Result<()> {
        let id = id.into();
        if self.blockers.iter().any(|b| b.id == id) {
            return Err(SpacetimeError::new(format!("duplicate blocker id: {id}")));
        }
        self.blockers.push(BlockerItem {
            id,
            goal_id: goal_id.into(),
            action_op_kind: "irreversible_effect".to_string(),
            effect: effect.into(),
            target: target.into(),
            reason: reason.into(),
            taint,
            created_turn: self.current_turn,
            status: BlockerStatus::Pending,
        });
        self.persist()
    }

    pub fn resolve_blocker(&mut self, blocker_id: &str, approve: bool) -> Result<()> {
        let blocker = self
            .blockers
            .iter_mut()
            .find(|b| b.id == blocker_id)
            .ok_or_else(|| SpacetimeError::new(format!("unknown blocker: {blocker_id}")))?;
        blocker.status = if approve {
            BlockerStatus::Approved
        } else {
            BlockerStatus::Denied
        };
        self.persist()
    }

    pub fn goals(&self) -> &[GoalNode] {
        &self.goals
    }

    pub fn facts(&self) -> &[FactRecord] {
        &self.facts
    }

    pub fn blockers(&self) -> &[BlockerItem] {
        &self.blockers
    }

    pub fn pending_blockers(&self) -> Vec<&BlockerItem> {
        self.blockers
            .iter()
            .filter(|b| b.status == BlockerStatus::Pending)
            .collect()
    }

    /// Discard and rebuild Layer 2 BM25 index strictly from Layer 3 `.space` records.
    pub fn rebuild_bm25_index(&mut self) {
        let mut docs = Vec::new();
        for goal in &self.goals {
            docs.push((
                format!("goal:{}", goal.id),
                "goal".to_string(),
                format!("{} {} {}", goal.id, goal.title, goal.kill_metric),
            ));
        }
        for fact in &self.facts {
            if fact.is_expired(self.current_turn) {
                continue;
            }
            let kind_label = match fact.kind {
                FactKind::Positive => "fact_positive",
                FactKind::Dead => "fact_dead",
            };
            docs.push((
                format!("fact:{}", fact.id),
                kind_label.to_string(),
                format!(
                    "{} {} {} {}",
                    fact.subject, fact.reason, fact.evidence_pointer, kind_label
                ),
            ));
        }
        for msg in &self.steering {
            if !msg.is_expired(self.current_turn) {
                docs.push((
                    format!("steering:{}", msg.id),
                    "steering".to_string(),
                    msg.text.clone(),
                ));
            }
        }
        self.bm25 = Bm25Index::build(&docs);
    }

    pub fn drop_and_rebuild_index(&mut self) -> usize {
        self.bm25 = Bm25Index::default();
        self.rebuild_bm25_index();
        self.bm25.doc_count()
    }

    pub fn retrieve_memory(&self, query: &str, top_k: usize) -> Vec<Bm25Hit> {
        self.bm25.search(query, top_k)
    }

    pub fn state_digest(&self) -> Result<String> {
        let material = to_json_pretty(&(
            self.current_turn,
            &self.goals,
            &self.steering,
            &self.facts,
            &self.blockers,
        ))?;
        Ok(sha256_hex(material.as_bytes()))
    }

    fn persist(&self) -> Result<()> {
        write_atomic(&self.root.join("turn.txt"), format!("{}\n", self.current_turn).as_bytes())?;
        write_atomic(
            &self.root.join("goals.json"),
            to_json_pretty(&self.goals)?.as_bytes(),
        )?;
        write_atomic(
            &self.root.join("steering.json"),
            to_json_pretty(&self.steering)?.as_bytes(),
        )?;
        write_atomic(
            &self.root.join("facts.json"),
            to_json_pretty(&self.facts)?.as_bytes(),
        )?;
        write_atomic(
            &self.root.join("blockers.json"),
            to_json_pretty(&self.blockers)?.as_bytes(),
        )?;
        Ok(())
    }
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| SpacetimeError::new("target path has no parent directory"))?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("state"),
        std::process::id()
    ));
    {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_supervisor_persists_resumes_and_rebuilds_bm25_memory() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("spacetime-space-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        {
            let mut sup = SpaceSupervisor::open_or_init(&dir)?;
            sup.add_goal(
                "g-upload",
                None,
                "Upload video draft safely",
                5_000,
                "verify_upload_draft",
                5,
            )?;
            sup.push_steering("steer-1", "Use dry run mock account only", 3)?;
            sup.record_fact(
                "fact-1",
                "mock_upload_button",
                FactKind::Positive,
                "Publish button requires human blocker approval",
                "ledger:event-0001",
                10,
            )?;
            sup.record_fact(
                "dead-1",
                "password_screenshot_pass",
                FactKind::Dead,
                "Raw screenshots forbidden when secret fields exist",
                "policy:ring-3",
                2,
            )?;
            sup.queue_blocker(
                "blk-1",
                "g-upload",
                "publish_video",
                "mock-channel",
                "Publish requires human approval",
                Taint::UntrustedToolOutput,
            )?;
            let hits = sup.retrieve_memory("publish approval blocker", 3);
            assert!(!hits.is_empty());
            assert_eq!(hits[0].doc_id, "fact:fact-1");
        }

        // Reopen from disk and confirm state survived restart & BM25 rebuild works.
        let mut resumed = SpaceSupervisor::open_or_init(&dir)?;
        assert_eq!(resumed.goals().len(), 1);
        assert_eq!(resumed.pending_blockers().len(), 1);
        let doc_count = resumed.drop_and_rebuild_index();
        assert_eq!(doc_count, 4);
        assert_eq!(
            resumed.retrieve_memory("screenshot secret forbidden", 2)[0].doc_id,
            "fact:dead-1"
        );

        // Advancing 2 turns expires dead-1 (ttl=2) without deleting fact-1 (ttl=10).
        resumed.advance_turn()?;
        resumed.advance_turn()?;
        assert_eq!(resumed.facts().len(), 1);
        assert_eq!(resumed.facts()[0].id, "fact-1");
        resumed.resolve_blocker("blk-1", true)?;
        assert!(resumed.pending_blockers().is_empty());

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
