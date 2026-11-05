use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub light: bool,
    pub chinese: bool,
}

impl Preferences {
    fn path() -> anyhow::Result<std::path::PathBuf> {
        Ok(
            zero_api_key_lib::config::Config::path()?.with_file_name(if cfg!(debug_assertions) {
                "desktop-dev.json"
            } else {
                "desktop.json"
            }),
        )
    }
    pub fn load() -> Self {
        Self::path()
            .ok()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path()?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, serde_json::to_vec(self)?)?;
        Ok(())
    }
}
