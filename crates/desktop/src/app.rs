use crate::{
    backend::Backend,
    input::{Changed, TextInput},
    model::{self, Row},
    preferences::Preferences,
    theme::Palette,
};
use gpui::{prelude::*, *};
use std::time::Duration;
use zero_api_key_lib::service::{
    CommitEntry, EntryInput, GroupView, ImportPreviewItem, Service, StatusInfo,
};

actions!(
    vault,
    [
        Quit,
        Search,
        CommandPalette,
        NewEntry,
        NewGroup,
        Settings,
        Lock,
        Refresh,
        Dismiss,
        Submit,
        NextField,
        PreviousField,
        NextEntry,
        PreviousEntry,
        CopyEntry,
        EditEntry
    ]
);

#[derive(Clone, Copy, PartialEq)]
enum SettingsTab {
    General,
    Cloud,
    Security,
}

enum DialogKind {
    NewGroup,
    Entry {
        old_key: Option<String>,
    },
    Import {
        items: Vec<ImportPreviewItem>,
        checked: Vec<bool>,
    },
    Sync {
        to_cloud: bool,
    },
    Settings(SettingsTab),
    Confirm {
        group: String,
        key: Option<String>,
    },
    Palette,
}

struct Field {
    label: &'static str,
    input: Entity<TextInput>,
}
struct Dialog {
    kind: DialogKind,
    fields: Vec<Field>,
    entry_type: String,
}

pub struct VaultApp {
    backend: Backend,
    prefs: Preferences,
    p: Palette,
    focus: FocusHandle,
    query: Entity<TextInput>,
    password: Entity<TextInput>,
    catalog: Vec<GroupView>,
    rows: Vec<Row>,
    visible: Vec<Row>,
    group: Option<String>,
    selected: Option<(String, String)>,
    mode: String,
    status: Option<StatusInfo>,
    locked: bool,
    busy: bool,
    initialized: bool,
    dialog: Option<Dialog>,
    error: Option<String>,
    toast: Option<String>,
    toast_seq: u64,
    generation: u64,
    revealed: Option<(String, String, String)>,
    reveal_seq: u64,
    scroll: UniformListScrollHandle,
    _subscriptions: Vec<Subscription>,
    dialog_subscriptions: Vec<Subscription>,
    demo: bool,
    palette_selection: usize,
}

