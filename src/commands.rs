use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use dialoguer::{Confirm, Input, Password, Select};

use crate::api::{Client, SecretInput};
use crate::auth;
use crate::config::Config;
use crate::importer::{self, EntryType};
use crate::models::{SecretMetadata, mask, normalize_key};
use crate::Command;

const DEFAULT_API_BASE: &str = "https://app.infisical.com";
const RATE_LIMIT_DELAY: Duration = Duration::from_millis(700);

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Init => init(),
        Command::Add { group } => add(&group),
        Command::Get { group, key, reveal } => get(&group, key.as_deref(), reveal),
        Command::List { group } => list(group.as_deref()),
        Command::Rm { group, key } => rm(&group, key.as_deref()),
        Command::Export { group } => export(&group),
        Command::Import { file } => import(&file),
    }
}

fn authed() -> Result<(Config, Client)> {
    let config = Config::load()?;
    let token = auth::access_token(&config)?;
    Ok((config.clone(), Client::with_token(&config.api_base, &token)))
}

fn init() -> Result<()> {
    let api_base: String = Input::new().with_prompt("API base")
        .default(DEFAULT_API_BASE.to_string())
        .interact_text()?;
    let project_id: String = Input::new().with_prompt("Project ID").interact_text()?;
    let environment: String = Input::new().with_prompt("Environment")
        .default("dev".to_string())
        .interact_text()?;
    let client_id: String = Input::new().with_prompt("Client ID").interact_text()?;
    let client_secret: String = Password::new().with_prompt("Client Secret").interact()?;

    let config = Config {
        api_base,
        project_id,
        environment,
    };
    let login = Client::new(&config.api_base)
        .login(&client_id, &client_secret)
        .context("登录验证失败，请检查 api_base / client_id / client_secret")?;
    config.save()?;
    auth::store_credentials(&client_id, &client_secret)?;
    auth::store_token(&login)?;
    println!("初始化完成，配置写入 {}", Config::path()?.display());
    Ok(())
}

fn upsert(
    config: &Config,
    client: &Client,
    path: &str,
    name: &str,
    value: &str,
    entry_type: &str,
    extra: Vec<(String, String)>,
) -> Result<()> {
    let mut metadata = vec![SecretMetadata::new("type", entry_type)];
    metadata.extend(extra.iter().map(|(k, v)| SecretMetadata::new(k, v)));
    let input = SecretInput {
        name,
        value,
        path,
        metadata: &metadata,
    };
    let exists = client
        .list_secrets(&config.project_id, &config.environment, path)?
        .iter()
        .any(|s| s.secret_key == name);
    if exists {
        let overwrite = Confirm::new().with_prompt(format!("{path}/{name} 已存在，覆盖？"))
            .default(false)
            .interact()?;
        if !overwrite {
            println!("跳过 {name}");
            return Ok(());
        }
        client.update_secret(&config.project_id, &config.environment, &input)?;
    } else {
        client.create_secret(&config.project_id, &config.environment, &input)?;
    }
    println!("已写入 {path}/{name}");
    Ok(())
}

fn add(group: &str) -> Result<()> {
    let (config, client) = authed()?;
    client.ensure_folder(&config.project_id, &config.environment, group)?;
    let path = format!("/{group}");
    let kinds = ["api-key", "login", "note"];
    match Select::new().with_prompt("条目类型").items(&kinds).default(0).interact()? {
        0 => {
            let value: String = Input::new().with_prompt("API_KEY 值").interact_text()?;
            let env_name: String = Input::new().with_prompt("环境变量名 env_name（可空）")
                .allow_empty(true)
                .interact_text()?;
            let extra = normalize_key(&env_name)
                .map(|n| vec![("env_name".to_string(), n)])
                .unwrap_or_default();
            upsert(&config, &client, &path, "API_KEY", &value, "api-key", extra)?;
            let base_url: String = Input::new().with_prompt("BASE_URL（可空）")
                .allow_empty(true)
                .interact_text()?;
            if !base_url.is_empty() {
                upsert(&config, &client, &path, "BASE_URL", &base_url, "api-key", vec![])?;
            }
        }
        1 => {
            let username: String = Input::new().with_prompt("USERNAME").interact_text()?;
            let password: String = Password::new().with_prompt("PASSWORD").interact()?;
            upsert(&config, &client, &path, "USERNAME", &username, "login", vec![])?;
            upsert(&config, &client, &path, "PASSWORD", &password, "login", vec![])?;
            let url: String = Input::new().with_prompt("URL（可空）")
                .allow_empty(true)
                .interact_text()?;
            if !url.is_empty() {
                upsert(&config, &client, &path, "BASE_URL", &url, "login", vec![])?;
            }
        }
        _ => {
            let key_name: String = Input::new().with_prompt("键名").interact_text()?;
            let name = normalize_key(&key_name).context("键名无效（需要包含字母或数字）")?;
            let value: String = Input::new().with_prompt("值").interact_text()?;
            upsert(&config, &client, &path, &name, &value, "note", vec![])?;
        }
    }
    Ok(())
}

