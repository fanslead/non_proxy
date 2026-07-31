use sha2::{Digest, Sha256};

pub(crate) struct DiagnosticRedactor {
    salt: [u8; 32],
}

impl DiagnosticRedactor {
    #[must_use]
    pub const fn new(salt: [u8; 32]) -> Self {
        Self { salt }
    }

    #[must_use]
    pub fn pseudonym(&self, prefix: &str, value: &str) -> String {
        let mut digest = Sha256::new();
        digest.update(self.salt);
        digest.update([0]);
        digest.update(prefix.as_bytes());
        digest.update([0]);
        digest.update(value.as_bytes());
        let hash = digest.finalize();
        format!("{prefix}-{}", hex(&hash[..8]))
    }
}

#[must_use]
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::DiagnosticRedactor;

    #[test]
    fn pseudonyms_are_stable_inside_one_export_and_salted_between_exports() {
        let first = DiagnosticRedactor::new([1; 32]);
        let second = DiagnosticRedactor::new([2; 32]);

        assert_eq!(
            first.pseudonym("target", "private.example"),
            first.pseudonym("target", "private.example")
        );
        assert_ne!(
            first.pseudonym("target", "private.example"),
            second.pseudonym("target", "private.example")
        );
    }
}