impl VaultApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>, demo: bool) -> Self {
        let prefs = Preferences::load();
        let query = cx.new(|cx| {
            TextInput::new(
                if prefs.chinese {
                    "搜索密钥、分组、环境变量…"
                } else {
                    "Search keys, groups, environment…"
                },
                false,
                cx,
            )
        });
        let password = cx.new(|cx| TextInput::new("Password", true, cx));
        let subscription = cx.subscribe(&query, |this, _, _: &Changed, cx| this.filter(cx));
        let mut this = Self {
            backend: Backend::new(),
            p: Palette::new(prefs.light),
            prefs,
            focus: cx.focus_handle(),
            query,
            password,
            catalog: vec![],
            rows: vec![],
            visible: vec![],
            group: None,
            selected: None,
            mode: "local".into(),
            status: None,
            locked: true,
            busy: false,
            initialized: false,
            dialog: None,
            error: None,
            toast: None,
            toast_seq: 0,
            generation: 0,
            revealed: None,
            reveal_seq: 0,
            scroll: UniformListScrollHandle::new(),
            _subscriptions: vec![subscription],
            dialog_subscriptions: vec![],
            demo,
            palette_selection: 0,
        };
        this.apply_theme(cx);
        window.focus(&this.focus);
        if demo {
            this.locked = false;
            this.initialized = true;
            this.status = Some(StatusInfo {
                default_mode: Some("local".into()),
                local_ready: true,
                cloud_ready: false,
                touch_id_enabled: false,
                ui_password_enabled: false,
            });
            this.catalog = demo_catalog();
            this.rows = model::index(&this.catalog);
            this.filter(cx);
        } else {
            this.boot(window, cx);
        }
        this
    }

    fn t(&self, en: &'static str, zh: &'static str) -> &'static str {
        if self.prefs.chinese { zh } else { en }
    }

    fn run<T: Send + 'static>(
        &mut self,
        job: impl FnOnce(&mut Service) -> anyhow::Result<T> + Send + 'static,
        then: impl FnOnce(&mut Self, T, &mut Window, &mut Context<Self>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        if self.demo {
            self.show_toast("Preview mode · changes are disabled", cx);
            return;
        }
        self.busy = true;
        self.error = None;
        let generation = self.generation;
        let result = self.backend.execute(job);
        cx.spawn_in(window, async move |this, cx| {
            let result = result
                .recv()
                .await
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Vault worker stopped")));
            let _ = this.update_in(cx, |this, window, cx| {
                if this.generation != generation {
                    return;
                }
                this.busy = false;
                match result {
                    Ok(value) => then(this, value, window, cx),
                    Err(error) => this.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn boot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.run(
            |s| Ok((s.is_locked(), s.status()?)),
            |this, (locked, status), window, cx| {
                this.mode = status
                    .default_mode
                    .clone()
                    .unwrap_or_else(|| "local".into());
                this.status = Some(status);
                this.locked = locked;
                this.initialized = true;
                if !locked {
                    this.reload(window, cx);
                } else {
                    window.focus(&this.password.focus_handle(cx));
                }
            },
            window,
            cx,
        );
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked {
            return;
        }
        let mode = self.mode.clone();
        self.run(
            move |s| s.catalog(&mode),
            |this, catalog, _, cx| {
                this.catalog = catalog;
                this.rows = model::index(&this.catalog);
                if this
                    .group
                    .as_ref()
                    .is_some_and(|g| !this.catalog.iter().any(|c| &c.name == g))
                {
                    this.group = None;
                }
                this.revealed = None;
                this.filter(cx);
            },
            window,
            cx,
        );
    }

    fn filter(&mut self, cx: &mut Context<Self>) {
        self.visible = model::filter(
            &self.rows,
            self.group.as_deref(),
            self.query.read(cx).value(),
        );
        if !self.visible.iter().any(|r| Some(r.id()) == self.selected) {
            self.selected = self.visible.first().map(Row::id);
            self.revealed = None;
        }
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    fn show_toast(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.toast = Some(text.into());
        self.toast_seq += 1;
        let seq = self.toast_seq;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(3)).await;
            let _ = this.update(cx, |this, cx| {
                if this.toast_seq == seq {
                    this.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn apply_theme(&mut self, cx: &mut Context<Self>) {
        self.p = Palette::new(self.prefs.light);
        let light = self.prefs.light;
        let placeholder = self.t(
            "Search keys, groups, environment…",
            "搜索密钥、分组、环境变量…",
        );
        self.query.update(cx, |input, cx| {
            input.set_light(light, cx);
            input.set_placeholder(placeholder, cx);
        });
        self.password
            .update(cx, |input, cx| input.set_light(light, cx));
        if let Some(dialog) = &self.dialog {
            for field in &dialog.fields {
                field.input.update(cx, |i, cx| i.set_light(light, cx));
            }
        }
    }

    fn persist_preferences(&mut self, cx: &mut Context<Self>) {
        self.apply_theme(cx);
        if !self.demo
            && let Err(error) = self.prefs.save()
        {
            self.error = Some(error.to_string());
        }
        cx.notify();
    }

    fn select(&mut self, id: (String, String), window: &mut Window, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.revealed = None;
        self.reveal_seq += 1;
        window.focus(&self.focus);
        cx.notify();
    }

    fn lock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.demo {
            return;
        }
        self.generation += 1;
        self.locked = true;
        self.busy = false;
        self.catalog.clear();
        self.rows.clear();
        self.visible.clear();
        self.selected = None;
        self.revealed = None;
        self.dialog = None;
        self.dialog_subscriptions.clear();
        self.error = None;
        self.toast = None;
        self.query.update(cx, |i, cx| i.set_text("", cx));
        self.password.update(cx, |i, cx| i.set_text("", cx));
        window.focus(&self.password.focus_handle(cx));
        self.run(
            |s| {
                s.lock();
                Ok(())
            },
            |_, _, _, _| {},
            window,
            cx,
        );
    }

    fn unlock(&mut self, biometric: bool, window: &mut Window, cx: &mut Context<Self>) {
        let password = self.password.read(cx).value().to_owned();
        self.run(
            move |s| {
                if biometric {
                    s.unlock_touch_id()?;
                } else {
                    s.unlock(&password)?;
                }
                s.status()
            },
            |this, status, window, cx| {
                this.password.update(cx, |i, cx| i.set_text("", cx));
                this.mode = status
                    .default_mode
                    .clone()
                    .unwrap_or_else(|| "local".into());
                this.status = Some(status);
                this.locked = false;
                this.initialized = true;
                window.focus(&this.focus);
                this.reload(window, cx);
            },
            window,
            cx,
        );
    }

    fn current(&self) -> Option<Row> {
        self.visible
            .iter()
            .find(|r| Some(r.id()) == self.selected)
            .cloned()
    }

    fn copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.current() else {
            return;
        };
        let mode = self.mode.clone();
        self.run(
            move |s| s.reveal(&mode, &row.group, &row.entry.key),
            |this, value, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(value));
                this.show_toast(this.t("Copied to clipboard", "已复制到剪贴板"), cx);
            },
            window,
            cx,
        );
    }

    fn reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.revealed.take().is_some() {
            self.reveal_seq += 1;
            cx.notify();
            return;
        }
        let Some(row) = self.current() else {
            return;
        };
        let mode = self.mode.clone();
        let id = row.id();
        self.run(
            move |s| s.reveal(&mode, &row.group, &row.entry.key),
            move |this, value, _, cx| {
                if this.selected.as_ref() != Some(&id) {
                    return;
                }
                this.revealed = Some((id.0, id.1, value));
                this.reveal_seq += 1;
                let seq = this.reveal_seq;
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_secs(5)).await;
                    let _ = this.update(cx, |this, cx| {
                        if this.reveal_seq == seq {
                            this.revealed = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            },
            window,
            cx,
        );
    }

    fn new_field(
        &mut self,
        label: &'static str,
        value: &str,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> Field {
        let light = self.prefs.light;
        let input = cx.new(|cx| {
            let mut input = TextInput::new(label, secret, cx);
            input.set_text(value.to_owned(), cx);
            input.set_light(light, cx);
            input
        });
        self.dialog_subscriptions
            .push(cx.observe(&input, |_, _, cx| cx.notify()));
        self.dialog_subscriptions
            .push(cx.subscribe(&input, |this, _, _: &Changed, cx| {
                this.palette_selection = 0;
                cx.notify();
            }));
        Field { label, input }
    }

    fn open_dialog(&mut self, kind: DialogKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy {
            return;
        }
        self.error = None;
        self.dialog_subscriptions.clear();
        self.palette_selection = 0;
        let specs: Vec<(&'static str, String, bool)> = match &kind {
            DialogKind::NewGroup => vec![(self.t("Group name", "分组名称"), String::new(), false)],
            DialogKind::Entry { .. } => vec![
                (
                    self.t("Group", "分组"),
                    self.group
                        .clone()
                        .or_else(|| self.current().map(|r| r.group))
                        .unwrap_or_default(),
                    false,
                ),
                (self.t("Key", "键名"), "API_KEY".into(), false),
                (self.t("Value", "内容"), String::new(), true),
                (
                    self.t("Environment variable (optional)", "环境变量（可选）"),
                    String::new(),
                    false,
                ),
            ],
            DialogKind::Import { .. } => vec![
                (
                    self.t("INI file path", "INI 文件路径"),
                    String::new(),
                    false,
                ),
                (
                    self.t(
                        "Default group for ungrouped entries",
                        "未分组条目的默认分组",
                    ),
                    self.group.clone().unwrap_or_default(),
                    false,
                ),
            ],
            DialogKind::Settings(SettingsTab::Cloud) => {
                // Only non-secret configuration is read here. Keychain stays on the worker.
                let config = zero_api_key_lib::config::Config::load()
                    .unwrap_or_else(|_| zero_api_key_lib::config::Config::empty());
                vec![
                    (
                        "API base",
                        config
                            .api_base
                            .unwrap_or_else(|| "https://app.infisical.com".into()),
                        false,
                    ),
                    ("Project ID", config.project_id.unwrap_or_default(), false),
                    ("Environment", config.environment, false),
                    ("Client ID", String::new(), false),
                    ("Client secret", String::new(), true),
                ]
            }
            DialogKind::Settings(SettingsTab::Security) => vec![
                (self.t("Current password", "当前密码"), String::new(), true),
                (self.t("New password", "新密码"), String::new(), true),
                (
                    self.t("Confirm new password", "确认新密码"),
                    String::new(),
                    true,
                ),
            ],
            DialogKind::Palette => {
                vec![(self.t("Type a command…", "输入命令…"), String::new(), false)]
            }
            _ => vec![],
        };
        let fields = specs
            .iter()
            .map(|(label, value, secret)| self.new_field(label, value, *secret, cx))
            .collect::<Vec<_>>();
        if let Some(field) = fields.first() {
            window.focus(&field.input.focus_handle(cx));
        } else {
            window.focus(&self.focus);
        }
        self.dialog = Some(Dialog {
            kind,
            fields,
            entry_type: "api-key".into(),
        });
        cx.notify();
    }

    fn edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.current() else {
            return;
        };
        let mode = self.mode.clone();
        let group = row.group.clone();
        let key = row.entry.key.clone();
        self.run(
            move |s| s.reveal(&mode, &group, &key),
            move |this, value, window, cx| {
                this.open_dialog(
                    DialogKind::Entry {
                        old_key: Some(row.entry.key.clone()),
                    },
                    window,
                    cx,
                );
                if let Some(dialog) = &mut this.dialog {
                    dialog.entry_type = row.entry.entry_type;
                    for (field, text) in dialog.fields.iter().zip([
                        row.group,
                        row.entry.key,
                        value,
                        row.entry.env_name.unwrap_or_default(),
                    ]) {
                        field.input.update(cx, |i, cx| i.set_text(text, cx));
                    }
                }
            },
            window,
            cx,
        );
    }

    fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.dialog = None;
        self.dialog_subscriptions.clear();
        self.error = None;
        window.focus(&self.focus);
        cx.notify();
    }

    fn complete_mutation(
        &mut self,
        message: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_dialog(window, cx);
        self.show_toast(message, cx);
        self.reload(window, cx);
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.locked {
            if self.initialized {
                self.unlock(false, window, cx);
            } else {
                self.boot(window, cx);
            }
            return;
        }
        let Some(dialog) = &self.dialog else {
            return;
        };
        let values: Vec<String> = dialog
            .fields
            .iter()
            .map(|f| f.input.read(cx).value().to_owned())
            .collect();
        let mode = self.mode.clone();
        match &dialog.kind {
            DialogKind::NewGroup => {
                let group = values[0].trim().to_owned();
                if group.is_empty() {
                    self.error = Some(self.t("Enter a group name", "请输入分组名称").into());
                    return;
                }
                self.run(
                    move |s| s.create_group(&mode, &group),
                    |this, _, window, cx| this.complete_mutation("Group created", window, cx),
                    window,
                    cx,
                );
            }
            DialogKind::Entry { old_key } => {
                if values[0].trim().is_empty() || values[1].trim().is_empty() {
                    self.error = Some(
                        self.t("Group and key are required", "请填写分组和键名")
                            .into(),
                    );
                    return;
                }
                let group = values[0].trim().to_owned();
                let old_key = old_key.clone();
                if old_key.is_some() && self.current().is_some_and(|r| r.group != group) {
                    self.error = Some(
                        self.t("Create a new entry to change its group", "跨分组请新建条目")
                            .into(),
                    );
                    return;
                }
                let key = values[1].clone();
                let normalized = zero_api_key_lib::models::normalize_key(&key);
                if normalized.is_none() {
                    self.error = Some(
                        self.t("Key needs letters or numbers", "键名需要包含字母或数字")
                            .into(),
                    );
                    return;
                }
                if old_key.is_none()
                    && self
                        .rows
                        .iter()
                        .any(|r| r.group == group && Some(&r.entry.key) == normalized.as_ref())
                {
                    self.error = Some(
                        self.t(
                            "This key exists. Use Edit to update it.",
                            "键名已存在，请使用编辑功能。",
                        )
                        .into(),
                    );
                    return;
                }
                let entry = EntryInput {
                    key,
                    value: values[2].clone(),
                    entry_type: dialog.entry_type.clone(),
                    env_name: Some(values[3].clone()),
                };
                self.run(
                    move |s| s.save_entry(&mode, &group, old_key.as_deref(), entry),
                    |this, _, window, cx| this.complete_mutation("Entry saved", window, cx),
                    window,
                    cx,
                );
            }
            DialogKind::Import { items, checked } if !items.is_empty() => {
                let mut entries = Vec::new();
                for (item, checked) in items.iter().zip(checked) {
                    if !checked {
                        continue;
                    }
                    let group = item
                        .group
                        .as_deref()
                        .unwrap_or(&values[1])
                        .trim()
                        .to_owned();
                    if group.is_empty() {
                        self.error = Some(
                            self.t(
                                "Set a default group for ungrouped entries",
                                "请设置未分组条目的默认分组",
                            )
                            .into(),
                        );
                        return;
                    }
                    entries.push(CommitEntry {
                        group,
                        key: item.key.clone(),
                        value: item.value.clone(),
                        entry_type: item.entry_type.clone(),
                    });
                }
                if entries.is_empty() {
                    self.error = Some(
                        self.t("Select at least one entry", "请至少选择一个条目")
                            .into(),
                    );
                    return;
                }
                self.run(
                    move |s| s.import_commit(&mode, entries),
                    |this, count, window, cx| {
                        this.close_dialog(window, cx);
                        this.show_toast(format!("Imported {count} entries"), cx);
                        this.reload(window, cx);
                    },
                    window,
                    cx,
                );
            }
            DialogKind::Import { .. } => {
                let path = values[0].trim().to_owned();
                self.run(
                    move |s| s.import_preview(&path),
                    |this, items, _, _| {
                        if items.is_empty() {
                            this.error = Some(
                                this.t("No entries found in this file", "文件中未找到条目")
                                    .into(),
                            );
                        }
                        if let Some(Dialog {
                            kind:
                                DialogKind::Import {
                                    items: target,
                                    checked,
                                },
                            ..
                        }) = &mut this.dialog
                        {
                            *checked = vec![true; items.len()];
                            *target = items;
                        }
                    },
                    window,
                    cx,
                );
            }
            DialogKind::Sync { to_cloud } => {
                let direction = if *to_cloud {
                    "local-to-cloud"
                } else {
                    "cloud-to-local"
                };
                self.run(
                    move |s| s.sync(direction),
                    |this, stats, window, cx| {
                        this.close_dialog(window, cx);
                        this.show_toast(
                            format!(
                                "Sync complete · +{} / ↑{} / ={}",
                                stats.added, stats.updated, stats.skipped
                            ),
                            cx,
                        );
                        this.reload(window, cx);
                    },
                    window,
                    cx,
                );
            }
            DialogKind::Confirm { group, key } => {
                let group = group.clone();
                let key = key.clone();
                self.run(
                    move |s| s.remove(&mode, &group, key.as_deref()),
                    |this, _, window, cx| this.complete_mutation("Deleted", window, cx),
                    window,
                    cx,
                );
            }
            DialogKind::Settings(SettingsTab::Cloud) => {
                if values.iter().any(|v| v.trim().is_empty()) {
                    self.error = Some(
                        self.t("Complete all cloud fields", "请填写所有云端配置")
                            .into(),
                    );
                    return;
                }
                self.run(
                    move |s| {
                        s.configure_cloud(
                            &values[0], &values[1], &values[2], &values[3], &values[4],
                        )?;
                        s.status()
                    },
                    |this, status, window, cx| {
                        this.status = Some(status);
                        this.complete_mutation("Cloud connected", window, cx);
                    },
                    window,
                    cx,
                );
            }
            DialogKind::Settings(SettingsTab::Security) => {
                if values[1].is_empty() {
                    self.error = Some(self.t("Enter a new password", "请输入新密码").into());
                    return;
                }
                if values[1] != values[2] {
                    self.error = Some(
                        self.t("Passwords do not match", "两次输入的密码不同")
                            .into(),
                    );
                    return;
                }
                self.run(
                    move |s| {
                        s.set_password(Some(&values[0]), Some(&values[1]))?;
                        s.status()
                    },
                    |this, status, window, cx| {
                        this.status = Some(status);
                        this.complete_mutation("Password updated", window, cx);
                    },
                    window,
                    cx,
                );
            }
            DialogKind::Palette => self.execute_command(self.palette_selection, window, cx),
            _ => self.close_dialog(window, cx),
        }
    }

    fn move_selection(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(
            self.dialog.as_ref().map(|d| &d.kind),
            Some(DialogKind::Palette)
        ) {
            let count = self.matching_commands(cx).len();
            self.palette_selection = (self.palette_selection as isize + delta)
                .clamp(0, count.saturating_sub(1) as isize)
                as usize;
            cx.notify();
            return;
        }
        if self.locked || self.dialog.is_some() || self.visible.is_empty() {
            return;
        }
        let current = self
            .visible
            .iter()
            .position(|r| Some(r.id()) == self.selected)
            .unwrap_or(0);
        let next = (current as isize + delta).clamp(0, self.visible.len() as isize - 1) as usize;
        self.select(self.visible[next].id(), window, cx);
        self.scroll.scroll_to_item(next, ScrollStrategy::Center);
    }

    fn cycle_focus(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dialog) = &self.dialog {
            if !dialog.fields.is_empty() {
                let current = dialog
                    .fields
                    .iter()
                    .position(|f| f.input.focus_handle(cx).is_focused(window))
                    .unwrap_or(0);
                let count = dialog.fields.len();
                let next = if backwards {
                    (current + count - 1) % count
                } else {
                    (current + 1) % count
                };
                window.focus(&dialog.fields[next].input.focus_handle(cx));
                cx.notify();
            }
        } else if backwards {
            window.focus_prev();
        } else {
            window.focus_next();
        }
    }

    fn command_names(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            (self.t("New entry", "新建条目"), "⌘N"),
            (self.t("New group", "新建分组"), "⇧⌘N"),
            (self.t("Search secrets", "搜索密钥"), "⌘F"),
            (self.t("Import INI file", "导入 INI 文件"), ""),
            (self.t("Sync local and cloud", "同步本地与云端"), ""),
            (self.t("Settings", "设置"), "⌘,"),
            (self.t("Lock vault", "锁定保险库"), "⇧⌘L"),
            (self.t("Refresh vault", "刷新保险库"), "⌘R"),
        ]
    }
    fn matching_commands(&self, cx: &App) -> Vec<usize> {
        let query = self
            .dialog
            .as_ref()
            .and_then(|d| d.fields.first())
            .map(|f| f.input.read(cx).value().to_lowercase())
            .unwrap_or_default();
        self.command_names()
            .iter()
            .enumerate()
            .filter(|(_, (name, _))| name.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect()
    }
    fn execute_command(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(command) = self.matching_commands(cx).get(index).copied() else {
            return;
        };
        self.close_dialog(window, cx);
        match command {
            0 => self.open_dialog(DialogKind::Entry { old_key: None }, window, cx),
            1 => self.open_dialog(DialogKind::NewGroup, window, cx),
            2 => window.focus(&self.query.focus_handle(cx)),
            3 => self.open_dialog(
                DialogKind::Import {
                    items: vec![],
                    checked: vec![],
                },
                window,
                cx,
            ),
            4 => self.open_dialog(DialogKind::Sync { to_cloud: true }, window, cx),
            5 => self.open_dialog(DialogKind::Settings(SettingsTab::General), window, cx),
            6 => self.lock(window, cx),
            _ => self.reload(window, cx),
        }
    }
}

impl VaultApp {
    fn icon(&self, name: &'static str) -> Svg {
        svg().path(name).size(px(15.)).text_color(self.p.muted)
    }
    fn button(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        primary: bool,
        cx: &Context<Self>,
        callback: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .gap_2()
            .h(px(30.))
            .px_3()
            .rounded(px(5.))
            .border_1()
            .border_color(if primary { p.accent } else { p.border })
            .bg(if primary { p.accent } else { p.panel })
            .text_color(if primary { p.sidebar } else { p.text })
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .when(self.busy, |s| s.opacity(0.45))
            .hover(move |s| s.bg(if primary { p.text } else { p.hover }))
            .on_click(cx.listener(move |this, event, window, cx| {
                if !this.busy {
                    callback(this, event, window, cx);
                }
            }))
            .child(label.into())
    }

    fn icon_button(
        &self,
        id: &'static str,
        icon: &'static str,
        cx: &Context<Self>,
        callback: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .size(px(28.))
            .rounded(px(4.))
            .cursor_pointer()
            .hover(move |s| s.bg(p.hover))
            .when(self.busy, |s| s.opacity(0.4))
            .on_click(cx.listener(move |this, event, window, cx| {
                if !this.busy {
                    callback(this, event, window, cx);
                }
            }))
            .child(self.icon(icon))
    }

    fn field(&self, field: &Field, window: &Window, cx: &App) -> Div {
        let input = field.input.clone();
        let focus = input.focus_handle(cx);
        let focused = focus.is_focused(window);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(self.p.muted)
                    .child(field.label),
            )
            .child(
                div()
                    .id(("field", input.entity_id()))
                    .px_3()
                    .py_2()
                    .rounded(px(5.))
                    .bg(self.p.base)
                    .border_1()
                    .border_color(if focused {
                        self.p.accent
                    } else {
                        self.p.border
                    })
                    .on_mouse_down(MouseButton::Left, move |_, window, _| window.focus(&focus))
                    .child(input),
            )
    }

    fn section_label(&self, label: impl Into<SharedString>) -> Div {
        div()
            .text_size(px(10.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(self.p.muted)
            .child(label.into())
    }

    fn titlebar(&self) -> Div {
        div()
            .flex()
            .h(px(40.))
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .pl(px(86.))
            .pr_4()
            .bg(self.p.sidebar)
            .border_b_1()
            .border_color(self.p.border)
            .window_control_area(WindowControlArea::Drag)
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(self.icon("key"))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .child("ZeroApiKey"),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(self.p.muted)
                    .child(if self.demo {
                        "PREVIEW"
                    } else {
                        "PERSONAL VAULT"
                    }),
            )
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let all = self.group.is_none();
        let nav = div()
            .id("all")
            .flex()
            .items_center()
            .gap_2()
            .h(px(34.))
            .px_3()
            .rounded(px(5.))
            .cursor_pointer()
            .bg(if all { p.selected } else { p.sidebar })
            .text_color(if all { p.accent } else { p.text })
            .hover(move |s| s.bg(p.hover))
            .on_click(cx.listener(|this, _, window, cx| {
                this.group = None;
                this.revealed = None;
                this.filter(cx);
                window.focus(&this.focus);
            }))
            .child(self.icon("key"))
            .child(self.t("All secrets", "全部条目"))
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(p.muted)
                    .child(self.rows.len().to_string()),
            );
        let mut groups = div()
            .id("group-list")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap_1();
        for group in &self.catalog {
            let name = group.name.clone();
            let selected = self.group.as_ref() == Some(&name);
            groups = groups.child(
                div()
                    .id(SharedString::from(format!("group-{name}")))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(32.))
                    .flex_shrink_0()
                    .px_3()
                    .rounded(px(5.))
                    .bg(if selected { p.selected } else { p.sidebar })
                    .text_color(if selected { p.accent } else { p.muted })
                    .hover(move |s| s.bg(p.hover))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.group = Some(name.clone());
                        this.revealed = None;
                        this.filter(cx);
                        window.focus(&this.focus);
                    }))
                    .child(self.icon("folder"))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .child(group.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .child(group.entries.len().to_string()),
                    ),
            );
        }
        div()
            .w(px(202.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .p_3()
            .gap_3()
            .bg(p.sidebar)
            .border_r_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .p_1()
                    .gap_1()
                    .rounded(px(6.))
                    .bg(p.base)
                    .border_1()
                    .border_color(p.border)
                    .children(
                        [
                            ("local", self.t("Local", "本地")),
                            ("cloud", self.t("Cloud", "云端")),
                        ]
                        .into_iter()
                        .map(|(mode, label)| {
                            let active = self.mode == mode;
                            div()
                                .id(mode)
                                .flex()
                                .flex_1()
                                .justify_center()
                                .items_center()
                                .h(px(27.))
                                .rounded(px(4.))
                                .cursor_pointer()
                                .bg(if active { p.hover } else { p.base })
                                .text_color(if active { p.text } else { p.muted })
                                .text_size(px(11.))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if this.busy || this.mode == mode {
                                        return;
                                    }
                                    if this.demo {
                                        this.mode = mode.into();
                                        cx.notify();
                                        return;
                                    }
                                    this.mode = mode.into();
                                    this.group = None;
                                    this.selected = None;
                                    this.catalog.clear();
                                    this.rows.clear();
                                    this.visible.clear();
                                    this.revealed = None;
                                    this.query.update(cx, |i, cx| i.set_text("", cx));
                                    this.reload(window, cx);
                                }))
                                .child(label)
                        }),
                    ),
            )
            .child(nav)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .pt_3()
                    .child(self.section_label(self.t("GROUPS", "分组")))
                    .child(
                        self.icon_button("new-group", "plus", cx, |this, _, window, cx| {
                            this.open_dialog(DialogKind::NewGroup, window, cx)
                        }),
                    ),
            )
            .child(groups)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .border_t_1()
                    .border_color(p.border)
                    .pt_3()
                    .child(self.sidebar_link(
                        "sync",
                        self.t("Sync", "同步"),
                        "sync",
                        cx,
                        |this, _, window, cx| {
                            this.open_dialog(DialogKind::Sync { to_cloud: true }, window, cx)
                        },
                    ))
                    .child(self.sidebar_link(
                        "import",
                        self.t("Import", "导入"),
                        "import",
                        cx,
                        |this, _, window, cx| {
                            this.open_dialog(
                                DialogKind::Import {
                                    items: vec![],
                                    checked: vec![],
                                },
                                window,
                                cx,
                            )
                        },
                    ))
                    .child(self.sidebar_link(
                        "settings",
                        self.t("Settings", "设置"),
                        "settings",
                        cx,
                        |this, _, window, cx| {
                            this.open_dialog(DialogKind::Settings(SettingsTab::General), window, cx)
                        },
                    ))
                    .child(self.sidebar_link(
                        "lock",
                        self.t("Lock vault", "锁定"),
                        "lock",
                        cx,
                        |this, _, window, cx| this.lock(window, cx),
                    )),
            )
    }

    fn sidebar_link(
        &self,
        id: &'static str,
        label: &'static str,
        icon: &'static str,
        cx: &Context<Self>,
        callback: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_3()
            .h(px(30.))
            .px_3()
            .text_size(px(11.))
            .text_color(p.muted)
            .rounded(px(4.))
            .cursor_pointer()
            .hover(move |s| s.bg(p.hover).text_color(p.text))
            .on_click(cx.listener(move |this, event, window, cx| {
                if !this.busy {
                    callback(this, event, window, cx);
                }
            }))
            .child(self.icon(icon))
            .child(label)
    }

    fn toolbar(&self, cx: &Context<Self>) -> Div {
        let p = self.p;
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap_3()
            .h(px(56.))
            .px_5()
            .border_b_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_1()
                    .min_w_0()
                    .child(self.icon("search"))
                    .child(div().flex_1().min_w_0().child(self.query.clone()))
                    .child(div().text_size(px(10.)).text_color(p.muted).child("⌘F")),
            )
            .child(div().w(px(1.)).h(px(18.)).bg(p.border))
            .child(
                self.icon_button("commands", "settings", cx, |this, _, window, cx| {
                    this.open_dialog(DialogKind::Palette, window, cx)
                }),
            )
            .child(self.button(
                "new-entry",
                self.t("＋  New entry", "＋  新建条目"),
                true,
                cx,
                |this, _, window, cx| {
                    this.open_dialog(DialogKind::Entry { old_key: None }, window, cx)
                },
            ))
    }

    fn entry_list(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let title = self
            .group
            .clone()
            .unwrap_or_else(|| self.t("All secrets", "全部条目").into());
        div()
            .w(px(330.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(p.border)
            .child(
                div()
                    .h(px(68.))
                    .flex_shrink_0()
                    .px_5()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(15.))
                                    .child(title),
                            )
                            .child(div().text_color(p.muted).text_size(px(10.)).child(format!(
                                "{} {}",
                                self.visible.len(),
                                self.t("entries", "个条目")
                            ))),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .when(self.group.is_some(), |s| {
                                s.child(self.icon_button(
                                    "delete-group",
                                    "trash",
                                    cx,
                                    |this, _, window, cx| {
                                        if let Some(group) = this.group.clone() {
                                            this.open_dialog(
                                                DialogKind::Confirm { group, key: None },
                                                window,
                                                cx,
                                            );
                                        }
                                    },
                                ))
                            })
                            .child(self.icon_button(
                                "refresh",
                                "sync",
                                cx,
                                |this, _, window, cx| this.reload(window, cx),
                            )),
                    ),
            )
            .child(if self.visible.is_empty() {
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .text_color(p.muted)
                    .child(self.icon("search"))
                    .child(self.t("No entries found", "暂无条目"))
                    .child(div().text_size(px(10.)).child(self.t(
                        "Add an entry or try another search.",
                        "新建条目，或尝试其他搜索。",
                    )))
                    .into_any_element()
            } else {
                uniform_list(
                    "secrets",
                    self.visible.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _window, cx| {
                        range
                            .map(|ix| {
                                let row = &this.visible[ix];
                                let id = row.id();
                                let selected = this.selected.as_ref() == Some(&id);
                                let p = this.p;
                                div().id(ix).h(px(66.)).px_2().pb_1().child(
                                    div()
                                        .id(("row", ix))
                                        .flex()
                                        .size_full()
                                        .items_center()
                                        .gap_3()
                                        .px_3()
                                        .rounded(px(6.))
                                        .border_1()
                                        .cursor_pointer()
                                        .border_color(if selected { p.border } else { p.base })
                                        .bg(if selected { p.selected } else { p.base })
                                        .hover(move |s| s.bg(p.hover))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.select(id.clone(), window, cx)
                                        }))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .size(px(30.))
                                                .flex_shrink_0()
                                                .rounded(px(7.))
                                                .bg(p.panel)
                                                .child(this.icon(
                                                    if row.entry.entry_type == "login" {
                                                        "lock"
                                                    } else {
                                                        "key"
                                                    },
                                                )),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .flex()
                                                .flex_col()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .text_size(px(12.))
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .text_ellipsis()
                                                        .child(row.entry.key.clone()),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(10.))
                                                        .text_color(p.muted)
                                                        .text_ellipsis()
                                                        .child(format!(
                                                            "{}  /  {}",
                                                            row.group, row.entry.entry_type
                                                        )),
                                                ),
                                        )
                                        .when(selected, |s| {
                                            s.child(div().size(px(5.)).rounded_full().bg(p.accent))
                                        }),
                                )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(self.scroll.clone())
                .flex_1()
                .into_any_element()
            })
    }

    fn detail(&self, cx: &Context<Self>) -> AnyElement {
        let p = self.p;
        let Some(row) = self.current() else {
            return div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_4()
                .text_color(p.muted)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(64.))
                        .rounded(px(16.))
                        .border_1()
                        .border_color(p.border)
                        .child(self.icon("key").size(px(24.))),
                )
                .child(
                    div()
                        .text_size(px(18.))
                        .text_color(p.text)
                        .child(self.t("A quiet place for your keys.", "安心存放每一把密钥。")),
                )
                .child(div().text_size(px(11.)).child(self.t(
                    "Select an entry, or create your first one.",
                    "选择一个条目，或新建第一个条目。",
                )))
                .into_any_element();
        };
        let value = self
            .revealed
            .as_ref()
            .filter(|(g, k, _)| g == &row.group && k == &row.entry.key)
            .map(|(_, _, v)| v.clone());
        let delete_group = row.group.clone();
        let delete_key = row.entry.key.clone();
        let export_group = row.group.clone();
        div()
            .id("detail")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .px_7()
            .py_6()
            .flex()
            .flex_col()
            .gap_6()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(10.))
                            .text_color(p.muted)
                            .child(self.icon("folder"))
                            .child(row.group.clone())
                            .child("/")
                            .child(self.t("Secret", "条目")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(self.icon_button("edit", "edit", cx, |this, _, window, cx| {
                                this.edit(window, cx)
                            }))
                            .child(self.icon_button(
                                "delete",
                                "trash",
                                cx,
                                move |this, _, window, cx| {
                                    this.open_dialog(
                                        DialogKind::Confirm {
                                            group: delete_group.clone(),
                                            key: Some(delete_key.clone()),
                                        },
                                        window,
                                        cx,
                                    )
                                },
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(46.))
                            .rounded(px(12.))
                            .bg(p.selected)
                            .child(self.icon("key").size(px(23.)).text_color(p.accent)),
                    )
                    .child(
                        div()
                            .text_size(px(23.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(row.entry.key.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(self.badge(row.entry.entry_type.clone()))
                            .child(self.badge(self.mode.clone())),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.section_label(self.t("SECRET VALUE", "密钥内容")))
                    .child(
                        div()
                            .p_4()
                            .rounded(px(7.))
                            .bg(p.panel)
                            .border_1()
                            .border_color(p.border)
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_family("monospace")
                                    .overflow_hidden()
                                    .child(
                                        value
                                            .clone()
                                            .unwrap_or_else(|| "••••••••••••••••••••".into()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_size(px(10.)).text_color(p.muted).child(
                                        if value.is_some() {
                                            self.t(
                                                "Hides automatically in 5 seconds",
                                                "5 秒后自动隐藏",
                                            )
                                        } else {
                                            self.t("Hidden by default", "默认隐藏")
                                        },
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .gap_1()
                                            .child(self.icon_button(
                                                "reveal",
                                                "eye",
                                                cx,
                                                |this, _, window, cx| this.reveal(window, cx),
                                            ))
                                            .child(self.icon_button(
                                                "copy",
                                                "copy",
                                                cx,
                                                |this, _, window, cx| this.copy(window, cx),
                                            )),
                                    ),
                            ),
                    ),
            )
            .when(row.entry.env_name.is_some(), |s| {
                s.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(self.section_label(self.t("ENVIRONMENT VARIABLE", "环境变量")))
                        .child(
                            div()
                                .font_family("monospace")
                                .text_size(px(12.))
                                .text_color(p.accent)
                                .child(row.entry.env_name.clone().unwrap_or_default()),
                        ),
                )
            })
            .when(row.entry.comment.is_some(), |s| {
                s.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(self.section_label(self.t("NOTES", "备注")))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(p.muted)
                                .child(row.entry.comment.clone().unwrap_or_default()),
                        ),
                )
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .pt_1()
                    .child(self.button(
                        "copy-primary",
                        self.t("Copy value  ⌘⇧C", "复制内容  ⌘⇧C"),
                        true,
                        cx,
                        |this, _, window, cx| this.copy(window, cx),
                    ))
                    .child(self.button(
                        "edit-primary",
                        self.t("Edit", "编辑"),
                        false,
                        cx,
                        |this, _, window, cx| this.edit(window, cx),
                    )),
            )
            .child(div().flex_1())
            .child(
                div()
                    .border_t_1()
                    .border_color(p.border)
                    .pt_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(self.sidebar_link(
                        "export-group",
                        self.t("Copy group as shell exports", "复制分组的 Shell 导出语句"),
                        "export",
                        cx,
                        move |this, _, window, cx| {
                            let group = export_group.clone();
                            let mode = this.mode.clone();
                            this.run(
                                move |s| s.export(&mode, &group),
                                |this, value, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(value));
                                    this.show_toast(
                                        this.t("Shell exports copied", "已复制 Shell 导出语句"),
                                        cx,
                                    );
                                },
                                window,
                                cx,
                            );
                        },
                    ))
                    .child(div().text_size(px(10.)).text_color(p.muted).child(self.t(
                        "Encrypted on disk. Ready when you need it.",
                        "加密保存，随时取用。",
                    ))),
            )
            .into_any_element()
    }

    fn badge(&self, label: impl Into<SharedString>) -> Div {
        div()
            .px_2()
            .py(px(3.))
            .rounded(px(4.))
            .border_1()
            .border_color(self.p.border)
            .text_size(px(10.))
            .text_color(self.p.muted)
            .child(label.into())
    }

    fn footer(&self) -> Div {
        div()
            .flex()
            .flex_shrink_0()
            .h(px(27.))
            .px_4()
            .items_center()
            .justify_between()
            .border_t_1()
            .border_color(self.p.border)
            .bg(self.p.sidebar)
            .text_size(px(10.))
            .text_color(self.p.muted)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().size(px(5.)).rounded_full().bg(self.p.accent))
                    .child(if self.busy {
                        self.t("Working…", "处理中…")
                    } else if self.locked {
                        self.t("Vault locked", "已锁定")
                    } else {
                        self.t("Vault ready", "保险库就绪")
                    }),
            )
            .child(if let Some(toast) = &self.toast {
                toast.clone()
            } else {
                format!(
                    "{}   ·   ⌘K {}",
                    if self.mode == "local" {
                        self.t("Local encrypted vault", "本地加密保险库")
                    } else {
                        "Infisical Cloud"
                    },
                    self.t("commands", "命令")
                )
            })
    }

    fn lock_screen(&self, window: &Window, cx: &Context<Self>) -> Div {
        div().flex_1().flex().items_center().justify_center().child(
            div()
                .w(px(320.))
                .flex()
                .flex_col()
                .gap_5()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_4()
                        .child(
                            div()
                                .size(px(56.))
                                .rounded(px(15.))
                                .bg(self.p.selected)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(self.icon("lock").size(px(25.)).text_color(self.p.accent)),
                        )
                        .child(
                            div()
                                .text_size(px(24.))
                                .font_weight(FontWeight::MEDIUM)
                                .child("Your keys. Your space."),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(self.p.muted)
                                .child(self.t(
                                    "Unlock your personal vault to continue.",
                                    "解锁你的私人保险库。",
                                )),
                        ),
                )
                .when(self.initialized, |s| {
                    s.child(self.field(
                        &Field {
                            label: self.t("Unlock password", "解锁密码"),
                            input: self.password.clone(),
                        },
                        window,
                        cx,
                    ))
                })
                .when(self.error.is_some(), |s| {
                    s.child(
                        div()
                            .text_size(px(11.))
                            .text_color(self.p.danger)
                            .child(self.error.clone().unwrap_or_default()),
                    )
                })
                .child(self.button(
                    "unlock",
                    if !self.initialized {
                        self.t("Retry", "重试")
                    } else {
                        self.t("Unlock vault  ↵", "解锁保险库  ↵")
                    },
                    true,
                    cx,
                    |this, _, window, cx| this.submit(window, cx),
                ))
                .when(
                    self.status.as_ref().is_some_and(|s| s.touch_id_enabled),
                    |s| {
                        s.child(self.button(
                            "touch-id",
                            "Touch ID",
                            false,
                            cx,
                            |this, _, window, cx| this.unlock(true, window, cx),
                        ))
                    },
                )
                .child(
                    div()
                        .text_center()
                        .text_size(px(10.))
                        .text_color(self.p.muted)
                        .child(self.t("Local first. Encrypted at rest.", "本地优先，加密存储。")),
                ),
        )
    }

    fn dialog_view(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        let Some(dialog) = &self.dialog else {
            return div().into_any_element();
        };
        let p = self.p;
        let (title, subtitle, submit) = match &dialog.kind {
            DialogKind::NewGroup => (
                self.t("New group", "新建分组"),
                self.t(
                    "Give related secrets a place of their own.",
                    "为相关密钥建立一个分组。",
                ),
                self.t("Create group", "创建分组"),
            ),
            DialogKind::Entry { old_key } => (
                if old_key.is_some() {
                    self.t("Edit entry", "编辑条目")
                } else {
                    self.t("New entry", "新建条目")
                },
                self.t(
                    "Stored encrypted in your selected vault.",
                    "加密保存到当前保险库。",
                ),
                self.t("Save entry", "保存条目"),
            ),
            DialogKind::Import { items, .. } => (
                self.t("Import secrets", "导入密钥"),
                self.t(
                    "Preview and select entries before importing.",
                    "预览后选择需要导入的条目。",
                ),
                if items.is_empty() {
                    self.t("Preview file", "预览文件")
                } else {
                    self.t("Import selected", "导入所选条目")
                },
            ),
            DialogKind::Sync { .. } => (
                self.t("Sync vaults", "同步保险库"),
                self.t("Choose where your entries should go.", "选择同步方向。"),
                self.t("Start sync", "开始同步"),
            ),
            DialogKind::Settings(_) => (
                self.t("Settings", "设置"),
                self.t("Make this space yours.", "按你的习惯设置。"),
                self.t("Save", "保存"),
            ),
            DialogKind::Confirm { key, .. } => (
                if key.is_some() {
                    self.t("Delete entry?", "删除条目？")
                } else {
                    self.t("Delete group?", "删除分组？")
                },
                self.t(
                    "This permanently removes the selected data.",
                    "这将永久删除选中的数据。",
                ),
                self.t("Delete permanently", "永久删除"),
            ),
            DialogKind::Palette => (
                self.t("Command palette", "命令面板"),
                self.t("Everything, a few keystrokes away.", "常用操作，触手可及。"),
                "",
            ),
        };
        let mut body = div()
            .id("dialog-body")
            .flex()
            .flex_col()
            .gap_4()
            .p_6()
            .max_h(px(470.))
            .overflow_y_scroll();
        if let DialogKind::Settings(tab) = dialog.kind {
            body = body.child(
                div().flex().gap_2().pb_2().children(
                    [
                        (SettingsTab::General, self.t("General", "通用")),
                        (SettingsTab::Cloud, self.t("Cloud", "云端")),
                        (SettingsTab::Security, self.t("Security", "安全")),
                    ]
                    .into_iter()
                    .map(|(target, label)| {
                        self.button(
                            label,
                            label,
                            tab == target,
                            cx,
                            move |this, _, window, cx| {
                                this.open_dialog(DialogKind::Settings(target), window, cx)
                            },
                        )
                    }),
                ),
            );
            match tab {
                SettingsTab::General => {
                    body = body
                        .child(self.section_label(self.t("APPEARANCE", "外观")))
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(self.button(
                                    "dark",
                                    self.t("Dark · Forest", "深色 · 森林"),
                                    !self.prefs.light,
                                    cx,
                                    |this, _, _, cx| {
                                        this.prefs.light = false;
                                        this.persist_preferences(cx);
                                    },
                                ))
                                .child(self.button(
                                    "light",
                                    self.t("Light · Paper", "浅色 · 纸张"),
                                    self.prefs.light,
                                    cx,
                                    |this, _, _, cx| {
                                        this.prefs.light = true;
                                        this.persist_preferences(cx);
                                    },
                                )),
                        )
                        .child(self.section_label(self.t("LANGUAGE", "语言")))
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(self.button(
                                    "english",
                                    "English",
                                    !self.prefs.chinese,
                                    cx,
                                    |this, _, window, cx| {
                                        this.prefs.chinese = false;
                                        this.persist_preferences(cx);
                                        this.open_dialog(
                                            DialogKind::Settings(SettingsTab::General),
                                            window,
                                            cx,
                                        );
                                    },
                                ))
                                .child(self.button(
                                    "chinese",
                                    "简体中文",
                                    self.prefs.chinese,
                                    cx,
                                    |this, _, window, cx| {
                                        this.prefs.chinese = true;
                                        this.persist_preferences(cx);
                                        this.open_dialog(
                                            DialogKind::Settings(SettingsTab::General),
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                        )
                        .child(self.section_label(self.t("DEFAULT VAULT", "默认保险库")))
                        .child(
                            div().flex().gap_2().children(
                                [
                                    ("local", self.t("Local", "本地")),
                                    ("cloud", self.t("Cloud", "云端")),
                                ]
                                .into_iter()
                                .map(|(mode, label)| {
                                    self.button(
                                        SharedString::from(format!("default-{mode}")),
                                        label,
                                        self.status
                                            .as_ref()
                                            .and_then(|s| s.default_mode.as_deref())
                                            == Some(mode),
                                        cx,
                                        move |this, _, window, cx| {
                                            this.run(
                                                move |s| {
                                                    s.set_default(mode)?;
                                                    s.status()
                                                },
                                                |this, status, _, cx| {
                                                    this.status = Some(status);
                                                    this.show_toast(
                                                        this.t(
                                                            "Default vault updated",
                                                            "已更新默认保险库",
                                                        ),
                                                        cx,
                                                    );
                                                },
                                                window,
                                                cx,
                                            );
                                        },
                                    )
                                }),
                            ),
                        )
                        .child(
                            div()
                                .border_t_1()
                                .border_color(p.border)
                                .pt_4()
                                .text_size(px(11.))
                                .text_color(p.muted)
                                .child("ZeroApiKey 0.2.0 · Rust + GPUI"),
                        );
                }
                SettingsTab::Cloud => {
                    body = body.child(div().text_size(px(11.)).text_color(p.muted).child(self.t(
                        "Infisical credentials are stored in the macOS Keychain.",
                        "Infisical 凭证保存在 macOS 钥匙串中。",
                    )))
                }
                SettingsTab::Security => {
                    let enabled = self.status.as_ref().is_some_and(|s| s.touch_id_enabled);
                    body = body
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .pb_3()
                                .border_b_1()
                                .border_color(p.border)
                                .child(div().flex().flex_col().gap_1().child("Touch ID").child(
                                    div().text_size(px(10.)).text_color(p.muted).child(
                                        self.t("Use your fingerprint to unlock.", "使用指纹解锁。"),
                                    ),
                                ))
                                .child(self.button(
                                    "toggle-bio",
                                    if enabled {
                                        self.t("Enabled", "已启用")
                                    } else {
                                        self.t("Enable", "启用")
                                    },
                                    enabled,
                                    cx,
                                    move |this, _, window, cx| {
                                        this.run(
                                            move |s| {
                                                s.set_touch_id(!enabled)?;
                                                s.status()
                                            },
                                            |this, status, _, _| this.status = Some(status),
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                        )
                        .child(div().text_size(px(11.)).text_color(p.muted).child(self.t(
                            "Set an app unlock password. The CLI keeps its existing access.",
                            "设置桌面端解锁密码。CLI 沿用原有访问方式。",
                        )));
                }
            }
        }
        if let DialogKind::Entry { .. } = dialog.kind {
            body = body.child(
                div().flex().gap_2().children(
                    [
                        ("api-key", "API Key"),
                        ("login", self.t("Login", "登录")),
                        ("note", self.t("Note", "笔记")),
                    ]
                    .into_iter()
                    .map(|(kind, label)| {
                        self.button(
                            label,
                            label,
                            dialog.entry_type == kind,
                            cx,
                            move |this, _, _, cx| {
                                if let Some(dialog) = &mut this.dialog {
                                    dialog.entry_type = kind.into();
                                }
                                cx.notify();
                            },
                        )
                    }),
                ),
            );
        }
        for field in &dialog.fields {
            body = body.child(self.field(field, window, cx));
        }
        match &dialog.kind {
            DialogKind::Import { items, checked } => {
                body = body.child(self.button(
                    "browse",
                    self.t("Choose file…", "选择文件…"),
                    false,
                    cx,
                    |_this, _, window, cx| {
                        let paths = cx.prompt_for_paths(PathPromptOptions {
                            files: true,
                            directories: false,
                            multiple: false,
                            prompt: Some("Import INI".into()),
                        });
                        cx.spawn_in(window, async move |this, cx| {
                            if let Ok(Ok(Some(paths))) = paths.await
                                && let Some(path) = paths.first()
                            {
                                let path = path.to_string_lossy().to_string();
                                let _ = this.update_in(cx, |this, _, cx| {
                                    if let Some(Dialog {
                                        kind: DialogKind::Import { items, checked },
                                        fields,
                                        ..
                                    }) = &mut this.dialog
                                    {
                                        items.clear();
                                        checked.clear();
                                        fields[0].input.update(cx, |i, cx| i.set_text(path, cx));
                                    }
                                    cx.notify();
                                });
                            }
                        })
                        .detach();
                    },
                ));
                if !items.is_empty() {
                    body = body.child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .pt_2()
                            .child(self.section_label(format!(
                                "{} / {} {}",
                                checked.iter().filter(|c| **c).count(),
                                items.len(),
                                self.t("SELECTED", "已选择")
                            )))
                            .child(self.button(
                                "toggle-all",
                                self.t("Toggle all", "全选 / 全不选"),
                                false,
                                cx,
                                |this, _, _, cx| {
                                    if let Some(Dialog {
                                        kind: DialogKind::Import { checked, .. },
                                        ..
                                    }) = &mut this.dialog
                                    {
                                        let next = !checked.iter().all(|c| *c);
                                        checked.fill(next);
                                    }
                                    cx.notify();
                                },
                            )),
                    );
                    for (ix, item) in items.iter().enumerate() {
                        let selected = checked.get(ix).copied().unwrap_or(false);
                        body = body.child(
                            div()
                                .id(("preview", ix))
                                .flex()
                                .items_center()
                                .gap_3()
                                .px_3()
                                .py_2()
                                .rounded(px(4.))
                                .border_1()
                                .border_color(p.border)
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.busy {
                                        return;
                                    }
                                    if let Some(Dialog {
                                        kind: DialogKind::Import { checked, .. },
                                        ..
                                    }) = &mut this.dialog
                                        && let Some(check) = checked.get_mut(ix)
                                    {
                                        *check = !*check;
                                    }
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .text_color(if selected { p.accent } else { p.muted })
                                        .child(if selected { "☑" } else { "☐" }),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_size(px(12.))
                                                .text_ellipsis()
                                                .child(item.key.clone()),
                                        )
                                        .child(div().text_size(px(10.)).text_color(p.muted).child(
                                            item.group.clone().unwrap_or_else(|| {
                                                self.t("Default group", "默认分组").into()
                                            }),
                                        )),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(p.muted)
                                        .child("••••••••"),
                                ),
                        );
                    }
                    body = body.child(div().text_size(px(10.)).text_color(p.danger).child(self.t(
                        "Existing entries with the same group and key will be overwritten.",
                        "相同分组与键名的现有条目将被覆盖。",
                    )));
                }
            }
            DialogKind::Sync { to_cloud } => {
                body=body.child(div().flex().flex_col().gap_3()
                    .child(self.button("to-cloud",self.t("Local  →  Infisical Cloud","本地  →  Infisical 云端"),*to_cloud,cx,|this,_,_,cx|{if let Some(Dialog{kind:DialogKind::Sync{to_cloud},..})=&mut this.dialog{*to_cloud=true;}cx.notify();}))
                    .child(self.button("to-local",self.t("Infisical Cloud  →  Local","Infisical 云端  →  本地"),!*to_cloud,cx,|this,_,_,cx|{if let Some(Dialog{kind:DialogKind::Sync{to_cloud},..})=&mut this.dialog{*to_cloud=false;}cx.notify();})))
                    .child(div().p_3().rounded(px(5.)).bg(p.base).text_size(px(11.)).text_color(p.danger)
                        .child(self.t("The source wins when values differ. Existing target values may be overwritten.","值不同时以来源端为准，将覆盖目标端的对应值。")));
            }
            DialogKind::Confirm { group, key } => {
                body = body.child(
                    div()
                        .p_4()
                        .rounded(px(5.))
                        .bg(p.base)
                        .text_color(p.danger)
                        .child(format!(
                            "{group}{}",
                            key.as_ref()
                                .map(|k| format!(" / {k}"))
                                .unwrap_or_else(|| " / *".into())
                        )),
                )
            }
            DialogKind::Palette => {
                let names = self.command_names();
                let matches = self.matching_commands(cx);
                for (ix, command) in matches.iter().enumerate() {
                    let (name, hint) = names[*command];
                    body = body.child(
                        div()
                            .id(("command", ix))
                            .h(px(36.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .px_3()
                            .rounded(px(4.))
                            .bg(if ix == self.palette_selection {
                                p.selected
                            } else {
                                p.panel
                            })
                            .cursor_pointer()
                            .hover(move |s| s.bg(p.hover))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.execute_command(ix, window, cx)
                            }))
                            .child(name)
                            .child(div().text_size(px(10.)).text_color(p.muted).child(hint)),
                    );
                }
                if matches.is_empty() {
                    body = body.child(
                        div()
                            .p_3()
                            .text_color(p.muted)
                            .child(self.t("No matching commands", "没有匹配的命令")),
                    );
                }
            }
            _ => {}
        }
        if let Some(error) = &self.error {
            body = body.child(
                div()
                    .p_3()
                    .bg(p.base)
                    .rounded(px(5.))
                    .text_size(px(11.))
                    .text_color(p.danger)
                    .child(error.clone()),
            );
        }
        if let DialogKind::Settings(SettingsTab::Security) = dialog.kind
            && self.status.as_ref().is_some_and(|s| s.ui_password_enabled)
        {
            body = body.child(self.button(
                "remove-password",
                self.t(
                    "Remove password (requires current password)",
                    "移除密码（需要当前密码）",
                ),
                false,
                cx,
                |this, _, window, cx| {
                    let current = this
                        .dialog
                        .as_ref()
                        .map(|d| d.fields[0].input.read(cx).value().to_owned())
                        .unwrap_or_default();
                    this.run(
                        move |s| {
                            s.set_password(Some(&current), None)?;
                            s.status()
                        },
                        |this, status, window, cx| {
                            this.status = Some(status);
                            this.complete_mutation("Password removed", window, cx);
                        },
                        window,
                        cx,
                    );
                },
            ));
        }
        let show_submit = !matches!(dialog.kind, DialogKind::Palette);
        let submit_label = if self.busy {
            self.t("Working…", "处理中…")
        } else if matches!(dialog.kind, DialogKind::Settings(SettingsTab::General)) {
            self.t("Done", "完成")
        } else {
            submit
        };
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000088))
            .occlude()
            .child(
                div()
                    .w(px(510.))
                    .max_h(relative(0.9))
                    .flex()
                    .flex_col()
                    .rounded(px(10.))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.panel)
                    .shadow_lg()
                    .occlude()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px_6()
                            .py_5()
                            .border_b_1()
                            .border_color(p.border)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(18.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(p.muted)
                                            .child(subtitle),
                                    ),
                            )
                            .child(self.button(
                                "close",
                                "Esc",
                                false,
                                cx,
                                |this, _, window, cx| this.close_dialog(window, cx),
                            )),
                    )
                    .child(body)
                    .when(show_submit, |s| {
                        s.child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .px_6()
                                .py_4()
                                .border_t_1()
                                .border_color(p.border)
                                .child(self.button(
                                    "cancel",
                                    self.t("Cancel", "取消"),
                                    false,
                                    cx,
                                    |this, _, window, cx| this.close_dialog(window, cx),
                                ))
                                .child(self.button(
                                    "submit",
                                    submit_label,
                                    true,
                                    cx,
                                    |this, _, window, cx| this.submit(window, cx),
                                )),
                        )
                    }),
            )
            .into_any_element()
    }
}

