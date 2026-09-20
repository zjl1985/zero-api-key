use std::collections::BTreeMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryType {
    ApiKey,
    Login,
    Note,
}

impl EntryType {
    pub fn label(&self) -> &'static str {
        match self {
            EntryType::ApiKey => "api-key",
            EntryType::Login => "login",
            EntryType::Note => "note",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "api-key" | "apikey" | "api_key" => Some(EntryType::ApiKey),
            "login" => Some(EntryType::Login),
            "note" | "url" => Some(EntryType::Note),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub value: String,
    #[serde(rename = "type")]
    pub entry_type: EntryType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

impl Entry {
    pub fn new(key: &str, value: &str, entry_type: EntryType) -> Self {
        Self {
            key: key.to_string(),
            value: value.to_string(),
            entry_type,
            env_name: None,
            comment: None,
        }
    }
}

pub trait Store {
    fn name(&self) -> &'static str;
    fn list_groups(&self) -> Result<Vec<String>>;
    fn list_entries(&self, group: &str) -> Result<Vec<Entry>>;
    fn upsert(&self, group: &str, entry: &Entry) -> Result<()>;
    fn remove(&self, group: &str, key: Option<&str>) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAction {
    Add { group: String, entry: Entry },
    Conflict { group: String, source: Entry, target: Entry },
}

#[derive(Debug, Default)]
pub struct SyncPlan {
    pub actions: Vec<SyncAction>,
    pub identical: usize,
}

pub fn plan(source: &BTreeMap<String, Vec<Entry>>, target: &BTreeMap<String, Vec<Entry>>) -> SyncPlan {
    let mut plan = SyncPlan::default();
    for (group, entries) in source {
        let target_entries = target.get(group);
        for entry in entries {
            let existing = target_entries.and_then(|es| es.iter().find(|e| e.key == entry.key));
            match existing {
                None => plan.actions.push(SyncAction::Add {
                    group: group.clone(),
                    entry: entry.clone(),
                }),
                Some(t) if t.value == entry.value => plan.identical += 1,
                Some(t) => plan.actions.push(SyncAction::Conflict {
                    group: group.clone(),
                    source: entry.clone(),
                    target: t.clone(),
                }),
            }
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, value: &str) -> Entry {
        Entry::new(key, value, EntryType::ApiKey)
    }

    fn snapshot(groups: &[(&str, Vec<Entry>)]) -> BTreeMap<String, Vec<Entry>> {
        groups
            .iter()
            .map(|(g, es)| ((*g).to_string(), es.clone()))
            .collect()
    }

    #[test]
    fn missing_group_and_key_are_added() {
        let source = snapshot(&[("openai", vec![entry("API_KEY", "sk-1")])]);
        let plan = plan(&source, &BTreeMap::new());
        assert_eq!(plan.identical, 0);
        assert_eq!(
            plan.actions,
            vec![SyncAction::Add {
                group: "openai".into(),
                entry: entry("API_KEY", "sk-1"),
            }]
        );
    }

    #[test]
    fn identical_entries_produce_no_actions() {
        let source = snapshot(&[("openai", vec![entry("API_KEY", "sk-1")])]);
        let target = snapshot(&[("openai", vec![entry("API_KEY", "sk-1")])]);
        let plan = plan(&source, &target);
        assert!(plan.actions.is_empty());
        assert_eq!(plan.identical, 1);
    }

    #[test]
    fn differing_value_is_conflict() {
        let source = snapshot(&[("openai", vec![entry("API_KEY", "sk-new")])]);
        let target = snapshot(&[("openai", vec![entry("API_KEY", "sk-old")])]);
        let plan = plan(&source, &target);
        assert_eq!(plan.identical, 0);
        assert_eq!(
            plan.actions,
            vec![SyncAction::Conflict {
                group: "openai".into(),
                source: entry("API_KEY", "sk-new"),
                target: entry("API_KEY", "sk-old"),
            }]
        );
    }

    #[test]
    fn target_only_entries_are_untouched() {
        let source = snapshot(&[("openai", vec![entry("API_KEY", "sk-1")])]);
        let target = snapshot(&[
            ("openai", vec![entry("API_KEY", "sk-1")]),
            ("cursor", vec![entry("API_KEY", "sk-9")]),
        ]);
        let plan = plan(&source, &target);
        assert!(plan.actions.is_empty());
        assert_eq!(plan.identical, 1);
    }

    #[test]
    fn mixed_snapshot_plans_all_kinds() {
        let source = snapshot(&[(
            "openai",
            vec![
                entry("API_KEY", "sk-same"),
                entry("BASE_URL", "https://new"),
                entry("NOTE", "n1"),
            ],
        )]);
        let target = snapshot(&[(
            "openai",
            vec![entry("API_KEY", "sk-same"), entry("BASE_URL", "https://old")],
        )]);
        let plan = plan(&source, &target);
        assert_eq!(plan.identical, 1);
        assert_eq!(plan.actions.len(), 2);
        assert!(matches!(&plan.actions[0], SyncAction::Conflict { source, .. } if source.key == "BASE_URL"));
        assert!(matches!(&plan.actions[1], SyncAction::Add { entry, .. } if entry.key == "NOTE"));
    }

    #[test]
    fn entry_type_label_roundtrip() {
        for t in [EntryType::ApiKey, EntryType::Login, EntryType::Note] {
            assert_eq!(EntryType::from_label(t.label()), Some(t));
        }
        assert_eq!(EntryType::from_label("unknown"), None);
    }
}
