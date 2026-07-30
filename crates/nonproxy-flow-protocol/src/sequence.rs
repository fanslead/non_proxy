use crate::FlowProtocolError;

#[derive(Clone, Copy, Debug, Default)]
pub struct SequenceTracker {
    expected: u64,
}

impl SequenceTracker {
    pub fn accept(&mut self, sequence: u64) -> Result<(), FlowProtocolError> {
        if sequence != self.expected {
            return Err(FlowProtocolError::SequenceMismatch);
        }
        self.expected = self
            .expected
            .checked_add(1)
            .ok_or(FlowProtocolError::SequenceExhausted)?;
        Ok(())
    }

    #[must_use]
    pub const fn expected(&self) -> u64 {
        self.expected
    }
}