impl Render for VaultApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .font_family(".AppleSystemUIFont")
            .text_size(px(12.))
            .bg(self.p.base)
            .text_color(self.p.text)
            .key_context("Vault")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Search, window, cx| {
                if !this.locked && this.dialog.is_none() {
                    window.focus(&this.query.focus_handle(cx));
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &CommandPalette, window, cx| {
                this.open_dialog(DialogKind::Palette, window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewEntry, window, cx| {
                this.open_dialog(DialogKind::Entry { old_key: None }, window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewGroup, window, cx| {
                this.open_dialog(DialogKind::NewGroup, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Settings, window, cx| {
                this.open_dialog(DialogKind::Settings(SettingsTab::General), window, cx)
            }))
            .on_action(cx.listener(|this, _: &Lock, window, cx| this.lock(window, cx)))
            .on_action(cx.listener(|this, _: &Refresh, window, cx| {
                if this.dialog.is_none() {
                    this.reload(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| {
                if this.dialog.is_some() {
                    this.close_dialog(window, cx);
                } else {
                    this.revealed = None;
                    this.query.update(cx, |i, cx| i.set_text("", cx));
                    window.focus(&this.focus);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Submit, window, cx| this.submit(window, cx)))
            .on_action(
                cx.listener(|this, _: &NextField, window, cx| this.cycle_focus(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PreviousField, window, cx| {
                    this.cycle_focus(true, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &NextEntry, window, cx| this.move_selection(1, window, cx)),
            )
            .on_action(cx.listener(|this, _: &PreviousEntry, window, cx| {
                this.move_selection(-1, window, cx)
            }))
            .on_action(cx.listener(|this, _: &CopyEntry, window, cx| {
                if !this.locked && this.dialog.is_none() {
                    this.copy(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &EditEntry, window, cx| {
                if !this.locked && this.dialog.is_none() {
                    this.edit(window, cx);
                }
            }))
            .child(self.titlebar())
            .child(if self.locked {
                self.lock_screen(window, cx).into_any_element()
            } else {
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(self.toolbar(cx))
                            .when(self.error.is_some() && self.dialog.is_none(), |s| {
                                s.child(
                                    div()
                                        .px_5()
                                        .py_2()
                                        .text_size(px(11.))
                                        .text_color(self.p.danger)
                                        .child(self.error.clone().unwrap_or_default()),
                                )
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_1()
                                    .min_h_0()
                                    .child(self.entry_list(cx))
                                    .child(self.detail(cx)),
                            ),
                    )
                    .into_any_element()
            })
            .child(self.footer())
            .when(self.dialog.is_some(), |s| {
                s.child(self.dialog_view(window, cx))
            })
    }
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-f", Search, Some("Vault")),
        KeyBinding::new("cmd-k", CommandPalette, Some("Vault")),
        KeyBinding::new("cmd-n", NewEntry, Some("Vault")),
        KeyBinding::new("cmd-shift-n", NewGroup, Some("Vault")),
        KeyBinding::new("cmd-,", Settings, Some("Vault")),
        KeyBinding::new("cmd-shift-l", Lock, Some("Vault")),
        KeyBinding::new("cmd-r", Refresh, Some("Vault")),
        KeyBinding::new("escape", Dismiss, Some("Vault")),
        KeyBinding::new("enter", Submit, Some("Vault")),
        KeyBinding::new("tab", NextField, Some("Vault")),
        KeyBinding::new("shift-tab", PreviousField, Some("Vault")),
        KeyBinding::new("down", NextEntry, Some("Vault")),
        KeyBinding::new("up", PreviousEntry, Some("Vault")),
        KeyBinding::new("cmd-shift-c", CopyEntry, Some("Vault")),
        KeyBinding::new("cmd-e", EditEntry, Some("Vault")),
    ]);
}

fn demo_catalog() -> Vec<GroupView> {
    use zero_api_key_lib::service::EntryView;
    [
        (
            "openai",
            vec![
                ("API_KEY", "api-key", Some("OPENAI_API_KEY")),
                ("BASE_URL", "api-key", Some("OPENAI_BASE_URL")),
            ],
        ),
        (
            "anthropic",
            vec![("API_KEY", "api-key", Some("ANTHROPIC_API_KEY"))],
        ),
        (
            "github",
            vec![("PERSONAL_TOKEN", "api-key", Some("GITHUB_TOKEN"))],
        ),
        (
            "production",
            vec![
                ("DATABASE_URL", "login", Some("DATABASE_URL")),
                ("DEPLOY_KEY", "api-key", None),
            ],
        ),
        ("personal", vec![("RECOVERY_NOTE", "note", None)]),
    ]
    .into_iter()
    .map(|(name, entries)| GroupView {
        name: name.into(),
        entries: entries
            .into_iter()
            .map(|(key, kind, env)| EntryView {
                key: key.into(),
                entry_type: kind.into(),
                env_name: env.map(String::from),
                masked_value: "••••••••".into(),
                comment: None,
            })
            .collect(),
    })
    .collect()
}
