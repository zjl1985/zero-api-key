use anyhow::{Context, Result};
use keyring::Entry;

use crate::api::Client;
use crate::models::{CachedToken, LoginResponse};

pub(crate) const KEYRING_SERVICE: &str = "zero-api-key";
const CLIENT_ID: &str = "client_id";
const CLIENT_SECRET: &str = "client_secret";
const ACCESS_TOKEN: &str = "access_token";

const EXPIRY_MARGIN_SECS: u64 = 60;

fn entry(name: &str) -> Result<Entry> {
    Entry::new(KEYRING_SERVICE, name).context("无法访问系统钥匙串")
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn store_credentials(client_id: &str, client_secret: &str) -> Result<()> {
    entry(CLIENT_ID)?.set_password(client_id)?;
    entry(CLIENT_SECRET)?.set_password(client_secret)?;
    Ok(())
}

pub fn client_credentials() -> Result<Option<(String, String)>> {
    let id = match entry(CLIENT_ID)?.get_password() {
        Ok(v) => v,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let secret = match entry(CLIENT_SECRET)?.get_password() {
        Ok(v) => v,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    Ok(Some((id, secret)))
}

pub fn store_token(login: &LoginResponse) -> Result<()> {
    let cached = CachedToken {
        token: login.access_token.clone(),
        expires_at: now().saturating_add(login.expires_in.saturating_sub(EXPIRY_MARGIN_SECS)),
    };
    entry(ACCESS_TOKEN)?.set_password(&serde_json::to_string(&cached)?)?;
    Ok(())
}

fn cached_token() -> Result<Option<String>> {
    let raw = match entry(ACCESS_TOKEN)?.get_password() {
        Ok(v) => v,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let cached: CachedToken = match serde_json::from_str(&raw) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    Ok((cached.expires_at > now()).then_some(cached.token))
}

pub fn access_token(api_base: &str) -> Result<String> {
    if let Some(token) = cached_token()? {
        return Ok(token);
    }
    let (client_id, client_secret) =
        client_credentials()?.context("钥匙串中没有凭证，请先运行 zak init")?;
    let login = Client::new(api_base)
        .login(&client_id, &client_secret)
        .context("token 已过期且重新登录失败")?;
    store_token(&login)?;
    Ok(login.access_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_token_json_roundtrip() {
        let cached = CachedToken {
            token: "tok".into(),
            expires_at: 1_900_000_000,
        };
        let parsed: CachedToken = serde_json::from_str(&serde_json::to_string(&cached).unwrap()).unwrap();
        assert_eq!(parsed.token, "tok");
        assert_eq!(parsed.expires_at, 1_900_000_000);
    }
}
