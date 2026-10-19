use std::ffi::c_void;
use windows::{
    Win32::Networking::WinHttp::*,
    core::{PCWSTR, w},
};

use crate::{AppResult, translation};

struct Handle(*mut c_void);

impl Handle {
    fn new(raw: *mut c_void) -> AppResult<Self> {
        if raw.is_null() {
            Err(windows::core::Error::from_thread().into())
        } else {
            Ok(Self(raw))
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

pub struct Client {
    connection: Handle,
    _session: Handle,
}

impl Client {
    pub fn new() -> AppResult<Self> {
        unsafe {
            let session = Handle::new(WinHttpOpen(
                w!("MLE/0.1"),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ))?;
            WinHttpSetTimeouts(session.0, 5000, 5000, 5000, 10000)?;
            let host = super::wide(translation::HOST);
            let connection = Handle::new(WinHttpConnect(session.0, PCWSTR(host.as_ptr()), 443, 0))?;
            Ok(Self {
                connection,
                _session: session,
            })
        }
    }

    pub fn translate(&self, text: &str, key: &str, region: &str) -> AppResult<String> {
        let body = translation::request(text)?;
        let path = super::wide(translation::PATH);
        unsafe {
            let request = Handle::new(WinHttpOpenRequest(
                self.connection.0,
                w!("POST"),
                PCWSTR(path.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            ))?;
            // Never send a subscription key to a redirected host.
            WinHttpSetOption(
                Some(request.0),
                WINHTTP_OPTION_REDIRECT_POLICY,
                Some(&WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_ne_bytes()),
            )?;
            let mut headers =
                format!("Content-Type: application/json\r\nOcp-Apim-Subscription-Key: {key}\r\n");
            if !region.is_empty() {
                headers.push_str(&format!("Ocp-Apim-Subscription-Region: {region}\r\n"));
            }
            let headers = super::wide(&headers);
            WinHttpSendRequest(
                request.0,
                Some(&headers[..headers.len() - 1]),
                Some(body.as_ptr().cast()),
                body.len() as u32,
                body.len() as u32,
                0,
            )?;
            WinHttpReceiveResponse(request.0, std::ptr::null_mut())?;
            let mut status = 0u32;
            let mut length = size_of::<u32>() as u32;
            WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                PCWSTR::null(),
                Some((&mut status as *mut u32).cast()),
                &mut length,
                std::ptr::null_mut(),
            )?;
            if status != 200 {
                let reason = match status {
                    401 | 403 => "Check your subscription key and region in Settings.",
                    429 => "Translation quota or rate limit reached.",
                    _ => "The translation service could not complete the request.",
                };
                return Err(format!("Azure HTTP {status}. {reason}").into());
            }
            let mut response = Vec::new();
            loop {
                let mut buffer = [0u8; 8192];
                let mut read = 0;
                WinHttpReadData(
                    request.0,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                    &mut read,
                )?;
                if read == 0 {
                    break;
                }
                if response.len() + read as usize > 1_000_000 {
                    return Err("Translation response exceeded the size limit.".into());
                }
                response.extend_from_slice(&buffer[..read as usize]);
            }
            translation::response(&response)
        }
    }
}
