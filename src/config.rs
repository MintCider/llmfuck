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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
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
    save_to_path(config, &path)
}

fn save_to_path(config: &Config, path: &std::path::Path) -> Result<()> {
    let parent = path.parent().context("configuration path has no parent")?;
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, toml::to_string_pretty(config)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&tmp, path)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_plaintext_api_key_only_when_configured() {
        let mut config = Config::default();
        config.providers.insert(
            "test".to_string(),
            ProviderConfig {
                endpoint: "http://localhost/v1/chat/completions".to_string(),
                model: "test".to_string(),
                credential: None,
                api_key_env: None,
                api_key: Some("secret-value".to_string()),
            },
        );
        let serialized = toml::to_string(&config).unwrap();
        assert!(serialized.contains("api_key = \"secret-value\""));
    }

    #[cfg(unix)]
    #[test]
    fn writes_private_config_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config").join("config.toml");
        save_to_path(&Config::default(), &path).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}
