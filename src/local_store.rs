use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use keyring::Entry as KeyringEntry;
use serde::{Deserialize, Serialize};

use crate::auth::KEYRING_SERVICE;
use crate::config::Config;
use crate::store::{Entry, Store};

const MASTER_KEY_ENTRY: &str = "local_vault_key";
const NONCE_LEN: usize = 24;

#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vault {
    #[serde(default)]
    pub groups: BTreeMap<String, Vec<Entry>>,
}

pub struct LocalStore {
    path: PathBuf,
    key: [u8; 32],
}

impl LocalStore {
    pub fn new() -> Result<Self> {
        let dir = Config::path()?
            .parent()
            .map(|p| p.to_path_buf())
            .context("无法定位配置目录")?;
        Ok(Self {
            path: dir.join("vault.enc"),
            key: master_key()?,
        })
    }

    #[cfg(test)]
    fn with_key(path: PathBuf, key: [u8; 32]) -> Self {
        Self { path, key }
    }

    fn load(&self) -> Result<Vault> {
        if !self.path.exists() {
            return Ok(Vault::default());
        }
        let data = std::fs::read(&self.path)
            .with_context(|| format!("无法读取保险库 {}", self.path.display()))?;
        decrypt_vault(&self.key, &data)
    }

    fn save(&self, vault: &Vault) -> Result<()> {
        let data = encrypt_vault(&self.key, vault)?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.path, data)?;
        Ok(())
    }
}

impl Store for LocalStore {
    fn name(&self) -> &'static str {
        "local"
    }

    fn list_groups(&self) -> Result<Vec<String>> {
        Ok(self.load()?.groups.keys().cloned().collect())
    }

    fn list_entries(&self, group: &str) -> Result<Vec<Entry>> {
        Ok(self.load()?.groups.get(group).cloned().unwrap_or_default())
    }

    fn upsert(&self, group: &str, entry: &Entry) -> Result<()> {
        let mut vault = self.load()?;
        let entries = vault.groups.entry(group.to_string()).or_default();
        match entries.iter_mut().find(|e| e.key == entry.key) {
            Some(existing) => *existing = entry.clone(),
            None => {
                entries.push(entry.clone());
                entries.sort_by(|a, b| a.key.cmp(&b.key));
            }
        }
        self.save(&vault)
    }

    fn remove(&self, group: &str, key: Option<&str>) -> Result<()> {
        let mut vault = self.load()?;
        match key {
            None => {
                vault.groups.remove(group);
            }
            Some(k) => {
                if let Some(entries) = vault.groups.get_mut(group) {
                    entries.retain(|e| e.key != k);
                    if entries.is_empty() {
                        vault.groups.remove(group);
                    }
                }
            }
        }
        self.save(&vault)
    }
}

pub fn master_key() -> Result<[u8; 32]> {
    let entry =
        KeyringEntry::new(KEYRING_SERVICE, MASTER_KEY_ENTRY).context("无法访问系统钥匙串")?;
    match entry.get_password() {
        Ok(encoded) => {
            let bytes = B64
                .decode(encoded.trim())
                .context("钥匙串中的 local_vault_key 不是合法 base64")?;
            <[u8; 32]>::try_from(bytes.as_slice())
                .map_err(|_| anyhow!("local_vault_key 应为 32 字节，实际 {}", bytes.len()))
        }
        Err(keyring::Error::NoEntry) => {
            let key: [u8; 32] = rand::random();
            entry.set_password(&B64.encode(key))?;
            Ok(key)
        }
        Err(e) => Err(e.into()),
    }
}

pub fn encrypt_vault(key: &[u8; 32], vault: &Vault) -> Result<Vec<u8>> {
    let plain = serde_json::to_vec(vault)?;
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce: [u8; NONCE_LEN] = rand::random();
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), plain.as_ref())
        .map_err(|_| anyhow!("保险库加密失败"))?;
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn decrypt_vault(key: &[u8; 32], data: &[u8]) -> Result<Vault> {
    if data.len() < NONCE_LEN {
        bail!("保险库文件损坏（长度不足 {NONCE_LEN} 字节）");
    }
    let (nonce, ciphertext) = data.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(key.into());
    let plain = cipher
        .decrypt(XNonce::from_slice(nonce), ciphertext)
        .map_err(|_| anyhow!("保险库解密失败（文件损坏或主密钥不匹配）"))?;
    serde_json::from_slice(&plain).context("保险库内容格式错误")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::EntryType;

    fn test_key() -> [u8; 32] {
        [7u8; 32]
    }

    fn sample_vault() -> Vault {
        let mut groups = BTreeMap::new();
        groups.insert(
            "openai".to_string(),
            vec![
                Entry::new("API_KEY", "sk-test123", EntryType::ApiKey),
                Entry::new("BASE_URL", "https://api.openai.com", EntryType::Note),
            ],
        );
        Vault { groups }
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let vault = sample_vault();
        let data = encrypt_vault(&test_key(), &vault).unwrap();
        assert_ne!(data, serde_json::to_vec(&vault).unwrap());
        assert_eq!(decrypt_vault(&test_key(), &data).unwrap(), vault);
    }

    #[test]
    fn corrupted_file_errors() {
        assert!(decrypt_vault(&test_key(), b"garbage").is_err());
        let mut data = encrypt_vault(&test_key(), &sample_vault()).unwrap();
        data.truncate(data.len() - 3);
        assert!(decrypt_vault(&test_key(), &data).is_err());
    }

    #[test]
    fn wrong_key_errors() {
        let data = encrypt_vault(&test_key(), &sample_vault()).unwrap();
        assert!(decrypt_vault(&[9u8; 32], &data).is_err());
    }

    #[test]
    fn store_empty_then_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.enc");
        let store = LocalStore::with_key(path.clone(), test_key());

        assert!(store.list_groups().unwrap().is_empty());
        assert!(!path.exists());

        let mut entry = Entry::new("API_KEY", "sk-1", EntryType::ApiKey);
        entry.env_name = Some("OPENAI_API_KEY".to_string());
        store.upsert("openai", &entry).unwrap();
        store
            .upsert("openai", &Entry::new("USERNAME", "bob", EntryType::Login))
            .unwrap();
        assert!(path.exists());

        let reopened = LocalStore::with_key(path.clone(), test_key());
        assert_eq!(reopened.list_groups().unwrap(), vec!["openai".to_string()]);
        let entries = reopened.list_entries("openai").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, "API_KEY");
        assert_eq!(entries[0].env_name.as_deref(), Some("OPENAI_API_KEY"));

        reopened
            .upsert("openai", &Entry::new("API_KEY", "sk-2", EntryType::ApiKey))
            .unwrap();
        let entries = reopened.list_entries("openai").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].value, "sk-2");

        reopened.remove("openai", Some("USERNAME")).unwrap();
        assert_eq!(reopened.list_entries("openai").unwrap().len(), 1);
        reopened.remove("openai", Some("API_KEY")).unwrap();
        assert!(reopened.list_groups().unwrap().is_empty());

        reopened.upsert("a", &Entry::new("K", "v", EntryType::Note)).unwrap();
        reopened.remove("a", None).unwrap();
        assert!(reopened.list_groups().unwrap().is_empty());
    }
}
