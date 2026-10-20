use crate::{AppResult, config::Config};
use std::{
    cell::RefCell,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub const RECORDED: u32 = WM_APP + 22;
pub const RECORD_TIMER: usize = 73;
const TAP_INTERVAL: Duration = Duration::from_millis(350);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub modifiers: u32,
    pub key: u16,
    pub double_tap: bool,
}

impl Shortcut {
    pub fn from_config(config: &Config) -> Self {
        Self {
            modifiers: config.hotkey_modifiers,
            key: config.hotkey_key,
            double_tap: config.hotkey_double_tap,
        }
    }

    pub fn label(self) -> String {
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
        let label = parts.join("+");
        if self.double_tap {
            format!("{label}, {label}")
        } else {
            label
        }
    }
}

fn normalized(key: u16) -> u16 {
    match key {
        0xa0 | 0xa1 => 0x10,
        0xa2 | 0xa3 => 0x11,
        0xa4 | 0xa5 => 0x12,
        0x5c => 0x5b,
        _ => key,
    }
}

#[derive(Clone)]
struct Taps {
    keys: [bool; 256],
    active: Option<(u16, Instant)>,
    first: Option<(u16, Instant, isize)>,
}
impl Default for Taps {
    fn default() -> Self {
        Self {
            keys: [false; 256],
            active: None,
            first: None,
        }
    }
}
impl Taps {
    fn interrupt(&mut self) {
        self.active = None;
        self.first = None;
    }

    fn event(&mut self, key: u16, down: bool, foreground: HWND) -> Option<u16> {
        let index = key as usize;
        if index >= self.keys.len() {
            self.interrupt();
            return None;
        }
        if down {
            if self.keys[index] {
                return None;
            }
            let alone = !self.keys.iter().any(|pressed| *pressed);
            self.keys[index] = true;
            if !alone {
                self.interrupt();
            } else {
                self.active = Some((key, Instant::now()));
            }
            return None;
        }
        self.keys[index] = false;
        let (pressed, start) = self.active.take()?;
        if pressed != key || start.elapsed() > TAP_INTERVAL || self.keys.iter().any(|p| *p) {
            self.interrupt();
            return None;
        }
        let key = normalized(key);
        let now = Instant::now();
        if self.first.take().is_some_and(|(previous, time, window)| {
            previous == key
                && now.duration_since(time) <= TAP_INTERVAL
                && window == foreground.0 as isize
        }) {
            Some(key)
        } else {
            self.first = Some((key, now, foreground.0 as isize));
            None
        }
    }
}

struct Recording {
    window: isize,
    modifiers: u32,
    candidate: Option<Shortcut>,
    keys: [bool; 256],
    cancel: bool,
    taps: Taps,
    released: Option<Instant>,
}

#[derive(Default)]
struct State {
    recording: Option<Recording>,
    result: Option<Option<Shortcut>>,
    shortcut: Option<Shortcut>,
    taps: Taps,
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

pub fn configure(config: &Config) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.shortcut = (config.trigger == crate::config::Trigger::Hotkey
            && config.hotkey_double_tap)
            .then(|| Shortcut::from_config(config));
        state.taps.interrupt();
    });
}

pub fn interrupt() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.taps.interrupt();
        if let Some(recording) = state.recording.as_mut() {
            recording.taps.interrupt();
        }
    });
}

pub fn record(window: HWND) -> AppResult<()> {
    cancel();
    if unsafe { SetTimer(Some(window), RECORD_TIMER, 30, None) } == 0 {
        return Err("Cannot start shortcut recording timer.".into());
    }
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.result = None;
        let taps = state.taps.clone();
        state.recording = Some(Recording {
            window: window.0 as isize,
            modifiers: 0,
            candidate: None,
            keys: taps.keys,
            cancel: false,
            taps,
            released: None,
        });
    });
    Ok(())
}

pub fn cancel() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Some(recording) = state.recording.take() {
            state.taps = recording.taps;
            if let Err(error) =
                unsafe { KillTimer(Some(HWND(recording.window as *mut _)), RECORD_TIMER) }
            {
                super::defer_error(error.to_string());
            }
        }
        state.taps.interrupt();
    });
}

