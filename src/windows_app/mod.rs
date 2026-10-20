mod clipboard;
mod credentials;
mod http;
mod popup;
mod providers;
mod selection;
mod settings;
mod shortcut;
mod theme;

use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicBool, AtomicIsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        Security::*,
        System::{LibraryLoader::GetModuleHandleW, Threading::*},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};

use crate::{
    AppResult,
    config::{Config, Provider, Trigger},
    dictionary::Dictionary,
};

const TRAY_EVENT: u32 = WM_APP + 1;
const TRIGGER_EVENT: u32 = WM_APP + 2;
const RESULT_EVENT: u32 = WM_APP + 3;
const DISMISS_EVENT: u32 = WM_APP + 4;
const INPUT_ERROR_EVENT: u32 = WM_APP + 5;
const UI_ERROR_EVENT: u32 = WM_APP + 6;
const HOLD_TIMER: usize = 1;
const HOLD_MS: u32 = 500;
const POPUP_TIMER: usize = 2;
const HOTKEY: i32 = 1;
const ESCAPE_HOTKEY: i32 = 2;
const OPEN_SETTINGS: usize = 100;
const QUIT: usize = 101;
const REPLAY: usize = 0x4d4c4531;

static HOST: AtomicIsize = AtomicIsize::new(0);
static POPUP: AtomicIsize = AtomicIsize::new(0);
static RIGHT_HOLD: AtomicBool = AtomicBool::new(true);

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static GESTURE: RefCell<Gesture> = RefCell::new(Gesture::default());
    static UI_ERRORS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

#[derive(Default)]
struct Gesture {
    start: Option<POINT>,
    consumed: bool,
}

struct Work {
    id: u64,
    source: isize,
    config: Config,
}

struct Reply {
    id: u64,
    text: Result<popup::Content, String>,
}

struct App {
    host: HWND,
    popup: popup::Window,
    settings: Option<settings::Window>,
    config: Config,
    hook: HHOOK,
    _keyboard: shortcut::Hook,
    hotkey_registered: bool,
    sender: SyncSender<Work>,
    replies: Receiver<Reply>,
    generation: u64,
    source: HWND,
    popup_visible: bool,
}

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn send_message(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe { SendMessageW(hwnd, message, Some(wp), Some(lp)) }
}

pub fn show_error(text: &str) {
    let text = wide(text);
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            w!("MLE - Make Life Easier"),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn defer_error(error: String) {
    UI_ERRORS.with(|cell| cell.borrow_mut().push(error));
    unsafe {
        if let Err(post_error) = PostMessageW(
            Some(HWND(HOST.load(Ordering::Relaxed) as *mut _)),
            UI_ERROR_EVENT,
            WPARAM(0),
            LPARAM(0),
        ) {
            eprintln!("Could not deliver UI error: {post_error}");
        }
    }
}

fn worker(receiver: Receiver<Work>, replies: mpsc::Sender<Reply>, host: isize) {
    let mut client: Option<http::Client> = None;
    while let Ok(work) = receiver.recv() {
        let text = (|| -> AppResult<popup::Content> {
            let source = HWND(work.source as *mut _);
            if unsafe { GetForegroundWindow() } != source {
                return Err(
                    "The active application changed. Select the text and try again.".into(),
                );
            }
            let selection = selection::Selection::new()?.text(source)?;
            if unsafe { GetForegroundWindow() } != source {
                return Err("The active application changed while reading the selection.".into());
            }
            let dictionary = Dictionary::open(&crate::dictionary::default_path()?)?;
            if let Some(meaning) = dictionary.lookup(&selection)? {
                return Ok(popup::Content::Dictionary(meaning));
            }
            if work.config.provider != Provider::Azure {
                return providers::translate(work.config.provider, &selection)
                    .map(popup::Content::Translation);
            }
            if work.config.azure_endpoint.is_empty() {
                return Err("Add your Azure OpenAI endpoint and API key in Settings.".into());
            }
            let endpoint = crate::translation::Endpoint::parse(&work.config.azure_endpoint)?;
            let key = credentials::read()?
                .ok_or("Add your Azure OpenAI API key in Settings to translate this text.")?;
            if client
                .as_ref()
                .is_none_or(|client| client.host != endpoint.host)
            {
                client = Some(http::Client::new(endpoint.host)?);
            }
            client
                .as_ref()
                .ok_or("HTTP client is unavailable.")?
                .translate(&selection, &key, endpoint.path)
                .map(popup::Content::Translation)
        })()
        .map_err(|e| e.to_string());
        if replies.send(Reply { id: work.id, text }).is_err() {
            break;
        }
        if unsafe {
            PostMessageW(
                Some(HWND(host as *mut _)),
                RESULT_EVENT,
                WPARAM(0),
                LPARAM(0),
            )
        }
        .is_err()
        {
            break;
        }
    }
}

pub fn run() -> AppResult<()> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)?;
        let mutex = CreateMutexW(None, false, w!("Local\\MLE.SelectionTranslator"))?;
        let already_running = GetLastError() == ERROR_ALREADY_EXISTS;
        if already_running {
            CloseHandle(mutex)?;
            return Err("MLE is already running. Open Settings from its tray icon.".into());
        }
        let result = run_inner();
        CloseHandle(mutex)?;
        result
    }
}

