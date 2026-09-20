use std::collections::BTreeMap;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use dialoguer::{Confirm, Input, MultiSelect, Password, Select};

use crate::api::CloudStore;
use crate::auth;
use crate::config::{Backend, Config};
use crate::importer;
use crate::local_store::LocalStore;
use crate::models::{mask, normalize_key};
use crate::store::{self, Entry, EntryType, Store, SyncAction};
use crate::Command;

const DEFAULT_API_BASE: &str = "https://app.infisical.com";
const RATE_LIMIT_DELAY: Duration = Duration::from_millis(700);

pub fn run(command: Command, local: bool, cloud: bool) -> Result<()> {
    match command {
        Command::Init => init(),
        Command::Sync { to, force } => sync(to.as_deref(), force),
        Command::Add { group } => {
            let store = resolve_store(local, cloud)?;
            add(store.as_ref(), &group)
        }
        Command::Get { group, key, reveal } => {
            let store = resolve_store(local, cloud)?;
            get(store.as_ref(), &group, key.as_deref(), reveal)
        }
        Command::List { group } => {
            let store = resolve_store(local, cloud)?;
            list(store.as_ref(), group.as_deref())
        }
        Command::Rm { group, key } => {
            let store = resolve_store(local, cloud)?;
            rm(store.as_ref(), &group, key.as_deref())
        }
        Command::Export { group } => {
            let store = resolve_store(local, cloud)?;
            export(store.as_ref(), &group)
        }
        Command::Import { file } => {
            let store = resolve_store(local, cloud)?;
            import(store.as_ref(), &file)
        }
    }
}

fn resolve_store(local: bool, cloud: bool) -> Result<Box<dyn Store>> {
    let config = Config::load().ok();
    let backend = if local {
        Backend::Local
    } else if cloud {
        Backend::Cloud
    } else {
        config
            .as_ref()
            .and_then(|c| c.default_mode)
            .unwrap_or(Backend::Cloud)
    };
    match backend {
        Backend::Local => Ok(Box::new(LocalStore::new()?)),
        Backend::Cloud => {
            let config = config.context("cloud 后端未配置，请先运行 zak init 并选择 cloud")?;
            Ok(Box::new(CloudStore::new(&config)?))
        }
    }
}

// Infisical Cloud 免费档写限速 90 次/分钟，云端写入间隔约 700ms
fn throttle(store: &dyn Store) {
    if store.name() == "cloud" {
        sleep(RATE_LIMIT_DELAY);
    }
}

fn init() -> Result<()> {
    let options = ["local（本地加密保险库）", "cloud（Infisical Cloud）"];
    let picked = MultiSelect::new()
        .with_prompt("配置哪些后端（空格选择，回车确认）")
        .items(&options)
        .interact()?;
    if picked.is_empty() {
        bail!("至少选择一个后端");
    }
    let want_local = picked.contains(&0);
    let want_cloud = picked.contains(&1);

    let mut config = Config::load().unwrap_or_else(|_| Config::empty());
    if want_local {
        LocalStore::new()?;
        println!("local 后端就绪（主密钥已生成并存入钥匙串）");
    }
    if want_cloud {
        let api_base: String = Input::new()
            .with_prompt("API base")
            .default(DEFAULT_API_BASE.to_string())
            .interact_text()?;
        let project_id: String = Input::new().with_prompt("Project ID").interact_text()?;
        let environment: String = Input::new()
            .with_prompt("Environment")
            .default("dev".to_string())
            .interact_text()?;
        let client_id: String = Input::new().with_prompt("Client ID").interact_text()?;
        let client_secret: String = Password::new().with_prompt("Client Secret").interact()?;

        let login = crate::api::Client::new(&api_base)
            .login(&client_id, &client_secret)
            .context("登录验证失败，请检查 api_base / client_id / client_secret")?;
        config.api_base = Some(api_base);
        config.project_id = Some(project_id);
        config.environment = environment;
        auth::store_credentials(&client_id, &client_secret)?;
        auth::store_token(&login)?;
        println!("cloud 后端就绪（凭证已入钥匙串）");
    }
    config.default_mode = match (want_local, want_cloud) {
        (true, false) => Some(Backend::Local),
        (false, true) => Some(Backend::Cloud),
        _ => {
            let defaults = ["local", "cloud"];
            let sel = Select::new()
                .with_prompt("默认使用哪个后端")
                .items(&defaults)
                .default(0)
                .interact()?;
            Some(if sel == 0 { Backend::Local } else { Backend::Cloud })
        }
    };
    config.save()?;
    println!(
        "初始化完成，默认后端 {}，配置写入 {}",
        config.default_mode.map(|b| b.label()).unwrap_or("-"),
        Config::path()?.display()
    );
    Ok(())
}

