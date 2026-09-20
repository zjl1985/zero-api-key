import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import * as api from "./api";
import type {
  Backend,
  EntryView,
  ImportPreviewItem,
  StatusInfo,
  SyncStats,
} from "./types";
import {
  BackIcon,
  CopyIcon,
  ExportIcon,
  EyeIcon,
  EyeOffIcon,
  ImportIcon,
  PlusIcon,
  SettingsIcon,
  SyncIcon,
  TrashIcon,
} from "./components/Icons";
import "./App.css";

const TYPE_LABELS: Record<string, string> = {
  "api-key": "API Key",
  login: "登录",
  note: "笔记",
};

const REVEAL_MS = 5000;

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    document.body.appendChild(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }
}

export default function App() {
  const [status, setStatus] = useState<StatusInfo | null>(null);
  const [mode, setMode] = useState<Backend | null>(null);
  const [groups, setGroups] = useState<string[]>([]);
  const [group, setGroup] = useState<string | null>(null);
  const [entries, setEntries] = useState<EntryView[]>([]);
  const [revealed, setRevealed] = useState<Record<string, string>>({});
  const [toast, setToast] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<"vault" | "settings">("vault");
  const [dialog, setDialog] = useState<"add" | "import" | "sync" | null>(null);
  const [busy, setBusy] = useState(false);
  const revealTimers = useRef<Record<string, number>>({});
  const toastTimer = useRef<number | undefined>(undefined);

  const showToast = useCallback((text: string) => {
    setToast(text);
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 3000);
  }, []);

  const refreshStatus = useCallback(async () => {
    try {
      const s = await api.getStatus();
      setStatus(s);
      setMode((current) => {
        if (current) return current;
        if (s.defaultMode) return s.defaultMode;
        return s.localReady ? "local" : "cloud";
      });
    } catch (e) {
      setError(errText(e));
    }
  }, []);

  useEffect(() => {
    refreshStatus();
  }, [refreshStatus]);

  const reloadGroups = useCallback(
    async (m: Backend) => {
      try {
        const list = await api.listGroups(m);
        setGroups(list);
        setGroup((current) => (current && list.includes(current) ? current : null));
        setError(null);
      } catch (e) {
        setGroups([]);
        setError(errText(e));
      }
    },
    [],
  );

  useEffect(() => {
    if (mode) reloadGroups(mode);
  }, [mode, reloadGroups]);

  const reloadEntries = useCallback(async (m: Backend, g: string) => {
    try {
      setEntries(await api.listEntries(m, g));
      setError(null);
    } catch (e) {
      setEntries([]);
      setError(errText(e));
    }
  }, []);

  useEffect(() => {
    setRevealed({});
    if (mode && group) reloadEntries(mode, group);
    else setEntries([]);
  }, [mode, group, reloadEntries]);

  const ready = (b: Backend) => (b === "local" ? status?.localReady : status?.cloudReady);

  const switchMode = (m: Backend) => {
    setGroup(null);
    setMode(m);
  };

  const reveal = async (key: string) => {
    if (!mode || !group) return;
    if (revealed[key]) {
      window.clearTimeout(revealTimers.current[key]);
      setRevealed((r) => {
        const next = { ...r };
        delete next[key];
        return next;
      });
      return;
    }
    try {
      const value = await api.revealEntry(mode, group, key);
      setRevealed((r) => ({ ...r, [key]: value }));
      revealTimers.current[key] = window.setTimeout(() => {
        setRevealed((r) => {
          const next = { ...r };
          delete next[key];
          return next;
        });
      }, REVEAL_MS);
    } catch (e) {
      setError(errText(e));
    }
  };

  const copyEntry = async (key: string) => {
    if (!mode || !group) return;
    try {
      const value = revealed[key] ?? (await api.revealEntry(mode, group, key));
      await copyText(value);
      showToast(`已复制 ${key}（剪贴板不会自动清除）`);
    } catch (e) {
      setError(errText(e));
    }
  };

  const removeEntry = async (key: string) => {
    if (!mode || !group) return;
    if (!window.confirm(`删除 ${group}/${key}？`)) return;
    try {
      await api.removeEntry(mode, group, key);
      await reloadEntries(mode, group);
      if (!(await api.listEntries(mode, group)).length) reloadGroups(mode);
      showToast(`已删除 ${key}`);
    } catch (e) {
      setError(errText(e));
    }
  };

  const removeGroup = async () => {
    if (!mode || !group) return;
    if (!window.confirm(`删除整个分组 ${group}（含全部条目）？`)) return;
    try {
      await api.removeGroup(mode, group);
      setGroup(null);
      await reloadGroups(mode);
      showToast(`已删除分组 ${group}`);
    } catch (e) {
      setError(errText(e));
    }
  };

  const exportGroup = async () => {
    if (!mode || !group) return;
    try {
      const text = await api.exportGroup(mode, group);
      await copyText(text);
      showToast("export 语句已复制到剪贴板");
    } catch (e) {
      setError(errText(e));
    }
  };

  const newGroup = async () => {
    if (!mode) return;
    const name = window.prompt("新分组名（如 openai）");
    if (!name?.trim()) return;
    try {
      await api.createGroup(mode, name.trim());
      await reloadGroups(mode);
      setGroup(name.trim());
    } catch (e) {
      setError(errText(e));
    }
  };

  if (view === "settings") {
    return (
      <div class="app">
        <SettingsView
          status={status}
          onBack={() => {
            setView("vault");
            refreshStatus();
          }}
          onError={setError}
          onToast={showToast}
        />
        {toast && <div class="toast">{toast}</div>}
      </div>
    );
  }

  return (
    <div class="app">
      <header class="topbar">
        <span class="brand">ZeroApiKey</span>
        <div class="backend-switch">
          {(["local", "cloud"] as Backend[]).map((b) => (
            <button
              key={b}
              class={`backend-btn ${mode === b ? "active" : ""}`}
              onClick={() => switchMode(b)}
            >
              <span class={`dot ${ready(b) ? "ok" : "off"}`} />
              {b === "local" ? "本地" : "云端"}
            </button>
          ))}
        </div>
        <button class="icon-btn" title="设置" onClick={() => setView("settings")}>
          <SettingsIcon />
        </button>
      </header>

      {error && (
        <div class="error-banner" onClick={() => setError(null)}>
          {error}
        </div>
      )}

      <div class="body">
        <aside class="sidebar">
          <div class="sidebar-head">
            <span>分组</span>
            <button class="icon-btn" title="新建分组" onClick={newGroup}>
              <PlusIcon />
            </button>
          </div>
          {groups.length === 0 && <div class="sidebar-empty">暂无分组</div>}
          {groups.map((g) => (
            <button
              key={g}
              class={`group-item ${group === g ? "active" : ""}`}
              onClick={() => setGroup(g)}
            >
              {g}
            </button>
          ))}
        </aside>

        <main class="main">
          {group === null ? (
            <div class="placeholder">选择左侧分组，或新建一个分组</div>
          ) : (
            <>
              <div class="toolbar">
                <span class="toolbar-title">{group}</span>
                <button class="btn" onClick={() => setDialog("add")}>
                  <PlusIcon size={14} /> 添加条目
                </button>
                <button class="btn" onClick={exportGroup}>
                  <ExportIcon size={14} /> 导出
                </button>
                <button class="btn" onClick={() => setDialog("sync")}>
                  <SyncIcon size={14} /> 同步
                </button>
                <button class="btn" onClick={() => setDialog("import")}>
                  <ImportIcon size={14} /> 导入 ini
                </button>
                <button class="btn danger" onClick={removeGroup}>
                  <TrashIcon size={14} /> 删除分组
                </button>
              </div>

              {entries.length === 0 ? (
                <div class="placeholder">该分组下没有条目</div>
              ) : (
                <table class="entries">
                  <thead>
                    <tr>
                      <th>类型</th>
                      <th>键名</th>
                      <th>值</th>
                      <th>环境变量</th>
                      <th class="actions-col">操作</th>
                    </tr>
                  </thead>
                  <tbody>
                    {entries.map((entry) => (
                      <tr key={entry.key}>
                        <td>
                          <span class={`badge badge-${entry.entryType}`}>
                            {TYPE_LABELS[entry.entryType] ?? entry.entryType}
                          </span>
                        </td>
                        <td class="mono">{entry.key}</td>
                        <td class="mono value-cell">
                          {revealed[entry.key] ?? entry.maskedValue}
                        </td>
                        <td class="mono dim">{entry.envName ?? "—"}</td>
                        <td class="actions-cell">
                          <button
                            class="icon-btn"
                            title={revealed[entry.key] ? "重新掩码" : "显示 5 秒"}
                            onClick={() => reveal(entry.key)}
                          >
                            {revealed[entry.key] ? <EyeOffIcon /> : <EyeIcon />}
                          </button>
                          <button
                            class="icon-btn"
                            title="复制明文"
                            onClick={() => copyEntry(entry.key)}
                          >
                            <CopyIcon />
                          </button>
                          <button
                            class="icon-btn danger"
                            title="删除"
                            onClick={() => removeEntry(entry.key)}
                          >
                            <TrashIcon />
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </>
          )}
        </main>
      </div>

      {dialog === "add" && mode && group && (
        <AddEntryDialog
          busy={busy}
          onClose={() => setDialog(null)}
          onSubmit={async (input) => {
            setBusy(true);
            try {
              await api.addEntry(mode, group, input);
              setDialog(null);
              await reloadEntries(mode, group);
              showToast(`已写入 ${input.key}`);
            } catch (e) {
              setError(errText(e));
            } finally {
              setBusy(false);
            }
          }}
        />
      )}
      {dialog === "import" && mode && (
        <ImportDialog
          onClose={() => setDialog(null)}
          onDone={async (count) => {
            setDialog(null);
            await reloadGroups(mode);
            if (group) await reloadEntries(mode, group);
            showToast(`导入完成，写入 ${count} 条`);
          }}
          mode={mode}
          onError={setError}
        />
      )}
      {dialog === "sync" && (
        <SyncDialog onClose={() => setDialog(null)} onError={setError} />
      )}
      {toast && <div class="toast">{toast}</div>}
    </div>
  );
}

function AddEntryDialog(props: {
  busy: boolean;
  onClose: () => void;
  onSubmit: (input: {
    key: string;
    value: string;
    entryType: string;
    envName: string | null;
  }) => void;
}) {
  const [entryType, setEntryType] = useState("api-key");
  const [key, setKey] = useState("API_KEY");
  const [value, setValue] = useState("");
  const [envName, setEnvName] = useState("");

  const changeType = (t: string) => {
    setEntryType(t);
    setKey(t === "login" ? "PASSWORD" : t === "note" ? "NOTE" : "API_KEY");
  };

  return (
    <div class="backdrop" onClick={props.onClose}>
      <div class="dialog" onClick={(e) => e.stopPropagation()}>
        <h2>添加条目</h2>
        <label class="field">
          <span>类型</span>
          <select value={entryType} onChange={(e) => changeType((e.target as HTMLSelectElement).value)}>
            <option value="api-key">API Key</option>
            <option value="login">登录</option>
            <option value="note">笔记</option>
          </select>
        </label>
        <label class="field">
          <span>键名（自动转为 UPPER_SNAKE）</span>
          <input value={key} onInput={(e) => setKey((e.target as HTMLInputElement).value)} />
        </label>
        <label class="field">
          <span>值</span>
          <input
            type="password"
            value={value}
            onInput={(e) => setValue((e.target as HTMLInputElement).value)}
          />
        </label>
        {entryType === "api-key" && (
          <label class="field">
            <span>环境变量名（可空）</span>
            <input
              value={envName}
              placeholder="如 OPENAI_API_KEY"
              onInput={(e) => setEnvName((e.target as HTMLInputElement).value)}
            />
          </label>
        )}
        <div class="dialog-actions">
          <button class="btn" onClick={props.onClose}>
            取消
          </button>
          <button
            class="btn primary"
            disabled={props.busy || !key.trim() || !value}
            onClick={() =>
              props.onSubmit({
                key: key.trim(),
                value,
                entryType,
                envName: envName.trim() || null,
              })
            }
          >
            保存
          </button>
        </div>
      </div>
    </div>
  );
}

function ImportDialog(props: {
  mode: Backend;
  onClose: () => void;
  onDone: (count: number) => void;
  onError: (msg: string) => void;
}) {
  const [path, setPath] = useState("");
  const [items, setItems] = useState<ImportPreviewItem[] | null>(null);
  const [checked, setChecked] = useState<Record<number, boolean>>({});
  const [busy, setBusy] = useState(false);

  const parse = async () => {
    setBusy(true);
    try {
      const parsed = await api.importIni(props.mode, path.trim());
      setItems(parsed);
      setChecked(Object.fromEntries(parsed.map((_, i) => [i, true])));
    } catch (e) {
      props.onError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const commit = async () => {
    if (!items) return;
    const selected = items
      .filter((_, i) => checked[i])
      .map((item) => ({
        group: item.group ?? "misc",
        key: item.key,
        value: item.value,
        entryType: item.entryType,
      }));
    if (!selected.length) return;
    setBusy(true);
    try {
      const count = await api.importCommit(props.mode, selected);
      props.onDone(count);
    } catch (e) {
      props.onError(errText(e));
      setBusy(false);
    }
  };

  const selectedCount = Object.values(checked).filter(Boolean).length;

  return (
    <div class="backdrop" onClick={props.onClose}>
      <div class="dialog wide" onClick={(e) => e.stopPropagation()}>
        <h2>导入 ini 文件</h2>
        <div class="import-path">
          <input
            value={path}
            placeholder="文件绝对路径，如 /Users/you/Documents/api_key.ini"
            onInput={(e) => setPath((e.target as HTMLInputElement).value)}
          />
          <button class="btn primary" disabled={busy || !path.trim()} onClick={parse}>
            解析
          </button>
        </div>
        {items && (
          <>
            {items.length === 0 ? (
              <div class="placeholder">未解析到任何条目</div>
            ) : (
              <>
                <div class="import-meta">
                  解析到 {items.length} 条，勾选 {selectedCount} 条
                  <button
                    class="btn small"
                    onClick={() =>
                      setChecked(
                        Object.fromEntries(
                          items.map((_, i) => [i, selectedCount < items.length]),
                        ),
                      )
                    }
                  >
                    {selectedCount < items.length ? "全选" : "全不选"}
                  </button>
                </div>
                <div class="import-list">
                  {items.map((item, i) => (
                    <label class="import-row" key={i}>
                      <input
                        type="checkbox"
                        checked={!!checked[i]}
                        onChange={(e) =>
                          setChecked((c) => ({
                            ...c,
                            [i]: (e.target as HTMLInputElement).checked,
                          }))
                        }
                      />
                      <span class="dim">{item.group ?? "(未分组)"}</span>
                      <span class={`badge badge-${item.entryType}`}>
                        {TYPE_LABELS[item.entryType] ?? item.entryType}
                      </span>
                      <span class="mono">{item.key}</span>
                      <span class="mono dim">{item.maskedValue}</span>
                    </label>
                  ))}
                </div>
              </>
            )}
          </>
        )}
        <div class="dialog-actions">
          <button class="btn" onClick={props.onClose}>
            取消
          </button>
          <button
            class="btn primary"
            disabled={busy || !items || selectedCount === 0}
            onClick={commit}
          >
            写入 {selectedCount} 条
          </button>
        </div>
      </div>
    </div>
  );
}

function SyncDialog(props: {
  onClose: () => void;
  onError: (msg: string) => void;
}) {
  const [direction, setDirection] = useState("local-to-cloud");
  const [result, setResult] = useState<SyncStats | null>(null);
  const [busy, setBusy] = useState(false);

  const run = async () => {
    setBusy(true);
    try {
      setResult(await api.sync(direction));
    } catch (e) {
      props.onError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="backdrop" onClick={props.onClose}>
      <div class="dialog" onClick={(e) => e.stopPropagation()}>
        <h2>同步</h2>
        <label class="radio-row">
          <input
            type="radio"
            name="direction"
            checked={direction === "local-to-cloud"}
            onChange={() => setDirection("local-to-cloud")}
          />
          本地 → 云端
        </label>
        <label class="radio-row">
          <input
            type="radio"
            name="direction"
            checked={direction === "cloud-to-local"}
            onChange={() => setDirection("cloud-to-local")}
          />
          云端 → 本地
        </label>
        <p class="dim">冲突策略：保留源（覆盖目标）</p>
        {result && (
          <p class="sync-result">
            完成：新增 {result.added}，更新 {result.updated}，跳过 {result.skipped}
          </p>
        )}
        <div class="dialog-actions">
          <button class="btn" onClick={props.onClose}>
            关闭
          </button>
          <button class="btn primary" disabled={busy} onClick={run}>
            {busy ? "同步中…" : "开始同步"}
          </button>
        </div>
      </div>
    </div>
  );
}

function SettingsView(props: {
  status: StatusInfo | null;
  onBack: () => void;
  onError: (msg: string) => void;
  onToast: (msg: string) => void;
}) {
  const [apiBase, setApiBase] = useState("https://app.infisical.com");
  const [projectId, setProjectId] = useState("");
  const [environment, setEnvironment] = useState("dev");
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [busy, setBusy] = useState(false);

  const changeDefault = async (m: Backend) => {
    try {
      await api.setDefaultMode(m);
      props.onToast(`默认后端已切换为 ${m === "local" ? "本地" : "云端"}`);
    } catch (e) {
      props.onError(errText(e));
    }
  };

  const initCloud = async () => {
    setBusy(true);
    try {
      await api.initCloud(apiBase.trim(), projectId.trim(), environment.trim(), clientId.trim(), clientSecret);
      props.onToast("cloud 后端已配置并验证连通");
      setClientSecret("");
    } catch (e) {
      props.onError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="settings">
      <div class="settings-head">
        <button class="icon-btn" title="返回" onClick={props.onBack}>
          <BackIcon />
        </button>
        <h1>设置</h1>
      </div>

      <section class="settings-section">
        <h2>默认后端</h2>
        <label class="radio-row">
          <input
            type="radio"
            name="default-mode"
            checked={props.status?.defaultMode === "local"}
            onChange={() => changeDefault("local")}
          />
          本地（local）
        </label>
        <label class="radio-row">
          <input
            type="radio"
            name="default-mode"
            checked={props.status?.defaultMode === "cloud"}
            onChange={() => changeDefault("cloud")}
          />
          云端（cloud）
        </label>
        <p class="dim">
          本地保险库主密钥在首次使用时自动生成并存入系统钥匙串，无需手动配置。
        </p>
      </section>

      <section class="settings-section">
        <h2>
          云端（Infisical）
          {props.status?.cloudReady ? (
            <span class="badge badge-api-key">已配置</span>
          ) : (
            <span class="badge badge-note">未配置</span>
          )}
        </h2>
        <label class="field">
          <span>API base</span>
          <input value={apiBase} onInput={(e) => setApiBase((e.target as HTMLInputElement).value)} />
        </label>
        <label class="field">
          <span>Project ID</span>
          <input value={projectId} onInput={(e) => setProjectId((e.target as HTMLInputElement).value)} />
        </label>
        <label class="field">
          <span>Environment</span>
          <input value={environment} onInput={(e) => setEnvironment((e.target as HTMLInputElement).value)} />
        </label>
        <label class="field">
          <span>Client ID</span>
          <input value={clientId} onInput={(e) => setClientId((e.target as HTMLInputElement).value)} />
        </label>
        <label class="field">
          <span>Client Secret</span>
          <input
            type="password"
            value={clientSecret}
            onInput={(e) => setClientSecret((e.target as HTMLInputElement).value)}
          />
        </label>
        <button
          class="btn primary"
          disabled={busy || !projectId.trim() || !clientId.trim() || !clientSecret}
          onClick={initCloud}
        >
          {busy ? "验证中…" : "验证并保存"}
        </button>
      </section>
    </div>
  );
}
