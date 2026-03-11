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
    /// Custom pricing overrides per model
    pub pricing: HashMap<String, PricingOverride>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PricingOverride {
    pub input_per_mtok: Option<f64>,
    pub output_per_mtok: Option<f64>,
    pub cache_5m_write_per_mtok: Option<f64>,
    pub cache_1h_write_per_mtok: Option<f64>,
    pub cache_read_per_mtok: Option<f64>,
}

impl Config {
    pub fn load(config_path: Option<&PathBuf>) -> Result<Self> {
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

        log::info!("No config file found, using defaults");
        Ok(Self::default())
    }

    fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(&path).context("Failed to read config file")?;
        let config: Self = serde_yaml::from_str(&content).context("Failed to parse config file")?;
        log::info!("Loaded config from: {}", path.as_ref().display());
        Ok(config)
    }

    /// Apply pricing overrides to a base pricing entry
    pub fn apply_overrides(&self, model: &str, base: &ModelPricing) -> ModelPricing {
        if let Some(overrides) = self.pricing.get(model) {
            ModelPricing {
                input_per_mtok: overrides.input_per_mtok.unwrap_or(base.input_per_mtok),
                output_per_mtok: overrides.output_per_mtok.unwrap_or(base.output_per_mtok),
                cache_5m_write_per_mtok: overrides
                    .cache_5m_write_per_mtok
                    .unwrap_or(base.cache_5m_write_per_mtok),
                cache_1h_write_per_mtok: overrides
                    .cache_1h_write_per_mtok
                    .unwrap_or(base.cache_1h_write_per_mtok),
                cache_read_per_mtok: overrides.cache_read_per_mtok.unwrap_or(base.cache_read_per_mtok),
            }
        } else {
            base.clone()
        }
    }
}
