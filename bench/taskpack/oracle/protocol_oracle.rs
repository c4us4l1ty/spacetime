// Hidden independent oracle for the Block 3 protocol-distillation task family.
//
// This file is never shipped inside a candidate workspace. `bench/taskpack/run.sh` prepends the
// pinned ABI, appends exactly one candidate `protocol.rs`, and compiles the result; the oracle then
// drives `protocol::replay` only through the pinned ABI.
//
// The expectation is never derived from the candidate. It is the trace recording at
// `bench/taskpack/corpus/protocol_traces.tsv`, whose outcome rows were captured by an independent
// auditor before the tasks were written. A candidate that agrees with itself still disagrees with
// the recording.
//
// Output is machine-readable:
//
// ```text
// trace <name> op=<index> pass|fail <detail>
// oracle failed_traces=<n> failed=<comma-separated names>
// ```
//
// Exit status is zero only when every trace replays exactly as recorded.
//
// A trace is graded on three things, in this order:
//
//   1. the operation index at which the replay refused, if it refused;
//   2. the recorded error name at that index;
//   3. the final state, field by field, when the recording says every operation is accepted.
//
// A replay that refuses a different operation, refuses an accepted trace, or accepts a refused one
// is a distinct failure, so a candidate cannot pass a trace by being wrong about it in a way that
// happens to produce the same exit status.

use std::collections::BTreeMap;

/// One trace from the recording: its name, its operations, and its recorded outcome.
struct Trace {
    name: String,
    ops: Vec<ProtocolOp>,
    outcome: TraceExpectation,
    /// Raw `put` columns, kept so the oracle can be sure it parsed exactly what the recording says.
    raw_ops: Vec<String>,
}

fn main() {
    let path = match std::env::args().nth(1) {
        Some(path) => path,
        None => {
            eprintln!("usage: grader <protocol_traces.tsv>");
            std::process::exit(2);
        }
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("cannot read recording {path}: {error}");
            std::process::exit(2);
        }
    };

    let traces = match load_recording(&text) {
        Ok(traces) => traces,
        Err(error) => {
            eprintln!("recording error: {error}");
            std::process::exit(2);
        }
    };
    if traces.is_empty() {
        eprintln!("the recording contained no traces");
        std::process::exit(2);
    }

    let mut failures: BTreeMap<String, String> = BTreeMap::new();
    for trace in &traces {
        if let Err(detail) = grade_trace(trace) {
            failures.insert(trace.name.clone(), detail);
        }
    }

    for trace in &traces {
        match failures.get(&trace.name) {
            Some(detail) => println!(
                "trace {} fail op={} {}",
                trace.name,
                failing_index(trace),
                detail
            ),
            None => println!("trace {} pass op=- accepted as recorded", trace.name),
        }
    }

    let failed: Vec<&str> = failures.keys().map(String::as_str).collect();
    println!(
        "oracle failed_traces={} failed={}",
        failed.len(),
        failed.join(",")
    );
    if !failed.is_empty() {
        std::process::exit(1);
    }
}

/// Grade one trace against its recording.
fn grade_trace(trace: &Trace) -> Result<(), String> {
    // Re-parse the raw `put`/`begin`/... columns and require the same count, so a trace the oracle
    // mis-parsed cannot be graded as if it had been read correctly.
    let mut reparsed: Vec<ProtocolOp> = Vec::with_capacity(trace.raw_ops.len());
    for raw in &trace.raw_ops {
        match parse_operation(raw) {
            Some(operation) => reparsed.push(operation),
            None => return Err(format!("the oracle cannot re-read the recorded op {raw:?}")),
        }
    }
    if reparsed != trace.ops {
        return Err(format!(
            "the oracle parsed {} operations from {} recorded rows",
            trace.ops.len(),
            trace.raw_ops.len()
        ));
    }
    let replay = protocol::replay(&trace.ops);
    match (&trace.outcome, replay) {
        (TraceExpectation::Error { op_index, error }, Err(got)) => {
            if !REASONS.contains(&got.reason) {
                return Err(format!(
                    "refused op {} with an unpinned reason {:?}",
                    got.op_index, got.reason
                ));
            }
            if got.op_index != *op_index {
                return Err(format!(
                    "refused at op {} instead of the recorded op {} ({})",
                    got.op_index,
                    op_index,
                    got.reason
                ));
            }
            if got.reason != error.as_str() {
                return Err(format!(
                    "refused op {op_index} with {} instead of the recorded {error}",
                    got.reason
                ));
            }
            Ok(())
        }
        (
            TraceExpectation::Error {
                op_index,
                error: expected_error,
            },
            Ok(state),
        ) => Err(format!(
            "accepted the whole trace; the recording refuses op {op_index} with {expected_error} (reached {state:?})"
        )),
        (TraceExpectation::Final { state: expected }, Ok(got)) => compare_state(expected, &got),
        (TraceExpectation::Final { state: expected }, Err(got)) => Err(format!(
            "refused op {} with {}; the recording accepts every operation (expected {expected:?})",
            got.op_index, got.reason
        )),
    }
}

