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
  loadLocale,
  saveLocale,
  strings,
  type Locale,
  type Strings,
} from "./i18n";
import {
  CopyIcon,
  EditIcon,
  ExportIcon,
  EyeIcon,
  EyeOffIcon,
  FolderIcon,
  ImportIcon,
  LockIcon,
  PlusIcon,
  SettingsIcon,
  SyncIcon,
  TrashIcon,
} from "./components/Icons";
import "./App.css";

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

function typeLabel(t: Strings, entryType: string): string {
  if (entryType === "api-key") return "API Key";
  if (entryType === "login") return t.typeLogin;
  if (entryType === "note") return t.typeNote;
  return entryType;
}

function backendLabel(t: Strings, b: Backend): string {
  return b === "local" ? t.localBackend : t.cloudBackend;
}

export default function App() {
  const [locale, setLocaleState] = useState<Locale>(loadLocale());
  const t = strings[locale];
  const setLocale = (next: Locale) => {
    saveLocale(next);
    setLocaleState(next);
  };
  const [status, setStatus] = useState<StatusInfo | null>(null);
  const [mode, setMode] = useState<Backend | null>(null);
  const [groups, setGroups] = useState<string[]>([]);
  const [group, setGroup] = useState<string | null>(null);
  const [entries, setEntries] = useState<EntryView[]>([]);
  const [revealed, setRevealed] = useState<Record<string, string>>({});
  const [toast, setToast] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dialog, setDialog] = useState<
    "add" | "import" | "sync" | "settings" | "newGroup" | null
  >(null);
  const [confirmState, setConfirmState] = useState<{
    message: string;
    resolve: (ok: boolean) => void;
  } | null>(null);
  const [editTarget, setEditTarget] = useState<{
    key: string;
    value: string;
    entryType: string;
    envName: string | null;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const revealTimers = useRef<Record<string, number>>({});
  const toastTimer = useRef<number | undefined>(undefined);

  const showToast = useCallback((text: string) => {
    setToast(text);
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 3000);
  }, []);

  const askConfirm = useCallback(
    (message: string) =>
      new Promise<boolean>((resolve) => setConfirmState({ message, resolve })),
    [],
  );

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

  const reloadGroups = useCallback(async (m: Backend) => {
    try {
      const list = await api.listGroups(m);
      setGroups(list);
      setGroup((current) => (current && list.includes(current) ? current : null));
      setError(null);
    } catch (e) {
      setGroups([]);
      setError(errText(e));
    }
  }, []);

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
      showToast(t.copiedToast(key));
    } catch (e) {
      setError(errText(e));
    }
  };

  const openEdit = async (entry: EntryView) => {
    if (!mode || !group) return;
    try {
      const value = await api.revealEntry(mode, group, entry.key);
      setEditTarget({
        key: entry.key,
        value,
        entryType: entry.entryType,
        envName: entry.envName,
      });
      setDialog("add");
    } catch (e) {
      setError(errText(e));
    }
  };

  const removeEntry = async (key: string) => {
    if (!mode || !group) return;
    if (!(await askConfirm(t.deleteEntryConfirm(group, key)))) return;
    try {
      await api.removeEntry(mode, group, key);
      await reloadEntries(mode, group);
      if (!(await api.listEntries(mode, group)).length) reloadGroups(mode);
      showToast(t.deletedEntryToast(key));
    } catch (e) {
      setError(errText(e));
    }
  };

  const removeGroup = async () => {
    if (!mode || !group) return;
    if (!(await askConfirm(t.deleteGroupConfirm(group)))) return;
    try {
      await api.removeGroup(mode, group);
      setGroup(null);
      await reloadGroups(mode);
      showToast(t.deletedGroupToast(group));
    } catch (e) {
      setError(errText(e));
    }
  };

  const exportGroup = async () => {
    if (!mode || !group) return;
    try {
      const text = await api.exportGroup(mode, group);
      await copyText(text);
      showToast(t.exportCopiedToast);
    } catch (e) {
      setError(errText(e));
    }
  };

  return (
    <div class="app">
      <header class="topbar">
        <div class="brand">
          <LockIcon size={16} />
          <span>ZeroApiKey</span>
        </div>
        <div class="backend-switch">
          {(["local", "cloud"] as Backend[]).map((b) => (
            <button
              key={b}
              class={`backend-btn ${mode === b ? "active" : ""}`}
              onClick={() => switchMode(b)}
            >
              <span class={`dot ${ready(b) ? "ok" : "off"}`} />
              {backendLabel(t, b)}
            </button>
          ))}
        </div>
        <button class="icon-btn" title={t.settings} onClick={() => setDialog("settings")}>
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
            <span>{t.groups}</span>
            <button class="icon-btn" title={t.newGroup} onClick={() => setDialog("newGroup")}>
              <PlusIcon />
            </button>
          </div>
          {groups.length === 0 && (
            <div class="sidebar-empty">
              <p>{t.noGroups}</p>
              <p class="dim">{t.noGroupsHint}</p>
            </div>
          )}
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
            <div class="empty-state">
              <FolderIcon size={40} />
              <p class="empty-title">{t.noGroupSelected}</p>
              <p class="empty-hint">{t.noGroupSelectedHint}</p>
            </div>
          ) : (
            <>
              <div class="toolbar">
                <nav class="breadcrumb">
                  <span class="dim">{mode ? backendLabel(t, mode) : ""}</span>
                  <span class="crumb-sep">/</span>
                  <span>{group}</span>
                </nav>
                <button
                  class="btn"
                  onClick={() => {
                    setEditTarget(null);
                    setDialog("add");
                  }}
                >
                  <PlusIcon size={14} /> {t.addEntry}
                </button>
                <button class="btn" onClick={exportGroup}>
                  <ExportIcon size={14} /> {t.exportGroup}
                </button>
                <button class="btn" onClick={() => setDialog("sync")}>
                  <SyncIcon size={14} /> {t.sync}
                </button>
                <button class="btn" onClick={() => setDialog("import")}>
                  <ImportIcon size={14} /> {t.importIni}
                </button>
                <button class="btn danger" onClick={removeGroup}>
                  <TrashIcon size={14} /> {t.deleteGroup}
                </button>
              </div>

              {entries.length === 0 ? (
                <div class="empty-state">
                  <LockIcon size={40} />
                  <p class="empty-title">{t.noEntries}</p>
                  <p class="empty-hint">{t.noEntriesHint}</p>
                </div>
              ) : (
                <table class="entries">
                  <thead>
                    <tr>
                      <th>{t.colType}</th>
                      <th>{t.colKey}</th>
                      <th>{t.colValue}</th>
                      <th>{t.colEnv}</th>
                      <th class="actions-col">{t.colActions}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {entries.map((entry) => (
                      <tr key={entry.key}>
                        <td>
                          <span class={`badge badge-${entry.entryType}`}>
                            {typeLabel(t, entry.entryType)}
                          </span>
                        </td>
                        <td class="mono">{entry.key}</td>
                        <td class="value-cell">
                          {revealed[entry.key] ? (
                            <span class="mono revealed">{revealed[entry.key]}</span>
                          ) : (
                            <span class="dots">••••••••••</span>
                          )}
                        </td>
                        <td class="mono dim">{entry.envName ?? "—"}</td>
                        <td class="actions-cell">
                          <button
                            class="icon-btn"
                            title={revealed[entry.key] ? t.maskAgain : t.revealFor5s}
                            onClick={() => reveal(entry.key)}
                          >
                            {revealed[entry.key] ? <EyeOffIcon /> : <EyeIcon />}
                          </button>
                          <button
                            class="icon-btn"
                            title={t.edit}
                            onClick={() => openEdit(entry)}
                          >
                            <EditIcon />
                          </button>
                          <button
                            class="icon-btn"
                            title={t.copyPlaintext}
                            onClick={() => copyEntry(entry.key)}
                          >
                            <CopyIcon />
                          </button>
                          <button
                            class="icon-btn danger"
                            title={t.delete}
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
          t={t}
          busy={busy}
          initial={editTarget}
          onClose={() => setDialog(null)}
          onSubmit={async (input) => {
            setBusy(true);
            try {
              await api.addEntry(mode, group, input);
              setDialog(null);
              await reloadEntries(mode, group);
              showToast(t.savedToast(input.key));
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
          t={t}
          mode={mode}
          onClose={() => setDialog(null)}
          onDone={async (count) => {
            setDialog(null);
            await reloadGroups(mode);
            if (group) await reloadEntries(mode, group);
            showToast(t.importDoneToast(count));
          }}
          onError={setError}
        />
      )}
      {dialog === "sync" && (
        <SyncDialog t={t} onClose={() => setDialog(null)} onError={setError} />
      )}
      {dialog === "settings" && (
        <SettingsDialog
          t={t}
          locale={locale}
          onLocaleChange={setLocale}
          status={status}
          onClose={() => {
            setDialog(null);
            refreshStatus();
          }}
          onError={setError}
          onToast={showToast}
        />
      )}
      {dialog === "newGroup" && mode && (
        <NewGroupDialog
          t={t}
          onClose={() => setDialog(null)}
          onSubmit={async (name) => {
            try {
              await api.createGroup(mode, name);
              setDialog(null);
              await reloadGroups(mode);
              setGroup(name);
            } catch (e) {
              setError(errText(e));
            }
          }}
        />
      )}
      {confirmState && (
        <ConfirmDialog
          t={t}
          message={confirmState.message}
          onResult={(ok) => {
            confirmState.resolve(ok);
            setConfirmState(null);
          }}
        />
      )}
      {toast && <div class="toast">{toast}</div>}
    </div>
  );
}

function Modal(props: {
  title: string;
  wide?: boolean;
  onClose: () => void;
  children: preact.ComponentChildren;
}) {
  return (
    <div class="backdrop" onClick={props.onClose}>
      <div
        class={`dialog ${props.wide ? "wide" : ""}`}
        onClick={(e) => e.stopPropagation()}
      >
        <div class="dialog-head">
          <h2>{props.title}</h2>
        </div>
        {props.children}
      </div>
    </div>
  );
}

function ConfirmDialog(props: {
  t: Strings;
  message: string;
  onResult: (ok: boolean) => void;
}) {
  const { t } = props;
  return (
    <Modal title={t.confirmTitle} onClose={() => props.onResult(false)}>
      <p class="confirm-text">{props.message}</p>
      <div class="dialog-actions">
        <button class="btn" onClick={() => props.onResult(false)}>
          {t.cancel}
        </button>
        <button class="btn primary danger-solid" onClick={() => props.onResult(true)}>
          {t.confirmDelete}
        </button>
      </div>
    </Modal>
  );
}

function NewGroupDialog(props: {
  t: Strings;
  onClose: () => void;
  onSubmit: (name: string) => void;
}) {
  const { t } = props;
  const [name, setName] = useState("");
  return (
    <Modal title={t.newGroup} onClose={props.onClose}>
      <label class="field">
        <span>{t.groupName}</span>
        <input
          value={name}
          placeholder={t.groupNamePlaceholder}
          onInput={(e) => setName((e.target as HTMLInputElement).value)}
        />
      </label>
      <div class="dialog-actions">
        <button class="btn" onClick={props.onClose}>
          {t.cancel}
        </button>
        <button
          class="btn primary"
          disabled={!name.trim()}
          onClick={() => props.onSubmit(name.trim())}
        >
          {t.create}
        </button>
      </div>
    </Modal>
  );
}

function AddEntryDialog(props: {
  t: Strings;
  busy: boolean;
  initial?: {
    key: string;
    value: string;
    entryType: string;
    envName: string | null;
  } | null;
  onClose: () => void;
  onSubmit: (input: {
    key: string;
    value: string;
    entryType: string;
    envName: string | null;
  }) => void;
}) {
  const { t } = props;
  const editing = !!props.initial;
  const [entryType, setEntryType] = useState(props.initial?.entryType ?? "api-key");
  const [key, setKey] = useState(props.initial?.key ?? "API_KEY");
  const [value, setValue] = useState(props.initial?.value ?? "");
  const [envName, setEnvName] = useState(props.initial?.envName ?? "");

  const changeType = (type: string) => {
    setEntryType(type);
    if (!editing) setKey(type === "login" ? "PASSWORD" : type === "note" ? "NOTE" : "API_KEY");
  };

  return (
    <Modal title={editing ? t.editEntryTitle : t.addEntryTitle} onClose={props.onClose}>
      <label class="field">
        <span>{t.colType}</span>
        <select
          value={entryType}
          onChange={(e) => changeType((e.target as HTMLSelectElement).value)}
        >
          <option value="api-key">API Key</option>
          <option value="login">{t.typeLogin}</option>
          <option value="note">{t.typeNote}</option>
        </select>
      </label>
      <label class="field">
        <span>{t.keyLabel}</span>
        <input
          value={key}
          disabled={editing}
          onInput={(e) => setKey((e.target as HTMLInputElement).value)}
        />
      </label>
      <label class="field">
        <span>{t.valueLabel}</span>
        <input
          type={editing ? "text" : "password"}
          value={value}
          onInput={(e) => setValue((e.target as HTMLInputElement).value)}
        />
      </label>
      {entryType === "api-key" && (
        <label class="field">
          <span>{t.envLabel}</span>
          <input
            value={envName}
            placeholder={t.envPlaceholder}
            onInput={(e) => setEnvName((e.target as HTMLInputElement).value)}
          />
        </label>
      )}
      <div class="dialog-actions">
        <button class="btn" onClick={props.onClose}>
          {t.cancel}
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
          {t.save}
        </button>
      </div>
    </Modal>
  );
}

function ImportDialog(props: {
  t: Strings;
  mode: Backend;
  onClose: () => void;
  onDone: (count: number) => void;
  onError: (msg: string) => void;
}) {
  const { t } = props;
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
    <Modal title={t.importTitle} wide onClose={props.onClose}>
      <div class="import-path">
        <input
          value={path}
          placeholder={t.importPathPlaceholder}
          onInput={(e) => setPath((e.target as HTMLInputElement).value)}
        />
        <button class="btn primary" disabled={busy || !path.trim()} onClick={parse}>
          {t.parse}
        </button>
      </div>
      {items &&
        (items.length === 0 ? (
          <p class="dim">{t.noItemsParsed}</p>
        ) : (
          <>
            <div class="import-meta">
              <span>{t.parsedMeta(items.length, selectedCount)}</span>
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
                {selectedCount < items.length ? t.selectAll : t.selectNone}
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
                  <span class="dim">{item.group ?? t.ungrouped}</span>
                  <span class={`badge badge-${item.entryType}`}>
                    {typeLabel(t, item.entryType)}
                  </span>
                  <span class="mono">{item.key}</span>
                  <span class="dots">••••••••••</span>
                </label>
              ))}
            </div>
          </>
        ))}
      <div class="dialog-actions">
        <button class="btn" onClick={props.onClose}>
          {t.cancel}
        </button>
        <button
          class="btn primary"
          disabled={busy || !items || selectedCount === 0}
          onClick={commit}
        >
          {t.writeCount(selectedCount)}
        </button>
      </div>
    </Modal>
  );
}

function SyncDialog(props: {
  t: Strings;
  onClose: () => void;
  onError: (msg: string) => void;
}) {
  const { t } = props;
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
    <Modal title={t.syncTitle} onClose={props.onClose}>
      <label class="radio-row">
        <input
          type="radio"
          name="direction"
          checked={direction === "local-to-cloud"}
          onChange={() => setDirection("local-to-cloud")}
        />
        {t.localToCloud}
      </label>
      <label class="radio-row">
        <input
          type="radio"
          name="direction"
          checked={direction === "cloud-to-local"}
          onChange={() => setDirection("cloud-to-local")}
        />
        {t.cloudToLocal}
      </label>
      <p class="dim">{t.conflictPolicy}</p>
      {result && (
        <p class="sync-result">
          {t.syncResult(result.added, result.updated, result.skipped)}
        </p>
      )}
      <div class="dialog-actions">
        <button class="btn" onClick={props.onClose}>
          {t.close}
        </button>
        <button class="btn primary" disabled={busy} onClick={run}>
          {busy ? t.syncing : t.startSync}
        </button>
      </div>
    </Modal>
  );
}

