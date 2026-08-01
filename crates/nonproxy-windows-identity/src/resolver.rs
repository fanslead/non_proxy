use std::{
    collections::HashMap,
    ffi::OsStr,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

use nonproxy_model::{AppIdentity, Platform};
use tokio::sync::Semaphore;

use crate::{decode_wfp_app_id, package_publisher_signer_identity, package_stable_identity};

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PackageIdentitySource {
    pub publisher_id: String,
    pub creation_time: Option<u64>,
}

pub(super) trait IdentitySource: Send + Sync {
    fn desktop_process(
        &self,
        process_id: u32,
        expected_stable_id: &str,
    ) -> Option<ProcessIdentitySource>;

    fn package_process(
        &self,
        process_id: u32,
        expected_sid: &[u8],
    ) -> Option<PackageIdentitySource>;

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

    pub async fn resolve(&self, app_id: &[u8], package_sid: &[u8], process_id: u64) -> AppIdentity {
        if !package_sid.is_empty() {
            return self.resolve_package(package_sid, process_id).await;
        }
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

    async fn resolve_package(&self, package_sid: &[u8], process_id: u64) -> AppIdentity {
        let Some(stable_id) = package_stable_identity(package_sid) else {
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
        let package_sid = package_sid.to_vec();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            resolver.resolve_package_blocking(stable_id, &package_sid, process_id)
        })
        .await
        .unwrap_or(fallback)
    }

    fn resolve_blocking(&self, stable_id: String, process_id: u32) -> AppIdentity {
        let Some(process) = self.source.desktop_process(process_id, &stable_id) else {
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

    fn resolve_package_blocking(
        &self,
        stable_id: String,
        package_sid: &[u8],
        process_id: u32,
    ) -> AppIdentity {
        let Some(package) = self.source.package_process(process_id, package_sid) else {
            return identity(stable_id, None, None);
        };
        let Some(signer) = package_publisher_signer_identity(&package.publisher_id) else {
            return identity(stable_id, None, None);
        };
        let Some(creation_time) = package.creation_time else {
            return identity(stable_id, None, None);
        };
        let key = CacheKey {
            process_id,
            creation_time,
            stable_id: stable_id.clone(),
        };
        let signer = self.cached_value(key, || Some(signer));
        identity(stable_id, signer, None)
    }

    fn signer_for(&self, key: CacheKey, path: &str) -> Option<String> {
        self.cached_value(key, || self.source.trusted_signer(path))
    }

    fn cached_value(
        &self,
        key: CacheKey,
        resolve: impl FnOnce() -> Option<String>,
    ) -> Option<String> {
        let Ok(mut cache) = self.cache.lock() else {
            return resolve();
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
        signer.get_or_init(resolve).clone()
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
#[path = "resolver_tests.rs"]
mod tests;
