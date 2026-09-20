use crate::models::normalize_key;
use crate::store::EntryType;

const API_KEY_PREFIXES: &[&str] = &["sk-", "xai-", "sk-or-", "tvly-", "ghp_", "gho_", "ak-"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedEntry {
    pub group: Option<String>,
    pub kind: EntryType,
    pub key: String,
    pub value: String,
}

pub fn parse(text: &str) -> Vec<ParsedEntry> {
    let mut entries = Vec::new();
    let mut group: Option<String> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let export_body = line.strip_prefix("export ").map(str::trim);
        if let Some(body) = export_body
            && let Some((k, v)) = split_kv(body)
        {
            entries.push(classify(group.clone(), k, v));
            continue;
        }
        if let Some((k, v)) = split_kv(line) {
            entries.push(classify(group.clone(), k, v));
            continue;
        }
        if is_header(line, group.is_some()) {
            group = Some(line.trim_end_matches([':', '：']).trim().to_string());
            continue;
        }
        entries.push(classify(group.clone(), "", line));
    }
    entries
}

fn split_kv(line: &str) -> Option<(&str, &str)> {
    for sep in ['：', ':', '='] {
        if let Some(idx) = line.find(sep) {
            let key = line[..idx].trim();
            let value = line[idx + sep.len_utf8()..].trim();
            if sep == ':' && value.starts_with("//") {
                continue;
            }
            if key.is_empty() || value.is_empty() {
                continue;
            }
            return Some((key, unquote(value)));
        }
    }
    None
}

fn unquote(value: &str) -> &str {
    let bytes = value.as_bytes();
    let quoted = value.len() >= 2
        && ((bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\''));
    if quoted { &value[1..value.len() - 1] } else { value }
}

fn is_header(line: &str, in_group: bool) -> bool {
    if line.ends_with([':', '：']) {
        return true;
    }
    if looks_like_secret(line) {
        return false;
    }
    if in_group
        && line.len() >= 6
        && line.chars().all(|c| c.is_ascii_graphic())
    {
        return false;
    }
    true
}

fn looks_like_secret(line: &str) -> bool {
    if line.chars().any(char::is_whitespace) {
        return false;
    }
    line.len() >= 16 || API_KEY_PREFIXES.iter().any(|p| line.starts_with(p))
}

fn looks_like_email(value: &str) -> bool {
    if value.chars().any(char::is_whitespace) || value.matches('@').count() != 1 {
        return false;
    }
    match value.rsplit_once('@') {
        Some((local, domain)) => {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
        }
        None => false,
    }
}

fn classify(group: Option<String>, key: &str, value: &str) -> ParsedEntry {
    let lower = key.to_lowercase();
    let kind = if value.starts_with("http://") || value.starts_with("https://") {
        EntryType::Note
    } else if lower.contains("password")
        || lower.contains("passwd")
        || key.contains('密')
        || looks_like_email(value)
    {
        EntryType::Login
    } else if API_KEY_PREFIXES.iter().any(|p| value.starts_with(p))
        || ["apikey", "api_key", "key", "token", "secret"]
            .iter()
            .any(|w| lower.contains(w))
    {
        EntryType::ApiKey
    } else {
        EntryType::Note
    };
    let normalized = normalize_key(key);
    let key = match (kind, normalized) {
        (EntryType::Login, None) if looks_like_email(value) => "USERNAME".to_string(),
        (_, Some(k)) => k,
        (kind, None) => default_key(kind, value).to_string(),
    };
    ParsedEntry {
        group,
        kind,
        key,
        value: value.to_string(),
    }
}

