import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AiConfig,
  AiProvider,
  AiRequest,
  AiResult,
  AiStatus,
  BacklinkEntry,
  CalendarDateField,
  CalendarDaySummary,
  CalendarNoteEntry,
  ClipboardPayload,
  CopyRequest,
  CssSnippet,
  DeletedEntry,
  NoteCard,
  NoteDocument,
  RecentVault,
  RenameEntryResult,
  SearchHit,
  SpellDictionary,
  TagSummary,
  TaskEntry,
  VaultFsEvent,
  VaultSnapshot,
} from './types';

/** Native save-dialog title and file-type filter name, in the UI language. */
export interface DialogLabels {
  title: string;
  filter: string;
}

export async function copyToClipboard(req: CopyRequest): Promise<ClipboardPayload> {
  return invoke<ClipboardPayload>('copy_to_clipboard', { req });
}

export async function previewClipboardPayload(req: CopyRequest): Promise<ClipboardPayload> {
  return invoke<ClipboardPayload>('preview_clipboard_payload', { req });
}

export async function pastePlainFromClipboard(): Promise<string> {
  return invoke<string>('paste_plain_from_clipboard');
}

export async function pickVaultFolder(title?: string): Promise<string | null> {
  return invoke<string | null>('pick_vault_folder', { title: title ?? null });
}

export async function openVault(path: string): Promise<VaultSnapshot> {
  return invoke<VaultSnapshot>('open_vault', { path });
}

export async function createNewVault(
  parentPath: string,
  vaultName: string
): Promise<VaultSnapshot> {
  return invoke<VaultSnapshot>('create_new_vault', { parentPath, vaultName });
}

export async function getVaultSnapshot(): Promise<VaultSnapshot | null> {
  return invoke<VaultSnapshot | null>('get_vault_snapshot');
}

export async function closeVault(): Promise<void> {
  return invoke<void>('close_vault');
}

export async function getRecentVaults(): Promise<RecentVault[]> {
  return invoke<RecentVault[]>('get_recent_vaults');
}

export async function removeRecentVaultEntry(path: string): Promise<RecentVault[]> {
  return invoke<RecentVault[]>('remove_recent_vault_entry', { path });
}

export async function readNote(relPath: string): Promise<NoteDocument> {
  return invoke<NoteDocument>('read_note', { relPath });
}

export async function createNote(
  relPath: string,
  initialContent?: string
): Promise<[VaultSnapshot, NoteDocument]> {
  return invoke<[VaultSnapshot, NoteDocument]>('create_note', {
    relPath,
    initialContent: initialContent ?? null,
  });
}

export async function saveNote(
  relPath: string,
  content: string
): Promise<NoteDocument> {
  return invoke<NoteDocument>('save_note', { relPath, content });
}

export async function createFolder(relPath: string): Promise<VaultSnapshot> {
  return invoke<VaultSnapshot>('create_folder', { relPath });
}

export async function renameEntry(
  oldRelPath: string,
  newRelPath: string
): Promise<RenameEntryResult> {
  return invoke<RenameEntryResult>('rename_entry', { oldRelPath, newRelPath });
}

export async function deleteEntry(relPath: string): Promise<VaultSnapshot> {
  return invoke<VaultSnapshot>('delete_entry', { relPath });
}

export async function queryCalendarDays(
  yearMonth: string,
  field: CalendarDateField
): Promise<CalendarDaySummary[]> {
  return invoke<CalendarDaySummary[]>('query_calendar_days', { yearMonth, field });
}

export async function queryCalendarDateNotes(
  date: string,
  field: CalendarDateField
): Promise<CalendarNoteEntry[]> {
  return invoke<CalendarNoteEntry[]>('query_calendar_date_notes', { date, field });
}

export async function getBacklinks(relPath: string): Promise<BacklinkEntry[]> {
  return invoke<BacklinkEntry[]>('get_backlinks', { relPath });
}

export async function getAllTags(): Promise<TagSummary[]> {
  return invoke<TagSummary[]>('get_all_tags');
}

export async function getNotesByTag(tag: string): Promise<CalendarNoteEntry[]> {
  return invoke<CalendarNoteEntry[]>('get_notes_by_tag', { tag });
}

export async function searchVault(
  query: string,
  limit = 40
): Promise<SearchHit[]> {
  return invoke<SearchHit[]>('search_vault', { query, limit });
}

export async function rebuildVaultIndex(): Promise<number> {
  return invoke<number>('rebuild_vault_index');
}

export async function savePastedImage(
  pngBytes?: number[] | null
): Promise<[string, string]> {
  return invoke<[string, string]>('save_pasted_image', {
    pngBytes: pngBytes ?? null,
  });
}

