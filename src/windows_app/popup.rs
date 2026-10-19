use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::*,
        UI::{
            Controls::{RichEdit::*, *},
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

use super::{DISMISS_EVENT, HOST, send_message, theme, wide};
use crate::{AppResult, dictionary};
use std::sync::atomic::Ordering;

const BODY: i32 = 303;

pub enum Content {
    Loading,
    Dictionary(String),
    Translation(String),
    Error(String),
}

pub struct Window {
    pub hwnd: HWND,
    body: HWND,
}

pub fn create_text(parent: HWND, id: i32) -> AppResult<HWND> {
    unsafe {
        LoadLibraryW(w!("Msftedit.dll"))?;
        let body = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("RICHEDIT50W"),
            w!(""),
            WS_CHILD
                | WS_VISIBLE
                | WS_VSCROLL
                | WINDOW_STYLE(ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32),
            0,
            0,
            0,
            0,
            Some(parent),
            Some(HMENU(id as *mut _)),
            Some(GetModuleHandleW(None)?.into()),
            None,
        )?;
        send_message(
            body,
            EM_SETBKGNDCOLOR,
            WPARAM(0),
            LPARAM(theme::SURFACE.0 as isize),
        );
        send_message(body, EM_SETTARGETDEVICE, WPARAM(0), LPARAM(0));
        send_message(body, EM_EXLIMITTEXT, WPARAM(0), LPARAM(100_000));
        Ok(body)
    }
}

pub fn format_text(body: HWND, font_size: u16, color: COLORREF) {
    let mut format = CHARFORMAT2W::default();
    format.Base.cbSize = size_of::<CHARFORMAT2W>() as u32;
    format.Base.dwMask = CFM_FACE | CFM_SIZE | CFM_COLOR | CFM_BOLD;
    // RichEdit uses twips; preferences use logical pixels at 96 DPI.
    format.Base.yHeight = i32::from(font_size) * 15;
    format.Base.crTextColor = color;
    let face = wide("Microsoft YaHei UI");
    format.Base.szFaceName[..face.len()].copy_from_slice(&face);
    send_message(
        body,
        EM_SETCHARFORMAT,
        WPARAM(SCF_ALL as usize),
        LPARAM((&format as *const CHARFORMAT2W) as isize),
    );
    let selection = CHARRANGE {
        cpMin: 0,
        cpMax: -1,
    };
    send_message(
        body,
        EM_EXSETSEL,
        WPARAM(0),
        LPARAM((&selection as *const CHARRANGE) as isize),
    );
    let mut paragraph = PARAFORMAT2::default();
    paragraph.Base.cbSize = size_of::<PARAFORMAT2>() as u32;
    paragraph.Base.dwMask = PFM_SPACEAFTER | PFM_LINESPACING;
    paragraph.dySpaceAfter = 90;
    paragraph.bLineSpacingRule = 0;
    send_message(
        body,
        EM_SETPARAFORMAT,
        WPARAM(0),
        LPARAM((&paragraph as *const PARAFORMAT2) as isize),
    );
    let selection = CHARRANGE { cpMin: 0, cpMax: 0 };
    send_message(
        body,
        EM_EXSETSEL,
        WPARAM(0),
        LPARAM((&selection as *const CHARRANGE) as isize),
    );
    send_message(body, EM_SCROLLCARET, WPARAM(0), LPARAM(0));
    theme::invalidate(body);
}

