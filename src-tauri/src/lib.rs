pub mod api;
pub mod auth;
pub mod cli;
pub mod commands;
pub mod config;
pub mod importer;
pub mod local_store;
pub mod models;
pub mod store;

use std::collections::BTreeMap;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use api::CloudStore;
use config::{Backend, Config};
use local_store::LocalStore;
use store::{Entry, EntryType, Store, SyncAction};

const RATE_LIMIT_DELAY: Duration = Duration::from_millis(700);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusInfo {
    pub default_mode: Option<String>,
    pub local_ready: bool,
    pub cloud_ready: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryView {
    pub key: String,
    pub masked_value: String,
    pub entry_type: String,
    pub env_name: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryInput {
    pub key: String,
    pub value: String,
    pub entry_type: String,
    pub env_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitEntry {
    pub group: String,
    pub key: String,
    pub value: String,
    pub entry_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStats {
    pub added: u32,
    pub updated: u32,
    pub skipped: u32,
}

#[derive(Debug, Serialize)]
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

fn build_entry(key: &str, value: &str, entry_type: &str, env_name: Option<String>) -> Result<Entry> {
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
    let mut map = BTreeMap::new();
    for group in store.list_groups()? {
        map.insert(group.clone(), store.list_entries(&group)?);
    }
    Ok(map)
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
            .and_then(|c| c.default_mode)
            .map(|b| b.label().to_string()),
        local_ready: local_store::is_configured(),
        cloud_ready,
    })
}

fn set_default(mode: &str) -> Result<()> {
    let backend = parse_backend(mode)?;
    let mut config = Config::load().unwrap_or_else(|_| Config::empty());
    config.default_mode = Some(backend);
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

#[tauri::command]
async fn get_status() -> Result<StatusInfo, String> {
    tauri::async_runtime::spawn_blocking(status)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
async fn set_default_mode(mode: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || set_default(&mode))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
async fn list_groups(mode: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || open_store(&mode)?.list_groups())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
async fn list_entries(mode: String, group: String) -> Result<Vec<EntryView>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(open_store(&mode)?
            .list_entries(&group)?
            .iter()
            .map(entry_view)
            .collect::<Vec<_>>())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
async fn reveal_entry(mode: String, group: String, key: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || reveal(&mode, &group, &key))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
async fn add_entry(mode: String, group: String, entry: EntryInput) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || add(&mode, &group, entry))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
async fn remove_entry(mode: String, group: String, key: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let key = models::normalize_key(&key).context("键名无效")?;
        open_store(&mode)?.remove(&group, Some(&key))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
async fn remove_group(mode: String, group: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || open_store(&mode)?.remove(&group, None))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
async fn export_group(mode: String, group: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || export(&mode, &group))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
async fn sync(direction: String, force: Option<bool>) -> Result<SyncStats, String> {
    let _ = force;
    tauri::async_runtime::spawn_blocking(move || do_sync(&direction))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
async fn import_ini(mode: String, path: String) -> Result<Vec<ImportPreviewItem>, String> {
    let _ = mode;
    tauri::async_runtime::spawn_blocking(move || import_parse(&path))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
async fn import_commit(mode: String, entries: Vec<CommitEntry>) -> Result<u32, String> {
    tauri::async_runtime::spawn_blocking(move || import_write(&mode, entries))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
async fn init_cloud(
    api_base: String,
    project_id: String,
    environment: String,
    client_id: String,
    client_secret: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        cloud_init(&api_base, &project_id, &environment, &client_id, &client_secret)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
async fn new_group(mode: String, group: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || create_group(&mode, &group))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e: anyhow::Error| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_status,
            set_default_mode,
            list_groups,
            list_entries,
            reveal_entry,
            add_entry,
            remove_entry,
            remove_group,
            export_group,
            sync,
            import_ini,
            import_commit,
            init_cloud,
            new_group,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
