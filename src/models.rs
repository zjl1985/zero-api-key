use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub access_token: String,
    pub expires_in: u64,
    #[allow(dead_code)]
    pub token_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretMetadata {
    pub key: String,
    pub value: String,
    pub is_encrypted: bool,
}

impl SecretMetadata {
    pub fn new(key: &str, value: &str) -> Self {
        Self {
            key: key.to_string(),
            value: value.to_string(),
            is_encrypted: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Secret {
    pub secret_key: String,
    #[serde(default)]
    pub secret_value: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub secret_comment: String,
    #[serde(default)]
    pub secret_metadata: Vec<SecretMetadata>,
    #[serde(default)]
    #[allow(dead_code)]
    pub secret_path: Option<String>,
}

impl Secret {
    pub fn metadata_value(&self, key: &str) -> Option<&str> {
        self.secret_metadata
            .iter()
            .find(|m| m.key == key)
            .map(|m| m.value.as_str())
    }
}

#[derive(Debug, Deserialize)]
pub struct SecretsResponse {
    #[serde(default)]
    pub secrets: Vec<Secret>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretUpsertBody<'a> {
    pub project_id: &'a str,
    pub environment: &'a str,
    pub secret_value: &'a str,
    pub secret_path: &'a str,
    pub secret_comment: &'a str,
    pub secret_metadata: &'a [SecretMetadata],
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub skip_multiline_encoding: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretDeleteBody<'a> {
    pub project_id: &'a str,
    pub environment: &'a str,
    pub secret_path: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderCreateBody<'a> {
    pub project_id: &'a str,
    pub environment: &'a str,
    pub name: &'a str,
    pub path: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct FoldersResponse {
    #[serde(default)]
    pub folders: Vec<Folder>,
}

#[derive(Debug, Deserialize)]
pub struct Folder {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedToken {
    pub token: String,
    pub expires_at: u64,
}

pub fn normalize_key(raw: &str) -> Option<String> {
    let mut out = String::new();
    let mut prev_underscore = true;
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_uppercase());
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() { None } else { Some(out) }
}

pub fn mask(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 6 {
        return "*".repeat(chars.len());
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 2..].iter().collect();
    format!("{}{}{}", head, "*".repeat(chars.len() - 6), tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_key_converts_to_upper_snake() {
        assert_eq!(normalize_key("api key").as_deref(), Some("API_KEY"));
        assert_eq!(normalize_key("api-key").as_deref(), Some("API_KEY"));
        assert_eq!(normalize_key("api.key").as_deref(), Some("API_KEY"));
        assert_eq!(normalize_key("a__b").as_deref(), Some("A_B"));
        assert_eq!(normalize_key("Base_URL").as_deref(), Some("BASE_URL"));
    }

    #[test]
    fn normalize_key_rejects_non_ascii_only() {
        assert_eq!(normalize_key("邮箱"), None);
        assert_eq!(normalize_key(""), None);
        assert_eq!(normalize_key("---"), None);
    }

    #[test]
    fn mask_keeps_head_and_tail() {
        assert_eq!(mask("abcdefghij"), "abcd****ij");
        assert_eq!(mask("sk-1234567890ab"), "sk-1*********ab");
    }

    #[test]
    fn mask_short_values_fully() {
        assert_eq!(mask("abc"), "***");
        assert_eq!(mask("abcdef"), "******");
        assert_eq!(mask(""), "");
    }
}
