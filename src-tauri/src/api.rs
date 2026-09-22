use anyhow::{Context, Result, anyhow, bail};
use reqwest::Method;
use reqwest::blocking::{Client as Http, Response};

use crate::auth;
use crate::config::Config;
use crate::models::{
    Folder, FolderCreateBody, FoldersResponse, LoginResponse, Secret, SecretDeleteBody,
    SecretMetadata, SecretUpsertBody, SecretsResponse,
};
use crate::store::{Entry, EntryType, Store};

pub struct Client {
    base: String,
    http: Http,
    token: Option<String>,
}

pub struct SecretInput<'a> {
    pub name: &'a str,
    pub value: &'a str,
    pub path: &'a str,
    pub comment: &'a str,
    pub metadata: &'a [SecretMetadata],
}

impl Client {
    pub fn new(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            http: Http::new(),
            token: None,
        }
    }

    pub fn with_token(base: &str, token: &str) -> Self {
        let mut client = Self::new(base);
        client.token = Some(token.to_string());
        client
    }

    pub fn login(&self, client_id: &str, client_secret: &str) -> Result<LoginResponse> {
        let resp = self
            .http
            .post(format!("{}/api/v1/auth/universal-auth/login", self.base))
            .json(&serde_json::json!({
                "clientId": client_id,
                "clientSecret": client_secret,
            }))
            .send()
            .context("登录请求发送失败")?;
        check(resp)?.json().context("登录响应格式错误")
    }

    pub fn list_secrets(&self, project_id: &str, environment: &str, path: &str) -> Result<Vec<Secret>> {
        let resp = self
            .request(Method::GET, format!("{}/api/v4/secrets", self.base))
            .query(&[
                ("workspaceId", project_id),
                ("environment", environment),
                ("secretPath", path),
                ("viewSecretValue", "true"),
            ])
            .send()
            .context("列出 secrets 请求失败")?;
        Ok(check(resp)?
            .json::<SecretsResponse>()
            .context("secrets 响应格式错误")?
            .secrets)
    }

    pub fn create_secret(&self, project_id: &str, environment: &str, input: &SecretInput<'_>) -> Result<()> {
        self.upsert_secret(Method::POST, project_id, environment, input)
    }

    pub fn update_secret(&self, project_id: &str, environment: &str, input: &SecretInput<'_>) -> Result<()> {
        self.upsert_secret(Method::PATCH, project_id, environment, input)
    }

    pub fn delete_secret(
        &self,
        project_id: &str,
        environment: &str,
        name: &str,
        path: &str,
    ) -> Result<()> {
        let body = SecretDeleteBody {
            project_id,
            environment,
            secret_path: path,
        };
        let resp = self
            .request(Method::DELETE, format!("{}/api/v4/secrets/{name}", self.base))
            .json(&body)
            .send()
            .context("删除 secret 请求失败")?;
        check(resp)?;
        Ok(())
    }

    pub fn list_folders(&self, project_id: &str, environment: &str, path: &str) -> Result<Vec<Folder>> {
        let resp = self
            .request(Method::GET, format!("{}/api/v1/folders", self.base))
            .query(&[
                ("workspaceId", project_id),
                ("environment", environment),
                ("path", path),
            ])
            .send()
            .context("列出 folders 请求失败")?;
        Ok(check(resp)?
            .json::<FoldersResponse>()
            .context("folders 响应格式错误")?
            .folders)
    }

    pub fn create_folder(&self, project_id: &str, environment: &str, name: &str) -> Result<()> {
        let body = FolderCreateBody {
            project_id,
            environment,
            name,
            path: "/",
        };
        let resp = self
            .request(Method::POST, format!("{}/api/v2/folders", self.base))
            .json(&body)
            .send()
            .context("创建 folder 请求失败")?;
        check(resp)?;
        Ok(())
    }

    pub fn ensure_folder(&self, project_id: &str, environment: &str, name: &str) -> Result<()> {
        if self
            .list_folders(project_id, environment, "/")?
            .iter()
            .any(|f| f.name == name)
        {
            return Ok(());
        }
        self.create_folder(project_id, environment, name)
    }

    pub fn delete_folder(
        &self,
        project_id: &str,
        environment: &str,
        path: &str,
        name: &str,
    ) -> Result<()> {
        let resp = self
            .request(Method::DELETE, format!("{}/api/v1/folders/{name}", self.base))
            .query(&[
                ("workspaceId", project_id),
                ("environment", environment),
                ("path", path),
            ])
            .send()
            .context("删除 folder 请求失败")?;
        check(resp)?;
        Ok(())
    }

    fn upsert_secret(
        &self,
        method: Method,
        project_id: &str,
        environment: &str,
        input: &SecretInput<'_>,
    ) -> Result<()> {
        let body = SecretUpsertBody {
            project_id,
            environment,
            secret_value: input.value,
            secret_path: input.path,
            secret_comment: input.comment,
            secret_metadata: input.metadata,
            kind: "shared",
            skip_multiline_encoding: true,
        };
        let resp = self
            .request(method, format!("{}/api/v4/secrets/{}", self.base, input.name))
            .json(&body)
            .send()
            .context("写入 secret 请求失败")?;
        check(resp)?;
        Ok(())
    }

    fn request(&self, method: Method, url: String) -> reqwest::blocking::RequestBuilder {
        let builder = self.http.request(method, url);
        match &self.token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }
}

