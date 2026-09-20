use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_base: String,
    pub project_id: String,
    #[serde(default = "default_environment")]
    pub environment: String,
}

fn default_environment() -> String {
    "dev".to_string()
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("无法定位 home 目录")?;
        Ok(home.join(".zero-api-key").join("config.toml"))
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
    fn environment_defaults_to_dev() {
        let config: Config = toml::from_str(
            r#"
api_base = "https://app.infisical.com"
project_id = "pid"
"#,
        )
        .unwrap();
        assert_eq!(config.environment, "dev");
    }

    #[test]
    fn roundtrip() {
        let config = Config {
            api_base: "https://example.com".into(),
            project_id: "p1".into(),
            environment: "prod".into(),
        };
        let text = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed.api_base, "https://example.com");
        assert_eq!(parsed.environment, "prod");
    }
}
