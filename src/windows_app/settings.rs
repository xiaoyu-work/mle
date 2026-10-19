use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{Controls::*, HiDpi::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};

use super::{APP, credentials, send_message, show_error, wide};
use crate::{
    AppResult,
    config::{Config, Trigger},
};

const SAVE: usize = 200;
const REMOVE_KEY: usize = 201;

pub struct Window {
    pub hwnd: HWND,
    trigger: HWND,
    duration: HWND,
    hotkey: HWND,
    region: HWND,
    key: HWND,
    online: HWND,
    key_status: HWND,
}

impl Window {
    pub fn create(config: &Config) -> AppResult<Self> {
        unsafe {
            let controls = INITCOMMONCONTROLSEX {
                dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_WIN95_CLASSES,
            };
            if !InitCommonControlsEx(&controls).as_bool() {
                return Err("Could not initialize Windows settings controls.".into());
            }
            let instance = GetModuleHandleW(None)?;
            let scale = GetDpiForSystem() as i32;
            let hwnd = CreateWindowExW(
                WS_EX_CONTROLPARENT | WS_EX_APPWINDOW,
                w!("MLE.Settings"),
                w!("MLE - Settings"),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                620 * scale / 96,
                580 * scale / 96,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            let create = |class: PCWSTR,
                          text: &str,
                          style: WINDOW_STYLE,
                          x,
                          y,
                          width,
                          height,
                          id|
             -> AppResult<HWND> {
                let text = wide(text);
                let child = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    PCWSTR(text.as_ptr()),
                    WS_CHILD | WS_VISIBLE | style,
                    x * scale / 96,
                    y * scale / 96,
                    width * scale / 96,
                    height * scale / 96,
                    Some(hwnd),
                    Some(HMENU(id as *mut _)),
                    Some(instance.into()),
                    None,
                )?;
                send_message(
                    child,
                    WM_SETFONT,
                    WPARAM(GetStockObject(DEFAULT_GUI_FONT).0 as usize),
                    LPARAM(1),
                );
                Ok(child)
            };
            let label = |text: &str, x, y, width, height| {
                create(w!("STATIC"), text, WINDOW_STYLE(0), x, y, width, height, 0)
            };
            label(
                "Translate selected text into Simplified Chinese",
                24,
                20,
                550,
                24,
            )?;
            label("Trigger", 24, 62, 160, 22)?;
            let trigger = create(
                w!("COMBOBOX"),
                "",
                WS_TABSTOP | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
                198,
                58,
                365,
                120,
                210,
            )?;
            for item in ["Hold right mouse button", "Custom keyboard shortcut"] {
                let text = wide(item);
                send_message(
                    trigger,
                    CB_ADDSTRING,
                    WPARAM(0),
                    LPARAM(text.as_ptr() as isize),
                );
            }
            send_message(
                trigger,
                CB_SETCURSEL,
                WPARAM(usize::from(config.trigger == Trigger::Hotkey)),
                LPARAM(0),
            );
            label("Hold duration (250-2000 ms)", 24, 103, 172, 24)?;
            let duration = create(
                w!("EDIT"),
                &config.hold_ms.to_string(),
                WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_NUMBER as u32 | ES_AUTOHSCROLL as u32),
                198,
                100,
                100,
                26,
                211,
            )?;
            label("Shortcut (press keys here)", 24, 145, 172, 24)?;
            let hotkey = create(
                w!("msctls_hotkey32"),
                "",
                WS_TABSTOP | WS_BORDER,
                198,
                140,
                365,
                28,
                212,
            )?;
            let mut modifiers = 0u16;
            if config.hotkey_modifiers & 1 != 0 {
                modifiers |= HOTKEYF_ALT as u16;
            }
            if config.hotkey_modifiers & 2 != 0 {
                modifiers |= HOTKEYF_CONTROL as u16;
            }
            if config.hotkey_modifiers & 4 != 0 {
                modifiers |= HOTKEYF_SHIFT as u16;
            }
            send_message(
                hotkey,
                HKM_SETHOTKEY,
                WPARAM((config.hotkey_key | modifiers << 8) as usize),
                LPARAM(0),
            );
            label("Azure region (e.g. westus2)", 24, 195, 172, 24)?;
            let region = create(
                w!("EDIT"),
                &config.azure_region,
                WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                198,
                190,
                365,
                28,
                213,
            )?;
            label("Azure subscription key", 24, 236, 172, 24)?;
            let key = create(
                w!("EDIT"),
                "",
                WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_PASSWORD as u32 | ES_AUTOHSCROLL as u32),
                198,
                232,
                365,
                28,
                214,
            )?;
            send_message(key, EM_SETLIMITTEXT, WPARAM(512), LPARAM(0));
            let stored = credentials::read()?.is_some();
            let status = if stored {
                "A key is saved. Leave the field blank to keep it."
            } else {
                "No key saved. Local dictionary works without a key."
            };
            let key_status = label(status, 24, 276, 540, 24)?;
            create(
                w!("BUTTON"),
                "Delete saved key",
                WS_TABSTOP,
                198,
                306,
                165,
                28,
                REMOVE_KEY,
            )?;
            let online = create(
                w!("BUTTON"),
                "Allow online translation when local lookup has no entry",
                WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
                24,
                360,
                545,
                26,
                215,
            )?;
            send_message(
                online,
                BM_SETCHECK,
                WPARAM(usize::from(config.allow_online)),
                LPARAM(0),
            );
            label(
                "Online mode sends selected text to Microsoft Azure only when triggered.\nNo selection history is saved. Keys stay in Windows Credential Manager.",
                24,
                396,
                550,
                46,
            )?;
            label(
                "English words: local dictionary. Other text: Azure if enabled.\nEsc or an outside click closes the translation popup.",
                24,
                451,
                550,
                40,
            )?;
            create(
                w!("BUTTON"),
                "Save",
                WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
                458,
                496,
                105,
                30,
                SAVE,
            )?;
            Ok(Self {
                hwnd,
                trigger,
                duration,
                hotkey,
                region,
                key,
                online,
                key_status,
            })
        }
    }

    fn read(&self) -> AppResult<(Config, String)> {
        {
            let trigger = match send_message(self.trigger, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 {
                0 => Trigger::RightHold,
                1 => Trigger::Hotkey,
                _ => return Err("Select a trigger.".into()),
            };
            let hotkey = send_message(self.hotkey, HKM_GETHOTKEY, WPARAM(0), LPARAM(0)).0 as u16;
            let flags = hotkey >> 8;
            let mut modifiers = 0;
            if flags & HOTKEYF_ALT as u16 != 0 {
                modifiers |= 1;
            }
            if flags & HOTKEYF_CONTROL as u16 != 0 {
                modifiers |= 2;
            }
            if flags & HOTKEYF_SHIFT as u16 != 0 {
                modifiers |= 4;
            }
            let config = Config {
                trigger,
                hold_ms: text(self.duration)?
                    .trim()
                    .parse()
                    .map_err(|_| "Enter a numeric hold duration.")?,
                hotkey_modifiers: modifiers,
                hotkey_key: hotkey & 0xff,
                azure_region: text(self.region)?.trim().to_owned(),
                allow_online: send_message(self.online, BM_GETCHECK, WPARAM(0), LPARAM(0)).0
                    == BST_CHECKED.0 as isize,
            };
            config.validate()?;
            Ok((config, text(self.key)?.trim().to_owned()))
        }
    }
}

