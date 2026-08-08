use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub default_provider: Option<String>,
    pub privacy: PrivacyMode,
    pub providers: BTreeMap<String, ProviderConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_provider: None,
            privacy: PrivacyMode::Smart,
            providers: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum PrivacyMode {
    Minimal,
    Smart,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub endpoint: String,
    pub model: String,
    #[serde(default)]
    pub credential: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
}

pub fn config_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("dev", "llmfuck", "llmfuck")
        .context("cannot determine the platform configuration directory")?;
    Ok(dirs.config_dir().join("config.toml"))
}

pub fn load() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

pub fn save(config: &Config) -> Result<()> {
    let path = config_path()?;
    let parent = path.parent().context("configuration path has no parent")?;
    fs::create_dir_all(parent)?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, toml::to_string_pretty(config)?)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn active_provider(config: &Config) -> Result<(&str, &ProviderConfig)> {
    let name = config
        .default_provider
        .as_deref()
        .context("no provider configured; run `fuck config`")?;
    let provider = config
        .providers
        .get(name)
        .with_context(|| format!("provider `{name}` does not exist"))?;
    if provider.endpoint.trim().is_empty() || provider.model.trim().is_empty() {
        bail!("provider `{name}` has an incomplete endpoint or model");
    }
    Ok((name, provider))
}
