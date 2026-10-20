use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::*, HiDpi::*, Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::*,
        },
    },
    core::{PCWSTR, w},
};

use super::{APP, credentials, popup, providers, send_message, shortcut, show_error, theme, wide};
use crate::{
    AppResult,
    config::{Config, Provider, Trigger},
};

const SAVE: usize = 200;
const RIGHT_HOLD: i32 = 210;
const HOTKEY: i32 = 211;
const SHORTCUT: i32 = 212;
const ENDPOINT: i32 = 213;
const KEY: i32 = 214;
const SHORTCUT_LABEL: i32 = 215;
const REVEAL: i32 = 216;
const ENDPOINT_LABEL: i32 = 217;
const KEY_LABEL: i32 = 218;
const FONT_SIZE: i32 = 219;
const PREVIEW: i32 = 220;
const FONT_VALUE: i32 = 221;
const AZURE: i32 = 222;
const CODEX: i32 = 223;
const COPILOT: i32 = 224;
const CONNECT: i32 = 225;
const ACCOUNT: i32 = 227;
const ACTIVE_SOURCE: i32 = 228;
const SHORTCUT_STATUS: i32 = 229;
const REFRESH_DPI: u32 = WM_APP + 20;
const ACCOUNT_DONE: u32 = WM_APP + 21;
const TBM_GETPOS: u32 = WM_USER;

type AccountTask = std::sync::mpsc::Receiver<Result<providers::AccountStatus, String>>;
thread_local! {
    static ACCOUNT_TASK: std::cell::RefCell<Option<AccountTask>> = const { std::cell::RefCell::new(None) };
    static LOGIN_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static SAVED_PROVIDER: std::cell::Cell<Provider> = const { std::cell::Cell::new(Provider::Azure) };
    static SHORTCUT_VALUE: std::cell::Cell<shortcut::Shortcut> = const {
        std::cell::Cell::new(shortcut::Shortcut {modifiers:3,key:b'T' as u16,double_ctrl:false})
    };
}

pub struct Window {
    pub hwnd: HWND,
    hotkey: HWND,
    endpoint: HWND,
    key: HWND,
    font: theme::Font,
}

