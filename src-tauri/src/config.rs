use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Local,
    Cloud,
}

impl Backend {
    pub fn label(&self) -> &'static str {
        match self {
            Backend::Local => "local",
            Backend::Cloud => "cloud",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default = "default_environment")]
    pub environment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<Backend>,
    #[serde(default)]
    pub touch_id_enabled: bool,
}

fn default_environment() -> String {
    "dev".to_string()
}

impl Config {
    pub fn empty() -> Self {
        Self {
            api_base: None,
            project_id: None,
            environment: default_environment(),
            default_mode: None,
            touch_id_enabled: false,
        }
    }

    pub fn path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("无法定位 home 目录")?;
        let name = if cfg!(debug_assertions) {
            "config-dev.toml"
        } else {
            "config.toml"
        };
        Ok(home.join(".zero-api-key").join(name))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("无法读取 {}，请先运行 zak init", path.display()))?;
        toml::from_str(&text).with_context(|| format!("配置文件格式错误：{}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_id_defaults_to_disabled() {
        let config: Config = toml::from_str("").unwrap();
        assert!(!config.touch_id_enabled);
    }

    #[test]
    fn environment_defaults_to_dev() {
        let config: Config = toml::from_str(
            r#"
api_base = "https://app.infisical.com"
project_id = "pid"
"#,
        )
        .unwrap();
        assert_eq!(config.environment, "dev");
        assert_eq!(config.default_mode, None);
    }

    #[test]
    fn legacy_cloud_config_without_default_mode_parses() {
        let config: Config = toml::from_str(
            r#"
api_base = "https://app.infisical.com"
project_id = "pid"
environment = "dev"
"#,
        )
        .unwrap();
        assert_eq!(config.api_base.as_deref(), Some("https://app.infisical.com"));
        assert_eq!(config.default_mode, None);
    }

    #[test]
    fn roundtrip_with_backend() {
        let mut config = Config::empty();
        config.default_mode = Some(Backend::Local);
        let text = toml::to_string_pretty(&config).unwrap();
        assert!(text.contains("default_mode = \"local\""));
        assert!(!text.contains("api_base"));
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed.default_mode, Some(Backend::Local));
    }
}
