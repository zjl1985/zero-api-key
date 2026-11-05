use std::collections::BTreeMap;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::api::CloudStore;
use crate::config::{Backend, Config};
use crate::local_store::LocalStore;
use crate::store::{Entry, EntryType, Store, SyncAction};
use crate::{api, auth, importer, local_store, models, store, touch_id, ui_password};

const RATE_LIMIT_DELAY: Duration = Duration::from_millis(700);

/// A desktop session. The CLI uses the stores directly; every desktop data
/// operation passes through this gate, independently of what the view renders.
pub struct Service {
    locked: bool,
}

#[derive(Debug, Clone)]
pub struct GroupView {
    pub name: String,
    pub entries: Vec<EntryView>,
}

impl Service {
    pub fn new() -> Result<Self> {
        // A corrupt existing configuration must never silently disable the gate.
        let config = if Config::path()?.exists() {
            Config::load()?
        } else {
            Config::empty()
        };
        Ok(Self {
            locked: config.ui_password_hash.is_some() || config.touch_id_enabled,
        })
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }
    pub fn lock(&mut self) {
        self.locked = true;
    }
    pub fn status(&self) -> Result<StatusInfo> {
        status()
    }

    fn ensure_unlocked(&self) -> Result<()> {
        if self.locked {
            bail!("界面已锁定，请先解锁 / Vault is locked");
        }
        Ok(())
    }

    pub fn unlock(&mut self, password: &str) -> Result<()> {
        let config = if Config::path()?.exists() {
            Config::load()?
        } else {
            Config::empty()
        };
        if let Some(hash) = config.ui_password_hash {
            if !ui_password::verify_password(&hash, password) {
                bail!("密码不正确 / Incorrect password");
            }
        } else if config.touch_id_enabled {
            bail!("请使用 Touch ID 解锁 / Use Touch ID to unlock");
        }
        self.locked = false;
        Ok(())
    }

    pub fn unlock_touch_id(&mut self) -> Result<()> {
        if !Config::load()?.touch_id_enabled {
            bail!("Touch ID 未启用");
        }
        touch_id::authenticate("解锁 ZeroApiKey")?;
        self.locked = false;
        Ok(())
    }

    pub fn catalog(&self, mode: &str) -> Result<Vec<GroupView>> {
        self.ensure_unlocked()?;
        let store = open_store(mode)?;
        store
            .snapshot()?
            .into_iter()
            .map(|(name, entries)| {
                let entries = entries.iter().map(entry_view).collect();
                Ok(GroupView { name, entries })
            })
            .collect()
    }

