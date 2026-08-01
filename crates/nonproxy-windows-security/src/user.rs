use std::{io, ptr, slice};

use windows_sys::{
    Win32::{
        Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, HANDLE, LocalFree},
        Security::{
            Authorization::ConvertSidToStringSidW, GetTokenInformation, IsValidSid, TOKEN_QUERY,
            TOKEN_USER, TokenSessionId, TokenUser,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    },
    core::PWSTR,
};

use crate::validate_interactive_user_sid;

const MAXIMUM_SID_TEXT_UNITS: usize = 184;

pub fn current_process_user_sid() -> io::Result<String> {
    let token = open_current_process_token()?;
    require_interactive_user_session(token.handle)?;
    let mut required = 0_u32;
    // SAFETY: token 是有效查询句柄；空缓冲区调用只获取所需长度。
    let first =
        unsafe { GetTokenInformation(token.handle, TokenUser, ptr::null_mut(), 0, &mut required) };
    if first != 0
        || required == 0
        || io::Error::last_os_error().raw_os_error()
            != i32::try_from(ERROR_INSUFFICIENT_BUFFER).ok()
    {
        return Err(io::Error::last_os_error());
    }
    let bytes = usize::try_from(required).map_err(|_| io::Error::other("TokenUser 长度溢出"))?;
    let words = bytes.div_ceil(size_of::<usize>());
    let mut storage = vec![0_usize; words];
    // SAFETY: storage 以 usize 对齐且至少有 required 字节；token 和输出长度有效。
    let loaded = unsafe {
        GetTokenInformation(
            token.handle,
            TokenUser,
            storage.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    };
    if loaded == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: GetTokenInformation(TokenUser) 成功写入至少一个 TOKEN_USER。
    let user = unsafe { &*storage.as_ptr().cast::<TOKEN_USER>() };
    if user.User.Sid.is_null() || unsafe { IsValidSid(user.User.Sid) } == 0 {
        return Err(io::Error::other("当前进程用户 SID 无效"));
    }
    sid_to_string(user.User.Sid)
}

fn require_interactive_user_session(token: HANDLE) -> io::Result<()> {
    let mut session_id = 0_u32;
    let mut returned = 0_u32;
    let session_id_bytes =
        u32::try_from(size_of::<u32>()).map_err(|_| io::Error::other("SessionId 长度溢出"))?;
    // SAFETY: token 是有效查询句柄；session_id 缓冲区及返回长度指针均有效。
    let loaded = unsafe {
        GetTokenInformation(
            token,
            TokenSessionId,
            (&raw mut session_id).cast(),
            session_id_bytes,
            &mut returned,
        )
    };
    if loaded == 0 {
        return Err(io::Error::last_os_error());
    }
    if returned != session_id_bytes || session_id == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Windows Adapter 只能在交互用户会话中运行",
        ));
    }
    Ok(())
}

fn open_current_process_token() -> io::Result<OwnedHandle> {
    let mut handle: HANDLE = ptr::null_mut();
    // SAFETY: GetCurrentProcess 返回当前进程伪句柄，输出指针有效。
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) };
    if opened == 0 || handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    Ok(OwnedHandle { handle })
}

fn sid_to_string(sid: *mut core::ffi::c_void) -> io::Result<String> {
    let mut text: PWSTR = ptr::null_mut();
    // SAFETY: sid 已由 IsValidSid 验证，输出指针由 LocalFree 释放。
    let converted = unsafe { ConvertSidToStringSidW(sid, &mut text) };
    if converted == 0 || text.is_null() {
        return Err(io::Error::last_os_error());
    }
    let text = LocalString { pointer: text };
    let mut length = 0_usize;
    // SAFETY: API 返回 NUL 结尾缓冲区；硬上限避免无界扫描。
    while length <= MAXIMUM_SID_TEXT_UNITS && unsafe { *text.pointer.add(length) } != 0 {
        length += 1;
    }
    if length == 0 || length > MAXIMUM_SID_TEXT_UNITS {
        return Err(io::Error::other("当前进程用户 SID 文本无效"));
    }
    // SAFETY: 已确认前 length 个 UTF-16 单元可读且随后是 NUL。
    let value = String::from_utf16(unsafe { slice::from_raw_parts(text.pointer, length) })
        .map_err(|_| io::Error::other("当前进程用户 SID 不是有效 UTF-16"))?;
    validate_interactive_user_sid(&value)?;
    Ok(value)
}

struct OwnedHandle {
    handle: HANDLE,
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: handle 仅来自成功的 OpenProcessToken，guard 是唯一所有者。
        let _closed = unsafe { CloseHandle(self.handle) };
    }
}

struct LocalString {
    pointer: PWSTR,
}

impl Drop for LocalString {
    fn drop(&mut self) {
        // SAFETY: pointer 仅来自成功的 ConvertSidToStringSidW，guard 是唯一所有者。
        let _released = unsafe { LocalFree(self.pointer.cast()) };
    }
}
