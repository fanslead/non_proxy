use std::{ffi::c_void, io, ptr};

use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1},
        PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
    },
};

pub struct SecureNamedPipeFactory {
    sddl: Vec<u16>,
}

impl SecureNamedPipeFactory {
    pub fn new(sddl: &str) -> io::Result<Self> {
        if sddl.is_empty() || sddl.contains('\0') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "命名管道安全描述符无效",
            ));
        }
        let mut encoded = sddl.encode_utf16().collect::<Vec<_>>();
        encoded.push(0);
        let factory = Self { sddl: encoded };
        let _descriptor = factory.create_descriptor()?;
        Ok(factory)
    }

    pub fn create(&self, options: &ServerOptions, pipe_name: &str) -> io::Result<NamedPipeServer> {
        let descriptor = self.create_descriptor()?;
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
                .map_err(|_| io::Error::other("SECURITY_ATTRIBUTES 长度溢出"))?,
            lpSecurityDescriptor: descriptor.pointer.cast(),
            bInheritHandle: 0,
        };
        // SAFETY: attributes 在同步 CreateNamedPipeW 调用期间保持有效；
        // 描述符由 Windows API 分配，并在调用返回后由 guard 释放。
        unsafe {
            options.create_with_security_attributes_raw(
                pipe_name,
                ptr::from_mut(&mut attributes).cast::<c_void>(),
            )
        }
    }

    fn create_descriptor(&self) -> io::Result<LocalSecurityDescriptor> {
        let mut pointer: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: sddl 是 NUL 结尾的 UTF-16 缓冲区，输出指针有效；
        // 成功时所有权按 API 契约交给 LocalFree。
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                self.sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut pointer,
                ptr::null_mut(),
            )
        };
        if converted == 0 || pointer.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(LocalSecurityDescriptor { pointer })
    }
}

struct LocalSecurityDescriptor {
    pointer: PSECURITY_DESCRIPTOR,
}

impl Drop for LocalSecurityDescriptor {
    fn drop(&mut self) {
        // SAFETY: pointer 仅来自成功的 ConvertStringSecurityDescriptor...
        // 调用，且 guard 是唯一所有者。
        let _result = unsafe { LocalFree(self.pointer.cast()) };
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::SecureNamedPipeFactory;

    #[test]
    fn rejects_empty_or_embedded_nul_sddl_before_ffi() {
        for sddl in ["", "D:P\0(A;;GA;;;SY)"] {
            let error = SecureNamedPipeFactory::new(sddl);
            let Err(error) = error else {
                panic!("无效 SDDL 不应创建命名管道工厂");
            };

            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        }
    }

    #[test]
    fn accepts_protected_local_service_sddl() {
        let factory = SecureNamedPipeFactory::new("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)");

        assert!(factory.is_ok());
    }
}