fn finish(state: &mut State) {
    let Some(recording) = state.recording.take() else {
        return;
    };
    state.result = Some(if recording.cancel {
        None
    } else {
        recording.candidate
    });
    state.taps = recording.taps;
    state.taps.interrupt();
    unsafe {
        let window = HWND(recording.window as *mut _);
        if let Err(error) = KillTimer(Some(window), RECORD_TIMER) {
            super::defer_error(error.to_string());
        }
        if let Err(error) = PostMessageW(Some(window), RECORDED, WPARAM(0), LPARAM(0)) {
            super::defer_error(error.to_string());
        }
    }
}

pub fn poll(window: HWND) -> AppResult<()> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.recording.as_ref().is_some_and(|recording| {
            recording.window == window.0 as isize
                && recording
                    .released
                    .is_some_and(|time| time.elapsed() > TAP_INTERVAL)
        }) {
            finish(&mut state);
        }
    });
    Ok(())
}

pub fn take_result() -> Option<Option<Shortcut>> {
    STATE.with(|state| state.borrow_mut().result.take())
}

pub fn check(shortcut: Shortcut, host: HWND, current: &Config) -> AppResult<&'static str> {
    let config = Config {
        hotkey_modifiers: shortcut.modifiers,
        hotkey_key: shortcut.key,
        hotkey_double_tap: shortcut.double_tap,
        trigger: crate::config::Trigger::Hotkey,
        ..current.clone()
    };
    config.validate()?;
    if shortcut.key == VK_F12.0 || shortcut.modifiers & 3 == 3 && shortcut.key == VK_DELETE.0 {
        return Err("This shortcut is reserved by Windows.".into());
    }
    if shortcut.double_tap {
        if matches!(normalized(shortcut.key), 0x12 | 0x5b) {
            return Err("Double-tapping Alt or Win opens system menus. Choose another key.".into());
        }
        return Ok("Double tap passes through. App gesture conflicts cannot be checked.");
    }
    if matches!(shortcut.key, 0x10..=0x12 | 0x5b | 0x5c | 0xa0..=0xa5) {
        return Err("Add another key, or double-tap this modifier.".into());
    }
    if shortcut.modifiers == 2 && matches!(shortcut.key, 0x41 | 0x43 | 0x56 | 0x58 | 0x59 | 0x5a) {
        return Err(
            "This shortcut is used for text editing. Add Alt or choose another key.".into(),
        );
    }
    if current.trigger == crate::config::Trigger::Hotkey
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
    Ok(
        if shortcut.modifiers == 0 && !matches!(shortcut.key, 0x70..=0x87) {
            "Available. This key will be reserved globally while enabled."
        } else {
            "Available as a global shortcut. Save to apply."
        },
    )
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
                    recording.cancel = true;
                    finish(&mut state);
                    return false;
                }
                let key = event.vkCode as usize;
                if key >= 256 {
                    return true;
                }
                recording.keys[key] = down;
                if down {
                    recording.released = None;
                }
                let double = recording.taps.event(event.vkCode as u16, down, foreground);
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
                        double_tap: false,
                    });
                }
                if let Some(key) = double {
                    recording.candidate = Some(Shortcut {
                        modifiers: 0,
                        key,
                        double_tap: true,
                    });
                }
                if !down
                    && !recording.keys.iter().any(|down| *down)
                    && (recording.candidate.is_some() || recording.cancel)
                {
                    if recording.cancel
                        || recording
                            .candidate
                            .is_some_and(|s| s.modifiers != 0 || s.double_tap)
                    {
                        finish(&mut state);
                    } else {
                        recording.released = Some(Instant::now());
                    }
                }
                return true;
            }
            let double = state.taps.event(event.vkCode as u16, down, foreground);
            if let Some(shortcut) = state.shortcut
                && double == Some(normalized(shortcut.key))
            {
                let host = HWND(super::HOST.load(Ordering::Relaxed) as *mut _);
                if let Err(error) =
                    PostMessageW(Some(host), super::TRIGGER_EVENT, WPARAM(0), LPARAM(0))
                {
                    super::defer_error(error.to_string());
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