fn run_inner() -> AppResult<()> {
    let (config, first_run) = Config::load()?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let (reply_sender, replies) = mpsc::channel();
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let cursor = LoadCursorW(None, IDC_ARROW)?;
        for (name, procedure) in [
            (
                w!("MLE.Host"),
                Some(host_proc as unsafe extern "system" fn(_, _, _, _) -> _),
            ),
            (
                w!("MLE.Popup"),
                Some(popup::window_proc as unsafe extern "system" fn(_, _, _, _) -> _),
            ),
            (
                w!("MLE.Settings"),
                Some(settings::window_proc as unsafe extern "system" fn(_, _, _, _) -> _),
            ),
        ] {
            let class = WNDCLASSW {
                lpfnWndProc: procedure,
                hInstance: instance.into(),
                hCursor: cursor,
                hIcon: theme::icon(32)?,
                hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut _),
                lpszClassName: name,
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                return Err(windows::core::Error::from_thread().into());
            }
        }
        let host = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("MLE.Host"),
            w!("MLE"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        HOST.store(host.0 as isize, Ordering::Relaxed);
        let popup = popup::Window::create(host)?;
        POPUP.store(popup.hwnd.0 as isize, Ordering::Relaxed);
        let keyboard = shortcut::Hook::new()?;
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(instance.into()), 0)?;
        let mut app = App {
            host,
            popup,
            settings: None,
            config,
            hook,
            _keyboard: keyboard,
            hotkey_registered: false,
            sender,
            replies,
            generation: 0,
            source: HWND::default(),
            popup_visible: false,
        };
        if let Err(error) = app.install_trigger() {
            UnhookWindowsHookEx(hook)?;
            return Err(error);
        }
        if let Err(error) = app.tray(NIM_ADD) {
            UnhookWindowsHookEx(hook)?;
            return Err(error);
        }
        APP.with(|cell| *cell.borrow_mut() = Some(app));
        let host_id = host.0 as isize;
        std::thread::Builder::new()
            .name("mle-query".into())
            .spawn(move || worker(receiver, reply_sender, host_id))?;
        // Display on first run; subsequent launches stay in the tray.
        if (first_run || std::env::args().any(|arg| arg == "--settings"))
            && let Err(error) = open_settings()
        {
            show_error(&error.to_string());
        }
        let mut message = MSG::default();
        loop {
            let status = GetMessageW(&mut message, None, 0, 0);
            if status.0 == -1 {
                return Err(windows::core::Error::from_thread().into());
            }
            if !status.as_bool() {
                break;
            }
            let settings = APP.with(|cell| {
                cell.borrow()
                    .as_ref()
                    .and_then(|app| app.settings.as_ref().map(|window| window.hwnd))
            });
            if settings.is_some_and(|window| IsDialogMessageW(window, &message).as_bool()) {
                continue;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        APP.with(|cell| {
            if let Some(app) = cell.borrow_mut().take() {
                let _ = app.tray(NIM_DELETE);
                let _ = UnhookWindowsHookEx(app.hook);
                if app.hotkey_registered {
                    let _ = UnregisterHotKey(Some(app.host), HOTKEY);
                }
                if app.popup_visible {
                    let _ = UnregisterHotKey(Some(app.host), ESCAPE_HOTKEY);
                }
            }
        });
        Ok(())
    }
}