impl Window {
    pub fn create() -> AppResult<Self> {
        unsafe {
            let controls = INITCOMMONCONTROLSEX {
                dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_WIN95_CLASSES,
            };
            if !InitCommonControlsEx(&controls).as_bool() {
                return Err("Could not initialize Windows settings controls.".into());
            }
            let instance = GetModuleHandleW(None)?;
            let hwnd = CreateWindowExW(
                WS_EX_CONTROLPARENT | WS_EX_APPWINDOW,
                w!("MLE.Settings"),
                w!("MLE - Make Life Easier"),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                480,
                440,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            theme::polish(hwnd, false);
            let create = |class: PCWSTR, title: &str, style, id| -> AppResult<HWND> {
                let title = wide(title);
                Ok(CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    PCWSTR(title.as_ptr()),
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
            for (title, id, style) in [
                ("Hold right click", RIGHT_HOLD, theme::CHOICE),
                ("Keyboard shortcut", HOTKEY, theme::CHOICE),
                ("Azure OpenAI", AZURE, theme::CHOICE),
                ("Codex", CODEX, theme::CHOICE),
                ("Copilot", COPILOT, theme::CHOICE),
            ] {
                let button = create(
                    w!("BUTTON"),
                    title,
                    WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                    id,
                )?;
                theme::button(button, style);
            }
            create(w!("STATIC"), "Shortcut", WINDOW_STYLE(0), SHORTCUT_LABEL)?;
            create(w!("STATIC"), "Endpoint", WINDOW_STYLE(0), ENDPOINT_LABEL)?;
            create(w!("STATIC"), "API key", WINDOW_STYLE(0), KEY_LABEL)?;
            create(w!("STATIC"), "", WINDOW_STYLE(0), ACCOUNT)?;
            create(w!("STATIC"), "", WINDOW_STYLE(0), ACTIVE_SOURCE)?;
            let hotkey = create(
                w!("BUTTON"),
                "Ctrl+Alt+T",
                WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                SHORTCUT,
            )?;
            theme::button(hotkey, theme::CHOICE);
            create(
                w!("STATIC"),
                "Click to record a combination, or double-tap Ctrl.",
                WINDOW_STYLE(0),
                SHORTCUT_STATUS,
            )?;
            let endpoint = create(
                w!("EDIT"),
                "",
                WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                ENDPOINT,
            )?;
            send_message(endpoint, EM_SETLIMITTEXT, WPARAM(2048), LPARAM(0));
            send_message(
                endpoint,
                EM_SETCUEBANNER,
                WPARAM(0),
                LPARAM(w!("https://your-resource.openai.azure.com/...").as_ptr() as isize),
            );
            let key = create(
                w!("EDIT"),
                "",
                WS_TABSTOP | WINDOW_STYLE(ES_PASSWORD as u32 | ES_AUTOHSCROLL as u32),
                KEY,
            )?;
            send_message(key, EM_SETLIMITTEXT, WPARAM(512), LPARAM(0));
            let font_size = create(
                w!("msctls_trackbar32"),
                "Translation font size",
                WS_TABSTOP | WINDOW_STYLE(TBS_NOTICKS),
                FONT_SIZE,
            )?;
            send_message(font_size, TBM_SETRANGEMIN, WPARAM(0), LPARAM(12));
            send_message(font_size, TBM_SETRANGEMAX, WPARAM(0), LPARAM(32));
            send_message(font_size, TBM_SETPOS, WPARAM(1), LPARAM(17));
            create(w!("STATIC"), "17 px", WINDOW_STYLE(0), FONT_VALUE)?;
            let preview = popup::create_text(hwnd, PREVIEW)?;
            theme::set_text(
                preview,
                "\u{8fd9}\u{662f}\u{7ffb}\u{8bd1}\u{6548}\u{679c}\u{9884}\u{89c8}\u{3002}",
            )?;
            update_preview(hwnd)?;
            for (title, id, style) in [
                ("Show", REVEAL, theme::QUIET),
                ("Cancel", IDCANCEL.0, theme::QUIET),
                ("Save", SAVE as i32, theme::PRIMARY),
                ("Sign in", CONNECT, theme::PRIMARY),
            ] {
                let button = create(
                    w!("BUTTON"),
                    title,
                    WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                    id,
                )?;
                theme::button(button, style);
            }
            let font = theme::Font::new(GetDpiForWindow(hwnd), 14, 400)?;
            let window = Self {
                hwnd,
                hotkey,
                endpoint,
                key,
                font,
            };
            window.apply_font();
            choose_trigger(hwnd, false)?;
            choose_provider(hwnd, Provider::Azure)?;
            Ok(window)
        }
    }

    fn apply_font(&self) {
        unsafe {
            for id in [
                RIGHT_HOLD,
                HOTKEY,
                SHORTCUT,
                SHORTCUT_STATUS,
                ENDPOINT,
                KEY,
                SHORTCUT_LABEL,
                REVEAL,
                ENDPOINT_LABEL,
                KEY_LABEL,
                FONT_SIZE,
                FONT_VALUE,
                AZURE,
                CODEX,
                COPILOT,
                CONNECT,
                ACCOUNT,
                ACTIVE_SOURCE,
                IDCANCEL.0,
                SAVE as i32,
            ] {
                if let Ok(child) = GetDlgItem(Some(self.hwnd), id) {
                    self.font.apply(child);
                }
            }
        }
    }

    pub fn load(&self, config: &Config) -> AppResult<()> {
        shortcut::cancel();
        let shortcut = shortcut::Shortcut::from_config(config);
        SHORTCUT_VALUE.with(|value| value.set(shortcut));
        theme::set_text(self.hotkey, &shortcut.label())?;
        reset_shortcut_status(self.hwnd)?;
        SAVED_PROVIDER.with(|provider| provider.set(config.provider));
        choose_trigger(self.hwnd, config.trigger == Trigger::Hotkey)?;
        theme::set_text(self.endpoint, &config.azure_endpoint)?;
        unsafe {
            send_message(
                GetDlgItem(Some(self.hwnd), FONT_SIZE)?,
                TBM_SETPOS,
                WPARAM(1),
                LPARAM(config.font_size as isize),
            );
        }
        update_preview(self.hwnd)?;
        theme::set_text(self.key, &credentials::read()?.unwrap_or_default())?;
        mask_key(self.hwnd, true)?;
        choose_provider(self.hwnd, config.provider)
    }

    fn read(&self) -> AppResult<(Config, String)> {
        let shortcut = unsafe { theme::is_selected(GetDlgItem(Some(self.hwnd), HOTKEY)?) };
        let hotkey = SHORTCUT_VALUE.with(|value| value.get());
        let config = Config {
            provider: selected_provider(self.hwnd)?,
            trigger: if shortcut {
                Trigger::Hotkey
            } else {
                Trigger::RightHold
            },
            hotkey_modifiers: hotkey.modifiers,
            hotkey_key: hotkey.key,
            hotkey_double_ctrl: hotkey.double_ctrl,
            azure_endpoint: theme::text(self.endpoint).trim().to_owned(),
            font_size: selected_font_size(self.hwnd)?,
        };
        config.validate()?;
        Ok((config, theme::text(self.key).trim().to_owned()))
    }
}

fn selected_font_size(hwnd: HWND) -> AppResult<u16> {
    let size = unsafe {
        send_message(
            GetDlgItem(Some(hwnd), FONT_SIZE)?,
            TBM_GETPOS,
            WPARAM(0),
            LPARAM(0),
        )
        .0
    };
    if !(12..=32).contains(&size) {
        return Err("Select a translation font size.".into());
    }
    Ok(size as u16)
}

fn update_preview(hwnd: HWND) -> AppResult<()> {
    let size = selected_font_size(hwnd)?;
    unsafe {
        let preview = GetDlgItem(Some(hwnd), PREVIEW)?;
        popup::format_text(preview, size, theme::TEXT);
        ShowScrollBar(preview, SB_VERT, false)?;
        theme::set_text(GetDlgItem(Some(hwnd), FONT_VALUE)?, &format!("{size} px"))?;
        theme::invalidate(GetDlgItem(Some(hwnd), FONT_SIZE)?);
    }
    Ok(())
}

fn choose_trigger(hwnd: HWND, shortcut: bool) -> AppResult<()> {
    super::shortcut::cancel();
    reset_shortcut_status(hwnd)?;
    unsafe {
        theme::select_button(GetDlgItem(Some(hwnd), RIGHT_HOLD)?, !shortcut);
        theme::select_button(GetDlgItem(Some(hwnd), HOTKEY)?, shortcut);
    }
    layout(hwnd)
}

fn reset_shortcut_status(hwnd: HWND) -> AppResult<()> {
    unsafe {
        let value = SHORTCUT_VALUE.with(|value| value.get());
        theme::set_text(GetDlgItem(Some(hwnd), SHORTCUT)?, &value.label())?;
        theme::set_text(
            GetDlgItem(Some(hwnd), SHORTCUT_STATUS)?,
            if value.double_ctrl {
                "Double Ctrl: other apps' double-tap actions cannot be checked."
            } else {
                "Click to record a combination, or double-tap Ctrl."
            },
        )
    }
}

fn recorded_shortcut(hwnd: HWND) -> AppResult<()> {
    let Some(result) = shortcut::take_result() else {
        return Ok(());
    };
    let Some(value) = result else {
        return reset_shortcut_status(hwnd);
    };
    let validation = APP.with(|cell| {
        let state = cell.borrow();
        let app = state.as_ref().ok_or("Application is not ready.")?;
        shortcut::check(value, app.host, &app.config)
    });
    match validation {
        Ok(message) => {
            SHORTCUT_VALUE.with(|shortcut| shortcut.set(value));
            unsafe {
                theme::set_text(GetDlgItem(Some(hwnd), SHORTCUT)?, &value.label())?;
                theme::set_text(GetDlgItem(Some(hwnd), SHORTCUT_STATUS)?, message)?;
            }
        }
        Err(error) => {
            reset_shortcut_status(hwnd)?;
            unsafe {
                theme::set_text(GetDlgItem(Some(hwnd), SHORTCUT_STATUS)?, &error.to_string())?;
            }
        }
    }
    Ok(())
}

fn selected_provider(hwnd: HWND) -> AppResult<Provider> {
    unsafe {
        for (id, provider) in [
            (AZURE, Provider::Azure),
            (CODEX, Provider::Codex),
            (COPILOT, Provider::Copilot),
        ] {
            if theme::is_selected(GetDlgItem(Some(hwnd), id)?) {
                return Ok(provider);
            }
        }
    }
    Err("Choose a translation source.".into())
}

fn choose_provider(hwnd: HWND, provider: Provider) -> AppResult<()> {
    if LOGIN_ACTIVE.with(|active| active.get()) {
        return Ok(());
    }
    ACCOUNT_TASK.with(|task| *task.borrow_mut() = None);
    unsafe {
        for (id, choice) in [
            (AZURE, Provider::Azure),
            (CODEX, Provider::Codex),
            (COPILOT, Provider::Copilot),
        ] {
            theme::select_button(GetDlgItem(Some(hwnd), id)?, choice == provider);
        }
    }
    layout(hwnd)?;
    update_active_source(hwnd)?;
    if provider != Provider::Azure {
        account_task(hwnd, false)?;
    }
    Ok(())
}

fn account_task(hwnd: HWND, connect: bool) -> AppResult<()> {
    if LOGIN_ACTIVE.with(|active| active.get()) {
        return Ok(());
    }
    let provider = selected_provider(hwnd)?;
    LOGIN_ACTIVE.with(|active| active.set(connect));
    unsafe {
        theme::set_text(
            GetDlgItem(Some(hwnd), ACCOUNT)?,
            if connect {
                if providers::executable(provider).is_some() {
                    "Complete sign-in in your browser. Keep MLE running."
                } else {
                    "Complete installation in the opened window."
                }
            } else {
                "Checking sign-in..."
            },
        )?;
        theme::set_text(
            GetDlgItem(Some(hwnd), CONNECT)?,
            if connect {
                "Signing in..."
            } else {
                "Checking..."
            },
        )?;
        let _ = EnableWindow(GetDlgItem(Some(hwnd), CONNECT)?, false);
        if connect {
            for id in [AZURE, CODEX, COPILOT] {
                let _ = EnableWindow(GetDlgItem(Some(hwnd), id)?, false);
            }
        }
    }
    let (tx, rx) = std::sync::mpsc::channel();
    ACCOUNT_TASK.with(|task| *task.borrow_mut() = Some(rx));
    let window = hwnd.0 as isize;
    std::thread::spawn(move || {
        let result = if connect {
            providers::connect(provider)
        } else {
            providers::status(provider)
        };
        if tx.send(result.map_err(|error| error.to_string())).is_ok() {
            unsafe {
                if let Err(error) = PostMessageW(
                    Some(HWND(window as *mut _)),
                    ACCOUNT_DONE,
                    WPARAM(0),
                    LPARAM(0),
                ) {
                    eprintln!("Could not deliver sign-in result: {error}");
                }
            }
        }
    });
    Ok(())
}

fn account_done(hwnd: HWND) -> AppResult<()> {
    let result = ACCOUNT_TASK.with(|task| {
        let mut task = task.borrow_mut();
        let result = task.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(result) => Some(result),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(
                "The sign-in check stopped unexpectedly. Reopen Settings to retry.".into(),
            )),
        });
        if result.is_some() {
            *task = None;
        }
        result
    });
    if let Some(result) = result {
        let signing_in = LOGIN_ACTIVE.with(|active| active.replace(false));
        let (signed_in, message) = match result {
            Ok(status) => (status.signed_in, status.message),
            Err(error) => (false, error),
        };
        unsafe {
            theme::set_text(GetDlgItem(Some(hwnd), ACCOUNT)?, &message)?;
            theme::set_text(
                GetDlgItem(Some(hwnd), CONNECT)?,
                if signed_in {
                    "Signed in"
                } else if providers::executable(selected_provider(hwnd)?).is_some() {
                    "Sign in"
                } else {
                    "Install"
                },
            )?;
            let _ = EnableWindow(GetDlgItem(Some(hwnd), CONNECT)?, !signed_in);
            for id in [AZURE, CODEX, COPILOT] {
                let _ = EnableWindow(GetDlgItem(Some(hwnd), id)?, true);
            }
            if signing_in {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                if !SetForegroundWindow(hwnd).as_bool() {
                    let notification = FLASHWINFO {
                        cbSize: size_of::<FLASHWINFO>() as u32,
                        hwnd,
                        dwFlags: FLASHW_TRAY | FLASHW_TIMERNOFG,
                        uCount: 3,
                        dwTimeout: 0,
                    };
                    let _ = FlashWindowEx(&notification);
                }
            }
        }
    }
    Ok(())
}

