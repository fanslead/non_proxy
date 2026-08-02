use std::collections::HashSet;

use nonproxy_model::{OutboundGroupId, OutboundId};

use crate::StorageError;

pub const MINIMUM_OUTBOUND_GROUP_MEMBERS: usize = 2;
pub const MAXIMUM_OUTBOUND_GROUP_MEMBERS: usize = 32;
const MAXIMUM_DISPLAY_NAME_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutboundGroupStrategy {
    Failover,
}

impl OutboundGroupStrategy {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Failover => "failover",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "failover" => Ok(Self::Failover),
            _ => Err(StorageError::CorruptData {
                field: "outbound_group.strategy",
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboundGroup {
    id: OutboundGroupId,
    display_name: String,
    strategy: OutboundGroupStrategy,
    members: Vec<OutboundId>,
    revision: u64,
}

impl OutboundGroup {
    pub fn new(
        id: OutboundGroupId,
        display_name: impl Into<String>,
        strategy: OutboundGroupStrategy,
        members: Vec<OutboundId>,
        revision: u64,
    ) -> Result<Self, StorageError> {
        let display_name = display_name.into();
        let unique_members = members.iter().collect::<HashSet<_>>();
        if display_name.is_empty()
            || display_name.trim() != display_name
            || display_name.len() > MAXIMUM_DISPLAY_NAME_BYTES
            || display_name.chars().any(char::is_control)
            || !(MINIMUM_OUTBOUND_GROUP_MEMBERS..=MAXIMUM_OUTBOUND_GROUP_MEMBERS)
                .contains(&members.len())
            || unique_members.len() != members.len()
            || revision == 0
        {
            return Err(StorageError::OutboundGroupInvalid);
        }
        Ok(Self {
            id,
            display_name,
            strategy,
            members,
            revision,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &OutboundGroupId {
        &self.id
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub const fn strategy(&self) -> OutboundGroupStrategy {
        self.strategy
    }

    #[must_use]
    pub fn members(&self) -> &[OutboundId] {
        &self.members
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
}