impl App {
    fn tray(&self, command: NOTIFY_ICON_MESSAGE) -> AppResult<()> {
        unsafe {
            let mut data = NOTIFYICONDATAW {
                cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.host,
                uID: 1,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: TRAY_EVENT,
                hIcon: theme::icon(16)?,
                ..Default::default()
            };
            let tip = wide("MLE - Make Life Easier");
            data.szTip[..tip.len()].copy_from_slice(&tip);
            if !Shell_NotifyIconW(command, &data).as_bool() {
                return Err("Could not update the tray icon.".into());
            }
            Ok(())
        }
    }

    fn install_trigger(&mut self) -> AppResult<()> {
        unsafe {
            if self.hotkey_registered {
                UnregisterHotKey(Some(self.host), HOTKEY)?;
                self.hotkey_registered = false;
            }
            RIGHT_HOLD.store(self.config.trigger == Trigger::RightHold, Ordering::Relaxed);
            shortcut::configure(&self.config);
            if self.config.trigger == Trigger::Hotkey && !self.config.hotkey_double_tap {
                RegisterHotKey(
                    Some(self.host),
                    HOTKEY,
                    HOT_KEY_MODIFIERS(self.config.hotkey_modifiers) | MOD_NOREPEAT,
                    self.config.hotkey_key as u32,
                )
                .map_err(|_| "This shortcut is already in use. Choose another shortcut.")?;
                self.hotkey_registered = true;
            }
        }
        Ok(())
    }

    fn open_settings(&mut self) -> AppResult<()> {
        if self.settings.is_none() {
            self.settings = Some(settings::Window::create()?);
        }
        if let Some(window) = &self.settings {
            window.load(&self.config)?;
            unsafe {
                let _ = ShowWindow(window.hwnd, SW_SHOW);
                let _ = SetForegroundWindow(window.hwnd);
            }
        }
        Ok(())
    }

    fn dismiss(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.popup_visible = false;
        unsafe {
            let _ = UnregisterHotKey(Some(self.host), ESCAPE_HOTKEY);
            let _ = ShowWindow(self.popup.hwnd, SW_HIDE);
            let _ = KillTimer(Some(self.host), POPUP_TIMER);
        }
    }

    fn trigger(&mut self) -> AppResult<()> {
        let source = unsafe { GetForegroundWindow() };
        if source == self.popup.hwnd
            || self
                .settings
                .as_ref()
                .is_some_and(|window| source == window.hwnd)
        {
            return Ok(());
        }
        if !self.popup_visible {
            unsafe {
                RegisterHotKey(Some(self.host), ESCAPE_HOTKEY, MOD_NOREPEAT, VK_ESCAPE.0 as u32)
                    .map_err(|_| "Cannot register Escape to dismiss the popup; another application owns this key.")?;
            }
        }
        self.generation = self.generation.wrapping_add(1);
        match self.sender.try_send(Work {
            id: self.generation,
            source: source.0 as isize,
            config: self.config.clone(),
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.dismiss();
                return Err("A translation is still running. Please try again shortly.".into());
            }
            Err(TrySendError::Disconnected(_)) => {
                self.dismiss();
                return Err("The query worker stopped. Restart MLE.".into());
            }
        }
        self.source = source;
        unsafe {
            let mut point = POINT::default();
            GetCursorPos(&mut point)?;
            let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return Err(windows::core::Error::from_thread().into());
            }
            let dpi = GetDpiForWindow(self.popup.hwnd) as i32;
            let width = (420 * dpi / 96).min(info.rcWork.right - info.rcWork.left);
            let height = (80 * dpi / 96).min(info.rcWork.bottom - info.rcWork.top);
            let x = (point.x + 16).clamp(
                info.rcWork.left,
                (info.rcWork.right - width).max(info.rcWork.left),
            );
            let y = (point.y + 20).clamp(
                info.rcWork.top,
                (info.rcWork.bottom - height).max(info.rcWork.top),
            );
            SetWindowPos(
                self.popup.hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )?;
            self.popup
                .render(&popup::Content::Loading, self.config.font_size)?;
            self.popup_visible = true;
            if SetTimer(Some(self.host), POPUP_TIMER, 100, None) == 0 {
                return Err("Could not start popup dismissal timer.".into());
            }
        }
        Ok(())
    }

    fn receive(&mut self) -> AppResult<()> {
        while let Ok(reply) = self.replies.try_recv() {
            if reply.id == self.generation && self.popup_visible {
                let content = match reply.text {
                    Ok(content) => content,
                    Err(error) => popup::Content::Error(error),
                };
                self.popup.render(&content, self.config.font_size)?;
            }
        }
        Ok(())
    }
}

