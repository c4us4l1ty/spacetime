// CANDIDATE PROTOCOL ABI — pinned for the Block 3 protocol-distillation task family.
//
// `bench/taskpack/assemble.sh` prepends this file to every protocol candidate, reference, and
// oracle copy, so the contract cannot drift between them. It models a tiny write-ahead commit
// protocol: no storage engine, no sockets, no threads, no persistence.
//
// The task family is *trace replay*. A captured trace is a list of protocol operations. The
// candidate replays the trace and reports what the service did. The expectation is never derived
// from the candidate: it comes from the trace's own recorded outcome, which was captured before the
// task was written.
//
// ## Required candidate surface
//
// A candidate `protocol` module must expose exactly this item:
//
// ```text
// pub fn replay(operations: &[ProtocolOp]) -> Result<ProtocolState, ProtocolError>
// ```
//
// `ProtocolState` is pinned here rather than by the candidate, because the grader parses a state
// out of the recording and compares it with the replayed one. Pinning it also stops a candidate
// from widening the state so that a wrong replay still compares equal.

/// The state a replayed trace reaches. Pinned, not candidate-defined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolState {
    /// A transaction is open.
    pub begun: bool,
    /// A transaction has committed.
    pub committed: bool,
    /// A transaction has aborted.
    pub aborted: bool,
    /// Records appended by the open or committed transaction, in sequence order.
    pub records: Vec<String>,
    /// How many committed reads the trace performed.
    pub reads: u64,
}

/// One protocol operation, exactly as captured in a trace recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolOp {
    /// Open a transaction. Rejected unless no transaction is open and none has finished.
    Begin,
    /// Append `value` at `sequence`. Rejected unless a transaction is open and `sequence` is the
    /// next free index.
    Put { sequence: usize, value: String },
    /// Close the open transaction. Rejected unless a transaction is open.
    Commit,
    /// Discard the open transaction and its records. Rejected unless a transaction is open.
    Abort,
    /// Read the committed integer payload. Rejected unless a transaction has committed and none
    /// has aborted.
    Read,
}

/// Why a replay refused to apply an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    /// The operation is not legal in the current state.
    InvalidOperation,
    /// A record index or counter would not fit the recording's integer type.
    OutOfRange,
}

impl ProtocolError {
    /// Stable name used in the machine-readable grader output.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProtocolError::InvalidOperation => "invalid_operation",
            ProtocolError::OutOfRange => "out_of_range",
        }
    }
}

/// One line of a trace recording, as captured by the protocol's own auditor.
///
/// The recording is the *expectation*: the auditor observed what the service actually did. A
/// recording either names the error the service returned, or reports the state it reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceExpectation {
    /// The replay must stop at this operation with this error.
    Error { op_index: usize, error: String },
    /// The replay must accept every operation and reach this state.
    Final { state: ProtocolState },
}

/// Split a recording into its operation lines and its single outcome line.
///
/// Blank lines and `#` comments are skipped by the caller. Returns the raw `op` column and the raw
/// `expect` column so a recording can be rejected before any of it is trusted.
pub fn split_recording_line(line: &str) -> Option<(String, String)> {
    let mut fields = line.split('\t');
    let op = fields.next()?.trim().to_string();
    let expect = fields.next()?.trim().to_string();
    if fields.next().is_some() {
        return None;
    }
    Some((op, expect))
}

/// Parse one operation column such as `put 0 alpha`.
///
/// Returns `None` for anything the ABI does not define, so a malformed recording is a recording
/// error rather than a candidate failure.
pub fn parse_operation(column: &str) -> Option<ProtocolOp> {
    let mut words = column.split(' ');
    match words.next()? {
        "begin" if words.next().is_none() => Some(ProtocolOp::Begin),
        "commit" if words.next().is_none() => Some(ProtocolOp::Commit),
        "abort" if words.next().is_none() => Some(ProtocolOp::Abort),
        "read" if words.next().is_none() => Some(ProtocolOp::Read),
        "put" => {
            let sequence = words.next()?.parse().ok()?;
            let value = words.next()?.to_string();
            if words.next().is_some() {
                return None;
            }
            Some(ProtocolOp::Put { sequence, value })
        }
        _ => None,
    }
}

/// Parse the state column of an outcome line such as
/// `state begun=false committed=true aborted=false reads=1 records=alpha,beta`.
pub fn parse_state(column: &str) -> Option<ProtocolState> {
    let mut words = column.split(' ');
    if words.next()? != "state" {
        return None;
    }
    let mut state = ProtocolState {
        begun: false,
        committed: false,
        aborted: false,
        records: Vec::new(),
        reads: 0,
    };
    let mut seen = 0_u8;
    for word in words {
        let (key, value) = word.split_once('=')?;
        match key {
            "begun" => {
                state.begun = parse_bool(value)?;
                seen |= 1;
            }
            "committed" => {
                state.committed = parse_bool(value)?;
                seen |= 2;
            }
            "aborted" => {
                state.aborted = parse_bool(value)?;
                seen |= 4;
            }
            "reads" => {
                state.reads = value.parse().ok()?;
                seen |= 8;
            }
            "records" if value == "-" => {
                seen |= 16;
            }
            "records" => {
                state.records = value.split(',').map(str::to_string).collect();
                seen |= 16;
            }
            _ => return None,
        }
    }
    if seen == 31 {
        Some(state)
    } else {
        None
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}
