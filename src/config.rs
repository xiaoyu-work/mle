use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::AppResult;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum Trigger {
    #[default]
    RightHold,
    Hotkey,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum Provider {
    #[default]
    Azure,
    Codex,
    Copilot,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub provider: Provider,
    pub trigger: Trigger,
    pub hotkey_modifiers: u32,
    pub hotkey_key: u16,
    pub azure_endpoint: String,
    pub font_size: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: Provider::Azure,
            trigger: Trigger::RightHold,
            hotkey_modifiers: 3,
            hotkey_key: b'T' as u16,
            azure_endpoint: String::new(),
            font_size: 17,
        }
    }
}

impl Config {
    pub fn validate(&self) -> AppResult<()> {
        if !(12..=32).contains(&self.font_size) {
            return Err("Translation font size must be between 12 and 32 px.".into());
        }
        if self.hotkey_key == 0 || self.hotkey_modifiers & !7 != 0 {
            return Err("Choose a valid shortcut using Ctrl, Alt, or Shift.".into());
        }
        if self.trigger == Trigger::Hotkey && self.hotkey_modifiers & 3 == 0 {
            return Err("The shortcut must include Ctrl or Alt.".into());
        }
        if self.provider == Provider::Azure && !self.azure_endpoint.is_empty() {
            crate::translation::Endpoint::parse(&self.azure_endpoint)?;
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
