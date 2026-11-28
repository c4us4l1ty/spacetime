// SEEDED CANDIDATE — planted bug `read-before-commit`.
//
// Do not "fix" this file before running the oracle: the seeded state is the point. The candidate
// compiles and its public surface matches the pinned ABI, so the defect only shows up under the
// hidden oracle. This one is a guard bug: `read` is accepted as soon as a transaction begins, not only after a commit.

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
                ProtocolOp::Read if state.begun || state.committed => {
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
