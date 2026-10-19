use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::LibraryLoader::GetModuleHandleW,
        UI::{Controls::*, HiDpi::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};

use super::{send_message, wide};
use crate::AppResult;

pub const BACKGROUND: COLORREF = SURFACE;
pub const SURFACE: COLORREF = rgb(255, 255, 255);
pub const TEXT: COLORREF = rgb(30, 35, 50);
pub const MUTED: COLORREF = rgb(112, 119, 138);
pub const BORDER: COLORREF = rgb(227, 230, 239);
pub const ACCENT: COLORREF = rgb(91, 83, 219);
pub const TINT: COLORREF = rgb(239, 237, 253);
pub const ERROR: COLORREF = rgb(179, 65, 76);

pub const PRIMARY: isize = 1;
pub const QUIET: isize = 2;
pub const CHOICE: isize = 3;
const SELECTED: isize = 16;

pub const fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    COLORREF(red as u32 | (green as u32) << 8 | (blue as u32) << 16)
}

pub fn px(hwnd: HWND, value: i32) -> i32 {
    unsafe { value * GetDpiForWindow(hwnd).max(96) as i32 / 96 }
}

pub struct Font(pub HFONT);

impl Font {
    pub fn new(dpi: u32, size: i32, weight: i32) -> AppResult<Self> {
        let font = unsafe {
            CreateFontW(
                -size * dpi as i32 / 96,
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                w!("Segoe UI"),
            )
        };
        if font.is_invalid() {
            return Err(windows::core::Error::from_thread().into());
        }
        Ok(Self(font))
    }

    pub fn apply(&self, hwnd: HWND) {
        send_message(hwnd, WM_SETFONT, WPARAM(self.0.0 as usize), LPARAM(1));
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0.into());
        }
    }
}

pub fn text(hwnd: HWND) -> String {
    unsafe {
        let mut buffer = vec![0; GetWindowTextLengthW(hwnd) as usize + 1];
        let count = GetWindowTextW(hwnd, &mut buffer);
        String::from_utf16_lossy(&buffer[..count as usize])
    }
}

pub fn invalidate(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

pub fn polish(hwnd: HWND, popup: bool) {
    unsafe {
        // Cosmetic attributes are optional on Windows 10; functionality is unchanged.
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&DWMWCP_ROUND as *const DWM_WINDOW_CORNER_PREFERENCE).cast(),
            4,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            (&BORDER as *const COLORREF).cast(),
            4,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            (&BACKGROUND as *const COLORREF).cast(),
            4,
        );
        if popup {
            let margins = MARGINS {
                cxLeftWidth: 1,
                cxRightWidth: 1,
                cyTopHeight: 1,
                cyBottomHeight: 1,
            };
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        }
    }
}

pub fn button(hwnd: HWND, style: isize) {
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, style);
    }
}

pub fn select_button(hwnd: HWND, selected: bool) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWLP_USERDATA) & !SELECTED;
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            style | if selected { SELECTED } else { 0 },
        );
    }
    invalidate(hwnd);
}

pub fn is_selected(hwnd: HWND) -> bool {
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) & SELECTED != 0 }
}

pub fn icon(size: i32) -> AppResult<HICON> {
    unsafe {
        let handle = LoadImageW(
            Some(GetModuleHandleW(None)?.into()),
            PCWSTR(std::ptr::without_provenance(1)),
            IMAGE_ICON,
            size,
            size,
            LR_SHARED,
        )?;
        Ok(HICON(handle.0))
    }
}

