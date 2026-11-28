// Reference solution for the Block 3 protocol-distillation tasks.
//
// This is the post-fix solution: a hand-written state machine that implements the pinned ABI
// exactly, written from the specification rather than from the trace recording. It is graded by
// `oracle/protocol_oracle.rs` against recordings captured by an independent auditor, so agreement
// with itself proves nothing.

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
        for operation in operations {
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
                        return Err(ProtocolError::InvalidOperation);
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
                    // An aborted transaction leaves nothing behind.
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
