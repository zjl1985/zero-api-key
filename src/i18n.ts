export type Locale = "en" | "zh";

export interface Strings {
  localBackend: string;
  cloudBackend: string;
  settings: string;
  groups: string;
  newGroup: string;
  noGroups: string;
  noGroupsHint: string;
  noGroupSelected: string;
  noGroupSelectedHint: string;
  addEntry: string;
  exportGroup: string;
  sync: string;
  importIni: string;
  deleteGroup: string;
  noEntries: string;
  noEntriesHint: string;
  colType: string;
  colKey: string;
  colValue: string;
  colEnv: string;
  colActions: string;
  maskAgain: string;
  revealFor5s: string;
  copyPlaintext: string;
  edit: string;
  delete: string;
  confirmTitle: string;
  cancel: string;
  confirmDelete: string;
  typeLogin: string;
  typeNote: string;
  groupName: string;
  groupNamePlaceholder: string;
  create: string;
  addEntryTitle: string;
  editEntryTitle: string;
  keyLabel: string;
  valueLabel: string;
  envLabel: string;
  envPlaceholder: string;
  save: string;
  importTitle: string;
  importPathPlaceholder: string;
  parse: string;
  noItemsParsed: string;
  parsedMeta: (parsed: number, checked: number) => string;
  selectAll: string;
  selectNone: string;
  ungrouped: string;
  writeCount: (count: number) => string;
  syncTitle: string;
  localToCloud: string;
  cloudToLocal: string;
  conflictPolicy: string;
  syncResult: (added: number, updated: number, skipped: number) => string;
  close: string;
  syncing: string;
  startSync: string;
  settingsTitle: string;
  language: string;
  defaultBackend: string;
  masterKeyNote: string;
  touchIdUnlock: string;
  touchIdRequire: string;
  touchIdNote: string;
  lockedTitle: string;
  unlockPasswordLabel: string;
  unlock: string;
  wrongPassword: string;
  unlockWithTouchId: string;
  unlockPasswordSection: string;
  currentPassword: string;
  newPassword: string;
  confirmPassword: string;
  passwordMismatch: string;
  passwordUpdatedToast: string;
  passwordClearedToast: string;
  clearPassword: string;
  passwordNote: string;
  cloudInfisical: string;
  configured: string;
  notConfigured: string;
  apiBase: string;
  projectId: string;
  environment: string;
  clientId: string;
  clientSecret: string;
  verifying: string;
  verifySave: string;
  copiedToast: (key: string) => string;
  deletedEntryToast: (key: string) => string;
  deletedGroupToast: (group: string) => string;
  exportCopiedToast: string;
  savedToast: (key: string) => string;
  importDoneToast: (count: number) => string;
  defaultModeToast: (label: string) => string;
  cloudConfiguredToast: string;
  deleteEntryConfirm: (group: string, key: string) => string;
  deleteGroupConfirm: (group: string) => string;
}

