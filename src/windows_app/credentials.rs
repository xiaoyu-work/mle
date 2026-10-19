use windows::{
    Win32::{Foundation::ERROR_NOT_FOUND, Security::Credentials::*},
    core::{PCWSTR, PWSTR, w},
};

use crate::AppResult;

const TARGET: PCWSTR = w!("MLE/AzureOpenAI");

pub fn read() -> AppResult<Option<String>> {
    unsafe {
        let mut credential = std::ptr::null_mut();
        if let Err(error) = CredReadW(TARGET, CRED_TYPE_GENERIC, None, &mut credential) {
            if error.code() == ERROR_NOT_FOUND.to_hresult() {
                return Ok(None);
            }
            return Err(error.into());
        }
        let value = &*credential;
        let bytes = if value.CredentialBlobSize == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(value.CredentialBlob, value.CredentialBlobSize as usize)
        };
        let result = String::from_utf8(bytes.to_vec());
        CredFree(credential.cast());
        Ok(Some(result?))
    }
}

pub fn write(key: &str) -> AppResult<()> {
    if key.is_empty()
        || key.len() > 512
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        return Err("Enter an Azure OpenAI API key, not a URL.".into());
    }
    let mut target: Vec<u16> = "MLE/AzureOpenAI\0".encode_utf16().collect();
    let mut username: Vec<u16> = "AzureOpenAI\0".encode_utf16().collect();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        CredentialBlobSize: key.len() as u32,
        CredentialBlob: key.as_ptr() as *mut u8,
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(username.as_mut_ptr()),
        ..Default::default()
    };
    unsafe {
        CredWriteW(&credential, 0)?;
    }
    Ok(())
}

pub fn delete() -> AppResult<()> {
    unsafe {
        if let Err(error) = CredDeleteW(TARGET, CRED_TYPE_GENERIC, None)
            && error.code() != ERROR_NOT_FOUND.to_hresult()
        {
            return Err(error.into());
        }
    }
    Ok(())
}