fn text(hwnd: HWND) -> AppResult<String> {
    unsafe {
        let length = GetWindowTextLengthW(hwnd);
        let mut buffer = vec![0; length as usize + 1];
        let read = GetWindowTextW(hwnd, &mut buffer);
        Ok(String::from_utf16(&buffer[..read as usize])?)
    }
}

fn save() -> AppResult<()> {
    APP.with(|cell| {
        let mut state = cell.borrow_mut();
        let app = state.as_mut().ok_or("Application is not ready.")?;
        let window = app.settings.as_ref().ok_or("Settings are not open.")?;
        let (config, key) = window.read()?;
        let old_key = credentials::read()?;
        if config.allow_online && key.is_empty() && old_key.is_none() {
            return Err(
                "Enter an Azure subscription key before enabling online translation.".into(),
            );
        }
        let previous = app.config.clone();
        app.config = config;
        let result = (|| -> AppResult<()> {
            app.install_trigger()?;
            if !key.is_empty() {
                credentials::write(&key)?;
            }
            app.config.save()?;
            Ok(())
        })();
        if let Err(error) = result {
            app.config = previous;
            app.install_trigger()
                .map_err(|restore| format!("{error}\nCould not restore trigger: {restore}"))?;
            if !key.is_empty() {
                match old_key {
                    Some(key) => credentials::write(&key)?,
                    None => credentials::delete()?,
                }
            }
            return Err(error);
        }
        if let Some(window) = app.settings.as_ref() {
            unsafe {
                SetWindowTextW(window.key, w!(""))?;
                let status = if credentials::read()?.is_some() {
                    w!("A key is saved. Leave the field blank to keep it.")
                } else {
                    w!("No key saved. Local dictionary works without a key.")
                };
                SetWindowTextW(window.key_status, status)?;
                let _ = ShowWindow(window.hwnd, SW_HIDE);
            }
        }
        Ok(())
    })
}

pub unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_COMMAND if wp.0 & 0xffff == SAVE || wp.0 & 0xffff == IDOK.0 as usize => {
                if let Err(error) = save() {
                    show_error(&error.to_string());
                }
                LRESULT(0)
            }
            WM_COMMAND if wp.0 & 0xffff == REMOVE_KEY => {
                if MessageBoxW(
                    Some(hwnd),
                    w!("Delete the saved Azure key? Online translation will be disabled."),
                    w!("MLE"),
                    MB_YESNO | MB_ICONQUESTION,
                ) == IDYES
                {
                    let result = (|| -> AppResult<()> {
                        credentials::delete()?;
                        APP.with(|cell| -> AppResult<()> {
                            if let Some(app) = cell.borrow_mut().as_mut() {
                                app.config.allow_online = false;
                                app.config.save()?;
                                if let Some(window) = app.settings.as_ref() {
                                    send_message(window.online, BM_SETCHECK, WPARAM(0), LPARAM(0));
                                    SetWindowTextW(window.key, w!(""))?;
                                    SetWindowTextW(
                                        window.key_status,
                                        w!("No key saved. Local dictionary works without a key."),
                                    )?;
                                }
                            }
                            Ok(())
                        })
                    })();
                    if let Err(error) = result {
                        show_error(&error.to_string());
                    }
                }
                LRESULT(0)
            }
            WM_COMMAND if wp.0 & 0xffff == IDCANCEL.0 as usize => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wp, lp),
        }
    }
}
