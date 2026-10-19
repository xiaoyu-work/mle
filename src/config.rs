use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::AppResult;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum Trigger {
    #[default]
    RightHold,
    Hotkey,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub trigger: Trigger,
    pub hold_ms: u32,
    pub hotkey_modifiers: u32,
    pub hotkey_key: u16,
    pub azure_region: String,
    pub allow_online: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            trigger: Trigger::RightHold,
            hold_ms: 500,
            hotkey_modifiers: 3,
            hotkey_key: b'T' as u16,
            azure_region: String::new(),
            allow_online: false,
        }
    }
}

impl Config {
    pub fn validate(&self) -> AppResult<()> {
        if !(250..=2000).contains(&self.hold_ms) {
            return Err("Hold duration must be between 250 and 2000 ms.".into());
        }
        if self.hotkey_key == 0 || self.hotkey_modifiers & !7 != 0 {
            return Err("Choose a valid shortcut using Ctrl, Alt, or Shift.".into());
        }
        if self.trigger == Trigger::Hotkey && self.hotkey_modifiers & 3 == 0 {
            return Err("The shortcut must include Ctrl or Alt.".into());
        }
        if !self
            .azure_region
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("Azure region must contain only letters, digits, or hyphens.".into());
        }
        Ok(())
    }

    pub fn load() -> AppResult<(Self, bool)> {
        let path = path()?;
        match std::fs::read(path) {
            Ok(bytes) => {
                let config: Self = serde_json::from_slice(&bytes)?;
                config.validate()?;
                Ok((config, false))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Self::default(), true)),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self) -> AppResult<()> {
        self.validate()?;
        let path = path()?;
        std::fs::create_dir_all(path.parent().ok_or("Invalid settings directory")?)?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(temporary, path)?;
        Ok(())
    }
}

fn path() -> AppResult<PathBuf> {
    let root = std::env::var_os("APPDATA").ok_or("APPDATA is not available.")?;
    Ok(PathBuf::from(root).join("MLE").join("settings.json"))
}