fn check(resp: Response) -> Result<Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().unwrap_or_default();
    Err(anyhow!("Infisical API 返回 {}: {}", status, body))
}

pub struct CloudStore {
    project_id: String,
    environment: String,
    client: Client,
}

impl CloudStore {
    pub fn new(config: &Config) -> Result<Self> {
        let api_base = config
            .api_base
            .clone()
            .context("cloud 后端未配置，请先运行 zak init 并选择 cloud")?;
        let project_id = config
            .project_id
            .clone()
            .context("cloud 后端未配置，请先运行 zak init 并选择 cloud")?;
        let token = auth::access_token(&api_base)?;
        Ok(Self {
            project_id,
            environment: config.environment.clone(),
            client: Client::with_token(&api_base, &token),
        })
    }

    pub fn create_group(&self, group: &str) -> Result<()> {
        self.client
            .ensure_folder(&self.project_id, &self.environment, group)
    }
}

impl Store for CloudStore {
    fn name(&self) -> &'static str {
        "cloud"
    }

    fn list_groups(&self) -> Result<Vec<String>> {
        let mut names: Vec<String> = self
            .client
            .list_folders(&self.project_id, &self.environment, "/")?
            .into_iter()
            .map(|f| f.name)
            .collect();
        names.sort();
        Ok(names)
    }

    fn list_entries(&self, group: &str) -> Result<Vec<Entry>> {
        let path = format!("/{group}");
        let mut entries: Vec<Entry> = self
            .client
            .list_secrets(&self.project_id, &self.environment, &path)?
            .into_iter()
            .map(secret_to_entry)
            .collect();
        entries.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(entries)
    }

    fn upsert(&self, group: &str, entry: &Entry) -> Result<()> {
        self.client
            .ensure_folder(&self.project_id, &self.environment, group)?;
        let path = format!("/{group}");
        let metadata = entry_metadata(entry);
        let input = SecretInput {
            name: &entry.key,
            value: &entry.value,
            path: &path,
            comment: entry.comment.as_deref().unwrap_or(""),
            metadata: &metadata,
        };
        let exists = self
            .client
            .list_secrets(&self.project_id, &self.environment, &path)?
            .iter()
            .any(|s| s.secret_key == entry.key);
        if exists {
            self.client
                .update_secret(&self.project_id, &self.environment, &input)
        } else {
            self.client
                .create_secret(&self.project_id, &self.environment, &input)
        }
    }

    fn remove(&self, group: &str, key: Option<&str>) -> Result<()> {
        match key {
            Some(k) => {
                let path = format!("/{group}");
                self.client
                    .delete_secret(&self.project_id, &self.environment, k, &path)
            }
            None => {
                self.client
                    .delete_folder(&self.project_id, &self.environment, "/", group)
            }
        }
    }

    fn rename(&self, group: &str, old_key: &str, entry: &Entry) -> Result<()> {
        if old_key == entry.key {
            return self.upsert(group, entry);
        }
        let path = format!("/{group}");
        let secrets = self
            .client
            .list_secrets(&self.project_id, &self.environment, &path)?;
        if !secrets.iter().any(|s| s.secret_key == old_key) {
            bail!("未找到 {group}/{old_key}");
        }
        if secrets.iter().any(|s| s.secret_key == entry.key) {
            bail!("{group}/{} 已存在，无法重命名为该键名", entry.key);
        }
        let metadata = entry_metadata(entry);
        let input = SecretInput {
            name: &entry.key,
            value: &entry.value,
            path: &path,
            comment: entry.comment.as_deref().unwrap_or(""),
            metadata: &metadata,
        };
        // 云端无法原子重命名：先建新键，再删旧键
        self.client
            .create_secret(&self.project_id, &self.environment, &input)?;
        if let Err(e) = self
            .client
            .delete_secret(&self.project_id, &self.environment, old_key, &path)
        {
            bail!(
                "新键 {group}/{} 已写入，但删除旧键 {old_key} 失败：{e:#}；新旧键当前并存，请手动删除旧键",
                entry.key
            );
        }
        Ok(())
    }
}