fn get(group: &str, key: Option<&str>, reveal: bool) -> Result<()> {
    let (config, client) = authed()?;
    let path = format!("/{group}");
    let filter = match key {
        Some(k) => Some(normalize_key(k).context("键名无效（需要包含字母或数字）")?),
        None => None,
    };
    let secrets = client.list_secrets(&config.project_id, &config.environment, &path)?;
    let mut found = false;
    for s in &secrets {
        if let Some(f) = &filter
            && &s.secret_key != f
        {
            continue;
        }
        found = true;
        let shown = if reveal {
            s.secret_value.clone()
        } else {
            mask(&s.secret_value)
        };
        println!("{} = {}", s.secret_key, shown);
    }
    if !found {
        match filter {
            Some(f) => bail!("未找到 {path}/{f}"),
            None => println!("{path} 下没有条目"),
        }
    }
    Ok(())
}

fn list(group: Option<&str>) -> Result<()> {
    let (config, client) = authed()?;
    match group {
        None => {
            let folders = client.list_folders(&config.project_id, &config.environment, "/")?;
            if folders.is_empty() {
                println!("（无分组）");
            }
            for f in folders {
                println!("{}", f.name);
            }
        }
        Some(g) => {
            let path = format!("/{g}");
            let secrets = client.list_secrets(&config.project_id, &config.environment, &path)?;
            if secrets.is_empty() {
                println!("{path} 下没有条目");
            }
            for s in secrets {
                let entry_type = s.metadata_value("type").unwrap_or("-");
                println!("{:<20} {}", s.secret_key, entry_type);
            }
        }
    }
    Ok(())
}

fn rm(group: &str, key: Option<&str>) -> Result<()> {
    let (config, client) = authed()?;
    let path = format!("/{group}");
    match key {
        Some(k) => {
            let name = normalize_key(k).context("键名无效（需要包含字母或数字）")?;
            let confirmed = Confirm::new().with_prompt(format!("删除 {path}/{name}？"))
                .default(false)
                .interact()?;
            if !confirmed {
                return Ok(());
            }
            client.delete_secret(&config.project_id, &config.environment, &name, &path)?;
            println!("已删除 {path}/{name}");
        }
        None => {
            let confirmed = Confirm::new().with_prompt(format!("删除整个分组 {path}（含全部条目）？"))
                .default(false)
                .interact()?;
            if !confirmed {
                return Ok(());
            }
            client.delete_folder(&config.project_id, &config.environment, "/", group)?;
            println!("已删除分组 {group}");
        }
    }
    Ok(())
}

fn export(group: &str) -> Result<()> {
    let (config, client) = authed()?;
    let path = format!("/{group}");
    let prefix = normalize_key(group).unwrap_or_else(|| "GROUP".to_string());
    let secrets = client.list_secrets(&config.project_id, &config.environment, &path)?;
    for s in secrets.iter().filter(|s| !s.secret_value.is_empty()) {
        let env_name = s
            .metadata_value("env_name")
            .map(str::to_string)
            .unwrap_or_else(|| format!("{prefix}_{}", s.secret_key));
        println!("export {}='{}'", env_name, s.secret_value.replace('\'', "'\\''"));
    }
    Ok(())
}

fn import(file: &Path) -> Result<()> {
    let text = std::fs::read_to_string(file)
        .with_context(|| format!("无法读取 {}", file.display()))?;
    let entries = importer::parse(&text);
    if entries.is_empty() {
        println!("未解析到任何条目");
        return Ok(());
    }
    let (config, client) = authed()?;
    let total = entries.len();
    println!("解析到 {total} 条，逐条确认：");
    let mut uploaded = 0u32;
    for (i, entry) in entries.into_iter().enumerate() {
        let group_label = entry.group.clone().unwrap_or_else(|| "(未分组)".to_string());
        println!(
            "\n[{}/{}] 分组 {} | 类型 {} | 键 {} | 值 {}",
            i + 1,
            total,
            group_label,
            entry.kind.label(),
            entry.key,
            mask(&entry.value)
        );
        let actions = ["上传", "修改键名后上传", "跳过"];
        let choice = Select::new().with_prompt("操作").items(&actions).default(0).interact()?;
        if choice == 2 {
            continue;
        }
        let mut key = entry.key.clone();
        if choice == 1 {
            let input: String = Input::new().with_prompt("新键名")
                .with_initial_text(&key)
                .interact_text()?;
            key = normalize_key(&input).context("键名无效（需要包含字母或数字）")?;
        }
        let group = match &entry.group {
            Some(g) => g.clone(),
            None => Input::new().with_prompt("分组名")
                .default("misc".to_string())
                .interact_text()?,
        };
        client.ensure_folder(&config.project_id, &config.environment, &group)?;
        let path = format!("/{group}");
        let entry_type = match entry.kind {
            EntryType::ApiKey => "api-key",
            EntryType::Login => "login",
            EntryType::Note => "note",
        };
        upsert(&config, &client, &path, &key, &entry.value, entry_type, vec![])?;
        uploaded += 1;
        // Infisical Cloud 免费档写限速 90 次/分钟，上传间隔约 700ms 避免触发限流
        sleep(RATE_LIMIT_DELAY);
    }
    println!("完成，上传 {uploaded} 条");
    Ok(())
}
