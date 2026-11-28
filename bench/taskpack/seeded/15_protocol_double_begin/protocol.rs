// SEEDED CANDIDATE — planted bug `begin-without-guard`.
//
// Do not "fix" this file before running the oracle: the seeded state is the point. The candidate
// compiles and its public surface matches the pinned ABI, so the defect only shows up under the
// hidden oracle. This one is a guard bug: a second `begin` opens a transaction that is already open.

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
                ProtocolOp::Begin => {
                    state.begun = true;
                }
                _ => return Err(ProtocolError::InvalidOperation),
            }
        }
        Ok(state)
    }
}
