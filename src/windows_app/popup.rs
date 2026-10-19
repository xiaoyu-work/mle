use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{DataExchange::*, LibraryLoader::*, Memory::*},
        UI::{
            Controls::{RichEdit::*, *},
            HiDpi::*,
            Input::KeyboardAndMouse::EnableWindow,
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

use super::{DISMISS_EVENT, HOST, send_message, show_error, theme, wide};
use crate::{AppResult, dictionary};
use std::sync::atomic::Ordering;

const SOURCE: i32 = 301;
const STATUS: i32 = 302;
const BODY: i32 = 303;
const COPY: i32 = 304;
const CLOSE: i32 = 305;

pub enum Content {
    Loading,
    Dictionary(String),
    Translation(String),
    Error(String),
}

pub struct Window {
    pub hwnd: HWND,
    source: HWND,
    status: HWND,
    body: HWND,
    copy: HWND,
    title_font: theme::Font,
    source_font: theme::Font,
    label_font: theme::Font,
    dpi: u32,
}

impl Window {
    pub fn create(host: HWND) -> AppResult<Self> {
        unsafe {
            // RichEdit is supplied by Windows; no rendering runtime is bundled.
            LoadLibraryW(w!("Msftedit.dll"))?;
            let instance = GetModuleHandleW(None)?;
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("MLE.Popup"),
                w!("MLE translation"),
                WS_POPUP | WS_CLIPCHILDREN,
                0,
                0,
                480,
                280,
                Some(host),
                None,
                Some(instance.into()),
                None,
            )?;
            theme::polish(hwnd, true);
            let create = |class, title, style, id| -> AppResult<HWND> {
                Ok(CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    title,
                    WS_CHILD | WS_VISIBLE | style,
                    0,
                    0,
                    0,
                    0,
                    Some(hwnd),
                    Some(HMENU(id as *mut _)),
                    Some(instance.into()),
                    None,
                )?)
            };
            let source = create(w!("STATIC"), w!(""), WINDOW_STYLE(0), SOURCE)?;
            let status = create(w!("STATIC"), w!(""), WINDOW_STYLE(0), STATUS)?;
            let body = create(
                w!("RICHEDIT50W"),
                w!(""),
                WS_VSCROLL
                    | WINDOW_STYLE(
                        ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32,
                    ),
                BODY,
            )?;
            send_message(
                body,
                EM_SETBKGNDCOLOR,
                WPARAM(0),
                LPARAM(theme::SURFACE.0 as isize),
            );
            send_message(body, EM_SETTARGETDEVICE, WPARAM(0), LPARAM(0));
            send_message(body, EM_EXLIMITTEXT, WPARAM(0), LPARAM(100_000));
            send_message(
                body,
                EM_SETEVENTMASK,
                WPARAM(0),
                LPARAM(ENM_REQUESTRESIZE as isize),
            );
            let copy = create(
                w!("BUTTON"),
                w!("Copy"),
                WINDOW_STYLE(BS_OWNERDRAW as u32),
                COPY,
            )?;
            let close = create(
                w!("BUTTON"),
                w!("\u{00d7}"),
                WINDOW_STYLE(BS_OWNERDRAW as u32),
                CLOSE,
            )?;
            theme::button(copy, theme::QUIET);
            theme::button(close, theme::QUIET);
            let dpi = GetDpiForWindow(hwnd);
            let title_font = theme::Font::new(dpi, 26, 600)?;
            let source_font = theme::Font::new(dpi, 15, 400)?;
            let label_font = theme::Font::new(dpi, 11, 600)?;
            label_font.apply(status);
            let window = Self {
                hwnd,
                source,
                status,
                body,
                copy,
                title_font,
                source_font,
                label_font,
                dpi,
            };
            layout(hwnd)?;
            Ok(window)
        }
    }

    pub fn render(&mut self, source: &str, content: &Content) -> AppResult<()> {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        if dpi != self.dpi {
            self.title_font = theme::Font::new(dpi, 26, 600)?;
            self.source_font = theme::Font::new(dpi, 15, 400)?;
            self.label_font = theme::Font::new(dpi, 11, 600)?;
            self.dpi = dpi;
        }
        if source.chars().count() <= 40 && !source.contains('\n') {
            self.title_font.apply(self.source);
        } else {
            self.source_font.apply(self.source);
        }
        self.label_font.apply(self.status);
        theme::set_text(
            self.source,
            if source.is_empty() {
                if matches!(content, Content::Loading) {
                    "Reading selection..."
                } else {
                    "Translation unavailable"
                }
            } else {
                source
            },
        )?;
        let mut spans = Vec::new();
        let (status, body, color) = match content {
            Content::Loading => (
                "TRANSLATING".to_owned(),
                "A moment, finding the right words...".to_owned(),
                theme::MUTED,
            ),
            Content::Dictionary(meaning) => {
                let senses = dictionary::senses(meaning);
                let mut body = String::new();
                for (index, sense) in senses.iter().enumerate() {
                    let start = body.encode_utf16().count() as i32;
                    body.push_str(&format!("{:02}", index + 1));
                    if let Some(domain) = sense.domain {
                        body.push_str(&format!("   {domain}"));
                    }
                    spans.push((start, body.encode_utf16().count() as i32));
                    body.push('\r');
                    body.push_str(sense.meaning);
                    body.push('\r');
                }
                (
                    format!(
                        "DICTIONARY  /  {} {}",
                        senses.len(),
                        if senses.len() == 1 {
                            "ENTRY"
                        } else {
                            "ENTRIES"
                        }
                    ),
                    body.trim_end_matches('\r').to_owned(),
                    theme::TEXT,
                )
            }
            Content::Translation(text) => (
                "TRANSLATION".to_owned(),
                text.replace("\r\n", "\n")
                    .replace('\r', "\n")
                    .replace('\n', "\r"),
                theme::TEXT,
            ),
            Content::Error(error) => (
                "UNABLE TO TRANSLATE".to_owned(),
                error.to_owned(),
                theme::ERROR,
            ),
        };
        theme::set_text(self.status, &status)?;
        send_message(self.body, WM_SETREDRAW, WPARAM(0), LPARAM(0));
        let update = (|| -> AppResult<()> {
            theme::set_text(self.body, &body)?;
            let mut format = CHARFORMAT2W::default();
            format.Base.cbSize = size_of::<CHARFORMAT2W>() as u32;
            format.Base.dwMask = CFM_FACE | CFM_SIZE | CFM_COLOR | CFM_BOLD;
            format.Base.yHeight = 255;
            format.Base.crTextColor = color;
            let face = wide("Microsoft YaHei UI");
            format.Base.szFaceName[..face.len()].copy_from_slice(&face);
            send_message(
                self.body,
                EM_SETCHARFORMAT,
                WPARAM(SCF_ALL as usize),
                LPARAM((&format as *const CHARFORMAT2W) as isize),
            );
            let mut paragraph = PARAFORMAT2::default();
            paragraph.Base.cbSize = size_of::<PARAFORMAT2>() as u32;
            paragraph.Base.dwMask = PFM_SPACEAFTER | PFM_LINESPACING;
            paragraph.dySpaceAfter = 180;
            paragraph.bLineSpacingRule = 0;
            let selection = CHARRANGE {
                cpMin: 0,
                cpMax: -1,
            };
            send_message(
                self.body,
                EM_EXSETSEL,
                WPARAM(0),
                LPARAM((&selection as *const CHARRANGE) as isize),
            );
            send_message(
                self.body,
                EM_SETPARAFORMAT,
                WPARAM(0),
                LPARAM((&paragraph as *const PARAFORMAT2) as isize),
            );
            format.Base.yHeight = 165;
            format.Base.dwEffects = CFE_BOLD;
            format.Base.crTextColor = theme::ACCENT;
            paragraph.dySpaceAfter = 45;
            for (start, end) in spans {
                let range = CHARRANGE {
                    cpMin: start,
                    cpMax: end,
                };
                send_message(
                    self.body,
                    EM_EXSETSEL,
                    WPARAM(0),
                    LPARAM((&range as *const CHARRANGE) as isize),
                );
                send_message(
                    self.body,
                    EM_SETCHARFORMAT,
                    WPARAM(SCF_SELECTION as usize),
                    LPARAM((&format as *const CHARFORMAT2W) as isize),
                );
                send_message(
                    self.body,
                    EM_SETPARAFORMAT,
                    WPARAM(0),
                    LPARAM((&paragraph as *const PARAFORMAT2) as isize),
                );
            }
            let selection = CHARRANGE { cpMin: 0, cpMax: 0 };
            send_message(
                self.body,
                EM_EXSETSEL,
                WPARAM(0),
                LPARAM((&selection as *const CHARRANGE) as isize),
            );
            send_message(self.body, EM_SCROLLCARET, WPARAM(0), LPARAM(0));
            theme::set_text(self.copy, "Copy")?;
            unsafe {
                let _ = EnableWindow(
                    self.copy,
                    matches!(content, Content::Dictionary(_) | Content::Translation(_)),
                );
            }
            send_message(self.body, EM_REQUESTRESIZE, WPARAM(0), LPARAM(0));
            let document_height = unsafe { GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) } as i32;
            if document_height <= 0 {
                return Err("Could not measure the translation layout.".into());
            }
            let height = (theme::px(self.hwnd, 210) + document_height)
                .clamp(theme::px(self.hwnd, 266), theme::px(self.hwnd, 530));
            fit_height(self.hwnd, height)?;
            Ok(())
        })();
        send_message(self.body, WM_SETREDRAW, WPARAM(1), LPARAM(0));
        theme::invalidate(self.body);
        theme::invalidate(self.hwnd);
        update
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

