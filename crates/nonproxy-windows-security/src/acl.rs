use std::{ffi::c_void, io, os::windows::ffi::OsStrExt, path::Path, ptr};

use windows_sys::Win32::{
    Foundation::{ERROR_SUCCESS, LocalFree},
    Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
        Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW,
            GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT, SetNamedSecurityInfoW,
        },
        CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation,
        GetLengthSid, GetSecurityDescriptorControl, GetSecurityDescriptorDacl, IsValidSid,
        OBJECT_INHERIT_ACE, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
        SE_DACL_PROTECTED,
    },
    Storage::FileSystem::FILE_ALL_ACCESS,
    System::SystemServices::ACCESS_ALLOWED_ACE_TYPE,
};

use crate::{current_process_user_sid, validate_regular_directory, validate_regular_file};

const SYSTEM_SID: &str = "S-1-5-18";
const ADMINISTRATORS_SID: &str = "S-1-5-32-544";

pub fn protect_current_user_directory(path: &Path) -> io::Result<()> {
    protect(path, true)
}

pub fn protect_current_user_file(path: &Path) -> io::Result<()> {
    protect(path, false)
}

#[cfg(test)]
pub(crate) fn verify_current_user_file(path: &Path) -> io::Result<()> {
    verify_exact_dacl(path, &current_process_user_sid()?, false)
}

fn protect(path: &Path, directory: bool) -> io::Result<()> {
    if directory {
        validate_regular_directory(path)?;
    } else {
        validate_regular_file(path)?;
    }
    let user_sid = current_process_user_sid()?;
    let inherit = if directory { "OICI" } else { "" };
    let sddl =
        format!("D:P(A;{inherit};FA;;;SY)(A;{inherit};FA;;;BA)(A;{inherit};FA;;;{user_sid})");
    let encoded_sddl = wide_text(&sddl)?;
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // SAFETY: encoded_sddl 是 NUL 结尾 UTF-16，输出指针有效且由 LocalFree 释放。
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            encoded_sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        )
    };
    if converted == 0 || descriptor.is_null() {
        return Err(io::Error::last_os_error());
    }
    let descriptor = LocalAllocation {
        pointer: descriptor,
    };
    let mut present = 0;
    let mut defaulted = 0;
    let mut dacl: *mut ACL = ptr::null_mut();
    // SAFETY: descriptor 是有效自相对安全描述符，输出指针均有效。
    let read = unsafe {
        GetSecurityDescriptorDacl(descriptor.pointer, &mut present, &mut dacl, &mut defaulted)
    };
    if read == 0 || present == 0 || dacl.is_null() {
        return Err(io::Error::last_os_error());
    }
    let mut encoded_path = wide_path(path)?;
    // SAFETY: encoded_path 是 NUL 结尾路径；DACL 在同步调用期间由 descriptor 持有。
    let result = unsafe {
        SetNamedSecurityInfoW(
            encoded_path.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            dacl,
            ptr::null_mut(),
        )
    };
    if result != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(result.cast_signed()));
    }
    verify_exact_dacl(path, &user_sid, directory)
}