fn layout(hwnd: HWND) -> AppResult<()> {
    unsafe {
        let shortcut = theme::is_selected(GetDlgItem(Some(hwnd), HOTKEY)?);
        let offset = if shortcut { 72 } else { 0 };
        let azure = theme::is_selected(GetDlgItem(Some(hwnd), AZURE)?);
        let compact = if azure { 0 } else { 40 };
        for id in [ENDPOINT_LABEL, ENDPOINT, KEY_LABEL, KEY, REVEAL] {
            let _ = ShowWindow(
                GetDlgItem(Some(hwnd), id)?,
                if azure { SW_SHOW } else { SW_HIDE },
            );
        }
        for id in [CONNECT, ACCOUNT] {
            let _ = ShowWindow(
                GetDlgItem(Some(hwnd), id)?,
                if azure { SW_HIDE } else { SW_SHOW },
            );
        }
        for id in [SHORTCUT, SHORTCUT_LABEL, SHORTCUT_STATUS] {
            let _ = ShowWindow(
                GetDlgItem(Some(hwnd), id)?,
                if shortcut { SW_SHOW } else { SW_HIDE },
            );
        }
        for (id, x, y, width, height) in [
            (RIGHT_HOLD, 20, 82, 214, 34),
            (HOTKEY, 242, 82, 218, 34),
            (SHORTCUT_LABEL, 20, 132, 94, 22),
            (SHORTCUT, 124, 126, 336, 34),
            (SHORTCUT_STATUS, 20, 166, 440, 34),
            (AZURE, 20, 164 + offset, 152, 34),
            (CODEX, 180, 164 + offset, 136, 34),
            (COPILOT, 324, 164 + offset, 136, 34),
            (ENDPOINT_LABEL, 20, 216 + offset, 440, 20),
            (ENDPOINT, 20, 246 + offset, 440, 24),
            (KEY_LABEL, 20, 286 + offset, 440, 20),
            (KEY, 20, 316 + offset, 372, 24),
            (REVEAL, 400, 310 + offset, 60, 30),
            (ACCOUNT, 20, 218 + offset, 440, 40),
            (CONNECT, 20, 260 + offset, 104, 34),
            (ACTIVE_SOURCE, 20, 476 + offset - compact, 240, 36),
            (FONT_SIZE, 212, 360 + offset - compact, 184, 28),
            (FONT_VALUE, 410, 364 + offset - compact, 50, 22),
            (PREVIEW, 20, 402 + offset - compact, 440, 50),
            (IDCANCEL.0, 272, 476 + offset - compact, 84, 34),
            (SAVE as i32, 368, 476 + offset - compact, 92, 34),
        ] {
            MoveWindow(
                GetDlgItem(Some(hwnd), id)?,
                theme::px(hwnd, x),
                theme::px(hwnd, y),
                theme::px(hwnd, width),
                theme::px(hwnd, height),
                true,
            )?;
        }
        let dpi = GetDpiForWindow(hwnd);
        let mut bounds = RECT {
            right: theme::px(hwnd, 480),
            bottom: theme::px(hwnd, 526 + offset - compact),
            ..Default::default()
        };
        AdjustWindowRectExForDpi(
            &mut bounds,
            WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32),
            false,
            WINDOW_EX_STYLE(GetWindowLongW(hwnd, GWL_EXSTYLE) as u32),
            dpi,
        )?;
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        )?;
    }
    unsafe {
        let _ = RedrawWindow(
            Some(hwnd),
            None,
            None,
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
        );
    }
    Ok(())
}

