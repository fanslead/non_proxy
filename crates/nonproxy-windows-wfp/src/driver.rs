use std::{ffi::c_void, mem::MaybeUninit, ptr};

use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
    },
    Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING},
    System::IO::DeviceIoControl,
};

use crate::{IOCTL_APPLY_CONFIG, IOCTL_QUERY_STATUS, WfpConfig, WfpStatus, WindowsWfpError};

const DEVICE_PATH: &str = r"\\.\NonProxyWfp";

pub struct WfpDriver {
    handle: HANDLE,
}

impl WfpDriver {
    pub fn open() -> Result<Self, WindowsWfpError> {
        let path = DEVICE_PATH.encode_utf16().chain([0]).collect::<Vec<_>>();
        // SAFETY: path 是 NUL 结尾 UTF-16；返回 handle 由本对象唯一拥有。
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(last_error("打开 NonProxy WFP 驱动"));
        }
        Ok(Self { handle })
    }

    pub fn apply(&self, config: &WfpConfig) -> Result<WfpStatus, WindowsWfpError> {
        config.validate()?;
        let mut status = MaybeUninit::<WfpStatus>::uninit();
        let bytes = ioctl(
            self.handle,
            IOCTL_APPLY_CONFIG,
            as_input(config),
            status.as_mut_ptr().cast(),
            size_of::<WfpStatus>(),
            "更新 WFP 驱动配置",
        )?;
        if bytes != size_of::<WfpStatus>() {
            return Err(WindowsWfpError::InvalidData("WFP 驱动状态长度无效"));
        }
        // SAFETY: DeviceIoControl 成功并写回完整 WfpStatus。
        let status = unsafe { status.assume_init() };
        status.validate()?;
        Ok(status)
    }

    pub fn status(&self) -> Result<WfpStatus, WindowsWfpError> {
        let mut status = MaybeUninit::<WfpStatus>::uninit();
        let bytes = ioctl(
            self.handle,
            IOCTL_QUERY_STATUS,
            &[],
            status.as_mut_ptr().cast(),
            size_of::<WfpStatus>(),
            "查询 WFP 驱动状态",
        )?;
        if bytes != size_of::<WfpStatus>() {
            return Err(WindowsWfpError::InvalidData("WFP 驱动状态长度无效"));
        }
        // SAFETY: DeviceIoControl 成功并写回完整 WfpStatus。
        let status = unsafe { status.assume_init() };
        status.validate()?;
        Ok(status)
    }
}

impl Drop for WfpDriver {
    fn drop(&mut self) {
        if self.handle != INVALID_HANDLE_VALUE {
            // SAFETY: handle 由本对象唯一拥有，且只关闭一次。
            let _close_result = unsafe { CloseHandle(self.handle) };
            self.handle = INVALID_HANDLE_VALUE;
        }
    }
}

fn ioctl(
    handle: HANDLE,
    code: u32,
    input: &[u8],
    output: *mut c_void,
    output_length: usize,
    operation: &'static str,
) -> Result<usize, WindowsWfpError> {
    let input_length = u32::try_from(input.len())
        .map_err(|_| WindowsWfpError::InvalidData("WFP IOCTL 输入长度溢出"))?;
    let output_length = u32::try_from(output_length)
        .map_err(|_| WindowsWfpError::InvalidData("WFP IOCTL 输出长度溢出"))?;
    let mut returned = 0_u32;
    let input_pointer = if input.is_empty() {
        ptr::null()
    } else {
        input.as_ptr().cast()
    };
    // SAFETY: handle 有效；输入切片和输出缓冲区在同步调用期间保持有效。
    let succeeded = unsafe {
        DeviceIoControl(
            handle,
            code,
            input_pointer,
            input_length,
            output,
            output_length,
            &mut returned,
            ptr::null_mut(),
        )
    };
    if succeeded == 0 {
        return Err(last_error(operation));
    }
    usize::try_from(returned).map_err(|_| WindowsWfpError::InvalidData("WFP IOCTL 返回长度溢出"))
}

fn as_input<T>(value: &T) -> &[u8] {
    // SAFETY: value 在返回切片的借用期内有效，调用方只把字节同步传给 DeviceIoControl。
    unsafe { std::slice::from_raw_parts(ptr::from_ref(value).cast(), size_of::<T>()) }
}

fn last_error(operation: &'static str) -> WindowsWfpError {
    // SAFETY: GetLastError 无前置条件，紧随失败的 Win32 调用读取线程错误码。
    WindowsWfpError::windows(operation, unsafe { GetLastError() })
}

// Windows kernel handle 可在线程间用于同步 DeviceIoControl；对象仍维持唯一关闭所有权。
unsafe impl Send for WfpDriver {}
unsafe impl Sync for WfpDriver {}
