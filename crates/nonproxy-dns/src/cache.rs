use std::{collections::HashMap, sync::Mutex};

use crate::{DnsCacheKey, DnsError, ParsedDnsResponse};

const MAXIMUM_CACHE_ENTRIES: usize = 65_536;
const DEFAULT_CACHE_ENTRIES: usize = 4_096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CachedDnsResponse {
    bytes: Vec<u8>,
    remaining_ttl_seconds: u32,
}

impl CachedDnsResponse {
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub const fn remaining_ttl_seconds(&self) -> u32 {
        self.remaining_ttl_seconds
    }
}

#[derive(Clone)]
struct CacheEntry {
    response: ParsedDnsResponse,
    expires_at_unix_ms: u64,
    inserted_at_unix_ms: u64,
}

pub struct PartitionedDnsCache {
    capacity: usize,
    entries: Mutex<HashMap<DnsCacheKey, CacheEntry>>,
}

impl Default for PartitionedDnsCache {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_CACHE_ENTRIES,
            entries: Mutex::new(HashMap::with_capacity(DEFAULT_CACHE_ENTRIES)),
        }
    }
}

impl PartitionedDnsCache {
    pub fn new(capacity: usize) -> Result<Self, DnsError> {
        if capacity == 0 || capacity > MAXIMUM_CACHE_ENTRIES {
            return Err(DnsError::CacheCapacity);
        }
        Ok(Self {
            capacity,
            entries: Mutex::new(HashMap::with_capacity(capacity)),
        })
    }

    pub fn get(
        &self,
        key: &DnsCacheKey,
        transaction_id: u16,
        now_unix_ms: u64,
    ) -> Result<Option<CachedDnsResponse>, DnsError> {
        let mut entries = self.entries.lock().map_err(|_| DnsError::CacheLock)?;
        entries.retain(|_, entry| entry.expires_at_unix_ms > now_unix_ms);
        let Some(entry) = entries.get(key) else {
            return Ok(None);
        };
        let elapsed_ms = now_unix_ms.saturating_sub(entry.inserted_at_unix_ms);
        let elapsed_seconds = u32::try_from(elapsed_ms.div_ceil(1_000)).unwrap_or(u32::MAX);
        let remaining_ttl_seconds = entry
            .response
            .valid_for_seconds()
            .saturating_sub(elapsed_seconds);
        Ok(Some(CachedDnsResponse {
            bytes: entry
                .response
                .bytes_for_transaction(transaction_id, elapsed_seconds)?,
            remaining_ttl_seconds,
        }))
    }

    pub fn insert(
        &self,
        key: DnsCacheKey,
        response: ParsedDnsResponse,
        now_unix_ms: u64,
    ) -> Result<bool, DnsError> {
        let ttl = response.valid_for_seconds();
        if ttl == 0 {
            return Ok(false);
        }
        let expires_at_unix_ms = now_unix_ms
            .checked_add(u64::from(ttl) * 1_000)
            .ok_or(DnsError::InvalidResponse)?;
        let mut entries = self.entries.lock().map_err(|_| DnsError::CacheLock)?;
        entries.retain(|_, entry| entry.expires_at_unix_ms > now_unix_ms);
        if entries.len() >= self.capacity && !entries.contains_key(&key) {
            let victim = entries
                .iter()
                .min_by_key(|(_, entry)| (entry.expires_at_unix_ms, entry.inserted_at_unix_ms))
                .map(|(key, _)| key.clone());
            if let Some(victim) = victim {
                entries.remove(&victim);
            }
        }
        entries.insert(
            key,
            CacheEntry {
                response,
                expires_at_unix_ms,
                inserted_at_unix_ms: now_unix_ms,
            },
        );
        Ok(true)
    }

    pub fn clear(&self) -> Result<(), DnsError> {
        self.entries
            .lock()
            .map_err(|_| DnsError::CacheLock)?
            .clear();
        Ok(())
    }

    pub fn len(&self) -> Result<usize, DnsError> {
        Ok(self.entries.lock().map_err(|_| DnsError::CacheLock)?.len())
    }

    pub fn is_empty(&self) -> Result<bool, DnsError> {
        Ok(self
            .entries
            .lock()
            .map_err(|_| DnsError::CacheLock)?
            .is_empty())
    }
}
