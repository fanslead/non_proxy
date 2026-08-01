use std::{
    collections::HashMap,
    ffi::OsStr,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

use nonproxy_model::{AppIdentity, Platform};
use tokio::sync::Semaphore;

use crate::decode_wfp_app_id;

const MAXIMUM_CACHE_ENTRIES: usize = 4096;
const MAXIMUM_CONCURRENT_RESOLUTIONS: usize = 32;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CacheKey {
    process_id: u32,
    creation_time: u64,
    stable_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ProcessIdentitySource {
    pub path: String,
    pub creation_time: Option<u64>,
}

pub(super) trait IdentitySource: Send + Sync {
    fn process(&self, process_id: u32, expected_stable_id: &str) -> Option<ProcessIdentitySource>;

    fn trusted_signer(&self, path: &str) -> Option<String>;
}

#[derive(Default)]
struct IdentityCache {
    signers: HashMap<CacheKey, Arc<OnceLock<Option<String>>>>,
}

#[derive(Clone)]
pub struct WindowsAppIdentityResolver {
    source: Arc<dyn IdentitySource>,
    cache: Arc<Mutex<IdentityCache>>,
    capacity: Arc<Semaphore>,
}

impl WindowsAppIdentityResolver {
    #[cfg(windows)]
    #[must_use]
    pub fn new() -> Self {
        Self::with_source(Arc::new(crate::native::WindowsNativeIdentitySource))
    }

    fn with_source(source: Arc<dyn IdentitySource>) -> Self {
        Self {
            source,
            cache: Arc::new(Mutex::new(IdentityCache::default())),
            capacity: Arc::new(Semaphore::new(MAXIMUM_CONCURRENT_RESOLUTIONS)),
        }
    }

    pub async fn resolve(&self, app_id: &[u8], process_id: u64) -> AppIdentity {
        let Some(stable_id) = decode_wfp_app_id(app_id) else {
            return AppIdentity::unknown(Platform::Windows);
        };
        let fallback = identity(stable_id.clone(), None, None);
        let Ok(process_id) = u32::try_from(process_id) else {
            return fallback;
        };
        let Ok(permit) = Arc::clone(&self.capacity).try_acquire_owned() else {
            return fallback;
        };
        let resolver = self.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            resolver.resolve_blocking(stable_id, process_id)
        })
        .await
        .unwrap_or(fallback)
    }

    fn resolve_blocking(&self, stable_id: String, process_id: u32) -> AppIdentity {
        let Some(process) = self.source.process(process_id, &stable_id) else {
            return identity(stable_id, None, None);
        };
        let Some(creation_time) = process.creation_time else {
            return identity(stable_id, None, Some(process.path));
        };
        let key = CacheKey {
            process_id,
            creation_time,
            stable_id: stable_id.clone(),
        };
        let signer = self.signer_for(key, &process.path);
        identity(stable_id, signer, Some(process.path))
    }

    fn signer_for(&self, key: CacheKey, path: &str) -> Option<String> {
        let Ok(mut cache) = self.cache.lock() else {
            return self.source.trusted_signer(path);
        };
        if cache.signers.len() >= MAXIMUM_CACHE_ENTRIES && !cache.signers.contains_key(&key) {
            cache.signers.clear();
        }
        let signer = Arc::clone(
            cache
                .signers
                .entry(key)
                .or_insert_with(|| Arc::new(OnceLock::new())),
        );
        drop(cache);
        signer
            .get_or_init(|| self.source.trusted_signer(path))
            .clone()
    }
}

#[cfg(windows)]
impl Default for WindowsAppIdentityResolver {
    fn default() -> Self {
        Self::new()
    }
}