function SettingsDialog(props: {
  t: Strings;
  locale: Locale;
  onLocaleChange: (locale: Locale) => void;
  status: StatusInfo | null;
  onClose: () => void;
  onError: (msg: string) => void;
  onToast: (msg: string) => void;
}) {
  const { t } = props;
  const [apiBase, setApiBase] = useState("https://app.infisical.com");
  const [projectId, setProjectId] = useState("");
  const [environment, setEnvironment] = useState("dev");
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [busy, setBusy] = useState(false);

  const changeDefault = async (m: Backend) => {
    try {
      await api.setDefaultMode(m);
      props.onToast(t.defaultModeToast(backendLabel(t, m)));
    } catch (e) {
      props.onError(errText(e));
    }
  };

  const initCloud = async () => {
    setBusy(true);
    try {
      await api.initCloud(
        apiBase.trim(),
        projectId.trim(),
        environment.trim(),
        clientId.trim(),
        clientSecret,
      );
      props.onToast(t.cloudConfiguredToast);
      setClientSecret("");
    } catch (e) {
      props.onError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={t.settingsTitle} onClose={props.onClose}>
      <section class="settings-section">
        <h3>{t.language}</h3>
        <label class="field">
          <select
            value={props.locale}
            onChange={(e) =>
              props.onLocaleChange((e.target as HTMLSelectElement).value as Locale)
            }
          >
            <option value="en">English</option>
            <option value="zh">中文</option>
          </select>
        </label>
      </section>

      <section class="settings-section">
        <h3>{t.defaultBackend}</h3>
        <label class="radio-row">
          <input
            type="radio"
            name="default-mode"
            checked={props.status?.defaultMode === "local"}
            onChange={() => changeDefault("local")}
          />
          {t.localBackend} (local)
        </label>
        <label class="radio-row">
          <input
            type="radio"
            name="default-mode"
            checked={props.status?.defaultMode === "cloud"}
            onChange={() => changeDefault("cloud")}
          />
          {t.cloudBackend} (cloud)
        </label>
        <p class="dim">{t.masterKeyNote}</p>
      </section>

      <section class="settings-section">
        <h3>
          {t.cloudInfisical}{" "}
          {props.status?.cloudReady ? (
            <span class="badge badge-api-key">{t.configured}</span>
          ) : (
            <span class="badge badge-note">{t.notConfigured}</span>
          )}
        </h3>
        <label class="field">
          <span>{t.apiBase}</span>
          <input
            value={apiBase}
            onInput={(e) => setApiBase((e.target as HTMLInputElement).value)}
          />
        </label>
        <label class="field">
          <span>{t.projectId}</span>
          <input
            value={projectId}
            onInput={(e) => setProjectId((e.target as HTMLInputElement).value)}
          />
        </label>
        <label class="field">
          <span>{t.environment}</span>
          <input
            value={environment}
            onInput={(e) => setEnvironment((e.target as HTMLInputElement).value)}
          />
        </label>
        <label class="field">
          <span>{t.clientId}</span>
          <input
            value={clientId}
            onInput={(e) => setClientId((e.target as HTMLInputElement).value)}
          />
        </label>
        <label class="field">
          <span>{t.clientSecret}</span>
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
          {busy ? t.verifying : t.verifySave}
        </button>
      </section>
    </Modal>
  );
}
