use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use nonproxy_policy::CompiledPolicySnapshot;

use crate::GatewayError;

const SNAPSHOT_CACHE_CAPACITY: usize = 16;

pub(crate) struct SnapshotLookup {
    pub(crate) found: BTreeMap<u64, Arc<CompiledPolicySnapshot>>,
    pub(crate) missing: Vec<u64>,
}

#[derive(Clone, Default)]
pub(crate) struct DecisionSnapshotCache {
    inner: Arc<Mutex<BTreeMap<u64, Arc<CompiledPolicySnapshot>>>>,
}

impl DecisionSnapshotCache {
    pub(crate) fn lookup(&self, versions: &[u64]) -> Result<SnapshotLookup, GatewayError> {
        let cache = self
            .inner
            .lock()
            .map_err(|_| GatewayError::StateLockPoisoned("决策快照缓存"))?;
        let mut found = BTreeMap::new();
        let mut missing = Vec::new();
        for version in versions {
            if let Some(snapshot) = cache.get(version) {
                found.insert(*version, Arc::clone(snapshot));
            } else {
                missing.push(*version);
            }
        }
        Ok(SnapshotLookup { found, missing })
    }

    pub(crate) fn insert(
        &self,
        snapshots: &BTreeMap<u64, Arc<CompiledPolicySnapshot>>,
    ) -> Result<(), GatewayError> {
        let mut cache = self
            .inner
            .lock()
            .map_err(|_| GatewayError::StateLockPoisoned("决策快照缓存"))?;
        for (version, snapshot) in snapshots {
            cache.insert(*version, Arc::clone(snapshot));
        }
        while cache.len() > SNAPSHOT_CACHE_CAPACITY {
            cache.pop_first();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::DecisionSnapshotCache;

    #[test]
    fn empty_cache_reports_every_version_as_missing() {
        let result = DecisionSnapshotCache::default().lookup(&[1, 2]);

        assert!(matches!(
            result,
            Ok(lookup) if lookup.found.is_empty() && lookup.missing == vec![1, 2]
        ));
    }
}
