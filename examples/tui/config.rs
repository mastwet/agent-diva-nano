//! Configuration file handling.

use agent_diva_nano::NanoConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TuiConfigFile {
    pub model: String,
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_base: Option<String>,
}

impl TuiConfigFile {
    pub fn config_path() -> PathBuf {
        PathBuf::from(".nano/config.json")
    }

    pub fn load() -> Option<Self> {
        let path = Self::config_path();
        if path.exists() {
            let content = fs::read_to_string(path).ok()?;
            serde_json::from_str(&content).ok()
        } else {
            None
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn to_nano_config(&self) -> NanoConfig {
        NanoConfig {
            model: self.model.clone(),
            api_key: self.api_key.clone(),
            api_base: self.api_base.clone(),
            ..NanoConfig::default()
        }
    }

    pub fn from_nano_config(config: &NanoConfig) -> Self {
        Self {
            model: config.model.clone(),
            api_key: config.api_key.clone(),
            api_base: config.api_base.clone(),
        }
    }
}

pub fn load_initial_config() -> Result<NanoConfig, Box<dyn std::error::Error>> {
    // 1. Try config file
    if let Some(file_config) = TuiConfigFile::load() {
        return Ok(file_config.to_nano_config());
    }
    // 2. Try environment variables
    if let Ok(config) = NanoConfig::from_env() {
        // Save to file for next time
        let file_config = TuiConfigFile::from_nano_config(&config);
        let _ = file_config.save();
        return Ok(config);
    }
    // 3. No config available — wizard required
    Err("No configuration found. Please run the setup wizard.".into())
}