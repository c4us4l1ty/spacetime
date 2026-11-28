// SEEDED CANDIDATE — planted bug `abort-keeps-records`.
//
// Do not "fix" this file before running the oracle: the seeded state is the point. The candidate
// compiles and its public surface matches the pinned ABI, so the defect only shows up under the
// hidden oracle. This one is a state bug: `abort` discards the transaction but leaves its records behind.

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
        for (index, operation) in operations.iter().enumerate() {
            match operation {
                ProtocolOp::Abort if state.begun && !state.committed => {
                    state.begun = false;
                    state.aborted = true;
                }
                _ => return Err(ProtocolError::invalid_operation(index)),
            }
        }
        Ok(state)
    }
}
