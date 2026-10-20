use crate::AppResult;
use std::time::{Duration, Instant};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::{DeleteObject, GetObjectW, HGDIOBJ},
        System::{DataExchange::*, LibraryLoader::GetModuleHandleW, Memory::*, Ole::*},
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::w,
};

const UNICODE: u32 = 13;
const BITMAP: u32 = 2;
const MAX_BACKUP: usize = 64 * 1024 * 1024;

struct Open;
impl Open {
    fn new(owner: Option<HWND>) -> AppResult<Self> {
        let deadline = Instant::now() + Duration::from_millis(300);
        loop {
            match unsafe { OpenClipboard(owner) } {
                Ok(()) => return Ok(Self),
                Err(error) if Instant::now() >= deadline => {
                    return Err(format!("The clipboard is busy: {error}").into());
                }
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}
impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

struct Data {
    format: u32,
    handle: HANDLE,
}
impl Drop for Data {
    fn drop(&mut self) {
        if self.handle.is_invalid() {
            return;
        }
        unsafe {
            if self.format == BITMAP {
                let _ = DeleteObject(HGDIOBJ(self.handle.0));
            } else {
                let _ = GlobalFree(Some(HGLOBAL(self.handle.0)));
            }
        }
    }
}

struct Owner(HWND);
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

fn snapshot() -> AppResult<(Vec<Data>, u32)> {
    let _open = Open::new(None)?;
    let mut saved = Vec::new();
    let mut format = 0;
    let mut total: usize = 0;
    loop {
        unsafe {
            SetLastError(ERROR_SUCCESS);
            format = EnumClipboardFormats(format);
            if format == 0 {
                if GetLastError() != ERROR_SUCCESS {
                    return Err(windows::core::Error::from_thread().into());
                }
                break;
            }
            let source = GetClipboardData(format)?;
            // Owner-display, palettes and metafiles need specialized ownership. Never
            // risk the user's clipboard when we cannot back up every format.
            if matches!(format, 3 | 9 | 14 | 0x80..=0x8e) {
                return Err(
                    "Cannot safely preserve this clipboard format. Copy was not triggered.".into(),
                );
            }
            let bytes = if format == BITMAP {
                let mut bitmap = windows::Win32::Graphics::Gdi::BITMAP::default();
                if GetObjectW(
                    HGDIOBJ(source.0),
                    size_of_val(&bitmap) as i32,
                    Some((&mut bitmap as *mut windows::Win32::Graphics::Gdi::BITMAP).cast()),
                ) == 0
                {
                    return Err(
                        "Cannot back up the clipboard bitmap. Copy was not triggered.".into(),
                    );
                }
                (bitmap.bmWidthBytes.unsigned_abs() as usize)
                    .checked_mul(bitmap.bmHeight.unsigned_abs() as usize)
                    .and_then(|bytes| bytes.checked_mul(bitmap.bmPlanes as usize))
                    .ok_or("Clipboard bitmap is too large.")?
            } else {
                GlobalSize(HGLOBAL(source.0))
            };
            if bytes == 0 {
                return Err("Cannot fully back up the clipboard. Copy was not triggered.".into());
            }
            total = total
                .checked_add(bytes)
                .ok_or("Clipboard backup is too large.")?;
            if total > MAX_BACKUP {
                return Err(
                    "Clipboard exceeds the 64 MB backup limit. Copy was not triggered.".into(),
                );
            }
            let handle = OleDuplicateData(source, CLIPBOARD_FORMAT(format as u16), GMEM_MOVEABLE);
            if handle.is_invalid() {
                return Err(format!(
                    "Could not preserve clipboard format {format}. Copy was not triggered."
                )
                .into());
            }
            saved.push(Data { format, handle });
        }
    }
    Ok((saved, unsafe { GetClipboardSequenceNumber() }))
}

fn restore(saved: &mut [Data], _open: &Open, sequence: u32) -> AppResult<bool> {
    unsafe {
        if GetClipboardSequenceNumber() != sequence {
            return Ok(false);
        }
        EmptyClipboard()?;
        for data in saved {
            SetClipboardData(data.format, Some(data.handle)).map_err(|error| {
                format!(
                    "Could not restore clipboard format {}: {error}",
                    data.format
                )
            })?;
            data.handle = HANDLE::default();
        }
    }
    Ok(true)
}

fn text() -> AppResult<String> {
    unsafe {
        let handle = HGLOBAL(GetClipboardData(UNICODE)?.0);
        let length = GlobalSize(handle) / 2;
        let memory = GlobalLock(handle);
        if memory.is_null() {
            return Err("Could not read copied text.".into());
        }
        let units = std::slice::from_raw_parts(memory.cast::<u16>(), length);
        let end = units
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(units.len());
        let result = if end > 10000 {
            Err("Please select at most 5000 characters.".into())
        } else {
            String::from_utf16(&units[..end]).map_err(Into::into)
        };
        let _ = GlobalUnlock(handle);
        result
    }
}

pub fn copy_selection(source: HWND, focused_process: u32) -> AppResult<String> {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(source, Some(&mut pid));
        if !super::target_is_accessible(pid) {
            return Err("Cannot copy from a higher-privilege application.".into());
        }
        // Do not mix Ctrl+C with the still-held trigger shortcut.
        let deadline = Instant::now() + Duration::from_millis(600);
        while [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN, VK_C]
            .iter()
            .any(|key| GetAsyncKeyState(key.0 as i32) < 0)
        {
            if Instant::now() >= deadline {
                return Err("Release the shortcut keys, then try again.".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let owner = Owner(CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("MLE clipboard"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(GetModuleHandleW(None)?.into()),
            None,
        )?);
        let (mut saved, sequence) = snapshot()?;
        if GetForegroundWindow() != source {
            return Err("The active application changed before copying.".into());
        }
        if GetClipboardSequenceNumber() != sequence {
            return Err(
                "The clipboard changed before copying. Its new contents were preserved; try again."
                    .into(),
            );
        }
        let key = |code, flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: code,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        let inputs = [
            key(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
            key(VK_C, KEYBD_EVENT_FLAGS(0)),
            key(VK_C, KEYEVENTF_KEYUP),
            key(VK_CONTROL, KEYEVENTF_KEYUP),
        ];
        let sent = SendInput(&inputs, size_of::<INPUT>() as i32);
        if sent != inputs.len() as u32 {
            let releases = [key(VK_C, KEYEVENTF_KEYUP), key(VK_CONTROL, KEYEVENTF_KEYUP)];
            SendInput(&releases, size_of::<INPUT>() as i32);
        }
        let deadline = Instant::now() + Duration::from_millis(1000);
        while GetClipboardSequenceNumber() == sequence {
            if Instant::now() >= deadline {
                return Err("This application did not copy selected text. The original clipboard was not cleared.".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let copied = {
            let open = Open::new(Some(owner.0))
                .map_err(|error| format!("Could not read and restore the clipboard: {error}"))?;
            let clipboard_owner = GetClipboardOwner()?;
            let mut clipboard_pid = 0;
            GetWindowThreadProcessId(clipboard_owner, Some(&mut clipboard_pid));
            if clipboard_pid != pid
                && clipboard_pid != focused_process
                && GetAncestor(clipboard_owner, GA_ROOTOWNER) != source
            {
                return Err("The clipboard changed in another application. Its new contents were preserved; try again.".into());
            }
            let text = if sent == inputs.len() as u32 {
                text()
            } else {
                Err("Windows blocked part of the copy shortcut. The clipboard was restored.".into())
            };
            let sequence = GetClipboardSequenceNumber();
            if !restore(&mut saved, &open, sequence)? {
                return Err(
                    "The clipboard changed again. Its new contents were preserved; try again."
                        .into(),
                );
            }
            text
        };
        let copied = copied?;
        let copied = copied.trim();
        if copied.is_empty() {
            return Err("The application copied no selected text.".into());
        }
        if copied.chars().count() > 5000 {
            return Err("Please select at most 5000 characters.".into());
        }
        Ok(copied.to_owned())
    }
}
