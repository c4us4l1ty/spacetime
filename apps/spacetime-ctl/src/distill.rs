//! Block 11: Operator registry, drift quarantine, protocol distillation, and self-tested skills.

use crate::memory::write_atomic;
use spacetime_types::supervisor::{OperatorHealth, OperatorRecord, SkillRecord};
use spacetime_types::{from_json_str, to_json_pretty, Result, SpacetimeError};
use std::fs;
use std::path::{Path, PathBuf};

const MIN_PROMOTION_MUTATION_BPS: u16 = 8_000;

#[derive(Debug, Clone)]
pub struct OperatorSpec<'a> {
    pub id: &'a str,
    pub precondition: &'a str,
    pub op_name: &'a str,
    pub postcondition: &'a str,
    pub verifier_id: &'a str,
    pub mutation_score_bps: u16,
    pub differential_inputs: u64,
    pub current_turn: u64,
    pub provenance: &'a str,
}

#[derive(Debug, Clone)]
pub struct DistillationStore {
    root: PathBuf,
    operators: Vec<OperatorRecord>,
    skills: Vec<SkillRecord>,
}

impl DistillationStore {
    pub fn open_or_init(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let op_path = root.join("operators.json");
        let skill_path = root.join("skills.json");
        let operators = if op_path.is_file() {
            from_json_str(&fs::read_to_string(&op_path)?)?
        } else {
            Vec::new()
        };
        let skills = if skill_path.is_file() {
            from_json_str(&fs::read_to_string(&skill_path)?)?
        } else {
            Vec::new()
        };
        Ok(Self {
            root: root.to_path_buf(),
            operators,
            skills,
        })
    }

    pub fn operators(&self) -> &[OperatorRecord] {
        &self.operators
    }

    pub fn skills(&self) -> &[SkillRecord] {
        &self.skills
    }

    /// Promote a candidate operator only if it carries a real verifier, mutation score >= 80%,
    /// and deterministic differential runs >= 100.
    pub fn register_operator(&mut self, spec: OperatorSpec<'_>) -> Result<OperatorRecord> {
        if spec.verifier_id.is_empty() {
            return Err(SpacetimeError::new(
                "operator registration requires non-empty verifier_id",
            ));
        }
        if spec.mutation_score_bps < MIN_PROMOTION_MUTATION_BPS {
            return Err(SpacetimeError::new(format!(
                "operator '{}' mutation score {} bps below 8000 bps threshold",
                spec.id, spec.mutation_score_bps
            )));
        }
        if spec.differential_inputs < 100 {
            return Err(SpacetimeError::new(format!(
                "operator '{}' requires >= 100 differential inputs",
                spec.id
            )));
        }

        let version = self
            .operators
            .iter()
            .filter(|op| op.id == spec.id)
            .map(|op| op.version)
            .max()
            .unwrap_or(0)
            + 1;
        let record = OperatorRecord {
            id: spec.id.to_string(),
            version,
            precondition: spec.precondition.to_string(),
            op_name: spec.op_name.to_string(),
            postcondition: spec.postcondition.to_string(),
            verifier_id: spec.verifier_id.to_string(),
            mutation_score_bps: spec.mutation_score_bps,
            differential_inputs: spec.differential_inputs,
            health: OperatorHealth::Healthy,
            last_green_turn: spec.current_turn,
            provenance: spec.provenance.to_string(),
        };
        self.operators.push(record.clone());
        self.persist()?;
        Ok(record)
    }

    /// Re-verify an active operator against a drift/stability probe; transitions Healthy -> Suspect
    /// -> Dead when drift or regression occurs.
    pub fn audit_operator_drift(
        &mut self,
        operator_id: &str,
        current_turn: u64,
        deterministic_pass: bool,
        observed_mutation_bps: u16,
    ) -> Result<OperatorHealth> {
        let op = self
            .operators
            .iter_mut()
            .rfind(|op| op.id == operator_id)
            .ok_or_else(|| SpacetimeError::new(format!("unknown operator: {operator_id}")))?;

        if deterministic_pass && observed_mutation_bps >= MIN_PROMOTION_MUTATION_BPS {
            op.health = OperatorHealth::Healthy;
            op.last_green_turn = current_turn;
            op.mutation_score_bps = observed_mutation_bps;
        } else {
            op.health = match op.health {
                OperatorHealth::Healthy => OperatorHealth::Suspect,
                OperatorHealth::Suspect | OperatorHealth::Dead => OperatorHealth::Dead,
            };
        }
        let health = op.health;
        self.persist()?;
        Ok(health)
    }