pub fn draw_button(item: &DRAWITEMSTRUCT) {
    let canvas = Canvas::new(item.hDC, item.hwndItem);
    let state = unsafe { GetWindowLongPtrW(item.hwndItem, GWLP_USERDATA) };
    let selected = state & SELECTED != 0;
    let pressed = item.itemState.0 & ODS_SELECTED.0 != 0;
    let disabled = item.itemState.0 & ODS_DISABLED.0 != 0;
    let footer = state & 15 == PRIMARY || unsafe { GetDlgCtrlID(item.hwndItem) } == IDCANCEL.0;
    canvas.fill(item.rcItem, if footer { BACKGROUND } else { SURFACE });
    let (fill, color, border) = match state & 15 {
        PRIMARY => (
            if pressed { rgb(73, 65, 192) } else { ACCENT },
            SURFACE,
            None,
        ),
        CHOICE if selected => (TINT, ACCENT, None),
        CHOICE => (SURFACE, MUTED, None),
        _ => (
            if pressed {
                TINT
            } else if footer {
                BACKGROUND
            } else {
                SURFACE
            },
            MUTED,
            None,
        ),
    };
    let mut bounds = item.rcItem;
    bounds.left += canvas.scale(1);
    bounds.top += canvas.scale(1);
    bounds.right -= canvas.scale(1);
    bounds.bottom -= canvas.scale(1);
    canvas.rounded(bounds, 9, fill, border);
    let mut title = wide(&text(item.hwndItem));
    let font = Font::new(
        canvas.dpi,
        14,
        if selected || state & 15 == PRIMARY {
            600
        } else {
            400
        },
    );
    unsafe {
        let old = font
            .as_ref()
            .ok()
            .map(|font| SelectObject(item.hDC, font.0.into()));
        SetBkMode(item.hDC, TRANSPARENT);
        SetTextColor(item.hDC, if disabled { BORDER } else { color });
        let length = title.len() - 1;
        DrawTextW(
            item.hDC,
            &mut title[..length],
            &mut bounds,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        if let Some(old) = old {
            SelectObject(item.hDC, old);
        }
        if item.itemState.0 & ODS_FOCUS.0 != 0 {
            bounds.left += canvas.scale(4);
            bounds.top += canvas.scale(4);
            bounds.right -= canvas.scale(4);
            bounds.bottom -= canvas.scale(4);
            let _ = DrawFocusRect(item.hDC, &bounds);
        }
    }
}

pub struct Canvas {
    pub hdc: HDC,
    pub dpi: u32,
}

impl Canvas {
    pub fn new(hdc: HDC, hwnd: HWND) -> Self {
        Self {
            hdc,
            dpi: unsafe { GetDpiForWindow(hwnd).max(96) },
        }
    }

    pub fn scale(&self, value: i32) -> i32 {
        value * self.dpi as i32 / 96
    }

    pub fn rect(&self, x: i32, y: i32, width: i32, height: i32) -> RECT {
        RECT {
            left: self.scale(x),
            top: self.scale(y),
            right: self.scale(x + width),
            bottom: self.scale(y + height),
        }
    }

    pub fn fill(&self, rect: RECT, color: COLORREF) {
        unsafe {
            let brush = CreateSolidBrush(color);
            FillRect(self.hdc, &rect, brush);
            let _ = DeleteObject(brush.into());
        }
    }

    pub fn rounded(&self, rect: RECT, radius: i32, color: COLORREF, border: Option<COLORREF>) {
        unsafe {
            let brush = CreateSolidBrush(color);
            let pen = CreatePen(PS_SOLID, self.scale(1).max(1), border.unwrap_or(color));
            let old_brush = SelectObject(self.hdc, brush.into());
            let old_pen = SelectObject(self.hdc, pen.into());
            let _ = RoundRect(
                self.hdc,
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                self.scale(radius * 2),
                self.scale(radius * 2),
            );
            SelectObject(self.hdc, old_pen);
            SelectObject(self.hdc, old_brush);
            let _ = DeleteObject(pen.into());
            let _ = DeleteObject(brush.into());
        }
    }

    pub fn label(&self, label: &str, rect: RECT, size: i32, weight: i32, color: COLORREF) {
        let Ok(font) = Font::new(self.dpi, size, weight) else {
            return;
        };
        let mut label: Vec<u16> = label.encode_utf16().collect();
        let mut rect = rect;
        unsafe {
            let previous = SelectObject(self.hdc, font.0.into());
            SetBkMode(self.hdc, TRANSPARENT);
            SetTextColor(self.hdc, color);
            DrawTextW(
                self.hdc,
                &mut label,
                &mut rect,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
            SelectObject(self.hdc, previous);
        }
    }
}

pub fn control_colors(message: u32, wp: WPARAM) -> Option<LRESULT> {
    if message == WM_CTLCOLOREDIT || message == WM_CTLCOLORSTATIC {
        unsafe {
            let hdc = HDC(wp.0 as *mut _);
            SetTextColor(hdc, TEXT);
            SetBkColor(hdc, SURFACE);
            return Some(LRESULT(GetStockObject(WHITE_BRUSH).0 as isize));
        }
    }
    None
}

pub fn set_text(hwnd: HWND, text: &str) -> AppResult<()> {
    let text = wide(text);
    unsafe {
        SetWindowTextW(hwnd, PCWSTR(text.as_ptr()))?;
    }
    Ok(())
}