fn put(store: &dyn Store, group: &str, entry: &Entry) -> Result<()> {
    let exists = store
        .list_entries(group)?
        .iter()
        .any(|e| e.key == entry.key);
    if exists {
        let overwrite = Confirm::new()
            .with_prompt(format!("{group}/{} 已存在，覆盖？", entry.key))
            .default(false)
            .interact()?;
        if !overwrite {
            println!("跳过 {}", entry.key);
            return Ok(());
        }
    }
    store.upsert(group, entry)?;
    println!("已写入 {group}/{}", entry.key);
    Ok(())
}

fn add(store: &dyn Store, group: &str) -> Result<()> {
    println!("后端：{}", store.name());
    let kinds = ["api-key", "login", "note"];
    match Select::new()
        .with_prompt("条目类型")
        .items(&kinds)
        .default(0)
        .interact()?
    {
        0 => {
            let value: String = Input::new().with_prompt("API_KEY 值").interact_text()?;
            let env_name: String = Input::new()
                .with_prompt("环境变量名 env_name（可空）")
                .allow_empty(true)
                .interact_text()?;
            let mut entry = Entry::new("API_KEY", &value, EntryType::ApiKey);
            entry.env_name = normalize_key(&env_name);
            put(store, group, &entry)?;
            let base_url: String = Input::new()
                .with_prompt("BASE_URL（可空）")
                .allow_empty(true)
                .interact_text()?;
            if !base_url.is_empty() {
                put(store, group, &Entry::new("BASE_URL", &base_url, EntryType::ApiKey))?;
            }
        }
        1 => {
            let username: String = Input::new().with_prompt("USERNAME").interact_text()?;
            let password: String = Password::new().with_prompt("PASSWORD").interact()?;
            put(store, group, &Entry::new("USERNAME", &username, EntryType::Login))?;
            put(store, group, &Entry::new("PASSWORD", &password, EntryType::Login))?;
            let url: String = Input::new()
                .with_prompt("URL（可空）")
                .allow_empty(true)
                .interact_text()?;
            if !url.is_empty() {
                put(store, group, &Entry::new("BASE_URL", &url, EntryType::Login))?;
            }
        }
        _ => {
            let key_name: String = Input::new().with_prompt("键名").interact_text()?;
            let name = normalize_key(&key_name).context("键名无效（需要包含字母或数字）")?;
            let value: String = Input::new().with_prompt("值").interact_text()?;
            put(store, group, &Entry::new(&name, &value, EntryType::Note))?;
        }
    }
    Ok(())
}

fn get(store: &dyn Store, group: &str, key: Option<&str>, reveal: bool) -> Result<()> {
    let filter = match key {
        Some(k) => Some(normalize_key(k).context("键名无效（需要包含字母或数字）")?),
        None => None,
    };
    let entries = store.list_entries(group)?;
    let mut found = false;
    for entry in &entries {
        if let Some(f) = &filter
            && &entry.key != f
        {
            continue;
        }
        found = true;
        let shown = if reveal {
            entry.value.clone()
        } else {
            mask(&entry.value)
        };
        println!("{} = {}", entry.key, shown);
    }
    if !found {
        match filter {
            Some(f) => bail!("未找到 {group}/{f}（后端 {}）", store.name()),
            None => println!("{group} 下没有条目（后端 {}）", store.name()),
        }
    }
    Ok(())
}

fn list(store: &dyn Store, group: Option<&str>) -> Result<()> {
    match group {
        None => {
            let groups = store.list_groups()?;
            if groups.is_empty() {
                println!("（无分组，后端 {}）", store.name());
            }
            for g in groups {
                println!("{g}");
            }
        }
        Some(g) => {
            let entries = store.list_entries(g)?;
            if entries.is_empty() {
                println!("{g} 下没有条目（后端 {}）", store.name());
            }
            for entry in entries {
                println!("{:<20} {}", entry.key, entry.entry_type.label());
            }
        }
    }
    Ok(())
}

fn rm(store: &dyn Store, group: &str, key: Option<&str>) -> Result<()> {
    match key {
        Some(k) => {
            let name = normalize_key(k).context("键名无效（需要包含字母或数字）")?;
            let confirmed = Confirm::new()
                .with_prompt(format!("删除 {group}/{name}（后端 {}）？", store.name()))
                .default(false)
                .interact()?;
            if !confirmed {
                return Ok(());
            }
            store.remove(group, Some(&name))?;
            println!("已删除 {group}/{name}");
        }
        None => {
            let confirmed = Confirm::new()
                .with_prompt(format!(
                    "删除整个分组 {group}（含全部条目，后端 {}）？",
                    store.name()
                ))
                .default(false)
                .interact()?;
            if !confirmed {
                return Ok(());
            }
            store.remove(group, None)?;
            println!("已删除分组 {group}");
        }
    }
    Ok(())
}

