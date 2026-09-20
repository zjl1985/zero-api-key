export type Backend = "local" | "cloud";

export interface StatusInfo {
  defaultMode: Backend | null;
  localReady: boolean;
  cloudReady: boolean;
  touchIdEnabled: boolean;
}

export interface EntryView {
  key: string;
  maskedValue: string;
  entryType: string;
  envName: string | null;
  comment: string | null;
}

export interface EntryInput {
  key: string;
  value: string;
  entryType: string;
  envName: string | null;
}

export interface CommitEntry {
  group: string;
  key: string;
  value: string;
  entryType: string;
}

export interface SyncStats {
  added: number;
  updated: number;
  skipped: number;
}

export interface ImportPreviewItem {
  group: string | null;
  key: string;
  entryType: string;
  maskedValue: string;
  value: string;
}
