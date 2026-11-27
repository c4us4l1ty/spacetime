//! Durable supervisor, goal tree, steering, blockers, operators, facts, and skills types.

use crate::Taint;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalLifecycle {
    Open,
    InProgress,
    Verified,
    Blocked,
    Dead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalNode {
    pub id: String,
    pub parent: Option<String>,
    pub title: String,
    pub budget_tokens: u64,
    pub tokens_spent: u64,
    pub kill_metric: String,
    pub deadline_turns: u64,
    pub turns_elapsed: u64,
    pub status: GoalLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SteeringMessage {
    pub id: String,
    pub text: String,
    pub created_turn: u64,
    pub ttl_turns: u64,
}

impl SteeringMessage {
    pub fn is_expired(&self, current_turn: u64) -> bool {
        current_turn >= self.created_turn.saturating_add(self.ttl_turns)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockerStatus {
    Pending,
    Approved,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockerItem {
    pub id: String,
    pub goal_id: String,
    pub action_op_kind: String,
    pub effect: String,
    pub target: String,
    pub reason: String,
    pub taint: Taint,
    pub created_turn: u64,
    pub status: BlockerStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorHealth {
    Healthy,
    Suspect,
    Dead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorRecord {
    pub id: String,
    pub version: u32,
    pub precondition: String,
    pub op_name: String,
    pub postcondition: String,
    pub verifier_id: String,
    pub mutation_score_bps: u16,
    pub differential_inputs: u64,
    pub health: OperatorHealth,
    pub last_green_turn: u64,
    pub provenance: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    Positive,
    Dead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactRecord {
    pub id: String,
    pub subject: String,
    pub kind: FactKind,
    pub reason: String,
    pub evidence_pointer: String,
    pub created_turn: u64,
    pub ttl_turns: u64,
}

impl FactRecord {
    pub fn is_expired(&self, current_turn: u64) -> bool {
        current_turn >= self.created_turn.saturating_add(self.ttl_turns)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillRecord {
    pub id: String,
    pub version: u32,
    pub test_case: String,
    pub last_green_turn: u64,
    pub enabled: bool,
    pub success_count: u64,
    pub failure_count: u64,
    pub provenance: String,
}
