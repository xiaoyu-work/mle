use super::{HOST, TRIGGER_EVENT};
use crate::{AppResult, config::Config};
use std::sync::atomic::Ordering;
use std::{
    cell::RefCell,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub const RECORDED: u32 = WM_APP + 22;
const DOUBLE_TAP: Duration = Duration::from_millis(350);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub modifiers: u32,
    pub key: u16,
    pub double_ctrl: bool,
}

impl Shortcut {
    pub fn from_config(config: &Config) -> Self {
        Self {
            modifiers: config.hotkey_modifiers,
            key: config.hotkey_key,
            double_ctrl: config.hotkey_double_ctrl,
        }
    }

    pub fn label(self) -> String {
        if self.double_ctrl {
            return "Ctrl, Ctrl".into();
        }
        let mut parts = Vec::new();
        for (flag, name) in [(2, "Ctrl"), (1, "Alt"), (4, "Shift"), (8, "Win")] {
            if self.modifiers & flag != 0 {
                parts.push(name.to_owned());
            }
        }
        unsafe {
            let mut buffer = [0u16; 64];
            let scan = MapVirtualKeyW(self.key as u32, MAPVK_VK_TO_VSC);
            let extended = matches!(self.key, 0x21..=0x28 | 0x2d | 0x2e | 0x6f | 0x90);
            let count = GetKeyNameTextW(
                ((scan << 16) | if extended { 1 << 24 } else { 0 }) as i32,
                &mut buffer,
            );
            parts.push(if count > 0 {
                String::from_utf16_lossy(&buffer[..count as usize])
            } else {
                format!("Key {}", self.key)
            });
        }
        parts.join("+")
    }
}

#[derive(Default)]
struct Tap {
    down: Option<Instant>,
    first: Option<(Instant, isize)>,
    clean: bool,
}
impl Tap {
    fn key(&mut self, key: u32, down: bool, foreground: HWND) -> bool {
        if matches!(key, 0x11 | 0xa2 | 0xa3) {
            if down {
                if self.down.is_none() {
                    self.down = Some(Instant::now());
                    self.clean = true;
                }
            } else if let Some(start) = self.down.take() {
                if self.clean && start.elapsed() <= DOUBLE_TAP {
                    let now = Instant::now();
                    if self.first.take().is_some_and(|(last, window)| {
                        now.duration_since(last) <= DOUBLE_TAP && window == foreground.0 as isize
                    }) {
                        return true;
                    }
                    self.first = Some((now, foreground.0 as isize));
                } else {
                    self.first = None;
                }
            }
        } else if down {
            self.clean = false;
            self.first = None;
        }
        false
    }
}

struct Recording {
    window: isize,
    modifiers: u32,
    candidate: Option<Shortcut>,
    tap: Tap,
    keys: [bool; 256],
    cancel: bool,
}

struct State {
    recording: Option<Recording>,
    result: Option<Option<Shortcut>>,
    enabled: bool,
    tap: Tap,
    keys: [bool; 256],
}
impl Default for State {
    fn default() -> Self {
        Self {
            recording: None,
            result: None,
            enabled: false,
            tap: Tap::default(),
            keys: [false; 256],
        }
    }
}
thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub struct Hook(HHOOK);
impl Hook {
    pub fn new() -> AppResult<Self> {
        unsafe {
            Ok(Self(SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_hook),
                Some(GetModuleHandleW(None)?.into()),
                0,
            )?))
        }
    }
}
impl Drop for Hook {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.0);
        }
    }
}

pub fn configure(enabled: bool) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.enabled = enabled;
        state.tap = Tap::default();
    });
}

pub fn record(window: HWND) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.result = None;
        state.recording = Some(Recording {
            window: window.0 as isize,
            modifiers: 0,
            candidate: None,
            tap: Tap::default(),
            keys: [false; 256],
            cancel: false,
        });
    });
}

pub fn cancel() {
    STATE.with(|state| state.borrow_mut().recording = None);
}

pub fn interrupt() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.tap.clean = false;
        state.tap.first = None;
        if let Some(recording) = state.recording.as_mut() {
            recording.tap.clean = false;
            recording.tap.first = None;
        }
    });
}

pub fn take_result() -> Option<Option<Shortcut>> {
    STATE.with(|state| state.borrow_mut().result.take())
}

