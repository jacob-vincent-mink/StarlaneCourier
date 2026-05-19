use std::{
    fs,
    path::{Path, PathBuf},
};

use keyring::Entry;
use serde::{Deserialize, Serialize};

use crate::game::Difficulty;

const SETTINGS_FILE: &str = "settings.json";
const KEYRING_SERVICE: &str = "starlane-courier";
const KEYRING_USER: &str = "llm_api_key";

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct LlmSettings {
    pub(crate) enabled: bool,
    pub(crate) endpoint_url: String,
    pub(crate) model: String,
    pub(crate) timeout_secs: u64,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint_url: "https://api.openai.com/v1/chat/completions".to_string(),
            model: "gpt-4.1-mini".to_string(),
            timeout_secs: 20,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AppSettings {
    pub(crate) tick_speed_index: usize,
    pub(crate) difficulty: Difficulty,
    pub(crate) llm: LlmSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            tick_speed_index: 1,
            difficulty: Difficulty::Normal,
            llm: LlmSettings::default(),
        }
    }
}

pub(crate) trait SettingsStore {
    fn load(&self) -> Result<Option<AppSettings>, String>;
    fn save(&self, settings: &AppSettings) -> Result<(), String>;
}

pub(crate) struct FsSettingsStore;

impl FsSettingsStore {
    fn path() -> PathBuf {
        Path::new(SETTINGS_FILE).to_path_buf()
    }
}

impl SettingsStore for FsSettingsStore {
    fn load(&self) -> Result<Option<AppSettings>, String> {
        let path = Self::path();
        if !path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
        let settings = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
        Ok(Some(settings))
    }

    fn save(&self, settings: &AppSettings) -> Result<(), String> {
        let path = Self::path();
        let json = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
        fs::write(path, json).map_err(|error| error.to_string())
    }
}

pub(crate) trait SecretStore {
    fn get_api_key(&self) -> Result<Option<String>, String>;
    fn set_api_key(&self, api_key: &str) -> Result<(), String>;
    fn clear_api_key(&self) -> Result<(), String>;

    fn has_api_key(&self) -> Result<bool, String> {
        Ok(self.get_api_key()?.is_some())
    }
}

pub(crate) struct KeyringSecretStore;

impl KeyringSecretStore {
    fn entry() -> Result<Entry, String> {
        Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|error| error.to_string())
    }
}

impl SecretStore for KeyringSecretStore {
    fn get_api_key(&self) -> Result<Option<String>, String> {
        match Self::entry()?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn set_api_key(&self, api_key: &str) -> Result<(), String> {
        Self::entry()?
            .set_password(api_key)
            .map_err(|error| error.to_string())
    }

    fn clear_api_key(&self) -> Result<(), String> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}
