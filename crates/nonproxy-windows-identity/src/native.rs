use std::{
    mem::{size_of, zeroed},
    os::windows::ffi::OsStrExt,
    ptr::{self, null_mut},
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, FILETIME, HANDLE},
    NetworkManagement::WindowsFilteringPlatform::{
        FWP_BYTE_BLOB, FwpmFreeMemory0, FwpmGetAppIdFromFileName0,
    },
    Security::{
        FreeSid, GetLengthSid, IsValidSid,
        Isolation::DeriveAppContainerSidFromAppContainerName,
        PSID,
        WinTrust::{
            WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO,
            WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_DISABLE_MD2_MD4, WTD_REVOKE_NONE,
            WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
            WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain,
            WTHelperProvDataFromStateData, WinVerifyTrustEx,
        },
    },
    Storage::Packaging::Appx::{GetPackageFamilyName, PackageNameAndPublisherIdFromFamilyName},
    System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    },
};

use crate::{
    certificate_signer_identity, decode_wfp_app_id,
    resolver::{IdentitySource, PackageIdentitySource, ProcessIdentitySource},
};

const MAXIMUM_PROCESS_PATH_CHARACTERS: usize = 32_768;
const MAXIMUM_PACKAGE_NAME_CHARACTERS: usize = 256;
const MAXIMUM_PACKAGE_SID_BYTES: usize = 68;
const MAXIMUM_CERTIFICATE_BYTES: usize = 1024 * 1024;

pub(super) struct WindowsNativeIdentitySource;

impl IdentitySource for WindowsNativeIdentitySource {
    fn desktop_process(
        &self,
        process_id: u32,
        expected_stable_id: &str,
    ) -> Option<ProcessIdentitySource> {
        let process = ProcessHandle::open(process_id)?;
        let path = process.image_path()?;
        (stable_id_for_path(&path).as_deref() == Some(expected_stable_id)).then(|| {
            ProcessIdentitySource {
                path,
                creation_time: process.creation_time(),
            }
        })
    }

    fn package_process(
        &self,
        process_id: u32,
        expected_sid: &[u8],
    ) -> Option<PackageIdentitySource> {
        let process = ProcessHandle::open(process_id)?;
        let family_name = process.package_family_name()?;
        let derived_sid = package_sid_for_family(&family_name)?;
        if derived_sid != expected_sid {
            return None;
        }
        Some(PackageIdentitySource {
            publisher_id: package_publisher_id(&family_name)?,
            creation_time: process.creation_time(),
        })
    }

    fn trusted_signer(&self, path: &str) -> Option<String> {
        trusted_signer_identity(path)
    }
}

struct ProcessHandle(HANDLE);

impl ProcessHandle {
    fn open(process_id: u32) -> Option<Self> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        (!handle.is_null()).then_some(Self(handle))
    }

    fn image_path(&self) -> Option<String> {
        let mut buffer = vec![0_u16; MAXIMUM_PROCESS_PATH_CHARACTERS];
        let mut length = u32::try_from(buffer.len()).ok()?;
        let succeeded =
            unsafe { QueryFullProcessImageNameW(self.0, 0, buffer.as_mut_ptr(), &mut length) };
        if succeeded == 0 || length == 0 {
            return None;
        }
        buffer.truncate(usize::try_from(length).ok()?);
        let path = String::from_utf16(&buffer).ok()?;
        (!path.is_empty() && !path.chars().any(char::is_control)).then_some(path)
    }

    fn creation_time(&self) -> Option<u64> {
        let mut creation: FILETIME = unsafe { zeroed() };
        let mut exit: FILETIME = unsafe { zeroed() };
        let mut kernel: FILETIME = unsafe { zeroed() };
        let mut user: FILETIME = unsafe { zeroed() };
        let succeeded =
            unsafe { GetProcessTimes(self.0, &mut creation, &mut exit, &mut kernel, &mut user) };
        (succeeded != 0).then_some(
            (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime),
        )
    }

    fn package_family_name(&self) -> Option<String> {
        let mut length = 0_u32;
        let status = unsafe { GetPackageFamilyName(self.0, &mut length, null_mut()) };
        if status != ERROR_INSUFFICIENT_BUFFER
            || length < 2
            || usize::try_from(length).ok()? > MAXIMUM_PACKAGE_NAME_CHARACTERS
        {
            return None;
        }
        let mut buffer = vec![0_u16; usize::try_from(length).ok()?];
        let status = unsafe { GetPackageFamilyName(self.0, &mut length, buffer.as_mut_ptr()) };
        (status == ERROR_SUCCESS)
            .then(|| decode_wide_buffer(&buffer, length))
            .flatten()
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            let _closed = unsafe { CloseHandle(self.0) };
        }
    }
}

fn stable_id_for_path(path: &str) -> Option<String> {
    let wide = wide_path(path)?;
    let mut blob: *mut FWP_BYTE_BLOB = null_mut();
    let code = unsafe { FwpmGetAppIdFromFileName0(wide.as_ptr(), &mut blob) };
    if code != 0 || blob.is_null() {
        return None;
    }
    let owned = WfpBlob(blob);
    let blob = unsafe { &*owned.0 };
    if blob.size == 0 || blob.size > 4096 || blob.data.is_null() {
        return None;
    }
    let length = usize::try_from(blob.size).ok()?;
    let bytes = unsafe { std::slice::from_raw_parts(blob.data, length) };
    decode_wfp_app_id(bytes)
}

struct WfpBlob(*mut FWP_BYTE_BLOB);

