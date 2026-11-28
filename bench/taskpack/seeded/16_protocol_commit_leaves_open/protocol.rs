// SEEDED CANDIDATE — planted bug `commit-keeps-begun`.
//
// Do not "fix" this file before running the oracle: the seeded state is the point. The candidate
// compiles and its public surface matches the pinned ABI, so the defect only shows up under the
// hidden oracle. This one is a state bug: `commit` never clears `begun`, so a read afterwards looks like an open transaction.

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
                ProtocolOp::Commit if state.begun && !state.committed && !state.aborted => {
                    state.committed = true;
                }
                _ => return Err(ProtocolError::InvalidOperation),
            }
        }
        Ok(state)
    }
}
