use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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
const BIO_KEY_ENTRY: &str = "local_vault_key_bio";
const NONCE_LEN: usize = 24;

static MASTER_KEY: OnceLock<Result<[u8; 32], String>> = OnceLock::new();

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
        Ok(Self {
            path: vault_path()?,
            key: master_key()?,
        })
    }

    #[cfg(test)]
    fn with_key(path: PathBuf, key: [u8; 32]) -> Self {
        Self { path, key }
    }

    pub fn create_group(&self, group: &str) -> Result<()> {
        let mut vault = self.load()?;
        vault.groups.entry(group.to_string()).or_default();
        self.save(&vault)
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

    fn rename(&self, group: &str, old_key: &str, entry: &Entry) -> Result<()> {
        let mut vault = self.load()?;
        let entries = vault
            .groups
            .get_mut(group)
            .with_context(|| format!("分组 {group} 不存在"))?;
        let pos = entries
            .iter()
            .position(|e| e.key == old_key)
            .with_context(|| format!("未找到 {group}/{old_key}"))?;
        if entry.key != old_key && entries.iter().any(|e| e.key == entry.key) {
            bail!("{group}/{} 已存在，无法重命名为该键名", entry.key);
        }
        entries[pos] = entry.clone();
        entries.sort_by(|a, b| a.key.cmp(&b.key));
        self.save(&vault)
    }
}

fn data_dir() -> Result<PathBuf> {
    Config::path()?
        .parent()
        .map(|p| p.to_path_buf())
        .context("无法定位配置目录")
}

fn vault_path() -> Result<PathBuf> {
    let name = if cfg!(debug_assertions) {
        "vault-dev.enc"
    } else {
        "vault.enc"
    };
    Ok(data_dir()?.join(name))
}

fn master_key_path() -> Result<PathBuf> {
    let name = if cfg!(debug_assertions) {
        "master-dev.key"
    } else {
        "master.key"
    };
    Ok(data_dir()?.join(name))
}

pub fn is_configured() -> bool {
    master_key_path().map(|p| p.exists()).unwrap_or(false)
        || vault_path().map(|p| p.exists()).unwrap_or(false)
}

pub fn master_key() -> Result<[u8; 32]> {
    MASTER_KEY
        .get_or_init(|| load_master_key().map_err(|e| format!("{e:#}")))
        .as_ref()
        .map_err(|e| anyhow!(e.clone()))
        .copied()
}

fn keyring_entry(name: &str) -> Result<KeyringEntry> {
    KeyringEntry::new(KEYRING_SERVICE, name).context("无法访问系统钥匙串")
}

fn load_master_key() -> Result<[u8; 32]> {
    let path = master_key_path()?;
    if let Some(key) = read_key_file(&path)? {
        return Ok(key);
    }
    // 一次性迁移：钥匙串里的旧主密钥写入 0600 文件后删除钥匙串条目，
    // 这次读取可能弹最后一次钥匙串授权窗
    if let Some(key) = legacy_keychain_key() {
        write_key_file(&path, &key)?;
        clear_legacy_keychain_key();
        return Ok(key);
    }
    let key: [u8; 32] = rand::random();
    write_key_file(&path, &key)?;
    Ok(key)
}

fn read_key_file(path: &Path) -> Result<Option<[u8; 32]>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("无法读取主密钥文件 {}", path.display()))?;
    let bytes = B64
        .decode(text.trim())
        .with_context(|| format!("主密钥文件 {} 不是合法 base64", path.display()))?;
    bytes_to_key(&bytes).map(Some)
}