fn mask_key(hwnd: HWND, masked: bool) -> AppResult<()> {
    unsafe {
        let key = GetDlgItem(Some(hwnd), KEY)?;
        send_message(
            key,
            EM_SETPASSWORDCHAR,
            WPARAM(if masked { 0x2022 } else { 0 }),
            LPARAM(0),
        );
        theme::invalidate(key);
        theme::set_text(
            GetDlgItem(Some(hwnd), REVEAL)?,
            if masked { "Show" } else { "Hide" },
        )?;
    }
    Ok(())
}

fn update_active_source(hwnd: HWND) -> AppResult<()> {
    let selected = selected_provider(hwnd)?;
    let saved = SAVED_PROVIDER.with(|provider| provider.get());
    let label = match saved {
        Provider::Azure => "Azure API",
        Provider::Codex => "Codex (ChatGPT plan)",
        Provider::Copilot => "Copilot subscription",
    };
    let text = if selected == saved {
        format!("Active online: {label}")
    } else {
        format!(
            "Active online: {label}\nSave to switch to {}.",
            providers::name(selected)
        )
    };
    unsafe { theme::set_text(GetDlgItem(Some(hwnd), ACTIVE_SOURCE)?, &text) }
}

fn save() -> AppResult<()> {
    shortcut::cancel();
    APP.with(|cell| {
        let mut state = cell.borrow_mut();
        let app = state.as_mut().ok_or("Application is not ready.")?;
        let window = app.settings.as_ref().ok_or("Settings are not open.")?;
        let (config, key) = window.read()?;
        let old_key = credentials::read()?;
        let previous = app.config.clone();
        app.config = config;
        let result = (|| -> AppResult<()> {
            app.install_trigger()?;
            if key.is_empty() {
                credentials::delete()?;
            } else {
                credentials::write(&key)?;
            }
            app.config.save()?;
            Ok(())
        })();
        if let Err(error) = result {
            app.config = previous;
            app.install_trigger()
                .map_err(|restore| format!("{error}\nCould not restore trigger: {restore}"))?;
            match old_key {
                Some(key) => credentials::write(&key)?,
                None => credentials::delete()?,
            }
            return Err(error);
        }
        SAVED_PROVIDER.with(|provider| provider.set(app.config.provider));
        if let Some(window) = app.settings.as_ref() {
            theme::set_text(window.key, "")?;
            mask_key(window.hwnd, true)?;
            unsafe {
                let _ = ShowWindow(window.hwnd, SW_HIDE);
            }
        }
        Ok(())
    })
}