fn open_settings() -> AppResult<()> {
    APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.open_settings()?;
        }
        Ok(())
    })
}

unsafe extern "system" fn host_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match message {
            WM_TIMER if wp.0 == HOLD_TIMER => {
                let _ = KillTimer(Some(hwnd), HOLD_TIMER);
                let consumed = GESTURE.with(|state| {
                    let mut gesture = state.borrow_mut();
                    if gesture.start.is_some() {
                        gesture.consumed = true;
                        true
                    } else {
                        false
                    }
                });
                if consumed {
                    let _ = PostMessageW(Some(hwnd), TRIGGER_EVENT, WPARAM(0), LPARAM(0));
                }
            }
            WM_TIMER if wp.0 == POPUP_TIMER => {
                APP.with(|cell| {
                    if let Some(app) = cell.borrow_mut().as_mut()
                        && GetForegroundWindow() != app.source
                        && GetForegroundWindow() != app.popup.hwnd
                    {
                        app.dismiss();
                    }
                });
            }
            WM_HOTKEY if wp.0 == ESCAPE_HOTKEY as usize => {
                APP.with(|cell| {
                    if let Some(app) = cell.borrow_mut().as_mut() {
                        app.dismiss();
                    }
                });
            }
            WM_HOTKEY | TRIGGER_EVENT => {
                let result = APP.with(|cell| -> AppResult<()> {
                    if let Some(app) = cell.borrow_mut().as_mut()
                        && let Err(error) = app.trigger()
                    {
                        app.dismiss();
                        return Err(error);
                    }
                    Ok(())
                });
                if let Err(error) = result {
                    show_error(&error.to_string());
                }
            }
            RESULT_EVENT => {
                let result = APP.with(|cell| -> AppResult<()> {
                    if let Some(app) = cell.borrow_mut().as_mut() {
                        app.receive()?;
                    }
                    Ok(())
                });
                if let Err(error) = result {
                    show_error(&error.to_string());
                }
            }
            DISMISS_EVENT => APP.with(|cell| {
                if let Some(app) = cell.borrow_mut().as_mut() {
                    app.dismiss();
                }
            }),
            INPUT_ERROR_EVENT => show_error(
                "Windows blocked a replayed right click. Use keyboard shortcut mode for this application.",
            ),
            UI_ERROR_EVENT => {
                let errors = UI_ERRORS.with(|cell| std::mem::take(&mut *cell.borrow_mut()));
                for error in errors {
                    show_error(&error);
                }
            }
            TRAY_EVENT => {
                if lp.0 as u32 == WM_RBUTTONUP {
                    let result = (|| -> AppResult<()> {
                        let menu = CreatePopupMenu()?;
                        AppendMenuW(menu, MF_STRING, OPEN_SETTINGS, w!("Settings..."))?;
                        AppendMenuW(menu, MF_STRING, QUIT, w!("Exit"))?;
                        let mut point = POINT::default();
                        GetCursorPos(&mut point)?;
                        let _ = SetForegroundWindow(hwnd);
                        let chosen = TrackPopupMenu(
                            menu,
                            TPM_RETURNCMD | TPM_RIGHTBUTTON,
                            point.x,
                            point.y,
                            Some(0),
                            hwnd,
                            None,
                        )
                        .0 as usize;
                        DestroyMenu(menu)?;
                        if chosen == QUIT {
                            PostQuitMessage(0);
                        } else if chosen == OPEN_SETTINGS {
                            open_settings()?;
                        }
                        Ok(())
                    })();
                    if let Err(error) = result {
                        show_error(&error.to_string());
                    }
                } else if lp.0 as u32 == WM_LBUTTONUP
                    && let Err(error) = open_settings()
                {
                    show_error(&error.to_string());
                }
            }
            WM_DESTROY => PostQuitMessage(0),
            _ => return DefWindowProcW(hwnd, message, wp, lp),
        }
        LRESULT(0)
    }
}