fn write_key_file(path: &Path, key: &[u8; 32]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(path)
        .with_context(|| format!("无法写入主密钥文件 {}", path.display()))?;
    file.write_all(B64.encode(key).as_bytes())?;
    file.write_all(b"\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn legacy_keychain_key() -> Option<[u8; 32]> {
    for name in [BIO_KEY_ENTRY, MASTER_KEY_ENTRY] {
        if let Ok(entry) = keyring_entry(name)
            && let Ok(encoded) = entry.get_password()
            && let Ok(bytes) = B64.decode(encoded.trim())
            && let Ok(key) = bytes_to_key(&bytes)
        {
            return Some(key);
        }
    }
    None
}

fn clear_legacy_keychain_key() {
    for name in [BIO_KEY_ENTRY, MASTER_KEY_ENTRY] {
        if let Ok(entry) = keyring_entry(name) {
            let _ = entry.delete_credential();
        }
    }
}

fn bytes_to_key(bytes: &[u8]) -> Result<[u8; 32]> {
    <[u8; 32]>::try_from(bytes).map_err(|_| anyhow!("主密钥应为 32 字节，实际 {}", bytes.len()))
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
    fn key_file_roundtrip_and_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("master.key");
        let key = test_key();
        write_key_file(&path, &key).unwrap();
        assert_eq!(read_key_file(&path).unwrap(), Some(key));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn key_file_missing_and_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("master.key");
        assert_eq!(read_key_file(&path).unwrap(), None);
        std::fs::write(&path, "not-base64!!!").unwrap();
        assert!(read_key_file(&path).is_err());
        std::fs::write(&path, B64.encode([1u8; 8])).unwrap();
        assert!(read_key_file(&path).is_err());
    }

    #[test]
    fn key_file_rewrite_keeps_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("master.key");
        write_key_file(&path, &test_key()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        write_key_file(&path, &[9u8; 32]).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(read_key_file(&path).unwrap(), Some([9u8; 32]));
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

    #[test]
    fn rename_updates_key_and_value() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalStore::with_key(dir.path().join("vault.enc"), test_key());
        store
            .upsert("g", &Entry::new("OLD", "v1", EntryType::ApiKey))
            .unwrap();
        store
            .upsert("g", &Entry::new("OTHER", "v2", EntryType::Note))
            .unwrap();

        store
            .rename("g", "OLD", &Entry::new("NEW", "v3", EntryType::Login))
            .unwrap();
        let entries = store.list_entries("g").unwrap();
        assert_eq!(entries.len(), 2);
        let renamed = entries.iter().find(|e| e.key == "NEW").unwrap();
        assert_eq!(renamed.value, "v3");
        assert_eq!(renamed.entry_type, EntryType::Login);
        assert!(entries.iter().all(|e| e.key != "OLD"));

        // 新键名等于旧键名时退化为 upsert
        store
            .rename("g", "NEW", &Entry::new("NEW", "v4", EntryType::Login))
            .unwrap();
        let entries = store.list_entries("g").unwrap();
        assert_eq!(entries.iter().find(|e| e.key == "NEW").unwrap().value, "v4");
    }

    #[test]
    fn rename_conflicts_with_existing_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalStore::with_key(dir.path().join("vault.enc"), test_key());
        store
            .upsert("g", &Entry::new("A", "v1", EntryType::Note))
            .unwrap();
        store
            .upsert("g", &Entry::new("B", "v2", EntryType::Note))
            .unwrap();

        let err = store
            .rename("g", "A", &Entry::new("B", "v3", EntryType::Note))
            .unwrap_err();
        assert!(err.to_string().contains("已存在"), "错误信息：{err:#}");
        // 冲突时两端都不变
        let entries = store.list_entries("g").unwrap();
        assert_eq!(entries.iter().find(|e| e.key == "A").unwrap().value, "v1");
        assert_eq!(entries.iter().find(|e| e.key == "B").unwrap().value, "v2");
    }

    #[test]
    fn rename_missing_old_key_or_group_errors() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalStore::with_key(dir.path().join("vault.enc"), test_key());
        store
            .upsert("g", &Entry::new("A", "v1", EntryType::Note))
            .unwrap();

        assert!(
            store
                .rename("g", "NOPE", &Entry::new("B", "v", EntryType::Note))
                .is_err()
        );
        assert!(
            store
                .rename("no-such-group", "A", &Entry::new("B", "v", EntryType::Note))
                .is_err()
        );
        assert_eq!(store.list_entries("g").unwrap().len(), 1);
    }
}