fn hide(hwnd: HWND) -> AppResult<()> {
    shortcut::cancel();
    if !LOGIN_ACTIVE.with(|active| active.get()) {
        ACCOUNT_TASK.with(|task| *task.borrow_mut() = None);
    }
    unsafe {
        theme::set_text(GetDlgItem(Some(hwnd), KEY)?, "")?;
        mask_key(hwnd, true)?;
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    Ok(())
}

fn paint(hwnd: HWND) {
    unsafe {
        let mut paint = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut paint);
        let canvas = theme::Canvas::new(hdc, hwnd);
        let mut bounds = RECT::default();
        let _ = GetClientRect(hwnd, &mut bounds);
        canvas.fill(bounds, theme::SURFACE);
        canvas.label(
            "Make Life Easier",
            canvas.rect(20, 14, 440, 28),
            20,
            600,
            theme::TEXT,
        );
        let shortcut = GetDlgItem(Some(hwnd), HOTKEY).is_ok_and(theme::is_selected);
        let offset = if shortcut { 72 } else { 0 };
        canvas.label(
            "Activation",
            canvas.rect(20, 54, 440, 20),
            13,
            600,
            theme::TEXT,
        );
        let azure = GetDlgItem(Some(hwnd), AZURE).is_ok_and(theme::is_selected);
        let compact = if azure { 0 } else { 40 };
        for y in [344 + offset - compact, 462 + offset - compact] {
            canvas.fill(canvas.rect(20, y, 440, 1), theme::BORDER);
        }
        if azure {
            canvas.fill(canvas.rect(20, 274 + offset, 440, 1), theme::BORDER);
        }
        canvas.label(
            "Translation source",
            canvas.rect(20, 136 + offset, 440, 20),
            13,
            600,
            theme::TEXT,
        );
        canvas.label(
            "Translation font size",
            canvas.rect(20, 362 + offset - compact, 184, 26),
            13,
            600,
            theme::TEXT,
        );
        let _ = EndPaint(hwnd, &paint);
    }
}