fn verify_exact_dacl(path: &Path, user_sid: &str, directory: bool) -> io::Result<()> {
    let encoded_path = wide_path(path)?;
    let mut dacl: *mut ACL = ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // SAFETY: 路径与输出指针有效；返回描述符由 LocalFree 释放并持有 DACL。
    let result = unsafe {
        GetNamedSecurityInfoW(
            encoded_path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut dacl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if result != ERROR_SUCCESS || descriptor.is_null() || dacl.is_null() {
        return Err(if result == ERROR_SUCCESS {
            io::Error::other("Windows 私有 DACL 缺失")
        } else {
            io::Error::from_raw_os_error(result.cast_signed())
        });
    }
    let descriptor = LocalAllocation {
        pointer: descriptor,
    };
    verify_protected(descriptor.pointer)?;
    let mut information = ACL_SIZE_INFORMATION::default();
    let information_size = u32::try_from(size_of::<ACL_SIZE_INFORMATION>())
        .map_err(|_| io::Error::other("Windows ACL 元数据长度溢出"))?;
    // SAFETY: dacl 由 descriptor 持有，information 缓冲区大小正确。
    let loaded = unsafe {
        GetAclInformation(
            dacl,
            (&raw mut information).cast(),
            information_size,
            AclSizeInformation,
        )
    };
    if loaded == 0 || information.AceCount != 3 {
        return Err(if loaded == 0 {
            io::Error::last_os_error()
        } else {
            io::Error::other("Windows 私有 DACL 包含意外 ACE")
        });
    }
    let expected = [SYSTEM_SID, ADMINISTRATORS_SID, user_sid]
        .into_iter()
        .map(LocalSid::parse)
        .collect::<io::Result<Vec<_>>>()?;
    let expected_flags = if directory {
        u8::try_from(CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE)
            .map_err(|_| io::Error::other("Windows ACL 继承标志溢出"))?
    } else {
        0
    };
    let mut matched = [false; 3];
    for index in 0..information.AceCount {
        let mut pointer: *mut c_void = ptr::null_mut();
        // SAFETY: index 小于已读取的 AceCount，输出指针有效。
        if unsafe { GetAce(dacl, index, &mut pointer) } == 0 || pointer.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: GetAce 成功返回至少 ACE_HEADER。
        let header = unsafe { &*pointer.cast::<ACE_HEADER>() };
        let ace_size = usize::from(header.AceSize);
        if u32::from(header.AceType) != ACCESS_ALLOWED_ACE_TYPE
            || header.AceFlags != expected_flags
            || ace_size < size_of::<ACCESS_ALLOWED_ACE>()
        {
            return Err(io::Error::other("Windows 私有 DACL ACE 形状无效"));
        }
        // SAFETY: AceSize 已覆盖 ACCESS_ALLOWED_ACE 固定字段。
        let ace = unsafe { &*pointer.cast::<ACCESS_ALLOWED_ACE>() };
        if ace.Mask != FILE_ALL_ACCESS {
            return Err(io::Error::other("Windows 私有 DACL ACE 权限无效"));
        }
        let sid_offset = std::mem::offset_of!(ACCESS_ALLOWED_ACE, SidStart);
        let sid_bytes = ace_size.saturating_sub(sid_offset);
        if sid_bytes < 8 {
            return Err(io::Error::other("Windows 私有 DACL SID 长度无效"));
        }
        // SAFETY: sid_offset 和 sid_bytes 均由已验证的 AceSize 约束。
        let actual_sid = unsafe { pointer.cast::<u8>().add(sid_offset) }.cast::<c_void>();
        // SAFETY: SID 至少包含 revision 与 sub-authority count 两字节。
        let sub_authorities = usize::from(unsafe { *actual_sid.cast::<u8>().add(1) });
        let expected_sid_bytes = 8_usize
            .checked_add(sub_authorities.saturating_mul(size_of::<u32>()))
            .ok_or_else(|| io::Error::other("Windows 私有 DACL SID 长度溢出"))?;
        // SAFETY: 仅在手工长度边界确认后调用 Windows SID 校验函数。
        if expected_sid_bytes != sid_bytes
            || unsafe { IsValidSid(actual_sid) } == 0
            || usize::try_from(unsafe { GetLengthSid(actual_sid) }).ok() != Some(sid_bytes)
        {
            return Err(io::Error::other("Windows 私有 DACL SID 无效"));
        }
        let Some(position) = expected.iter().position(|sid| {
            // SAFETY: actual_sid 位于有效 ACE 内；expected SID 由 Windows API 验证并持有。
            (unsafe { EqualSid(actual_sid, sid.pointer) }) != 0
        }) else {
            return Err(io::Error::other("Windows 私有 DACL 包含意外主体"));
        };
        if matched[position] {
            return Err(io::Error::other("Windows 私有 DACL 包含重复主体"));
        }
        matched[position] = true;
    }
    if matched.into_iter().all(|value| value) {
        Ok(())
    } else {
        Err(io::Error::other("Windows 私有 DACL 缺少受信任主体"))
    }
}

fn verify_protected(descriptor: PSECURITY_DESCRIPTOR) -> io::Result<()> {
    let mut control = 0_u16;
    let mut revision = 0_u32;
    // SAFETY: descriptor 由 GetNamedSecurityInfoW 返回且在 guard 生命周期内有效。
    let result = unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    if control & SE_DACL_PROTECTED == 0 {
        return Err(io::Error::other("Windows 私有 DACL 未禁用继承"));
    }
    Ok(())
}

fn wide_path(path: &Path) -> io::Result<Vec<u16>> {
    wide_units(path.as_os_str().encode_wide())
}

fn wide_text(value: &str) -> io::Result<Vec<u16>> {
    wide_units(value.encode_utf16())
}

fn wide_units(units: impl Iterator<Item = u16>) -> io::Result<Vec<u16>> {
    let mut encoded = units.collect::<Vec<_>>();
    if encoded.is_empty() || encoded.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows 安全对象名称无效",
        ));
    }
    encoded.push(0);
    Ok(encoded)
}

struct LocalAllocation {
    pointer: PSECURITY_DESCRIPTOR,
}

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: pointer 仅来自 Windows LocalAlloc 系列 API，guard 是唯一所有者。
        let _released = unsafe { LocalFree(self.pointer.cast()) };
    }
}

struct LocalSid {
    pointer: *mut c_void,
}

impl LocalSid {
    fn parse(value: &str) -> io::Result<Self> {
        let encoded = wide_text(value)?;
        let mut pointer = ptr::null_mut();
        // SAFETY: encoded 是 NUL 结尾 SID 文本，输出指针有效且由 LocalFree 释放。
        let converted = unsafe { ConvertStringSidToSidW(encoded.as_ptr(), &mut pointer) };
        if converted == 0 || pointer.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { pointer })
    }
}

impl Drop for LocalSid {
    fn drop(&mut self) {
        // SAFETY: pointer 仅来自 ConvertStringSidToSidW，guard 是唯一所有者。
        let _released = unsafe { LocalFree(self.pointer) };
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{protect_current_user_directory, protect_current_user_file};

    #[test]
    fn applies_and_verifies_exact_private_directory_and_file_dacl() {
        let root = tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let directory = root.path().join("private");
        fs::create_dir(&directory).unwrap_or_else(|error| panic!("私有目录创建失败: {error}"));
        let file = directory.join("capability");
        fs::write(&file, b"private").unwrap_or_else(|error| panic!("私有文件创建失败: {error}"));

        protect_current_user_directory(&directory)
            .unwrap_or_else(|error| panic!("私有目录 DACL 设置失败: {error}"));
        protect_current_user_file(&file)
            .unwrap_or_else(|error| panic!("私有文件 DACL 设置失败: {error}"));
    }
}
