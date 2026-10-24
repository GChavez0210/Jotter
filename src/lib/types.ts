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

export type LeftSidebarView = 'files' | 'search' | 'tags' | 'calendar' | 'trash' | 'conflicts';

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