    /// Distill a verified workflow into a self-tested SkillRecord.
    pub fn distill_skill<F>(
        &mut self,
        id: impl Into<String>,
        test_case: impl Into<String>,
        current_turn: u64,
        provenance: impl Into<String>,
        mut self_test: F,
    ) -> Result<SkillRecord>
    where
        F: FnMut() -> Result<()>,
    {
        let id = id.into();
        let test_case = test_case.into();
        let passed = self_test().is_ok();
        if !passed {
            return Err(SpacetimeError::new(format!(
                "skill '{id}' failed self-test '{test_case}' and was not enabled"
            )));
        }
        let version = self
            .skills
            .iter()
            .filter(|s| s.id == id)
            .map(|s| s.version)
            .max()
            .unwrap_or(0)
            + 1;
        let record = SkillRecord {
            id,
            version,
            test_case,
            last_green_turn: current_turn,
            enabled: true,
            success_count: 1,
            failure_count: 0,
            provenance: provenance.into(),
        };
        self.skills.push(record.clone());
        self.persist()?;
        Ok(record)
    }

    pub fn rerun_skill_test<F>(
        &mut self,
        skill_id: &str,
        current_turn: u64,
        mut self_test: F,
    ) -> Result<bool>
    where
        F: FnMut() -> Result<()>,
    {
        let skill = self
            .skills
            .iter_mut()
            .rfind(|s| s.id == skill_id)
            .ok_or_else(|| SpacetimeError::new(format!("unknown skill: {skill_id}")))?;
        let ok = self_test().is_ok();
        if ok {
            skill.last_green_turn = current_turn;
            skill.success_count = skill.success_count.saturating_add(1);
        } else {
            skill.enabled = false;
            skill.failure_count = skill.failure_count.saturating_add(1);
        }
        self.persist()?;
        Ok(ok)
    }

    fn persist(&self) -> Result<()> {
        write_atomic(
            &self.root.join("operators.json"),
            to_json_pretty(&self.operators)?.as_bytes(),
        )?;
        write_atomic(
            &self.root.join("skills.json"),
            to_json_pretty(&self.skills)?.as_bytes(),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_drift_quarantines_and_skills_require_green_selftest() -> Result<()> {
        let dir =
            std::env::temp_dir().join(format!("spacetime-distill-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut store = DistillationStore::open_or_init(&dir)?;

        // Below mutation threshold fails promotion.
        assert!(store
            .register_operator(OperatorSpec {
                id: "op-bad",
                precondition: "pre",
                op_name: "fn",
                postcondition: "post",
                verifier_id: "verifier-1",
                mutation_score_bps: 7_000,
                differential_inputs: 200,
                current_turn: 1,
                provenance: "ledger:1",
            })
            .is_err());

        let op = store.register_operator(OperatorSpec {
            id: "op-tx-parse",
            precondition: "valid-header",
            op_name: "parse_tx",
            postcondition: "canonical-records",
            verifier_id: "protocol-state-machine-v1",
            mutation_score_bps: 9_500,
            differential_inputs: 1_024,
            current_turn: 1,
            provenance: "ledger:2",
        })?;
        assert_eq!(op.health, OperatorHealth::Healthy);

        // First drift turns Healthy -> Suspect; second turns Suspect -> Dead.
        assert_eq!(
            store.audit_operator_drift("op-tx-parse", 2, false, 6_000)?,
            OperatorHealth::Suspect
        );
        assert_eq!(
            store.audit_operator_drift("op-tx-parse", 3, false, 6_000)?,
            OperatorHealth::Dead
        );

        // Skill self-test must pass before enabling; later regression disables it.
        assert!(store
            .distill_skill("sk-bad", "test_bad", 3, "trace:bad", || Err(
                SpacetimeError::new("broken")
            ))
            .is_err());

        let skill =
            store.distill_skill("sk-upload-form", "test_upload_draft", 4, "trace:4", || Ok(()))?;
        assert!(skill.enabled);
        assert!(!store.rerun_skill_test("sk-upload-form", 5, || Err(
            SpacetimeError::new("ui selector drifted")
        ))?);
        assert!(!store.skills().last().expect("skill").enabled);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
