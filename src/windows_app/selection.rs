use windows::Win32::{System::Com::*, UI::Accessibility::*};

use crate::AppResult;

pub struct Selection {
    automation: std::mem::ManuallyDrop<IUIAutomation>,
}

impl Selection {
    pub fn new() -> AppResult<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            let result = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER);
            match result {
                Ok(automation) => Ok(Self {
                    automation: std::mem::ManuallyDrop::new(automation),
                }),
                Err(error) => {
                    CoUninitialize();
                    Err(error.into())
                }
            }
        }
    }

    pub fn text(&self) -> AppResult<String> {
        unsafe {
            let mut element = self.automation.GetFocusedElement()?;
            let walker = self.automation.ControlViewWalker()?;
            // Some apps focus a child of the control exposing TextPattern.
            for _ in 0..6 {
                if element.CurrentIsPassword()?.as_bool() {
                    return Err("Password fields are not supported.".into());
                }
                if let Ok(pattern) =
                    element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                {
                    let ranges = pattern.GetSelection()?;
                    let mut text = String::new();
                    for index in 0..ranges.Length()? {
                        // UIA bounds UTF-16 units, not Unicode scalar values.
                        let part = ranges.GetElement(index)?.GetText(10002)?.to_string();
                        if !part.trim().is_empty() {
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(&part);
                        }
                        if text.chars().count() > 5000 {
                            return Err("Please select at most 5000 characters.".into());
                        }
                    }
                    let text = text.trim();
                    if !text.is_empty() {
                        return Ok(text.to_owned());
                    }
                }
                element = match walker.GetParentElement(&element) {
                    Ok(parent) => parent,
                    Err(_) => break,
                };
            }
        }
        Err("Cannot read the selection in this application. Select text in a supported text control; password fields and image text are not supported.".into())
    }
}

impl Drop for Selection {
    fn drop(&mut self) {
        // Release COM objects before balancing this thread's COM initialization.
        unsafe {
            std::mem::ManuallyDrop::drop(&mut self.automation);
            CoUninitialize();
        }
    }
}