export async function readVaultImageDataUrl(target: string): Promise<string> {
  return invoke<string>('read_vault_image_data_url', { target });
}

/** Asks where to save, then writes `html`. Resolves to `null` when the user cancels. */
export async function exportActiveNoteHtml(
  title: string,
  html: string,
  labels?: DialogLabels
): Promise<string | null> {
  return invoke<string | null>('export_active_note_html', { title, html, labels: labels ?? null });
}

export async function listDeletedEntries(): Promise<DeletedEntry[]> {
  return invoke<DeletedEntry[]>('list_deleted_entries');
}

export async function restoreDeletedEntry(id: string): Promise<[string, VaultSnapshot]> {
  return invoke<[string, VaultSnapshot]>('restore_deleted_entry', { id });
}

export async function listCssSnippets(): Promise<CssSnippet[]> {
  return invoke<CssSnippet[]>('list_css_snippets');
}

export async function openSnippetsFolder(): Promise<void> {
  return invoke<void>('open_snippets_folder');
}

export async function listSpellDictionaries(): Promise<SpellDictionary[]> {
  return invoke<SpellDictionary[]>('list_spell_dictionaries');
}

export async function spellCheckWords(languages: string[], words: string[]): Promise<string[]> {
  return invoke<string[]>('spell_check_words', { languages, words });
}

export async function spellSuggest(languages: string[], word: string): Promise<string[]> {
  return invoke<string[]>('spell_suggest', { languages, word });
}

export async function spellAddWord(word: string): Promise<void> {
  return invoke<void>('spell_add_word', { word });
}

export async function openDictionariesFolder(): Promise<void> {
  return invoke<void>('open_dictionaries_folder');
}

export async function configureSummon(enabled: boolean, shortcut: string): Promise<void> {
  return invoke<void>('configure_summon', { enabled, shortcut });
}

export async function configureWindowBehavior(
  trayResident: boolean,
  hideOnClose: boolean
): Promise<void> {
  return invoke<void>('configure_window_behavior', { trayResident, hideOnClose });
}

export async function quitApp(quit = true): Promise<void> {
  return invoke<void>('quit_app', { quit });
}

export async function keepAppOpen(): Promise<void> {
  return invoke<void>('keep_app_open');
}

export async function setTrayLabels(show: string, quit: string): Promise<void> {
  return invoke<void>('set_tray_labels', { show, quit });
}

export async function listenQuitRequested(handler: (quit: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>('quit-requested', (event) => handler(event.payload));
}

export async function renderNoteHtmlPreview(
  title: string,
  markdown: string,
  darkMode: boolean
): Promise<string> {
  return invoke<string>('render_note_html_preview', {
    title,
    markdown,
    darkMode,
  });
}

export async function listenVaultFsEvents(
  handler: (payload: VaultFsEvent) => void
): Promise<UnlistenFn> {
  return listen<VaultFsEvent>('vault-fs-event', (event) => {
    handler(event.payload);
  });
}



export async function queryDueTasks(): Promise<TaskEntry[]> {
  return invoke<TaskEntry[]>('query_due_tasks');
}

export async function getNoteCards(): Promise<NoteCard[]> {
  return invoke<NoteCard[]>('get_note_cards');
}

export async function pickTaskCalendarPath(
  defaultName: string,
  labels?: DialogLabels
): Promise<string | null> {
  return invoke<string | null>('pick_task_calendar_path', { defaultName, labels: labels ?? null });
}

/** Rewrites the task calendar file. Resolves to whether the file changed. */
export async function writeTaskCalendar(targetPath: string): Promise<boolean> {
  return invoke<boolean>('write_task_calendar', { targetPath });
}

export async function aiGetStatus(): Promise<AiStatus> {
  return invoke<AiStatus>('ai_get_status');
}

export async function aiSaveConfig(config: AiConfig): Promise<AiStatus> {
  return invoke<AiStatus>('ai_save_config', { config });
}

/** Sends the key to Rust, which stores it in the OS credential store. The key never comes back. */
export async function aiSaveKey(provider: AiProvider, key: string): Promise<AiStatus> {
  return invoke<AiStatus>('ai_save_key', { provider, key });
}

export async function aiDeleteKey(provider: AiProvider): Promise<AiStatus> {
  return invoke<AiStatus>('ai_delete_key', { provider });
}

export async function aiRun(request: AiRequest): Promise<AiResult> {
  return invoke<AiResult>('ai_run', { request });
}

/** Resolves to whether a request with that ID was still running. */
export async function aiCancel(operationId: string): Promise<boolean> {
  return invoke<boolean>('ai_cancel', { operationId });
}
