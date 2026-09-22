import { invoke } from "@tauri-apps/api/core";
import type {
  Backend,
  CommitEntry,
  EntryInput,
  EntryView,
  ImportPreviewItem,
  StatusInfo,
  SyncStats,
} from "./types";

function assertDesktopRuntime() {
  if (!("__TAURI_INTERNALS__" in window)) {
    throw new Error("本地后端不可用。请通过 ZeroApiKey 桌面应用打开，而不是浏览器。");
  }
}

export async function getStatus(): Promise<StatusInfo> {
  assertDesktopRuntime();
  return invoke<StatusInfo>("get_status");
}

export async function setDefaultMode(mode: Backend): Promise<void> {
  assertDesktopRuntime();
  return invoke<void>("set_default_mode", { mode });
}

export async function setTouchIdEnabled(enabled: boolean): Promise<void> {
  assertDesktopRuntime();
  return invoke<void>("set_touch_id_enabled", { enabled });
}

export async function verifyUiPassword(password: string): Promise<boolean> {
  assertDesktopRuntime();
  return invoke<boolean>("verify_ui_password", { password });
}

export async function unlockTouchId(): Promise<void> {
  assertDesktopRuntime();
  return invoke<void>("unlock_touch_id");
}

export async function setUiPassword(
  current: string | null,
  newPassword: string | null,
): Promise<void> {
  assertDesktopRuntime();
  return invoke<void>("set_ui_password", { current, new: newPassword });
}

export async function listGroups(mode: Backend): Promise<string[]> {
  assertDesktopRuntime();
  return invoke<string[]>("list_groups", { mode });
}

export async function listEntries(mode: Backend, group: string): Promise<EntryView[]> {
  assertDesktopRuntime();
  return invoke<EntryView[]>("list_entries", { mode, group });
}

export async function revealEntry(mode: Backend, group: string, key: string): Promise<string> {
  assertDesktopRuntime();
  return invoke<string>("reveal_entry", { mode, group, key });
}

export async function addEntry(mode: Backend, group: string, entry: EntryInput): Promise<void> {
  assertDesktopRuntime();
  return invoke<void>("add_entry", { mode, group, entry });
}

export async function renameEntry(
  mode: Backend,
  group: string,
  oldKey: string,
  entry: EntryInput,
): Promise<void> {
  assertDesktopRuntime();
  return invoke<void>("rename_entry", { mode, group, oldKey, entry });
}

export async function removeEntry(mode: Backend, group: string, key: string): Promise<void> {
  assertDesktopRuntime();
  return invoke<void>("remove_entry", { mode, group, key });
}

export async function removeGroup(mode: Backend, group: string): Promise<void> {
  assertDesktopRuntime();
  return invoke<void>("remove_group", { mode, group });
}

export async function exportGroup(mode: Backend, group: string): Promise<string> {
  assertDesktopRuntime();
  return invoke<string>("export_group", { mode, group });
}

export async function sync(direction: string): Promise<SyncStats> {
  assertDesktopRuntime();
  return invoke<SyncStats>("sync", { direction, force: null });
}

export async function importIni(mode: Backend, path: string): Promise<ImportPreviewItem[]> {
  assertDesktopRuntime();
  return invoke<ImportPreviewItem[]>("import_ini", { mode, path });
}

export async function importCommit(mode: Backend, entries: CommitEntry[]): Promise<number> {
  assertDesktopRuntime();
  return invoke<number>("import_commit", { mode, entries });
}

export async function initCloud(
  apiBase: string,
  projectId: string,
  environment: string,
  clientId: string,
  clientSecret: string,
): Promise<void> {
  assertDesktopRuntime();
  return invoke<void>("init_cloud", {
    apiBase,
    projectId,
    environment,
    clientId,
    clientSecret,
  });
}

export async function createGroup(mode: Backend, group: string): Promise<void> {
  assertDesktopRuntime();
  return invoke<void>("new_group", { mode, group });
}
