use zero_api_key_lib::service::{EntryView, GroupView};

#[derive(Clone)]
pub struct Row {
    pub group: String,
    pub entry: EntryView,
    search: String,
}
impl Row {
    pub fn id(&self) -> (String, String) {
        (self.group.clone(), self.entry.key.clone())
    }
}

pub fn index(catalog: &[GroupView]) -> Vec<Row> {
    catalog
        .iter()
        .flat_map(|group| {
            group.entries.iter().map(|entry| Row {
                group: group.name.clone(),
                search: format!(
                    "{} {} {} {}",
                    group.name,
                    entry.key,
                    entry.env_name.as_deref().unwrap_or(""),
                    entry.comment.as_deref().unwrap_or("")
                )
                .to_lowercase(),
                entry: entry.clone(),
            })
        })
        .collect()
}

pub fn filter(rows: &[Row], group: Option<&str>, query: &str) -> Vec<Row> {
    let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    rows.iter()
        .filter(|r| {
            group.is_none_or(|g| g == r.group) && terms.iter().all(|t| r.search.contains(t))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_combines_group_env_comment_and_never_searches_values() {
        let catalog = vec![GroupView {
            name: "OpenAI".into(),
            entries: vec![EntryView {
                key: "API_KEY".into(),
                masked_value: "private-value".into(),
                entry_type: "api-key".into(),
                env_name: Some("OPENAI_TOKEN".into()),
                comment: Some("生产环境".into()),
            }],
        }];
        let rows = index(&catalog);
        assert_eq!(filter(&rows, None, "openai 生产 TOKEN").len(), 1);
        assert!(filter(&rows, Some("other"), "").is_empty());
        assert!(filter(&rows, None, "private-value").is_empty());
    }
}
