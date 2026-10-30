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

export type ThemePreference = 'system' | 'light' | 'dark' | 'high-contrast';

export type EditingMode = 'live' | 'source';

export type FocusMode = 'off' | 'minimal' | 'zen';

/** What the second editor pane holds: nothing, another note, or the open note rendered. */
export type SplitMode = 'off' | 'note' | 'preview';

/** Which of the two editor panes takes the next note the user opens. */
export type PaneId = 'primary' | 'secondary';

/** What the window's close button does. */
export type CloseAction = 'quit' | 'tray';

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

export type LeftSidebarView = 'files' | 'search' | 'tags' | 'conflicts' | 'health';

export interface TextEdit {
  from: number;
  to: number;
  /** The text expected at `from..to`; the edit is skipped when the note no longer has it. */
  expected: string;
  insert: string;
}

export interface NoteEdits {
  /** The note's path after the rename. */
  path: string;
  edits: TextEdit[];
}

export interface LinkUpdatePlan {
  notes: NoteEdits[];
  linkCount: number;
}

/** The scheduler's view of the open vault's backups. */
export interface BackupStatus {
  lastBackupMs: number | null;
  lastError: string | null;
}

export interface BackupConfig {
  vault: string;
  folder: string;
  intervalHours: number;
  keep: number;
}

/** What one CalDAV push changed on the server. */
export interface CaldavReport {
  createdOrUpdated: number;
  deleted: number;
  failed: number;
  firstError: string | null;
}

export interface AppliedEdits {
  applied: number;
  skipped: number;
  reverse: TextEdit[];
}

export interface BrokenLink {
  sourcePath: string;
  lineNumber: number;
  target: string;
  kind: 'wiki' | 'markdown';
  lineContext: string;
}

export interface HealthList<T> {
  /** The true count; `items` stops at a fixed limit. */
  total: number;
  items: T[];
}

export interface VaultHealthReport {
  brokenLinks: HealthList<BrokenLink>;
  unusedAttachments: HealthList<string>;
  emptyNotes: HealthList<string>;
  duplicateTitles: HealthList<{ title: string; paths: string[] }>;
}
export type RightPanelTab = 'note' | 'calendar' | 'trash';

/** One stored snapshot of a note, as the version history lists them, newest first. */
export interface NoteVersion {
  /** The time the snapshot was taken, in milliseconds, which also names its file. */
  id: string;
  savedAtMs: number;
  bytes: number;
}

/** A vault note or folder in the OS trash. */
export interface DeletedEntry {
  id: string;
  name: string;
  originalRelPath: string;
  deletedMs: number;
  isFolder: boolean;
}

export interface CssSnippet {
  name: string;
  css: string;
}

/** A Hunspell dictionary Jotter can check spelling with. */
export interface SpellDictionary {
  /** A language tag such as `en-US`, or the file name of a user dictionary. */
  id: string;
  name: string;
  /** Ships with Jotter, as opposed to a file the user added. */
  bundled: boolean;
}


/** A checklist item with a due date. */
export interface TaskEntry {
  path: string;
  title: string;
  lineNumber: number;
  text: string;
  done: boolean;
  /** `YYYY-MM-DD`. */
  dueDate: string;
  /** `HH:MM`, when the task has a time. */
  dueTime: string | null;
}

/** One note's metadata, tags, and links, for the Home page and the tags-and-links view. */
export interface NoteCard {
  path: string;
  title: string;
  modifiedMs: number;
  createdMs: number;
  preview: string;
  tags: string[];
  /** Vault paths of the notes this note links to. */
  linksTo: string[];
  /** Number of other notes that link to this note. */
  linkedFrom: number;
}

export type AiProvider = 'openai-compatible' | 'anthropic' | 'ollama';

export interface AiEndpoint {
  baseUrl: string;
  model: string;
}

/** Bring-your-own AI settings, stored in Jotter's app data folder. Never contains keys. */
export interface AiConfig {
  enabled: boolean;
  provider: AiProvider;
  openaiCompatible: AiEndpoint;
  anthropic: AiEndpoint;
  ollama: AiEndpoint;
  /** The user's writing style for Polish, the other rewrites, and summaries. */
  styleInstructions: string;
}

/** `endpointChanged`: a key is saved, but for a different address than the current base URL. */
export type AiKeyState = 'none' | 'saved' | 'endpointChanged';

export interface AiKeyStatus {
  provider: AiProvider;
  state: AiKeyState;
  origin: string | null;
}

export interface AiStatus {
  config: AiConfig;
  keys: AiKeyStatus[];
  /** Where keys are kept, such as "Windows Credential Manager". */
  credentialStore: string;
  defaultStyleInstructions: string;
}

export type RewriteStyle = 'polish' | 'shorter' | 'formal' | 'casual' | 'grammar';

export type AiOperation =
  | { kind: 'summarize' }
  | { kind: 'rewrite'; style: RewriteStyle }
  | { kind: 'translate'; targetLanguage: string };

export interface AiRequest {
  operationId: string;
  operation: AiOperation;
  text: string;
}

export interface AiResult {
  operationId: string;
  text: string;
  provider: AiProvider;
  model: string;
  usage: { inputTokens: number | null; outputTokens: number | null };
}

/** An update the release feed offers. `notes` is the release's Markdown. */
export interface UpdateInfo {
  version: string;
  currentVersion: string;
  notes: string | null;
  /** `YYYY-MM-DD`. */
  date: string | null;
  /** False when the install cannot replace itself, such as a .deb package. */
  canInstall: boolean;
}

export type UpdateProgress =
  | { phase: 'downloading'; downloaded: number; total: number | null }
  | { phase: 'installing' };
