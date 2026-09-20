use anyhow::{Context, Result, anyhow};
use reqwest::Method;
use reqwest::blocking::{Client as Http, Response};

use crate::models::{
    Folder, FolderCreateBody, FoldersResponse, LoginResponse, Secret, SecretDeleteBody,
    SecretMetadata, SecretUpsertBody, SecretsResponse,
};

pub struct Client {
    base: String,
    http: Http,
    token: Option<String>,
}

pub struct SecretInput<'a> {
    pub name: &'a str,
    pub value: &'a str,
    pub path: &'a str,
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
            secret_comment: "",
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