impl Window {
    pub fn create(host: HWND) -> AppResult<Self> {
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("MLE.Popup"),
                w!("MLE translation"),
                WS_POPUP | WS_CLIPCHILDREN,
                0,
                0,
                420,
                80,
                Some(host),
                None,
                Some(GetModuleHandleW(None)?.into()),
                None,
            )?;
            theme::polish(hwnd, true);
            let body = create_text(hwnd, BODY)?;
            send_message(
                body,
                EM_SETEVENTMASK,
                WPARAM(0),
                LPARAM(ENM_REQUESTRESIZE as isize),
            );
            layout(hwnd)?;
            Ok(Self { hwnd, body })
        }
    }

    pub fn render(&self, content: &Content, font_size: u16) -> AppResult<()> {
        let (body, color) = match content {
            Content::Loading => ("Translating...".to_owned(), theme::MUTED),
            Content::Dictionary(meaning) => {
                let senses = dictionary::senses(meaning);
                let body = senses
                    .iter()
                    .enumerate()
                    .map(|(index, sense)| {
                        let domain = sense
                            .domain
                            .map(|domain| format!("[{domain}] "))
                            .unwrap_or_default();
                        if senses.len() == 1 {
                            format!("{domain}{}", sense.meaning)
                        } else {
                            format!("{}. {domain}{}", index + 1, sense.meaning)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\r");
                (body, theme::TEXT)
            }
            Content::Translation(text) => (
                text.replace("\r\n", "\n")
                    .replace('\r', "\n")
                    .replace('\n', "\r"),
                theme::TEXT,
            ),
            Content::Error(error) => (error.to_owned(), theme::ERROR),
        };
        send_message(self.body, WM_SETREDRAW, WPARAM(0), LPARAM(0));
        let result = (|| -> AppResult<()> {
            theme::set_text(self.body, &body)?;
            format_text(self.body, font_size, color);
            send_message(self.body, EM_REQUESTRESIZE, WPARAM(0), LPARAM(0));
            let height = unsafe { GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) } as i32;
            if height <= 0 {
                return Err("Could not measure the translation layout.".into());
            }
            fit_height(
                self.hwnd,
                (height + theme::px(self.hwnd, 32))
                    .clamp(theme::px(self.hwnd, 48), theme::px(self.hwnd, 480)),
            )
        })();
        send_message(self.body, WM_SETREDRAW, WPARAM(1), LPARAM(0));
        theme::invalidate(self.body);
        theme::invalidate(self.hwnd);
        result
    }
}

fn fit_height(hwnd: HWND, height: i32) -> AppResult<()> {
    unsafe {
        let mut bounds = RECT::default();
        GetWindowRect(hwnd, &mut bounds)?;
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(
            MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        )
        .as_bool()
        {
            return Err(windows::core::Error::from_thread().into());
        }
        let height = height.min(monitor.rcWork.bottom - monitor.rcWork.top);
        let y = bounds
            .top
            .clamp(monitor.rcWork.top, monitor.rcWork.bottom - height);
        SetWindowPos(
            hwnd,
            None,
            bounds.left,
            y,
            bounds.right - bounds.left,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )?;
    }
    Ok(())
}

fn layout(hwnd: HWND) -> AppResult<()> {
    unsafe {
        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect)?;
        if let Ok(body) = GetDlgItem(Some(hwnd), BODY) {
            let margin = theme::px(hwnd, 14);
            MoveWindow(
                body,
                margin,
                margin,
                (rect.right - margin * 2).max(1),
                (rect.bottom - margin * 2).max(1),
                true,
            )?;
        }
    }
    Ok(())
}

pub unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_NOTIFY if (*(lp.0 as *const NMHDR)).code == EN_REQUESTRESIZE => {
                let request = *(lp.0 as *const REQRESIZE);
                SetWindowLongPtrW(
                    hwnd,
                    GWLP_USERDATA,
                    (request.rc.bottom - request.rc.top) as isize,
                );
                LRESULT(0)
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut paint);
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                theme::Canvas::new(hdc, hwnd).fill(rect, theme::SURFACE);
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_SIZE => {
                if let Err(error) = layout(hwnd) {
                    super::defer_error(error.to_string());
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = PostMessageW(
                    Some(HWND(HOST.load(Ordering::Relaxed) as *mut _)),
                    DISMISS_EVENT,
                    WPARAM(0),
                    LPARAM(0),
                );
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let rect = &*(lp.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                theme::invalidate(hwnd);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wp, lp),
        }
    }
}
