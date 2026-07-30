use std::{ffi::c_void, os::windows::io::RawSocket, ptr};

use windows_sys::Win32::{
    Foundation::ERROR_INSUFFICIENT_BUFFER,
    Networking::WinSock::{
        SIO_QUERY_WFP_CONNECTION_REDIRECT_CONTEXT, SIO_QUERY_WFP_CONNECTION_REDIRECT_RECORDS,
        SIO_SET_WFP_CONNECTION_REDIRECT_RECORDS, SOCKET_ERROR, WSAEINVAL, WSAGetLastError,
        WSAIoctl,
    },
};

use crate::{RedirectContext, WindowsWfpError};

const INITIAL_RECORD_BYTES: usize = 512;
const MAX_RECORD_BYTES: usize = 65_536;
const MAX_CONTEXT_BYTES: usize = 4_384;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedirectMetadata {
    records: Vec<u8>,
    context: RedirectContext,
}

impl RedirectMetadata {
    #[must_use]
    pub fn records(&self) -> &[u8] {
        &self.records
    }

    #[must_use]
    pub const fn context(&self) -> &RedirectContext {
        &self.context
    }
}

pub fn query_redirect_metadata(socket: RawSocket) -> Result<RedirectMetadata, WindowsWfpError> {
    let records = query(
        socket,
        SIO_QUERY_WFP_CONNECTION_REDIRECT_RECORDS,
        MAX_RECORD_BYTES,
    )?;
    let context_bytes = query(
        socket,
        SIO_QUERY_WFP_CONNECTION_REDIRECT_CONTEXT,
        MAX_CONTEXT_BYTES,
    )?;
    let context = RedirectContext::decode(&context_bytes)?;
    Ok(RedirectMetadata { records, context })
}

pub fn apply_redirect_records(socket: RawSocket, records: &[u8]) -> Result<(), WindowsWfpError> {
    if records.is_empty() || records.len() > MAX_RECORD_BYTES {
        return Err(WindowsWfpError::InvalidData(
            "WFP redirect records 长度无效",
        ));
    }
    let input_length =
        u32::try_from(records.len()).map_err(|_| WindowsWfpError::RedirectDataTooLarge)?;
    let socket = socket_value(socket)?;
    let mut returned = 0_u32;
    // SAFETY: socket 由调用方保证在调用期间有效；records 是只读输入缓冲区。
    let result = unsafe {
        WSAIoctl(
            socket,
            SIO_SET_WFP_CONNECTION_REDIRECT_RECORDS,
            records.as_ptr().cast(),
            input_length,
            ptr::null_mut(),
            0,
            &mut returned,
            ptr::null_mut(),
            None,
        )
    };
    if result == SOCKET_ERROR {
        return Err(last_wsa_error("传递 WFP redirect records"));
    }
    Ok(())
}

fn query(socket: RawSocket, code: u32, maximum: usize) -> Result<Vec<u8>, WindowsWfpError> {
    let socket = socket_value(socket)?;
    let mut capacity = INITIAL_RECORD_BYTES.min(maximum);
    loop {
        let mut output = vec![0_u8; capacity];
        let output_length =
            u32::try_from(output.len()).map_err(|_| WindowsWfpError::RedirectDataTooLarge)?;
        let mut returned = 0_u32;
        // SAFETY: socket 由调用方保证有效；output 是可写缓冲区且长度已转换为 u32。
        let result = unsafe {
            WSAIoctl(
                socket,
                code,
                ptr::null(),
                0,
                output.as_mut_ptr().cast::<c_void>(),
                output_length,
                &mut returned,
                ptr::null_mut(),
                None,
            )
        };
        if result != SOCKET_ERROR {
            let returned =
                usize::try_from(returned).map_err(|_| WindowsWfpError::RedirectDataTooLarge)?;
            if returned == 0 || returned > output.len() {
                return Err(WindowsWfpError::InvalidData(
                    "WFP redirect 查询返回长度无效",
                ));
            }
            output.truncate(returned);
            return Ok(output);
        }
        // SAFETY: WSAGetLastError 无前置条件，紧随失败的 WSAIoctl 调用。
        let code = unsafe { WSAGetLastError() };
        let required = usize::try_from(returned).unwrap_or(maximum.saturating_add(1));
        let Some(next) = next_query_capacity(code, required, capacity, maximum) else {
            return Err(WindowsWfpError::windows(
                "查询 WFP redirect 元数据",
                u32::from_ne_bytes(code.to_ne_bytes()),
            ));
        };
        capacity = next;
    }
}

fn next_query_capacity(
    code: i32,
    required: usize,
    current: usize,
    maximum: usize,
) -> Option<usize> {
    let insufficient = code == ERROR_INSUFFICIENT_BUFFER as i32;
    if !insufficient && code != WSAEINVAL {
        return None;
    }
    if insufficient && required > current && required <= maximum {
        return Some(required);
    }
    let next = current.saturating_mul(2).min(maximum);
    (next > current).then_some(next)
}

fn socket_value(socket: RawSocket) -> Result<usize, WindowsWfpError> {
    usize::try_from(socket).map_err(|_| WindowsWfpError::InvalidData("Windows Socket 句柄溢出"))
}

fn last_wsa_error(operation: &'static str) -> WindowsWfpError {
    // SAFETY: WSAGetLastError 无前置条件，紧随失败的 Winsock 调用。
    let code = unsafe { WSAGetLastError() };
    WindowsWfpError::windows(operation, u32::from_ne_bytes(code.to_ne_bytes()))
}

#[cfg(test)]
mod tests {
    use super::next_query_capacity;
    use windows_sys::Win32::{
        Foundation::ERROR_INSUFFICIENT_BUFFER, Networking::WinSock::WSAEINVAL,
    };

    #[test]
    fn redirect_query_grows_from_required_size_or_bounded_retry() {
        assert_eq!(
            next_query_capacity(ERROR_INSUFFICIENT_BUFFER as i32, 1_024, 512, 65_536),
            Some(1_024)
        );
        assert_eq!(next_query_capacity(WSAEINVAL, 0, 512, 4_384), Some(1_024));
        assert_eq!(next_query_capacity(WSAEINVAL, 0, 4_384, 4_384), None);
        assert_eq!(next_query_capacity(WSAEINVAL + 1, 1_024, 512, 4_384), None);
    }
}
