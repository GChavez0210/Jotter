/**
 * Shared IPC types mirroring `src-tauri/src/types.rs`.
 */

export type CopyMode = 'markdown_source' | 'plain' | 'rich';

export interface CopyRequest {
  text: string;
  mode: CopyMode;
}

export interface ClipboardPayload {
  plain: string;
  html: string;
  mode: CopyMode;
  lineCount: number;
  charCount: number;
}

export interface BackdropState {
  micaApplied: boolean;
  backdropKind: string;
  darkMode: boolean | null;
}

export type ThemePreference = 'system' | 'light' | 'dark';

export type EditingMode = 'live' | 'source';

export type FocusMode = 'off' | 'minimal' | 'zen';

export type VaultNodeKind = 'file' | 'folder';

export interface VaultNode {
  name: string;
  relPath: string;
  kind: VaultNodeKind;
  modifiedMs: number;
  createdMs: number;
  children: VaultNode[];
}

export interface VaultSnapshot {
  rootPath: string;
  name: string;
  tree: VaultNode[];
  noteCount: number;
}

export interface NoteDocument {
  relPath: string;
  title: string;
  content: string;
  modifiedMs: number;
  createdMs: number;
}

export interface RecentVault {
  path: string;
  name: string;
  lastOpenedMs: number;
}

export interface RenameEntryResult {
  snapshot: VaultSnapshot;
  oldRelPath: string;
  newRelPath: string;
}

export interface VaultFsEvent {
  changedPaths: string[];
}

export type CalendarDateField = 'modified' | 'created';

export interface CalendarDaySummary {
  date: string;
  count: number;
}

export interface CalendarNoteEntry {
  path: string;
  title: string;
  modifiedMs: number;
  createdMs: number;
  preview: string;
}

export interface BacklinkEntry {
  sourcePath: string;
  sourceTitle: string;
  target: string;
  heading: string | null;
  alias: string | null;
  lineNumber: number;
  lineContext: string;
}

export interface TagSummary {
  tag: string;
  count: number;
}

export interface SearchHit {
  path: string;
  title: string;
  snippetHtml: string;
  lineNumber: number;
  matchCount: number;
}

export type LeftSidebarView = 'files' | 'search' | 'tags' | 'calendar';

