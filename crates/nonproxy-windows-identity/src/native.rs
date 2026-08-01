use std::{
    mem::{size_of, zeroed},
    os::windows::ffi::OsStrExt,
    ptr::{self, null_mut},
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE},
    NetworkManagement::WindowsFilteringPlatform::{
        FWP_BYTE_BLOB, FwpmFreeMemory0, FwpmGetAppIdFromFileName0,
    },
    Security::WinTrust::{
        WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO,
        WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_DISABLE_MD2_MD4, WTD_REVOKE_NONE,
        WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvCertFromChain,
        WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrustEx,
    },
    System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    },
};

use crate::{
    certificate_signer_identity, decode_wfp_app_id,
    resolver::{IdentitySource, ProcessIdentitySource},
};

const MAXIMUM_PROCESS_PATH_CHARACTERS: usize = 32_768;
const MAXIMUM_CERTIFICATE_BYTES: usize = 1024 * 1024;

pub(super) struct WindowsNativeIdentitySource;

impl IdentitySource for WindowsNativeIdentitySource {
    fn process(&self, process_id: u32, expected_stable_id: &str) -> Option<ProcessIdentitySource> {
        let process = ProcessHandle::open(process_id)?;
        let path = process.image_path()?;
        (stable_id_for_path(&path).as_deref() == Some(expected_stable_id)).then(|| {
            ProcessIdentitySource {
                path,
                creation_time: process.creation_time(),
            }
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