fn replay(flags: MOUSE_EVENT_FLAGS) {
    let input = |flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                dwExtraInfo: REPLAY,
                ..Default::default()
            },
        },
    };
    let click = [input(MOUSEEVENTF_RIGHTDOWN), input(MOUSEEVENTF_RIGHTUP)];
    let single = [input(flags)];
    let inputs = if flags == MOUSEEVENTF_RIGHTDOWN | MOUSEEVENTF_RIGHTUP {
        &click[..]
    } else {
        &single[..]
    };
    unsafe {
        if SendInput(inputs, size_of::<INPUT>() as i32) != inputs.len() as u32 {
            let _ = PostMessageW(
                Some(HWND(HOST.load(Ordering::Relaxed) as *mut _)),
                INPUT_ERROR_EVENT,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

fn target_is_accessible(pid: u32) -> bool {
    // Never intercept input that UIPI could prevent us from replaying.
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut token = HANDLE::default();
        let opened = OpenProcessToken(process, TOKEN_QUERY, &mut token);
        let _ = CloseHandle(process);
        if opened.is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut length = 0;
        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        );
        let _ = CloseHandle(token);
        result.is_ok() && elevation.TokenIsElevated == 0
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code < 0 {
            return CallNextHookEx(None, code, wp, lp);
        }
        let input = &*(lp.0 as *const MSLLHOOKSTRUCT);
        if input.dwExtraInfo == REPLAY {
            return CallNextHookEx(None, code, wp, lp);
        }
        let host = HWND(HOST.load(Ordering::Relaxed) as *mut _);
        let event = wp.0 as u32;
        if matches!(event, WM_LBUTTONDOWN | WM_MBUTTONDOWN | WM_RBUTTONDOWN) {
            shortcut::interrupt();
        }
        if event == WM_MOUSEMOVE && GESTURE.with(|state| state.borrow().start.is_none()) {
            return CallNextHookEx(None, code, wp, lp);
        }
        let target = GetAncestor(WindowFromPoint(input.pt), GA_ROOT);
        if (event == WM_LBUTTONDOWN || event == WM_MBUTTONDOWN || event == WM_RBUTTONDOWN)
            && target.0 as isize != POPUP.load(Ordering::Relaxed)
            && !popup::context_menu_active()
        {
            let _ = PostMessageW(Some(host), DISMISS_EVENT, WPARAM(0), LPARAM(0));
        }
        if event == WM_RBUTTONDOWN && RIGHT_HOLD.load(Ordering::Relaxed) {
            let mut pid = 0;
            GetWindowThreadProcessId(target, Some(&mut pid));
            if pid == GetCurrentProcessId()
                || GetForegroundWindow() != target
                || !target_is_accessible(pid)
            {
                return CallNextHookEx(None, code, wp, lp);
            }
            GESTURE.with(|state| {
                *state.borrow_mut() = Gesture {
                    start: Some(input.pt),
                    consumed: false,
                }
            });
            if SetTimer(Some(host), HOLD_TIMER, HOLD_MS, None) == 0 {
                GESTURE.with(|state| state.borrow_mut().start = None);
                let _ = PostMessageW(Some(host), INPUT_ERROR_EVENT, WPARAM(0), LPARAM(0));
                return CallNextHookEx(None, code, wp, lp);
            }
            return LRESULT(1);
        }
        if event == WM_MOUSEMOVE {
            let cancelled = GESTURE.with(|state| {
                let mut gesture = state.borrow_mut();
                if gesture.start.is_some_and(|start| {
                    (start.x - input.pt.x).abs() > 8 || (start.y - input.pt.y).abs() > 8
                }) && !gesture.consumed
                {
                    gesture.start = None;
                    true
                } else {
                    false
                }
            });
            if cancelled {
                let _ = KillTimer(Some(host), HOLD_TIMER);
                replay(MOUSEEVENTF_RIGHTDOWN);
            }
        }
        if event == WM_RBUTTONUP {
            let gesture = GESTURE.with(|state| std::mem::take(&mut *state.borrow_mut()));
            if gesture.start.is_some() {
                let _ = KillTimer(Some(host), HOLD_TIMER);
                if !gesture.consumed {
                    replay(MOUSEEVENTF_RIGHTDOWN | MOUSEEVENTF_RIGHTUP);
                }
                return LRESULT(1);
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}