const en: Strings = {
  localBackend: "Local",
  cloudBackend: "Cloud",
  settings: "Settings",
  groups: "Groups",
  newGroup: "New group",
  noGroups: "No groups yet",
  noGroupsHint: "Click + above to create one",
  noGroupSelected: "No group selected",
  noGroupSelectedHint: "Select a group on the left to view its secrets",
  addEntry: "Add Entry",
  exportGroup: "Export",
  sync: "Sync",
  importIni: "Import INI",
  deleteGroup: "Delete Group",
  noEntries: "No entries in this group",
  noEntriesHint: 'Click "Add Entry" in the toolbar to store your first secret',
  colType: "Type",
  colKey: "Key",
  colValue: "Value",
  colEnv: "Env Var",
  colActions: "Actions",
  maskAgain: "Mask again",
  revealFor5s: "Reveal for 5s",
  copyPlaintext: "Copy plaintext",
  edit: "Edit",
  delete: "Delete",
  confirmTitle: "Confirm",
  cancel: "Cancel",
  confirmDelete: "Delete",
  typeLogin: "Login",
  typeNote: "Note",
  groupName: "Group name",
  groupNamePlaceholder: "e.g. openai",
  create: "Create",
  addEntryTitle: "Add Entry",
  editEntryTitle: "Edit Entry",
  keyLabel: "Key (auto-converted to UPPER_SNAKE)",
  valueLabel: "Value",
  envLabel: "Environment variable name (optional)",
  envPlaceholder: "e.g. OPENAI_API_KEY",
  save: "Save",
  importTitle: "Import INI File",
  importPathPlaceholder: "Absolute file path, e.g. /Users/you/Documents/api_key.ini",
  parse: "Parse",
  noItemsParsed: "No entries parsed",
  parsedMeta: (parsed, checked) => `Parsed ${parsed} entries, ${checked} selected`,
  selectAll: "Select all",
  selectNone: "Select none",
  ungrouped: "(ungrouped)",
  writeCount: (count) => `Write ${count} entries`,
  syncTitle: "Sync",
  localToCloud: "Local → Cloud",
  cloudToLocal: "Cloud → Local",
  conflictPolicy: "Conflict policy: keep source (overwrite target)",
  syncResult: (added, updated, skipped) =>
    `Done: ${added} added, ${updated} updated, ${skipped} skipped`,
  close: "Close",
  syncing: "Syncing…",
  startSync: "Start Sync",
  settingsTitle: "Settings",
  language: "Language",
  defaultBackend: "Default backend",
  masterKeyNote:
    "The local vault master key is generated on first use and stored in the system keychain.",
  touchIdUnlock: "Touch ID unlock",
  touchIdRequire: "Require Touch ID before reading the vault",
  touchIdNote:
    "The vault is always encrypted at rest. Touch ID only adds an unlock prompt on top; it is off by default.",
  lockedTitle: "ZeroApiKey is locked",
  unlockPasswordLabel: "Unlock password",
  unlock: "Unlock",
  wrongPassword: "Wrong password",
  unlockWithTouchId: "Unlock with Touch ID",
  unlockPasswordSection: "Unlock password",
  currentPassword: "Current password",
  newPassword: "New password",
  confirmPassword: "Confirm new password",
  passwordMismatch: "Passwords do not match",
  passwordUpdatedToast: "Unlock password updated",
  passwordClearedToast: "Unlock password cleared",
  clearPassword: "Clear password",
  passwordNote:
    "When set, the desktop app asks for this password on launch. The CLI never asks for anything.",
  cloudInfisical: "Cloud (Infisical)",
  configured: "Configured",
  notConfigured: "Not configured",
  apiBase: "API base",
  projectId: "Project ID",
  environment: "Environment",
  clientId: "Client ID",
  clientSecret: "Client Secret",
  verifying: "Verifying…",
  verifySave: "Verify & Save",
  copiedToast: (key) => `Copied ${key} (clipboard is not auto-cleared)`,
  deletedEntryToast: (key) => `Deleted ${key}`,
  deletedGroupToast: (group) => `Deleted group ${group}`,
  exportCopiedToast: "Export statements copied to clipboard",
  savedToast: (key) => `Saved ${key}`,
  importDoneToast: (count) => `Import complete, ${count} entries written`,
  defaultModeToast: (label) => `Default backend switched to ${label}`,
  cloudConfiguredToast: "Cloud backend configured and connectivity verified",
  deleteEntryConfirm: (group, key) => `Delete entry ${group} / ${key}?`,
  deleteGroupConfirm: (group) =>
    `Delete the entire group ${group} (including all entries)?`,
};

