use std::collections::HashSet;

use crate::{ModelError, OutboundGroupId, OutboundId};

const MINIMUM_MEMBERS: usize = 2;
const MAXIMUM_MEMBERS: usize = 32;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct OutboundGroupSpec {
    id: OutboundGroupId,
    revision: u64,
    members: Vec<OutboundId>,
}

impl OutboundGroupSpec {
    pub fn new(
        id: OutboundGroupId,
        revision: u64,
        members: Vec<OutboundId>,
    ) -> Result<Self, ModelError> {
        let unique = members.iter().collect::<HashSet<_>>();
        if revision == 0
            || !(MINIMUM_MEMBERS..=MAXIMUM_MEMBERS).contains(&members.len())
            || unique.len() != members.len()
        {
            return Err(ModelError::InvalidOutboundGroupSpec);
        }
        Ok(Self {
            id,
            revision,
            members,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &OutboundGroupId {
        &self.id
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn members(&self) -> &[OutboundId] {
        &self.members
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_snapshot_requires_ordered_unique_members() {
        let id = OutboundGroupId::new("office")
            .unwrap_or_else(|error| panic!("测试出口组标识无效: {error}"));
        let member =
            OutboundId::new("primary").unwrap_or_else(|error| panic!("测试出口标识无效: {error}"));

        assert!(matches!(
            OutboundGroupSpec::new(id, 1, vec![member.clone(), member]),
            Err(ModelError::InvalidOutboundGroupSpec)
        ));
    }
}
