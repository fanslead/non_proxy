use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use super::*;

const STABLE_ID: &str = "\\device\\harddiskvolume4\\apps\\office.exe";

#[tokio::test]
async fn matching_process_identity_adds_signer_and_reuses_process_generation_cache() {
    let source = Arc::new(FakeIdentitySource::new(Some(7)));
    let resolver = WindowsAppIdentityResolver::with_source(source.clone());
    let app_id = encoded_app_id(STABLE_ID);

    let first = resolver.resolve(&app_id, &[], 42).await;
    let second = resolver.resolve(&app_id, &[], 42).await;

    assert_eq!(first.stable_id(), STABLE_ID);
    assert_eq!(first.signer_id(), Some("cert-sha256:test"));
    assert_eq!(second.signer_id(), first.signer_id());
    assert_eq!(source.signature_reads.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn missing_process_generation_keeps_stable_identity_without_claiming_signer() {
    let source = Arc::new(FakeIdentitySource::new(None));
    let resolver = WindowsAppIdentityResolver::with_source(source.clone());

    let identity = resolver.resolve(&encoded_app_id(STABLE_ID), &[], 42).await;

    assert_eq!(identity.stable_id(), STABLE_ID);
    assert_eq!(identity.signer_id(), None);
    assert_eq!(source.signature_reads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn new_process_generation_requires_new_signature_verification() {
    let source = Arc::new(FakeIdentitySource::new(Some(7)));
    let resolver = WindowsAppIdentityResolver::with_source(source.clone());
    let app_id = encoded_app_id(STABLE_ID);

    let first = resolver.resolve(&app_id, &[], 42).await;
    source.creation_time.store(8, Ordering::SeqCst);
    let second = resolver.resolve(&app_id, &[], 42).await;

    assert_eq!(first.signer_id(), Some("cert-sha256:test"));
    assert_eq!(second.signer_id(), first.signer_id());
    assert_eq!(source.signature_reads.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_first_flows_share_one_signature_verification() {
    let source = Arc::new(FakeIdentitySource::new(Some(9)));
    let resolver = WindowsAppIdentityResolver::with_source(source.clone());
    let app_id = encoded_app_id(STABLE_ID);
    let first = resolver.resolve(&app_id, &[], 42);
    let second = resolver.resolve(&app_id, &[], 42);

    let (first, second) = tokio::join!(first, second);

    assert_eq!(first.signer_id(), Some("cert-sha256:test"));
    assert_eq!(second.signer_id(), first.signer_id());
    assert_eq!(source.signature_reads.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn matching_package_process_uses_sid_and_publisher_identity() {
    let source = Arc::new(FakeIdentitySource::new(Some(11)));
    let resolver = WindowsAppIdentityResolver::with_source(source);

    let identity = resolver.resolve(&[1], &package_sid(), 42).await;

    assert_eq!(identity.stable_id(), "package-sid:S-1-15-2-1-2-3-4-5-6-7");
    assert_eq!(
        identity.signer_id(),
        Some("package-publisher-id:8wekyb3d8bbwe")
    );
    assert_eq!(identity.executable_path_hint(), None);
}

#[tokio::test]
async fn package_sid_mismatch_keeps_identity_without_claiming_publisher() {
    let source = Arc::new(FakeIdentitySource::new(Some(11)));
    let resolver = WindowsAppIdentityResolver::with_source(source);
    let mut other_sid = package_sid();
    other_sid[12] = 9;

    let identity = resolver
        .resolve(&encoded_app_id(STABLE_ID), &other_sid, 42)
        .await;

    assert_eq!(identity.stable_id(), "package-sid:S-1-15-2-9-2-3-4-5-6-7");
    assert_eq!(identity.signer_id(), None);
}

#[tokio::test]
async fn package_process_without_generation_does_not_claim_publisher() {
    let source = Arc::new(FakeIdentitySource::new(None));
    let resolver = WindowsAppIdentityResolver::with_source(source);

    let identity = resolver.resolve(&[], &package_sid(), 42).await;

    assert_eq!(identity.stable_id(), "package-sid:S-1-15-2-1-2-3-4-5-6-7");
    assert_eq!(identity.signer_id(), None);
}

#[tokio::test]
async fn malformed_nonempty_package_sid_does_not_fall_back_to_desktop_identity() {
    let source = Arc::new(FakeIdentitySource::new(Some(11)));
    let resolver = WindowsAppIdentityResolver::with_source(source);

    let identity = resolver
        .resolve(&encoded_app_id(STABLE_ID), &[1, 2], 42)
        .await;

    assert_eq!(identity.stable_id(), "unknown-app");
    assert_eq!(identity.signer_id(), None);
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
    fn desktop_process(
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

    fn package_process(
        &self,
        process_id: u32,
        expected_sid: &[u8],
    ) -> Option<PackageIdentitySource> {
        (process_id == 42 && expected_sid == package_sid()).then(|| PackageIdentitySource {
            publisher_id: "8wekyb3d8bbwe".to_owned(),
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

fn package_sid() -> Vec<u8> {
    let mut bytes = vec![1, 8, 0, 0, 0, 0, 0, 15];
    bytes.extend(
        [2_u32, 1, 2, 3, 4, 5, 6, 7]
            .into_iter()
            .flat_map(u32::to_le_bytes),
    );
    bytes
}