fn export(store: &dyn Store, group: &str) -> Result<()> {
    let prefix = normalize_key(group).unwrap_or_else(|| "GROUP".to_string());
    for entry in store
        .list_entries(group)?
        .iter()
        .filter(|e| !e.value.is_empty())
    {
        let env_name = entry
            .env_name
            .clone()
            .unwrap_or_else(|| format!("{prefix}_{}", entry.key));
        println!("export {}='{}'", env_name, entry.value.replace('\'', "'\\''"));
    }
    Ok(())
}

fn import(store: &dyn Store, file: &Path) -> Result<()> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("无法读取 {}", file.display()))?;
    let entries = importer::parse(&text);
    if entries.is_empty() {
        println!("未解析到任何条目");
        return Ok(());
    }
    let total = entries.len();
    println!("解析到 {total} 条，写入后端 {}，逐条确认：", store.name());
    let mut uploaded = 0u32;
    for (i, parsed) in entries.into_iter().enumerate() {
        let group_label = parsed.group.clone().unwrap_or_else(|| "(未分组)".to_string());
        println!(
            "\n[{}/{}] 分组 {} | 类型 {} | 键 {} | 值 {}",
            i + 1,
            total,
            group_label,
            parsed.kind.label(),
            parsed.key,
            mask(&parsed.value)
        );
        let actions = ["上传", "修改键名后上传", "跳过"];
        let choice = Select::new()
            .with_prompt("操作")
            .items(&actions)
            .default(0)
            .interact()?;
        if choice == 2 {
            continue;
        }
        let mut key = parsed.key.clone();
        if choice == 1 {
            let input: String = Input::new()
                .with_prompt("新键名")
                .with_initial_text(&key)
                .interact_text()?;
            key = normalize_key(&input).context("键名无效（需要包含字母或数字）")?;
        }
        let group = match &parsed.group {
            Some(g) => g.clone(),
            None => Input::new()
                .with_prompt("分组名")
                .default("misc".to_string())
                .interact_text()?,
        };
        store.upsert(&group, &Entry::new(&key, &parsed.value, parsed.kind))?;
        uploaded += 1;
        throttle(store);
    }
    println!("完成，上传 {uploaded} 条");
    Ok(())
}

fn snapshot(store: &dyn Store) -> Result<BTreeMap<String, Vec<Entry>>> {
    let mut map = BTreeMap::new();
    for group in store.list_groups()? {
        map.insert(group.clone(), store.list_entries(&group)?);
    }
    Ok(map)
}

fn sync(to: Option<&str>, force: bool) -> Result<()> {
    let config = Config::load().context("cloud 后端未配置，请先运行 zak init 并选择 cloud")?;
    let local = LocalStore::new()?;
    let cloud = CloudStore::new(&config)?;
    let local_to_cloud = match to {
        Some("cloud") => true,
        Some("local") => false,
        _ => {
            let directions = ["local → cloud", "cloud → local"];
            Select::new()
                .with_prompt("同步方向")
                .items(&directions)
                .default(0)
                .interact()?
                == 0
        }
    };
    let (source, target): (&dyn Store, &dyn Store) = if local_to_cloud {
        (&local, &cloud)
    } else {
        (&cloud, &local)
    };
    let source_data = snapshot(source)?;
    let target_data = snapshot(target)?;
    let plan = store::plan(&source_data, &target_data);
    println!(
        "方向 {} → {}：新增/冲突 {} 条，已一致 {} 条",
        source.name(),
        target.name(),
        plan.actions.len(),
        plan.identical
    );
    if plan.actions.is_empty() {
        return Ok(());
    }
    let (mut added, mut updated, mut skipped) = (0u32, 0u32, plan.identical as u32);
    for action in plan.actions {
        match action {
            SyncAction::Add { group, entry } => {
                println!("新增 {group}/{}", entry.key);
                target.upsert(&group, &entry)?;
                added += 1;
                throttle(target);
            }
            SyncAction::Conflict {
                group,
                source: src,
                target: dst,
            } => {
                if force {
                    target.upsert(&group, &src)?;
                    updated += 1;
                    throttle(target);
                    continue;
                }
                println!(
                    "冲突 {group}/{}：源 {} ↔ 目标 {}",
                    src.key,
                    mask(&src.value),
                    mask(&dst.value)
                );
                let options = ["保留源（覆盖目标）", "保留目标", "跳过"];
                match Select::new()
                    .with_prompt("处理方式")
                    .items(&options)
                    .default(1)
                    .interact()?
                {
                    0 => {
                        target.upsert(&group, &src)?;
                        updated += 1;
                        throttle(target);
                    }
                    _ => skipped += 1,
                }
            }
        }
    }
    println!("同步完成：新增 {added}，更新 {updated}，跳过 {skipped}");
    Ok(())
}
