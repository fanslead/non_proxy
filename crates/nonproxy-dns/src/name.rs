use std::fmt;

use hickory_proto::rr::Name;

use crate::DnsError;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DnsName(String);

impl DnsName {
    pub fn parse_ascii(value: &str) -> Result<Self, DnsError> {
        let name = Name::from_ascii(value).map_err(|_| DnsError::Domain)?;
        Ok(Self::from_name(&name))
    }

    #[must_use]
    pub fn from_name(name: &Name) -> Self {
        let mut canonical = name.to_lowercase();
        canonical.set_fqdn(true);
        let ascii = canonical.to_ascii();
        let value = if ascii == "." {
            ascii
        } else {
            ascii.strip_suffix('.').unwrap_or(&ascii).to_owned()
        };
        Self(value)
    }

    #[must_use]
    pub fn as_ascii(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_root(&self) -> bool {
        self.0 == "."
    }
}

impl fmt::Display for DnsName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