pub fn layout(hwnd: HWND) -> AppResult<()> {
    unsafe {
        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect)?;
        let unit = |value| theme::px(hwnd, value);
        for (id, x, y, width, height) in [
            (SOURCE, unit(24), unit(78), rect.right - unit(48), unit(56)),
            (STATUS, unit(24), unit(146), rect.right - unit(48), unit(20)),
            (
                BODY,
                unit(24),
                unit(178),
                rect.right - unit(42),
                (rect.bottom - unit(202)).max(unit(32)),
            ),
            (COPY, rect.right - unit(120), unit(18), unit(64), unit(32)),
            (CLOSE, rect.right - unit(48), unit(18), unit(28), unit(32)),
        ] {
            if let Ok(child) = GetDlgItem(Some(hwnd), id) {
                MoveWindow(child, x, y, width, height, true)?;
            }
        }
    }
    Ok(())
}

fn copy(hwnd: HWND) -> AppResult<()> {
    unsafe {
        let body = GetDlgItem(Some(hwnd), BODY)?;
        let value = wide(&theme::text(body));
        let memory = GlobalAlloc(GMEM_MOVEABLE, value.len() * size_of::<u16>())?;
        let pointer = GlobalLock(memory);
        if pointer.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err(windows::core::Error::from_thread().into());
        }
        std::ptr::copy_nonoverlapping(value.as_ptr(), pointer.cast(), value.len());
        let _ = GlobalUnlock(memory);
        let mut transferred = false;
        let result = (|| -> AppResult<()> {
            OpenClipboard(Some(hwnd))?;
            let result = EmptyClipboard()
                .and_then(|_| SetClipboardData(13, Some(HANDLE(memory.0))))
                .map(|_| transferred = true);
            let closed = CloseClipboard();
            result?;
            closed?;
            Ok(())
        })();
        if !transferred {
            let _ = GlobalFree(Some(memory));
        }
        result?;
        theme::set_text(GetDlgItem(Some(hwnd), COPY)?, "Copied")?;
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
        if message == WM_CTLCOLORSTATIC {
            let hdc = HDC(wp.0 as *mut _);
            SetTextColor(
                hdc,
                if GetDlgCtrlID(HWND(lp.0 as *mut _)) == STATUS {
                    theme::ACCENT
                } else {
                    theme::TEXT
                },
            );
            SetBkColor(hdc, theme::SURFACE);
            return LRESULT(GetStockObject(WHITE_BRUSH).0 as isize);
        }
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
                let canvas = theme::Canvas::new(hdc, hwnd);
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                canvas.fill(rect, theme::SURFACE);
                canvas.logo(22, 18, 32);
                canvas.label("MLE", canvas.rect(64, 18, 120, 32), 16, 600, theme::TEXT);
                canvas.fill(
                    RECT {
                        top: canvas.scale(64),
                        bottom: canvas.scale(65),
                        ..rect
                    },
                    theme::BORDER,
                );
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_DRAWITEM => {
                theme::draw_button(&*(lp.0 as *const DRAWITEMSTRUCT));
                LRESULT(1)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_SIZE => {
                if let Err(error) = layout(hwnd) {
                    super::defer_error(error.to_string());
                }
                LRESULT(0)
            }
            WM_COMMAND if wp.0 & 0xffff == COPY as usize => {
                if let Err(error) = copy(hwnd) {
                    show_error(&error.to_string());
                }
                LRESULT(0)
            }
            WM_CLOSE | WM_COMMAND if message == WM_CLOSE || wp.0 & 0xffff == CLOSE as usize => {
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