    pub fn reveal(&self, mode: &str, group: &str, key: &str) -> Result<String> {
        self.ensure_unlocked()?;
        reveal(mode, group, key)
    }
    pub fn save_entry(
        &self,
        mode: &str,
        group: &str,
        old_key: Option<&str>,
        input: EntryInput,
    ) -> Result<()> {
        self.ensure_unlocked()?;
        match old_key {
            Some(key) => rename(mode, group, key, input),
            None => add(mode, group, input),
        }
    }
    pub fn remove(&self, mode: &str, group: &str, key: Option<&str>) -> Result<()> {
        self.ensure_unlocked()?;
        open_store(mode)?.remove(group, key)
    }
    pub fn create_group(&self, mode: &str, group: &str) -> Result<()> {
        self.ensure_unlocked()?;
        create_group(mode, group)
    }
    pub fn export(&self, mode: &str, group: &str) -> Result<String> {
        self.ensure_unlocked()?;
        export(mode, group)
    }
    pub fn sync(&self, direction: &str) -> Result<SyncStats> {
        self.ensure_unlocked()?;
        do_sync(direction)
    }
    pub fn import_preview(&self, path: &str) -> Result<Vec<ImportPreviewItem>> {
        self.ensure_unlocked()?;
        import_parse(path)
    }
    pub fn import_commit(&self, mode: &str, entries: Vec<CommitEntry>) -> Result<u32> {
        self.ensure_unlocked()?;
        import_write(mode, entries)
    }
    pub fn configure_cloud(
        &self,
        api_base: &str,
        project: &str,
        environment: &str,
        id: &str,
        secret: &str,
    ) -> Result<()> {
        self.ensure_unlocked()?;
        cloud_init(api_base, project, environment, id, secret)
    }
    pub fn set_default(&self, mode: &str) -> Result<()> {
        self.ensure_unlocked()?;
        set_default(mode)
    }
    pub fn set_touch_id(&self, enabled: bool) -> Result<()> {
        self.ensure_unlocked()?;
        set_touch_id(enabled)
    }
    pub fn set_password(&self, current: Option<&str>, new: Option<&str>) -> Result<()> {
        self.ensure_unlocked()?;
        update_ui_password(current, new)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusInfo {
    pub default_mode: Option<String>,
    pub local_ready: bool,
    pub cloud_ready: bool,
    pub touch_id_enabled: bool,
    pub ui_password_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryView {
    pub key: String,
    pub masked_value: String,
    pub entry_type: String,
    pub env_name: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryInput {
    pub key: String,
    pub value: String,
    pub entry_type: String,
    pub env_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitEntry {
    pub group: String,
    pub key: String,
    pub value: String,
    pub entry_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStats {
    pub added: u32,
    pub updated: u32,
    pub skipped: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewItem {
    pub group: Option<String>,
    pub key: String,
    pub entry_type: String,
    pub masked_value: String,
    pub value: String,
}

fn parse_backend(mode: &str) -> Result<Backend> {
    match mode {
        "local" => Ok(Backend::Local),
        "cloud" => Ok(Backend::Cloud),
        _ => Err(anyhow!("未知后端：{mode}")),
    }
}

fn open_store(mode: &str) -> Result<Box<dyn Store>> {
    match parse_backend(mode)? {
        Backend::Local => Ok(Box::new(LocalStore::new()?)),
        Backend::Cloud => {
            let config = Config::load().context("cloud 后端未配置，请先在设置页初始化")?;
            Ok(Box::new(CloudStore::new(&config)?))
        }
    }
}

fn entry_view(entry: &Entry) -> EntryView {
    EntryView {
        key: entry.key.clone(),
        masked_value: models::mask(&entry.value),
        entry_type: entry.entry_type.label().to_string(),
        env_name: entry.env_name.clone(),
        comment: entry.comment.clone(),
    }
}

fn build_entry(
    key: &str,
    value: &str,
    entry_type: &str,
    env_name: Option<String>,
) -> Result<Entry> {
    let key = models::normalize_key(key).context("键名无效（需要包含字母或数字）")?;
    let mut entry = Entry::new(
        &key,
        value,
        EntryType::from_label(entry_type).unwrap_or(EntryType::Note),
    );
    entry.env_name = env_name.filter(|s| !s.trim().is_empty());
    Ok(entry)
}

// Infisical Cloud 免费档写限速 90 次/分钟，云端写入间隔约 700ms
fn throttle(store: &dyn Store) {
    if store.name() == "cloud" {
        sleep(RATE_LIMIT_DELAY);
    }
}

fn snapshot(store: &dyn Store) -> Result<BTreeMap<String, Vec<Entry>>> {
    store.snapshot()
}

fn status() -> Result<StatusInfo> {
    let config = Config::load().ok();
    let cloud_ready = config
        .as_ref()
        .map(|c| c.api_base.is_some() && c.project_id.is_some())
        .unwrap_or(false)
        && auth::client_credentials()?.is_some();
    Ok(StatusInfo {
        default_mode: config
            .as_ref()
            .and_then(|c| c.default_mode)
            .map(|b| b.label().to_string()),
        local_ready: local_store::is_configured(),
        cloud_ready,
        touch_id_enabled: config.as_ref().map(|c| c.touch_id_enabled).unwrap_or(false),
        ui_password_enabled: config
            .as_ref()
            .map(|c| c.ui_password_hash.is_some())
            .unwrap_or(false),
    })
}

fn set_default(mode: &str) -> Result<()> {
    let backend = parse_backend(mode)?;
    let mut config = Config::load().unwrap_or_else(|_| Config::empty());
    config.default_mode = Some(backend);
    config.save()
}

fn set_touch_id(enabled: bool) -> Result<()> {
    let mut config = Config::load().unwrap_or_else(|_| Config::empty());
    config.touch_id_enabled = enabled;
    config.save()
}

fn update_ui_password(current: Option<&str>, new: Option<&str>) -> Result<()> {
    let mut config = Config::load().unwrap_or_else(|_| Config::empty());
    if let Some(hash) = &config.ui_password_hash {
        let current = current.context("需要输入当前解锁密码")?;
        if !ui_password::verify_password(hash, current) {
            bail!("当前解锁密码不正确");
        }
    }
    config.ui_password_hash = new.map(ui_password::hash_password);
    config.save()
}

fn reveal(mode: &str, group: &str, key: &str) -> Result<String> {
    let store = open_store(mode)?;
    let key = models::normalize_key(key).context("键名无效")?;
    store
        .list_entries(group)?
        .into_iter()
        .find(|e| e.key == key)
        .map(|e| e.value)
        .with_context(|| format!("未找到 {group}/{key}"))
}

fn add(mode: &str, group: &str, input: EntryInput) -> Result<()> {
    let group = group.trim();
    if group.is_empty() {
        bail!("分组名不能为空");
    }
    let entry = build_entry(&input.key, &input.value, &input.entry_type, input.env_name)?;
    open_store(mode)?.upsert(group, &entry)
}

fn rename(mode: &str, group: &str, old_key: &str, input: EntryInput) -> Result<()> {
    let entry = build_entry(&input.key, &input.value, &input.entry_type, input.env_name)?;
    let old_key = models::normalize_key(old_key).context("旧键名无效")?;
    open_store(mode)?.rename(group, &old_key, &entry)
}

fn export(mode: &str, group: &str) -> Result<String> {
    let store = open_store(mode)?;
    let prefix = models::normalize_key(group).unwrap_or_else(|| "GROUP".to_string());
    let mut lines = Vec::new();
    for entry in store
        .list_entries(group)?
        .iter()
        .filter(|e| !e.value.is_empty())
    {
        let env_name = entry
            .env_name
            .clone()
            .unwrap_or_else(|| format!("{prefix}_{}", entry.key));
        lines.push(format!(
            "export {}='{}'",
            env_name,
            entry.value.replace('\'', "'\\''")
        ));
    }
    Ok(lines.join("\n"))
}

fn do_sync(direction: &str) -> Result<SyncStats> {
    let config = Config::load().context("cloud 后端未配置，请先在设置页初始化")?;
    let local = LocalStore::new()?;
    let cloud = CloudStore::new(&config)?;
    let (source, target): (&dyn Store, &dyn Store) = match direction {
        "local-to-cloud" => (&local, &cloud),
        "cloud-to-local" => (&cloud, &local),
        _ => bail!("未知同步方向：{direction}"),
    };
    let plan = store::plan(&snapshot(source)?, &snapshot(target)?);
    let mut stats = SyncStats {
        added: 0,
        updated: 0,
        skipped: plan.identical as u32,
    };
    for action in plan.actions {
        match action {
            SyncAction::Add { group, entry } => {
                target.upsert(&group, &entry)?;
                stats.added += 1;
                throttle(target);
            }
            SyncAction::Conflict { group, source, .. } => {
                target.upsert(&group, &source)?;
                stats.updated += 1;
                throttle(target);
            }
        }
    }
    Ok(stats)
}

fn import_parse(path: &str) -> Result<Vec<ImportPreviewItem>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("无法读取 {path}"))?;
    Ok(importer::parse(&text)
        .iter()
        .map(|p| ImportPreviewItem {
            group: p.group.clone(),
            key: p.key.clone(),
            entry_type: p.kind.label().to_string(),
            masked_value: models::mask(&p.value),
            value: p.value.clone(),
        })
        .collect())
}

fn import_write(mode: &str, entries: Vec<CommitEntry>) -> Result<u32> {
    let store = open_store(mode)?;
    let mut written = 0u32;
    for input in entries {
        let group = input.group.trim().to_string();
        if group.is_empty() {
            bail!("分组名不能为空");
        }
        let entry = build_entry(&input.key, &input.value, &input.entry_type, None)?;
        store.upsert(&group, &entry)?;
        written += 1;
        throttle(store.as_ref());
    }
    Ok(written)
}

fn cloud_init(
    api_base: &str,
    project_id: &str,
    environment: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<()> {
    let login = api::Client::new(api_base)
        .login(client_id, client_secret)
        .context("登录验证失败，请检查 api_base / client_id / client_secret")?;
    let mut config = Config::load().unwrap_or_else(|_| Config::empty());
    config.api_base = Some(api_base.trim_end_matches('/').to_string());
    config.project_id = Some(project_id.to_string());
    if !environment.trim().is_empty() {
        config.environment = environment.trim().to_string();
    }
    if config.default_mode.is_none() {
        config.default_mode = Some(Backend::Cloud);
    }
    auth::store_credentials(client_id, client_secret)?;
    auth::store_token(&login)?;
    config.save()
}

fn create_group(mode: &str, group: &str) -> Result<()> {
    let group = group.trim();
    if group.is_empty() {
        bail!("分组名不能为空");
    }
    match parse_backend(mode)? {
        Backend::Local => LocalStore::new()?.create_group(group),
        Backend::Cloud => {
            let config = Config::load().context("cloud 后端未配置，请先在设置页初始化")?;
            CloudStore::new(&config)?.create_group(group)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_session_refuses_all_data_operations_before_accessing_stores() {
        let session = Service { locked: true };
        let input = EntryInput {
            key: "K".into(),
            value: "secret".into(),
            entry_type: "note".into(),
            env_name: None,
        };
        let results = [
            session.catalog("invalid").map(|_| ()),
            session.reveal("invalid", "g", "k").map(|_| ()),
            session.save_entry("invalid", "g", None, input),
            session.remove("invalid", "g", None),
            session.create_group("invalid", "g"),
            session.export("invalid", "g").map(|_| ()),
            session.sync("invalid").map(|_| ()),
            session.import_preview("/does-not-exist").map(|_| ()),
            session.import_commit("invalid", vec![]).map(|_| ()),
            session.configure_cloud("invalid", "", "", "", ""),
            session.set_default("invalid"),
            session.set_touch_id(false),
            session.set_password(None, None),
        ];
        for result in results {
            assert!(result.unwrap_err().to_string().contains("locked"));
        }
    }

    #[test]
    fn explicit_lock_closes_the_session() {
        let mut session = Service { locked: false };
        session.lock();
        assert!(session.is_locked());
        assert!(session.ensure_unlocked().is_err());
    }
}