pub fn check(shortcut: Shortcut, host: HWND, current: &Config) -> AppResult<&'static str> {
    if shortcut.double_ctrl {
        return Ok("Double Ctrl recorded. Other apps' double-tap actions cannot be checked.");
    }
    let config = Config {
        hotkey_modifiers: shortcut.modifiers,
        hotkey_key: shortcut.key,
        hotkey_double_ctrl: false,
        trigger: crate::config::Trigger::Hotkey,
        ..current.clone()
    };
    config.validate()?;
    if matches!(shortcut.key, 0x10..=0x12 | 0x5b | 0x5c | 0xa0..=0xa5) {
        return Err("Press a key with Ctrl, Alt or Win, or double-tap Ctrl.".into());
    }
    if shortcut.key == VK_F12.0 || shortcut.modifiers & 3 == 3 && shortcut.key == VK_DELETE.0 {
        return Err("This shortcut is reserved by Windows.".into());
    }
    if shortcut.modifiers == 2 && matches!(shortcut.key, 0x41 | 0x43 | 0x56 | 0x58 | 0x59 | 0x5a) {
        return Err(
            "This shortcut is used for text editing. Add Alt or choose another key.".into(),
        );
    }
    if current.trigger == crate::config::Trigger::Hotkey
        && !current.hotkey_double_ctrl
        && Shortcut::from_config(current) == shortcut
    {
        return Ok("Current shortcut. Save to keep it.");
    }
    unsafe {
        RegisterHotKey(
            Some(host),
            3,
            HOT_KEY_MODIFIERS(shortcut.modifiers) | MOD_NOREPEAT,
            shortcut.key as u32,
        )
        .map_err(|_| "This global shortcut is already in use. Record another.")?;
        UnregisterHotKey(Some(host), 3)?;
    }
    Ok("Available as a global shortcut. Save to apply.")
}

unsafe extern "system" fn keyboard_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code < 0 {
            return CallNextHookEx(None, code, wp, lp);
        }
        let event = &*(lp.0 as *const KBDLLHOOKSTRUCT);
        if event.dwExtraInfo == super::REPLAY {
            return CallNextHookEx(None, code, wp, lp);
        }
        let down = wp.0 as u32 == WM_KEYDOWN || wp.0 as u32 == WM_SYSKEYDOWN;
        let foreground = GetForegroundWindow();
        let suppress = STATE.with(|state| {
            let mut state = state.borrow_mut();
            if let Some(recording) = state.recording.as_mut() {
                if foreground.0 as isize != recording.window {
                    state.recording = None;
                    return false;
                }
                let key = event.vkCode as usize;
                if key >= 256 {
                    return true;
                }
                recording.keys[key] = down;
                let modifier = match event.vkCode {
                    0x11 | 0xa2 | 0xa3 => 2,
                    0x12 | 0xa4 | 0xa5 => 1,
                    0x10 | 0xa0 | 0xa1 => 4,
                    0x5b | 0x5c => 8,
                    _ => 0,
                };
                recording.modifiers = 0;
                for (flag, keys) in [
                    (2, &[0x11, 0xa2, 0xa3][..]),
                    (1, &[0x12, 0xa4, 0xa5][..]),
                    (4, &[0x10, 0xa0, 0xa1][..]),
                    (8, &[0x5b, 0x5c][..]),
                ] {
                    if keys.iter().any(|key| recording.keys[*key]) {
                        recording.modifiers |= flag;
                    }
                }
                if down && event.vkCode == VK_ESCAPE.0 as u32 {
                    recording.cancel = true;
                }
                if down
                    && modifier == 0
                    && event.vkCode != VK_ESCAPE.0 as u32
                    && recording.candidate.is_none()
                {
                    recording.candidate = Some(Shortcut {
                        modifiers: recording.modifiers,
                        key: event.vkCode as u16,
                        double_ctrl: false,
                    });
                }
                let double = if matches!(event.vkCode, 0x11 | 0xa2 | 0xa3)
                    && recording.modifiers & !2 != 0
                {
                    recording.tap = Tap::default();
                    false
                } else {
                    recording.tap.key(event.vkCode, down, foreground)
                };
                if double {
                    recording.candidate = Some(Shortcut {
                        modifiers: 0,
                        key: VK_CONTROL.0,
                        double_ctrl: true,
                    });
                }
                if !down
                    && !recording.keys.iter().any(|down| *down)
                    && (recording.candidate.is_some() || recording.cancel)
                {
                    let window = recording.window;
                    let result = if recording.cancel {
                        None
                    } else {
                        recording.candidate
                    };
                    state.recording = None;
                    state.result = Some(result);
                    if let Err(error) =
                        PostMessageW(Some(HWND(window as *mut _)), RECORDED, WPARAM(0), LPARAM(0))
                    {
                        super::defer_error(error.to_string());
                    }
                }
                return true;
            }
            let key = event.vkCode as usize;
            if key < 256 {
                state.keys[key] = down;
            }
            if state.enabled {
                let ctrl = matches!(event.vkCode, 0x11 | 0xa2 | 0xa3);
                if down
                    && ctrl
                    && state
                        .keys
                        .iter()
                        .enumerate()
                        .any(|(key, pressed)| *pressed && !matches!(key, 0x11 | 0xa2 | 0xa3))
                {
                    state.tap = Tap::default();
                } else if state.tap.key(event.vkCode, down, foreground) {
                    let host = HWND(HOST.load(Ordering::Relaxed) as *mut _);
                    if let Err(error) =
                        PostMessageW(Some(host), TRIGGER_EVENT, WPARAM(0), LPARAM(0))
                    {
                        super::defer_error(error.to_string());
                    }
                }
            }
            false
        });
        if suppress {
            LRESULT(1)
        } else {
            CallNextHookEx(None, code, wp, lp)
        }
    }
}