fn draw_slider(draw: &NMCUSTOMDRAW) -> LRESULT {
    if draw.dwDrawStage == CDDS_PREPAINT {
        return LRESULT(CDRF_NOTIFYITEMDRAW as isize);
    }
    if draw.dwDrawStage != CDDS_ITEMPREPAINT {
        return LRESULT(CDRF_DODEFAULT as isize);
    }
    let canvas = theme::Canvas::new(draw.hdc, draw.hdr.hwndFrom);
    let mut rect = draw.rc;
    if draw.dwItemSpec == TBCD_CHANNEL as usize {
        let center = (rect.top + rect.bottom) / 2;
        rect.top = center - canvas.scale(2);
        rect.bottom = center + canvas.scale(2);
        canvas.rounded(rect, 2, theme::BORDER, None);
    } else if draw.dwItemSpec == TBCD_THUMB as usize {
        let x = (rect.left + rect.right) / 2;
        let y = (rect.top + rect.bottom) / 2;
        let radius = canvas
            .scale(5)
            .min((rect.right - rect.left) / 2)
            .min((rect.bottom - rect.top) / 2);
        rect = RECT {
            left: x - radius,
            right: x + radius,
            top: y - radius,
            bottom: y + radius,
        };
        canvas.rounded(rect, 5, theme::ACCENT, None);
    }
    LRESULT(CDRF_SKIPDEFAULT as isize)
}

