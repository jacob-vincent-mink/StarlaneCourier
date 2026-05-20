use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::{
    fs::OpenOptions,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
};

use keyring::Entry;
use serde::{Deserialize, Serialize};

use crate::game::Difficulty;

const APP_CONFIG_DIR: &str = "spacecourier";
const SETTINGS_FILE: &str = "settings.json";
const API_KEY_FILE: &str = "api-key";
const KEYRING_SERVICE: &str = "starlane-courier";
const KEYRING_USER: &str = "llm_api_key";

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum LlmProviderPreset {
    OpenAI,
    OpenRouter,
    OpenAiCompatibleLocal,
    Ollama,
    LmStudio,
    Custom,
}

impl LlmProviderPreset {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::OpenAI => "OpenAI",
            Self::OpenRouter => "OpenRouter",
            Self::OpenAiCompatibleLocal => "OpenAI-Compatible Local",
            Self::Ollama => "Ollama Local",
            Self::LmStudio => "LM Studio",
            Self::Custom => "Custom",
        }
    }

    pub(crate) fn default_endpoint(self) -> &'static str {
        match self {
            Self::OpenAI => "https://api.openai.com/v1/chat/completions",
            Self::OpenRouter => "https://openrouter.ai/api/v1/chat/completions",
            Self::OpenAiCompatibleLocal => "http://localhost:8049/v1/chat/completions",
            Self::Ollama => "http://localhost:11434/v1/chat/completions",
            Self::LmStudio => "http://localhost:1234/v1/chat/completions",
            Self::Custom => "",
        }
    }

    pub(crate) fn default_model(self) -> &'static str {
        match self {
            Self::OpenAI => "gpt-4.1-mini",
            Self::OpenRouter => "openai/gpt-4.1-mini",
            Self::OpenAiCompatibleLocal => "nemotron",
            Self::Ollama => "llama3.1",
            Self::LmStudio => "local-model",
            Self::Custom => "",
        }
    }

    pub(crate) fn from_index(index: usize) -> Self {
        match index {
            0 => Self::OpenAI,
            1 => Self::OpenRouter,
            2 => Self::OpenAiCompatibleLocal,
            3 => Self::Ollama,
            4 => Self::LmStudio,
            _ => Self::Custom,
        }
    }

    pub(crate) fn index(self) -> usize {
        match self {
            Self::OpenAI => 0,
            Self::OpenRouter => 1,
            Self::OpenAiCompatibleLocal => 2,
            Self::Ollama => 3,
            Self::LmStudio => 4,
            Self::Custom => 5,
        }
    }

    pub(crate) fn requires_api_key(self) -> bool {
        matches!(
            self,
            Self::OpenAI | Self::OpenRouter | Self::OpenAiCompatibleLocal
        )
    }
}

pub(crate) const LLM_PROVIDER_PRESETS: [LlmProviderPreset; 6] = [
    LlmProviderPreset::OpenAI,
    LlmProviderPreset::OpenRouter,
    LlmProviderPreset::OpenAiCompatibleLocal,
    LlmProviderPreset::Ollama,
    LlmProviderPreset::LmStudio,
    LlmProviderPreset::Custom,
];

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct LlmSettings {
    pub(crate) enabled: bool,
    pub(crate) provider: LlmProviderPreset,
    pub(crate) endpoint_url: String,
    pub(crate) model: String,
    pub(crate) timeout_secs: u64,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: LlmProviderPreset::OpenAI,
            endpoint_url: LlmProviderPreset::OpenAI.default_endpoint().to_string(),
            model: LlmProviderPreset::OpenAI.default_model().to_string(),
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
    fn path() -> Result<PathBuf, String> {
        Ok(config_dir()?.join(SETTINGS_FILE))
    }

    fn legacy_path() -> PathBuf {
        Path::new(SETTINGS_FILE).to_path_buf()
    }
}

impl SettingsStore for FsSettingsStore {
    fn load(&self) -> Result<Option<AppSettings>, String> {
        let path = Self::path()?;
        if path.exists() {
            let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
            let settings = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
            return Ok(Some(settings));
        }

        let legacy = Self::legacy_path();
        if !legacy.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(&legacy).map_err(|error| error.to_string())?;
        let settings = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
        let _ = self.save(&settings);
        Ok(Some(settings))
    }

    fn save(&self, settings: &AppSettings) -> Result<(), String> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

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

pub(crate) struct FsSecretStore;

impl FsSecretStore {
    fn path() -> Result<PathBuf, String> {
        Ok(config_dir()?.join(API_KEY_FILE))
    }

    fn legacy_entry() -> Result<Entry, String> {
        Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|error| error.to_string())
    }

    fn read_file() -> Result<Option<String>, String> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
        let trimmed = raw.trim_end_matches(['\n', '\r']);
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed.to_string()))
        }
    }

    fn write_file(api_key: &str) -> Result<(), String> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        #[cfg(unix)]
        {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(&path)
                .map_err(|error| error.to_string())?;
            file.write_all(api_key.as_bytes())
                .map_err(|error| error.to_string())?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
        }

        #[cfg(not(unix))]
        {
            fs::write(&path, api_key).map_err(|error| error.to_string())?;
        }

        Ok(())
    }

    fn clear_file() -> Result<(), String> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(());
        }

        fs::remove_file(path).map_err(|error| error.to_string())
    }

    fn migrate_legacy_keyring() -> Result<Option<String>, String> {
        match Self::legacy_entry()?.get_password() {
            Ok(value) => {
                let _ = Self::write_file(&value);
                Ok(Some(value))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }
}

impl SecretStore for FsSecretStore {
    fn get_api_key(&self) -> Result<Option<String>, String> {
        if let Some(value) = Self::read_file()? {
            return Ok(Some(value));
        }

        Self::migrate_legacy_keyring()
    }

    fn set_api_key(&self, api_key: &str) -> Result<(), String> {
        Self::write_file(api_key)?;
        if let Ok(entry) = Self::legacy_entry() {
            let _ = entry.set_password(api_key);
        }
        Ok(())
    }

    fn clear_api_key(&self) -> Result<(), String> {
        Self::clear_file()?;
        if let Ok(entry) = Self::legacy_entry() {
            let _ = entry.delete_credential();
        }
        Ok(())
    }
}

fn config_dir() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(path).join(APP_CONFIG_DIR));
    }

    if let Some(home) = env::var_os("HOME") {
        return Ok(PathBuf::from(home).join(".config").join(APP_CONFIG_DIR));
    }

    Err("HOME or XDG_CONFIG_HOME is not set".to_string())
}