fn identity(stable_id: String, signer: Option<String>, path: Option<String>) -> AppIdentity {
    let mut value = match AppIdentity::new(Platform::Windows, stable_id) {
        Ok(value) => value,
        Err(_) => return AppIdentity::unknown(Platform::Windows),
    };
    if let Some(signer) = signer {
        value = match value.with_signer_id(signer) {
            Ok(value) => value,
            Err(_) => return AppIdentity::unknown(Platform::Windows),
        };
    }
    if let Some(path) = path {
        let display_name = Path::new(&path)
            .file_stem()
            .and_then(OsStr::to_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if let Ok(identity_with_path) = value.clone().with_path_hint(path) {
            value = identity_with_path;
        }
        if let Some(display_name) = display_name
            && let Ok(identity_with_name) = value.clone().with_display_name(display_name)
        {
            value = identity_with_name;
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    use super::*;

    const STABLE_ID: &str = "\\device\\harddiskvolume4\\apps\\office.exe";

    #[tokio::test]
    async fn matching_process_identity_adds_signer_and_reuses_process_generation_cache() {
        let source = Arc::new(FakeIdentitySource::new(Some(7)));
        let resolver = WindowsAppIdentityResolver::with_source(source.clone());
        let app_id = encoded_app_id(STABLE_ID);

        let first = resolver.resolve(&app_id, 42).await;
        let second = resolver.resolve(&app_id, 42).await;

        assert_eq!(first.stable_id(), STABLE_ID);
        assert_eq!(first.signer_id(), Some("cert-sha256:test"));
        assert_eq!(second.signer_id(), first.signer_id());
        assert_eq!(source.signature_reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn missing_process_generation_keeps_stable_identity_without_claiming_signer() {
        let source = Arc::new(FakeIdentitySource::new(None));
        let resolver = WindowsAppIdentityResolver::with_source(source.clone());

        let identity = resolver.resolve(&encoded_app_id(STABLE_ID), 42).await;

        assert_eq!(identity.stable_id(), STABLE_ID);
        assert_eq!(identity.signer_id(), None);
        assert_eq!(source.signature_reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn new_process_generation_requires_new_signature_verification() {
        let source = Arc::new(FakeIdentitySource::new(Some(7)));
        let resolver = WindowsAppIdentityResolver::with_source(source.clone());
        let app_id = encoded_app_id(STABLE_ID);

        let first = resolver.resolve(&app_id, 42).await;
        source.creation_time.store(8, Ordering::SeqCst);
        let second = resolver.resolve(&app_id, 42).await;

        assert_eq!(first.signer_id(), Some("cert-sha256:test"));
        assert_eq!(second.signer_id(), first.signer_id());
        assert_eq!(source.signature_reads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_first_flows_share_one_signature_verification() {
        let source = Arc::new(FakeIdentitySource::new(Some(9)));
        let resolver = WindowsAppIdentityResolver::with_source(source.clone());
        let app_id = encoded_app_id(STABLE_ID);
        let first = resolver.resolve(&app_id, 42);
        let second = resolver.resolve(&app_id, 42);

        let (first, second) = tokio::join!(first, second);

        assert_eq!(first.signer_id(), Some("cert-sha256:test"));
        assert_eq!(second.signer_id(), first.signer_id());
        assert_eq!(source.signature_reads.load(Ordering::SeqCst), 1);
    }

    struct FakeIdentitySource {
        creation_time: AtomicU64,
        signature_reads: AtomicUsize,
    }

    impl FakeIdentitySource {
        fn new(creation_time: Option<u64>) -> Self {
            Self {
                creation_time: AtomicU64::new(creation_time.unwrap_or_default()),
                signature_reads: AtomicUsize::new(0),
            }
        }
    }

    impl IdentitySource for FakeIdentitySource {
        fn process(
            &self,
            process_id: u32,
            expected_stable_id: &str,
        ) -> Option<ProcessIdentitySource> {
            (process_id == 42 && expected_stable_id == STABLE_ID).then(|| ProcessIdentitySource {
                path: "C:\\Apps\\Office.exe".to_owned(),
                creation_time: match self.creation_time.load(Ordering::SeqCst) {
                    0 => None,
                    value => Some(value),
                },
            })
        }

        fn trusted_signer(&self, _path: &str) -> Option<String> {
            self.signature_reads.fetch_add(1, Ordering::SeqCst);
            Some("cert-sha256:test".to_owned())
        }
    }

    fn encoded_app_id(value: &str) -> Vec<u8> {
        value
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    }
}
