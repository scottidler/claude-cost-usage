use eyre::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::pricing::ModelPricing;

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    /// Override the Claude projects directory
    pub projects_dir: Option<PathBuf>,
    /// Pricing table - keyed by model name
    pub pricing: HashMap<String, ModelPricing>,
}

impl Config {
    pub fn load(config_path: Option<&PathBuf>) -> Result<Self> {
        log::debug!("Config::load: config_path={:?}", config_path);

        if let Some(path) = config_path {
            return Self::load_from_file(path).context(format!("Failed to load config from {}", path.display()));
        }

        // Try ~/.config/ccu/ccu.yml
        if let Some(config_dir) = dirs::config_dir() {
            let primary_config = config_dir.join("ccu").join("ccu.yml");
            if primary_config.exists() {
                match Self::load_from_file(&primary_config) {
                    Ok(config) => return Ok(config),
                    Err(e) => {
                        log::warn!("Failed to load config from {}: {}", primary_config.display(), e);
                    }
                }
            }
        }

        eyre::bail!(
            "No config file found. Run `ccu pricing --update` to generate one.\n\
             Config location: ~/.config/ccu/ccu.yml"
        )
    }

    fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(&path).context("Failed to read config file")?;
        let config: Self = serde_yaml::from_str(&content).context("Failed to parse config file")?;
        log::info!("Loaded config from: {}", path.as_ref().display());
        Ok(config)
    }
}