const zh: Strings = {
  localBackend: "本地",
  cloudBackend: "云端",
  settings: "设置",
  groups: "分组",
  newGroup: "新建分组",
  noGroups: "还没有分组",
  noGroupsHint: "点上方 + 新建",
  noGroupSelected: "未选择分组",
  noGroupSelectedHint: "从左侧选择一个分组查看其中的密钥条目",
  addEntry: "添加条目",
  exportGroup: "导出",
  sync: "同步",
  importIni: "导入 ini",
  deleteGroup: "删除分组",
  noEntries: "此分组还没有条目",
  noEntriesHint: "点击工具栏「添加条目」写入第一个密钥",
  colType: "类型",
  colKey: "键名",
  colValue: "值",
  colEnv: "环境变量",
  colActions: "操作",
  maskAgain: "重新掩码",
  revealFor5s: "显示 5 秒",
  copyPlaintext: "复制明文",
  edit: "编辑",
  delete: "删除",
  confirmTitle: "确认操作",
  cancel: "取消",
  confirmDelete: "确认删除",
  typeLogin: "登录",
  typeNote: "笔记",
  groupName: "分组名",
  groupNamePlaceholder: "如 openai",
  create: "创建",
  addEntryTitle: "添加条目",
  editEntryTitle: "编辑条目",
  keyLabel: "键名（自动转为 UPPER_SNAKE）",
  valueLabel: "值",
  envLabel: "环境变量名（可空）",
  envPlaceholder: "如 OPENAI_API_KEY",
  save: "保存",
  importTitle: "导入 ini 文件",
  importPathPlaceholder: "文件绝对路径，如 /Users/you/Documents/api_key.ini",
  parse: "解析",
  noItemsParsed: "未解析到任何条目",
  parsedMeta: (parsed, checked) => `解析到 ${parsed} 条，勾选 ${checked} 条`,
  selectAll: "全选",
  selectNone: "全不选",
  ungrouped: "(未分组)",
  writeCount: (count) => `写入 ${count} 条`,
  syncTitle: "同步",
  localToCloud: "本地 → 云端",
  cloudToLocal: "云端 → 本地",
  conflictPolicy: "冲突策略：保留源（覆盖目标）",
  syncResult: (added, updated, skipped) =>
    `完成：新增 ${added}，更新 ${updated}，跳过 ${skipped}`,
  close: "关闭",
  syncing: "同步中…",
  startSync: "开始同步",
  settingsTitle: "设置",
  language: "语言",
  defaultBackend: "默认后端",
  masterKeyNote: "本地保险库主密钥在首次使用时自动生成并存入系统钥匙串。",
  touchIdUnlock: "指纹解锁",
  touchIdRequire: "读取保险库前需要指纹验证",
  touchIdNote: "保险库始终以密文存储，指纹只是额外的解锁验证，默认关闭。",
  lockedTitle: "ZeroApiKey 已锁定",
  unlockPasswordLabel: "解锁密码",
  unlock: "解锁",
  wrongPassword: "密码错误",
  unlockWithTouchId: "使用指纹解锁",
  unlockPasswordSection: "解锁密码",
  currentPassword: "当前密码",
  newPassword: "新密码",
  confirmPassword: "确认新密码",
  passwordMismatch: "两次输入不一致",
  passwordUpdatedToast: "解锁密码已更新",
  passwordClearedToast: "解锁密码已清除",
  clearPassword: "清除密码",
  passwordNote: "设置后桌面应用启动时需要输入密码；CLI 始终不需要任何密码。",
  cloudInfisical: "云端（Infisical）",
  configured: "已配置",
  notConfigured: "未配置",
  apiBase: "API base",
  projectId: "Project ID",
  environment: "Environment",
  clientId: "Client ID",
  clientSecret: "Client Secret",
  verifying: "验证中…",
  verifySave: "验证并保存",
  copiedToast: (key) => `已复制 ${key}（剪贴板不会自动清除）`,
  deletedEntryToast: (key) => `已删除 ${key}`,
  deletedGroupToast: (group) => `已删除分组 ${group}`,
  exportCopiedToast: "export 语句已复制到剪贴板",
  savedToast: (key) => `已写入 ${key}`,
  importDoneToast: (count) => `导入完成，写入 ${count} 条`,
  defaultModeToast: (label) => `默认后端已切换为${label}`,
  cloudConfiguredToast: "cloud 后端已配置并验证连通",
  deleteEntryConfirm: (group, key) => `删除条目 ${group} / ${key}？`,
  deleteGroupConfirm: (group) => `删除整个分组 ${group}（含全部条目）？`,
};

export const strings: Record<Locale, Strings> = { en, zh };

const LOCALE_KEY = "zero-api-key:locale";

export function loadLocale(): Locale {
  return localStorage.getItem(LOCALE_KEY) === "zh" ? "zh" : "en";
}

export function saveLocale(locale: Locale): void {
  localStorage.setItem(LOCALE_KEY, locale);
}