fn secret_to_entry(secret: Secret) -> Entry {
    Entry {
        entry_type: secret
            .metadata_value("type")
            .and_then(EntryType::from_label)
            .unwrap_or(EntryType::Note),
        env_name: secret.metadata_value("env_name").map(str::to_string),
        comment: if secret.secret_comment.is_empty() {
            None
        } else {
            Some(secret.secret_comment.clone())
        },
        key: secret.secret_key,
        value: secret.secret_value,
    }
}

fn entry_metadata(entry: &Entry) -> Vec<SecretMetadata> {
    let mut metadata = vec![SecretMetadata::new("type", entry.entry_type.label())];
    if let Some(env_name) = &entry.env_name {
        metadata.push(SecretMetadata::new("env_name", env_name));
    }
    metadata
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SecretMetadata;

    #[test]
    fn secret_to_entry_restores_metadata() {
        let secret = Secret {
            secret_key: "API_KEY".into(),
            secret_value: "sk-x".into(),
            secret_comment: "c".into(),
            secret_metadata: vec![
                SecretMetadata::new("type", "api-key"),
                SecretMetadata::new("env_name", "OPENAI_API_KEY"),
            ],
            secret_path: None,
        };
        let entry = secret_to_entry(secret);
        assert_eq!(entry.entry_type, EntryType::ApiKey);
        assert_eq!(entry.env_name.as_deref(), Some("OPENAI_API_KEY"));
        assert_eq!(entry.comment.as_deref(), Some("c"));
    }

    #[test]
    fn secret_to_entry_defaults_without_metadata() {
        let secret = Secret {
            secret_key: "NOTE".into(),
            secret_value: "v".into(),
            secret_comment: String::new(),
            secret_metadata: vec![],
            secret_path: None,
        };
        let entry = secret_to_entry(secret);
        assert_eq!(entry.entry_type, EntryType::Note);
        assert_eq!(entry.env_name, None);
        assert_eq!(entry.comment, None);
    }

    #[test]
    fn entry_metadata_writes_type_and_env_name() {
        let mut entry = Entry::new("API_KEY", "sk-x", EntryType::Login);
        entry.env_name = Some("GH_TOKEN".into());
        let metadata = entry_metadata(&entry);
        assert_eq!(metadata.len(), 2);
        assert_eq!(metadata[0].key, "type");
        assert_eq!(metadata[0].value, "login");
        assert_eq!(metadata[1].key, "env_name");
        assert_eq!(metadata[1].value, "GH_TOKEN");
    }
}