impl Drop for WfpBlob {
    fn drop(&mut self) {
        if !self.0.is_null() {
            let mut value = self.0.cast();
            unsafe { FwpmFreeMemory0(&mut value) };
            self.0 = null_mut();
        }
    }
}

fn package_sid_for_family(family_name: &str) -> Option<Vec<u8>> {
    let family_name = wide_path(family_name)?;
    let mut sid: PSID = null_mut();
    let result =
        unsafe { DeriveAppContainerSidFromAppContainerName(family_name.as_ptr(), &mut sid) };
    if result < 0 || sid.is_null() {
        return None;
    }
    let owned = OwnedSid(sid);
    if unsafe { IsValidSid(owned.0) } == 0 {
        return None;
    }
    let length = usize::try_from(unsafe { GetLengthSid(owned.0) }).ok()?;
    if length == 0 || length > MAXIMUM_PACKAGE_SID_BYTES {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(owned.0.cast::<u8>(), length) }.to_vec())
}

struct OwnedSid(PSID);

impl Drop for OwnedSid {
    fn drop(&mut self) {
        if !self.0.is_null() {
            let _released = unsafe { FreeSid(self.0) };
            self.0 = null_mut();
        }
    }
}

fn package_publisher_id(family_name: &str) -> Option<String> {
    let family_name = wide_path(family_name)?;
    let mut name_length = 0_u32;
    let mut publisher_length = 0_u32;
    let status = unsafe {
        PackageNameAndPublisherIdFromFamilyName(
            family_name.as_ptr(),
            &mut name_length,
            null_mut(),
            &mut publisher_length,
            null_mut(),
        )
    };
    if status != ERROR_INSUFFICIENT_BUFFER
        || name_length < 2
        || publisher_length < 2
        || usize::try_from(name_length).ok()? > MAXIMUM_PACKAGE_NAME_CHARACTERS
        || usize::try_from(publisher_length).ok()? > MAXIMUM_PACKAGE_NAME_CHARACTERS
    {
        return None;
    }
    let mut name = vec![0_u16; usize::try_from(name_length).ok()?];
    let mut publisher = vec![0_u16; usize::try_from(publisher_length).ok()?];
    let status = unsafe {
        PackageNameAndPublisherIdFromFamilyName(
            family_name.as_ptr(),
            &mut name_length,
            name.as_mut_ptr(),
            &mut publisher_length,
            publisher.as_mut_ptr(),
        )
    };
    (status == ERROR_SUCCESS)
        .then(|| decode_wide_buffer(&publisher, publisher_length))
        .flatten()
}

fn decode_wide_buffer(buffer: &[u16], length: u32) -> Option<String> {
    let used = usize::try_from(length).ok()?;
    let mut value = buffer.get(..used)?;
    if value.last() == Some(&0) {
        value = &value[..value.len() - 1];
    }
    let value = String::from_utf16(value).ok()?;
    (!value.is_empty()
        && !value
            .chars()
            .any(|character| character == '\0' || character.is_control()))
    .then_some(value)
}

fn trusted_signer_identity(path: &str) -> Option<String> {
    let wide = wide_path(path)?;
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: u32::try_from(size_of::<WINTRUST_FILE_INFO>()).ok()?,
        pcwszFilePath: wide.as_ptr(),
        hFile: null_mut(),
        pgKnownSubject: null_mut(),
    };
    let mut data = WINTRUST_DATA {
        cbStruct: u32::try_from(size_of::<WINTRUST_DATA>()).ok()?,
        pPolicyCallbackData: null_mut(),
        pSIPClientData: null_mut(),
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: windows_sys::Win32::Security::WinTrust::WINTRUST_DATA_0 {
            pFile: ptr::from_mut(&mut file),
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        hWVTStateData: null_mut(),
        pwszURLReference: null_mut(),
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL | WTD_DISABLE_MD2_MD4,
        dwUIContext: 0,
        pSignatureSettings: null_mut(),
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe { WinVerifyTrustEx(null_mut(), &mut action, &mut data) };
    let signer = (status == 0)
        .then(|| signer_certificate_hash(data.hWVTStateData))
        .flatten();
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    let _closed = unsafe { WinVerifyTrustEx(null_mut(), &mut action, &mut data) };
    signer
}

fn signer_certificate_hash(state: HANDLE) -> Option<String> {
    if state.is_null() {
        return None;
    }
    let provider = unsafe { WTHelperProvDataFromStateData(state) };
    if provider.is_null() {
        return None;
    }
    let signer = unsafe { WTHelperGetProvSignerFromChain(provider, 0, 0, 0) };
    if signer.is_null() {
        return None;
    }
    let certificate = unsafe { WTHelperGetProvCertFromChain(signer, 0) };
    if certificate.is_null() {
        return None;
    }
    let context = unsafe { (*certificate).pCert };
    if context.is_null() {
        return None;
    }
    let encoded_length = usize::try_from(unsafe { (*context).cbCertEncoded }).ok()?;
    let encoded = unsafe { (*context).pbCertEncoded };
    if encoded.is_null() || encoded_length == 0 || encoded_length > MAXIMUM_CERTIFICATE_BYTES {
        return None;
    }
    let certificate = unsafe { std::slice::from_raw_parts(encoded, encoded_length) };
    certificate_signer_identity(certificate)
}

fn wide_path(path: &str) -> Option<Vec<u16>> {
    if path.is_empty()
        || path
            .chars()
            .any(|value| value == '\0' || value.is_control())
    {
        return None;
    }
    let mut value = std::ffi::OsStr::new(path).encode_wide().collect::<Vec<_>>();
    value.push(0);
    Some(value)
}