/// Compare field by field so the detail names the first disagreeing field.
fn compare_state(expected: &ProtocolState, got: &ProtocolState) -> Result<(), String> {
    if expected.begun != got.begun {
        return Err(format!(
            "begun is {} but the recording says {}",
            got.begun, expected.begun
        ));
    }
    if expected.committed != got.committed {
        return Err(format!(
            "committed is {} but the recording says {}",
            got.committed, expected.committed
        ));
    }
    if expected.aborted != got.aborted {
        return Err(format!(
            "aborted is {} but the recording says {}",
            got.aborted, expected.aborted
        ));
    }
    if expected.records != got.records {
        return Err(format!(
            "records are {:?} but the recording says {:?}",
            got.records, expected.records
        ));
    }
    if expected.reads != got.reads {
        return Err(format!(
            "reads is {} but the recording says {}",
            got.reads, expected.reads
        ));
    }
    Ok(())
}

/// The operation index the grader reports on the summary line.
fn failing_index(trace: &Trace) -> String {
    match &trace.outcome {
        TraceExpectation::Error { op_index, .. } => op_index.to_string(),
        TraceExpectation::Final { .. } => "-".to_string(),
    }
}

/// Read the recording, rejecting anything malformed rather than trusting it.
fn load_recording(text: &str) -> Result<Vec<Trace>, String> {
    let mut names: Vec<String> = Vec::new();
    let mut ops: BTreeMap<String, Vec<(String, ProtocolOp)>> = BTreeMap::new();
    let mut outcomes: BTreeMap<String, TraceExpectation> = BTreeMap::new();

    for (index, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, column) = split_recording_line(line)
            .ok_or_else(|| format!("row {} does not have exactly three columns", index + 1))?;
        if name.is_empty() {
            return Err(format!("row {} has no trace name", index + 1));
        }
        if !names.iter().any(|known| known == &name) {
            names.push(name.clone());
        }
        if column.starts_with("result") {
            let outcome = parse_outcome(&column)
                .ok_or_else(|| format!("row {} has an unreadable outcome", index + 1))?;
            if outcomes.insert(name.clone(), outcome).is_some() {
                return Err(format!("trace {name} has more than one outcome"));
            }
            continue;
        }
        let operation = parse_operation(&column)
            .ok_or_else(|| format!("row {} has an unreadable operation", index + 1))?;
        ops.entry(name).or_default().push((column, operation));
    }

    let mut traces = Vec::new();
    for name in names {
        let outcome = outcomes
            .remove(&name)
            .ok_or_else(|| format!("trace {name} has no outcome"))?;
        let recorded = ops.remove(&name).unwrap_or_default();
        let raw_ops: Vec<String> = recorded.iter().map(|(raw, _)| raw.clone()).collect();
        let operations: Vec<ProtocolOp> = recorded.into_iter().map(|(_, op)| op).collect();
        if let TraceExpectation::Error { op_index, .. } = &outcome {
            if *op_index >= operations.len() {
                return Err(format!(
                    "trace {name} records an error at op {op_index} but has {} operations",
                    operations.len()
                ));
            }
        }
        traces.push(Trace {
            name,
            ops: operations,
            outcome,
            raw_ops,
        });
    }
    Ok(traces)
}

fn parse_outcome(column: &str) -> Option<TraceExpectation> {
    let payload = column.strip_prefix("result ")?;
    if let Some(error) = payload.strip_prefix("error ") {
        let mut index = None;
        let mut name = None;
        for field in error.split(' ') {
            let (key, value) = field.split_once('=')?;
            match key {
                "op" => index = Some(value.parse::<usize>().ok()?),
                "error" => name = Some(value.to_string()),
                _ => return None,
            }
        }
        return Some(TraceExpectation::Error {
            op_index: index?,
            error: name?,
        });
    }
    let state = parse_state(payload)?;
    Some(TraceExpectation::Final { state })
}
