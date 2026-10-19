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
const TRIGGER: usize = 210;
const REGIONS: &[(&str, &str)] = &[
    ("Global", ""),
    ("East US", "eastus"),
    ("East US 2", "eastus2"),
    ("West US", "westus"),
    ("West US 2", "westus2"),
    ("West US 3", "westus3"),
    ("Central US", "centralus"),
    ("South Central US", "southcentralus"),
    ("Canada Central", "canadacentral"),
    ("Brazil South", "brazilsouth"),
    ("North Europe", "northeurope"),
    ("West Europe", "westeurope"),
    ("UK South", "uksouth"),
    ("France Central", "francecentral"),
    ("Switzerland North", "switzerlandnorth"),
    ("East Asia", "eastasia"),
    ("Southeast Asia", "southeastasia"),
    ("Japan East", "japaneast"),
    ("Japan West", "japanwest"),
    ("Korea Central", "koreacentral"),
    ("Central India", "centralindia"),
    ("Australia East", "australiaeast"),
    ("South Africa North", "southafricanorth"),
    ("UAE North", "uaenorth"),
];

pub struct Window {
    pub hwnd: HWND,
    trigger: HWND,
    hotkey_label: HWND,
    hotkey: HWND,
    region_label: HWND,
    region: HWND,
    regions: Vec<String>,
    key_label: HWND,
    key: HWND,
    save: HWND,
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
                520 * scale / 96,
                250 * scale / 96,
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
            let label =
                |text: &str, y| create(w!("STATIC"), text, WINDOW_STYLE(0), 24, y + 4, 104, 22, 0);
            label("Trigger", 24)?;
            let trigger = create(
                w!("COMBOBOX"),
                "",
                WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
                136,
                24,
                336,
                120,
                TRIGGER,
            )?;
            for item in ["Hold right mouse button", "Keyboard shortcut"] {
                add_item(trigger, item)?;
            }
            let hotkey_label = label("Shortcut", 68)?;
            let hotkey = create(
                w!("msctls_hotkey32"),
                "",
                WS_TABSTOP | WS_BORDER,
                136,
                68,
                336,
                28,
                212,
            )?;
            let region_label = label("Azure region", 112)?;
            let region = create(
                w!("COMBOBOX"),
                "",
                WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
                136,
                112,
                336,
                240,
                213,
            )?;
            let mut regions = Vec::new();
            for &(label, code) in REGIONS {
                let display = if code.is_empty() {
                    label.to_owned()
                } else {
                    format!("{label} ({code})")
                };
                add_item(region, &display)?;
                regions.push(code.to_owned());
            }
            // Retain an existing region even if it is not in the preset list.
            if !regions.contains(&config.azure_region) {
                add_item(region, &config.azure_region)?;
                regions.push(config.azure_region.clone());
            }
            let key_label = label("API key", 156)?;
            let key = create(
                w!("EDIT"),
                "",
                WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_PASSWORD as u32 | ES_AUTOHSCROLL as u32),
                136,
                156,
                336,
                28,
                214,
            )?;
            send_message(key, EM_SETLIMITTEXT, WPARAM(512), LPARAM(0));
            let save = create(
                w!("BUTTON"),
                "Save",
                WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
                372,
                204,
                100,
                30,
                SAVE,
            )?;
            Ok(Self {
                hwnd,
                trigger,
                hotkey_label,
                hotkey,
                region_label,
                region,
                regions,
                key_label,
                key,
                save,
            })
        }
    }

    pub fn load(&self, config: &Config) -> AppResult<()> {
        send_message(
            self.trigger,
            CB_SETCURSEL,
            WPARAM(usize::from(config.trigger == Trigger::Hotkey)),
            LPARAM(0),
        );
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
            self.hotkey,
            HKM_SETHOTKEY,
            WPARAM((config.hotkey_key | modifiers << 8) as usize),
            LPARAM(0),
        );
        let region = self
            .regions
            .iter()
            .position(|code| code == &config.azure_region)
            .ok_or("The saved Azure region is not available.")?;
        send_message(self.region, CB_SETCURSEL, WPARAM(region), LPARAM(0));
        let key = wide(&credentials::read()?.unwrap_or_default());
        unsafe {
            SetWindowTextW(self.key, PCWSTR(key.as_ptr()))?;
        }
        self.layout()
    }

    fn layout(&self) -> AppResult<()> {
        let shortcut = send_message(self.trigger, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 == 1;
        let offset = if shortcut { 44 } else { 0 };
        unsafe {
            let scale = GetDpiForWindow(self.hwnd) as i32;
            let show = if shortcut { SW_SHOW } else { SW_HIDE };
            let _ = ShowWindow(self.hotkey_label, show);
            let _ = ShowWindow(self.hotkey, show);
            for (control, x, y, width, height) in [
                (self.region_label, 24, 72 + offset, 104, 22),
                (self.region, 136, 68 + offset, 336, 240),
                (self.key_label, 24, 116 + offset, 104, 22),
                (self.key, 136, 112 + offset, 336, 28),
                (self.save, 372, 160 + offset, 100, 30),
            ] {
                MoveWindow(
                    control,
                    x * scale / 96,
                    y * scale / 96,
                    width * scale / 96,
                    height * scale / 96,
                    true,
                )?;
            }
            SetWindowPos(
                self.hwnd,
                None,
                0,
                0,
                520 * scale / 96,
                (250 + offset) * scale / 96,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )?;
        }
        Ok(())
    }

    fn read(&self) -> AppResult<(Config, String)> {
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
        let region = send_message(self.region, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0;
        let config = Config {
            trigger,
            hotkey_modifiers: modifiers,
            hotkey_key: hotkey & 0xff,
            azure_region: self
                .regions
                .get(region as usize)
                .ok_or("Select an Azure region.")?
                .clone(),
        };
        config.validate()?;
        Ok((config, text(self.key)?.trim().to_owned()))
    }
}

fn add_item(combo: HWND, item: &str) -> AppResult<()> {
    let item = wide(item);
    if send_message(
        combo,
        CB_ADDSTRING,
        WPARAM(0),
        LPARAM(item.as_ptr() as isize),
    )
    .0 < 0
    {
        return Err("Could not populate settings choices.".into());
    }
    Ok(())
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
        if let Some(window) = app.settings.as_ref() {
            unsafe {
                SetWindowTextW(window.key, w!(""))?;
                let _ = ShowWindow(window.hwnd, SW_HIDE);
            }
        }
        Ok(())
    })
}

