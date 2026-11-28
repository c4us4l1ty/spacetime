// SEEDED CANDIDATE — planted bug `abort-keeps-records`.
//
// Do not "fix" this file before running the oracle: the seeded state is the point. The candidate
// compiles and its public surface matches the pinned ABI, so the defect only shows up under the
// hidden oracle. This one is a state bug: `abort` discards the transaction but leaves its records behind.

pub mod protocol {
    use crate::{ProtocolError, ProtocolOp, ProtocolState};

    /// Replay one captured trace, refusing the first operation that the contract forbids.
    ///
    /// The replay is a plain fold: every accepted operation advances the state, and the first
    /// rejected operation ends the trace. That is the whole protocol — there is no storage, no
    /// clock, and no external effect to model.
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
                ProtocolOp::Begin if !state.begun && !state.committed && !state.aborted => {
                    state.begun = true;
                }
                ProtocolOp::Put { sequence, value }
                    if state.begun && !state.committed && !state.aborted =>
                {
                    // A record may only be appended at the next free index, so a duplicate or a
                    // gap is refused rather than silently coerced.
                    if *sequence != state.records.len() {
                        return Err(ProtocolError::invalid_operation(index));
                    }
                    state.records.push(value.clone());
                }
                ProtocolOp::Commit if state.begun && !state.committed && !state.aborted => {
                    state.begun = false;
                    state.committed = true;
                }
                ProtocolOp::Abort if state.begun && !state.committed => {
                    state.begun = false;
                    state.aborted = true;
                }
                ProtocolOp::Read if state.committed && !state.aborted => {
                    state.reads = state
                        .reads
                        .checked_add(1)
                        .ok_or(ProtocolError::out_of_range(index))?;
                }
                _ => return Err(ProtocolError::invalid_operation(index)),
            }
        }
        Ok(state)
    }
}