pub unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    unsafe {
        if let Some(result) = theme::control_colors(message, wp) {
            return result;
        }
        let result = match message {
            WM_COMMAND if wp.0 & 0xffff == SHORTCUT as usize => (|| -> AppResult<()> {
                shortcut::record(hwnd);
                theme::set_text(GetDlgItem(Some(hwnd), SHORTCUT)?, "Press shortcut...")?;
                theme::set_text(
                    GetDlgItem(Some(hwnd), SHORTCUT_STATUS)?,
                    "Press a combination or double-tap Ctrl. Esc cancels.",
                )
            })(),
            shortcut::RECORDED => recorded_shortcut(hwnd),
            WM_ACTIVATE if wp.0 & 0xffff == WA_INACTIVE as usize => {
                shortcut::cancel();
                reset_shortcut_status(hwnd)
            }
            DM_GETDEFID => return LRESULT(((DC_HASDEFID as usize) << 16 | SAVE) as isize),
            WM_PAINT => {
                paint(hwnd);
                return LRESULT(0);
            }
            WM_ERASEBKGND => return LRESULT(1),
            WM_DRAWITEM => {
                theme::draw_button(&*(lp.0 as *const DRAWITEMSTRUCT));
                return LRESULT(1);
            }
            WM_NOTIFY
                if (*(lp.0 as *const NMHDR)).code == NM_CUSTOMDRAW
                    && (*(lp.0 as *const NMHDR)).idFrom == FONT_SIZE as usize =>
            {
                return draw_slider(&*(lp.0 as *const NMCUSTOMDRAW));
            }
            WM_COMMAND if [RIGHT_HOLD as usize, HOTKEY as usize].contains(&(wp.0 & 0xffff)) => {
                choose_trigger(hwnd, wp.0 & 0xffff == HOTKEY as usize)
            }
            WM_COMMAND
                if [AZURE as usize, CODEX as usize, COPILOT as usize]
                    .contains(&(wp.0 & 0xffff)) =>
            {
                let provider = match (wp.0 & 0xffff) as i32 {
                    CODEX => Provider::Codex,
                    COPILOT => Provider::Copilot,
                    _ => Provider::Azure,
                };
                choose_provider(hwnd, provider)
            }
            WM_COMMAND if wp.0 & 0xffff == CONNECT as usize => account_task(hwnd, true),
            ACCOUNT_DONE => account_done(hwnd),
            WM_HSCROLL if lp.0 != 0 && GetDlgCtrlID(HWND(lp.0 as *mut _)) == FONT_SIZE => {
                update_preview(hwnd)
            }
            WM_COMMAND if wp.0 & 0xffff == REVEAL as usize => GetDlgItem(Some(hwnd), KEY)
                .map_err(Into::into)
                .and_then(|key| {
                    let masked = send_message(key, EM_GETPASSWORDCHAR, WPARAM(0), LPARAM(0)).0 != 0;
                    mask_key(hwnd, !masked)
                }),
            WM_COMMAND if wp.0 & 0xffff == SAVE || wp.0 & 0xffff == IDOK.0 as usize => save(),
            WM_COMMAND if wp.0 & 0xffff == IDCANCEL.0 as usize => hide(hwnd),
            WM_CLOSE => hide(hwnd),
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
                // Refresh outside the nested DPI notification to avoid borrowing app state twice.
                PostMessageW(Some(hwnd), REFRESH_DPI, WPARAM(0), LPARAM(0)).map_err(Into::into)
            }
            REFRESH_DPI => APP.with(|cell| -> AppResult<()> {
                if let Some(window) = cell
                    .borrow_mut()
                    .as_mut()
                    .and_then(|app| app.settings.as_mut())
                {
                    window.font = theme::Font::new(GetDpiForWindow(hwnd), 14, 400)?;
                    window.apply_font();
                }
                layout(hwnd)?;
                update_preview(hwnd)
            }),
            _ => return DefWindowProcW(hwnd, message, wp, lp),
        };
        if let Err(error) = result {
            show_error(&error.to_string());
        }
        LRESULT(0)
    }
}