fn hide(hwnd: HWND) {
    let result = APP.with(|cell| -> windows::core::Result<()> {
        if let Some(window) = cell.borrow().as_ref().and_then(|app| app.settings.as_ref()) {
            unsafe {
                SetWindowTextW(window.key, w!(""))?;
            }
        }
        Ok(())
    });
    if let Err(error) = result {
        show_error(&error.to_string());
    }
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

pub unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_COMMAND if wp.0 & 0xffff == TRIGGER && (wp.0 >> 16) as u32 == CBN_SELCHANGE => {
                let result = APP.with(|cell| -> AppResult<()> {
                    if let Some(window) =
                        cell.borrow().as_ref().and_then(|app| app.settings.as_ref())
                    {
                        window.layout()?;
                    }
                    Ok(())
                });
                if let Err(error) = result {
                    show_error(&error.to_string());
                }
                LRESULT(0)
            }
            WM_COMMAND if wp.0 & 0xffff == SAVE || wp.0 & 0xffff == IDOK.0 as usize => {
                if let Err(error) = save() {
                    show_error(&error.to_string());
                }
                LRESULT(0)
            }
            WM_COMMAND if wp.0 & 0xffff == IDCANCEL.0 as usize => {
                hide(hwnd);
                LRESULT(0)
            }
            WM_CLOSE => {
                hide(hwnd);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wp, lp),
        }
    }
}
