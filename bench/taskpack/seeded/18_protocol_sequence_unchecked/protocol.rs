// SEEDED CANDIDATE — planted bug `sequence-ignored`.
//
// Do not "fix" this file before running the oracle: the seeded state is the point. The candidate
// compiles and its public surface matches the pinned ABI, so the defect only shows up under the
// hidden oracle. This one is a validation bug: `put` appends whatever index it is handed, so a duplicate sequence is accepted.

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
                ProtocolOp::Put { value, .. }
                    if state.begun && !state.committed && !state.aborted =>
                {
                    state.records.push(value.clone());
                }
                _ => return Err(ProtocolError::InvalidOperation),
            }
        }
        Ok(state)
    }
}