fn default_key(kind: EntryType, value: &str) -> &'static str {
    match kind {
        EntryType::ApiKey => "API_KEY",
        EntryType::Login => "PASSWORD",
        EntryType::Note => {
            if value.starts_with("http://") || value.starts_with("https://") {
                "URL"
            } else {
                "NOTE"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only_entry(text: &str) -> ParsedEntry {
        let entries = parse(text);
        assert_eq!(entries.len(), 1, "expect exactly one entry: {entries:?}");
        entries.into_iter().next().unwrap()
    }

    #[test]
    fn mixed_separators() {
        let text = "openai\nkey_a：sk-aaa111\nkey_b: sk-bbb222\nkey_c = sk-ccc333\n";
        let entries = parse(text);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].key, "KEY_A");
        assert_eq!(entries[0].value, "sk-aaa111");
        assert_eq!(entries[1].key, "KEY_B");
        assert_eq!(entries[1].value, "sk-bbb222");
        assert_eq!(entries[2].key, "KEY_C");
        assert_eq!(entries[2].value, "sk-ccc333");
        assert!(entries.iter().all(|e| e.group.as_deref() == Some("openai")));
    }

    #[test]
    fn bare_value_belongs_to_current_group() {
        let text = "openrouter:\nsk-or-v1-abcdef1234567890abcdef\n";
        let entry = only_entry(text);
        assert_eq!(entry.group.as_deref(), Some("openrouter"));
        assert_eq!(entry.kind, EntryType::ApiKey);
        assert_eq!(entry.key, "API_KEY");
        assert_eq!(entry.value, "sk-or-v1-abcdef1234567890abcdef");
    }

    #[test]
    fn multiple_bare_values_in_one_group() {
        let text = "minimax3:\nsk-aaaaaaaaaaaaaaaa\nsk-bbbbbbbbbbbbbbbb\n";
        let entries = parse(text);
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.group.as_deref() == Some("minimax3")));
    }

    #[test]
    fn export_lines_are_extracted() {
        let entry = only_entry("export OPENAI_API_KEY=sk-test1234567890\n");
        assert_eq!(entry.key, "OPENAI_API_KEY");
        assert_eq!(entry.value, "sk-test1234567890");
        assert_eq!(entry.kind, EntryType::ApiKey);
        let quoted = only_entry("export DEEPSEEK_KEY=\"sk-quoted123456\"\n");
        assert_eq!(quoted.value, "sk-quoted123456");
    }

    #[test]
    fn chinese_colon_and_password_keyword() {
        let entry = only_entry("root密码：hunter2hunter2\n");
        assert_eq!(entry.kind, EntryType::Login);
        assert_eq!(entry.value, "hunter2hunter2");
        assert_eq!(entry.key, "ROOT");
    }

    #[test]
    fn empty_lines_are_skipped() {
        let text = "\n\ngroup1:\n\nkey: value123456\n\n";
        let entries = parse(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].group.as_deref(), Some("group1"));
    }

    #[test]
    fn headerless_file_start() {
        let text = "sk-orphan-key-1234567890\nopenai:\nkey: sk-x1\n";
        let entries = parse(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].group, None);
        assert_eq!(entries[1].group.as_deref(), Some("openai"));
    }

    #[test]
    fn bare_url_is_note_not_kv() {
        let entry = only_entry("hooks:\nhttps://example.com/webhook/abc123\n");
        assert_eq!(entry.kind, EntryType::Note);
        assert_eq!(entry.key, "URL");
        assert_eq!(entry.value, "https://example.com/webhook/abc123");
        assert_eq!(entry.group.as_deref(), Some("hooks"));
    }

    #[test]
    fn email_value_is_login_username() {
        let entry = only_entry("github\n邮箱: someone@example.com\n");
        assert_eq!(entry.kind, EntryType::Login);
        assert_eq!(entry.key, "USERNAME");
    }

    #[test]
    fn header_without_colon_at_file_start() {
        let text = "cursor\nsk-cursor-1234567890abcdef\n";
        let entry = only_entry(text);
        assert_eq!(entry.group.as_deref(), Some("cursor"));
    }

    #[test]
    fn line_ending_with_colon_is_header() {
        let entries = parse("a:\nminimax3:\ntoken: tvly-abcdef123456\n");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].group.as_deref(), Some("minimax3"));
        assert_eq!(entries[0].kind, EntryType::ApiKey);
    }

    #[test]
    fn key_value_url_keeps_full_value() {
        let entry = only_entry("g:\nbase_url: https://api.example.com/v1\n");
        assert_eq!(entry.value, "https://api.example.com/v1");
        assert_eq!(entry.kind, EntryType::Note);
    }

    #[test]
    fn empty_value_is_not_kv() {
        let text = "group:\napikey：\nsk-real-value-123456\n";
        let entries = parse(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].value, "sk-real-value-123456");
    }
}
