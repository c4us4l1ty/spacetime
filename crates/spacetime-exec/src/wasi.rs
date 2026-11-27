//! Tier 1 constrained WASI-contract worker.
//!
//! Pure transforms, parsers, and narrow verifiers run under strict fuel, memory, path, and
//! network limits. This module uses a fuel-metered pure-Rust bytecode interpreter behind a
//! `WasiRuntime` trait so an external Wasmtime runtime can drop in later without changing callers
//! or running untrusted code inside the UI process.

use spacetime_types::{sha256_hex, Result, SpacetimeError};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasiWorkerSpec {
    pub worker_id: String,
    pub max_memory_bytes: usize,
    pub max_fuel: u64,
    pub timeout_millis: u64,
    pub allowed_readonly_paths: BTreeSet<PathBuf>,
    pub allowed_output_paths: BTreeSet<PathBuf>,
}

impl WasiWorkerSpec {
    pub fn strict_default(worker_id: impl Into<String>) -> Self {
        Self {
            worker_id: worker_id.into(),
            max_memory_bytes: 16 * 1024 * 1024,
            max_fuel: 100_000,
            timeout_millis: 500,
            allowed_readonly_paths: BTreeSet::new(),
            allowed_output_paths: BTreeSet::new(),
        }
    }

    pub fn validate_read_path(&self, path: &Path) -> Result<()> {
        if has_traversal(path) || !self.allowed_readonly_paths.contains(path) {
            return Err(SpacetimeError::new(format!(
                "wasi worker denied undeclared read path: {}",
                path.display()
            )));
        }
        Ok(())
    }

    pub fn validate_write_path(&self, path: &Path) -> Result<()> {
        if has_traversal(path) || !self.allowed_output_paths.contains(path) {
            return Err(SpacetimeError::new(format!(
                "wasi worker denied undeclared write path: {}",
                path.display()
            )));
        }
        Ok(())
    }
}

fn has_traversal(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasiInstruction {
    PushU64(u64),
    AddU64,
    MulU64,
    XorU64,
    ShlU64(u32),
    ReadInputByte(usize),
    AllocScratch(usize),
    OpenSocket,
    OpenPath(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasiOutcome {
    pub result_u64: u64,
    pub digest: String,
    pub fuel_consumed: u64,
    pub memory_allocated_bytes: usize,
}

pub trait WasiRuntime {
    fn execute(
        &self,
        spec: &WasiWorkerSpec,
        program: &[WasiInstruction],
        input: &[u8],
    ) -> Result<WasiOutcome>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ConstrainedWasiWorker;

impl WasiRuntime for ConstrainedWasiWorker {
    fn execute(
        &self,
        spec: &WasiWorkerSpec,
        program: &[WasiInstruction],
        input: &[u8],
    ) -> Result<WasiOutcome> {
        let mut stack: Vec<u64> = Vec::with_capacity(64);
        let mut fuel_consumed: u64 = 0;
        let mut memory_allocated = input.len();
        if memory_allocated > spec.max_memory_bytes {
            return Err(SpacetimeError::new("wasi input exceeds memory budget"));
        }

        for instruction in program {
            fuel_consumed = fuel_consumed.saturating_add(1);
            if fuel_consumed > spec.max_fuel {
                return Err(SpacetimeError::new("wasi worker fuel exhausted"));
            }
            match instruction {
                WasiInstruction::PushU64(value) => {
                    if stack.len() >= 1024 {
                        return Err(SpacetimeError::new("wasi operand stack overflow"));
                    }
                    stack.push(*value);
                }
                WasiInstruction::AddU64 => {
                    let right = pop(&mut stack)?;
                    let left = pop(&mut stack)?;
                    stack.push(left.wrapping_add(right));
                }
                WasiInstruction::MulU64 => {
                    let right = pop(&mut stack)?;
                    let left = pop(&mut stack)?;
                    stack.push(left.wrapping_mul(right));
                }
                WasiInstruction::XorU64 => {
                    let right = pop(&mut stack)?;
                    let left = pop(&mut stack)?;
                    stack.push(left ^ right);
                }
                WasiInstruction::ShlU64(shift) => {
                    let value = pop(&mut stack)?;
                    stack.push(value.wrapping_shl(*shift % 64));
                }
                WasiInstruction::ReadInputByte(index) => {
                    let byte = input
                        .get(*index)
                        .copied()
                        .ok_or_else(|| SpacetimeError::new("wasi input read out of bounds"))?;
                    stack.push(u64::from(byte));
                }
                WasiInstruction::AllocScratch(bytes) => {
                    memory_allocated = memory_allocated
                        .checked_add(*bytes)
                        .ok_or_else(|| SpacetimeError::new("wasi memory overflow"))?;
                    if memory_allocated > spec.max_memory_bytes {
                        return Err(SpacetimeError::new("wasi worker memory budget exceeded"));
                    }
                }
                WasiInstruction::OpenSocket => {
                    return Err(SpacetimeError::new(
                        "wasi worker has no network socket capability",
                    ));
                }
                WasiInstruction::OpenPath(path) => {
                    spec.validate_read_path(path)?;
                }
            }
        }

        let result_u64 = stack.pop().unwrap_or(0);
        let digest = sha256_hex(&result_u64.to_le_bytes());
        Ok(WasiOutcome {
            result_u64,
            digest,
            fuel_consumed,
            memory_allocated_bytes: memory_allocated,
        })
    }
}

fn pop(stack: &mut Vec<u64>) -> Result<u64> {
    stack
        .pop()
        .ok_or_else(|| SpacetimeError::new("wasi stack underflow"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasi_worker_executes_deterministic_transform_and_denies_undeclared_access() -> Result<()> {
        let mut spec = WasiWorkerSpec::strict_default("wasi-1");
        spec.max_fuel = 10;
        spec.max_memory_bytes = 1024;
        spec.allowed_readonly_paths
            .insert(PathBuf::from("/inputs/data.bin"));

        let worker = ConstrainedWasiWorker;
        let program = [
            WasiInstruction::OpenPath(PathBuf::from("/inputs/data.bin")),
            WasiInstruction::PushU64(5),
            WasiInstruction::PushU64(3),
            WasiInstruction::MulU64,
            WasiInstruction::PushU64(7),
            WasiInstruction::AddU64,
        ];
        let outcome = worker.execute(&spec, &program, b"abc")?;
        assert_eq!(outcome.result_u64, 22);
        assert_eq!(outcome.fuel_consumed, 6);

        assert!(worker
            .execute(&spec, &[WasiInstruction::OpenSocket], b"")
            .is_err());
        assert!(worker
            .execute(
                &spec,
                &[WasiInstruction::OpenPath(PathBuf::from("/etc/passwd"))],
                b"",
            )
            .is_err());
        assert!(worker
            .execute(&spec, &[WasiInstruction::AllocScratch(2048)], b"")
            .is_err());

        let too_long = vec![WasiInstruction::PushU64(1); 11];
        assert!(worker.execute(&spec, &too_long, b"").is_err());
        Ok(())
    }
}
