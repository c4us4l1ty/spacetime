// SEEDED CANDIDATE — planted bug `abort-without-transaction`.
//
// Do not "fix" this file before running the oracle: the seeded state is the point. The candidate
// compiles and its public surface matches the pinned ABI, so the defect only shows up under the
// hidden oracle. This one is a guard bug: `abort` is accepted even when no transaction is open,
// so a stray abort is reported as a successful rollback.

pub mod protocol {
    use crate::{ProtocolError, ProtocolOp, ProtocolState};

    /// Replay one captured trace, refusing the first operation that the contract forbids.
    pub fn replay(operations: &[ProtocolOp]) -> Result<ProtocolState, ProtocolError> {
        let mut state = ProtocolState {
            begun: false,
            committed: false,
            aborted: false,
            records: Vec::new(),
            reads: 0,
        };
        for operation in operations {
            match operation {
                ProtocolOp::Begin if !state.begun && !state.committed && !state.aborted => {
                    state.begun = true;
                }
                ProtocolOp::Put { sequence, value }
                    if state.begun && !state.committed && !state.aborted =>
                {
                    if *sequence != state.records.len() {
                        return Err(ProtocolError::InvalidOperation);
                    }
                    state.records.push(value.clone());
                }
                ProtocolOp::Commit if state.begun && !state.committed && !state.aborted => {
                    state.begun = false;
                    state.committed = true;
                }
                ProtocolOp::Abort => {
                    state.begun = false;
                    state.aborted = true;
                    state.records.clear();
                }
                ProtocolOp::Read if state.committed && !state.aborted => {
                    state.reads = state
                        .reads
                        .checked_add(1)
                        .ok_or(ProtocolError::OutOfRange)?;
                }
                _ => return Err(ProtocolError::InvalidOperation),
            }
        }
        Ok(state)
    }
}
