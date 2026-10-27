export const control = {
  read: async (path: string) => ({relPath:path,title:path,content:'disk'}),
  save: async (path: string, content: string) => ({relPath:path,title:path,content}),
  openCalls: 0,
  closeCalls: 0,
  noteCardCalls: 0,
  rename: async (_oldPath: string, _newPath: string): Promise<import('../src/lib/types').RenameEntryResult> => { throw Error('rename not configured'); },
};
export const getNoteCards = async () => {control.noteCardCalls++; return [];};
export const calendarControl = {pick: async (_name: string): Promise<string | null> => null, writes: [] as string[]};
export const pickTaskCalendarPath = (name: string) => calendarControl.pick(name);
export const writeTaskCalendar = async (path: string) => {calendarControl.writes.push(path); return true;};
export const saveNote = (path: string, content: string) => control.save(path, content);
export const readNote = (path: string) => control.read(path);
export const openVault = async () => {control.openCalls++; return {rootPath:'B',name:'B',tree:[],noteCount:0};};
export const closeVault = async () => {control.closeCalls++;};
export const createFolder = async () => {};
export const createNewVault = openVault;
export const createNote = async () => {};
export const deleteEntry = async () => {};
export const getRecentVaults = async () => [];
export const getVaultSnapshot = async () => null;
export const rebuildVaultIndex = async () => 0;
export const restoreDeletedEntry = async () => {};
export const removeRecentVaultEntry = async () => [];
export const renameEntry = (oldPath: string, newPath: string) => control.rename(oldPath, newPath);
export const linkControl = {
  plan: async (_oldPath: string, _newPath: string): Promise<import('../src/lib/types').LinkUpdatePlan> => ({ notes: [], linkCount: 0 }),
  apply: async (_path: string, _edits: import('../src/lib/types').TextEdit[]): Promise<import('../src/lib/types').AppliedEdits> => ({ applied: 0, skipped: 0, reverse: [] }),
};
export const planLinkUpdates = (oldPath: string, newPath: string) => linkControl.plan(oldPath, newPath);
export const applyLinkEdits = (path: string, edits: import('../src/lib/types').TextEdit[]) => linkControl.apply(path, edits);
export const quickCaptureAppend = async (relPath: string, _text: string) => relPath;

export const pinControl = { pinned: [] as string[] };
export const listPinnedNotes = async () => pinControl.pinned;
export const setNotePinned = async (relPath: string, pinned: boolean) => {
  pinControl.pinned = pinControl.pinned.filter((p) => p !== relPath);
  if (pinned) pinControl.pinned.push(relPath);
  return pinControl.pinned;
};

export const systemControl = {
  configure: async (_enabled: boolean, _shortcut: string): Promise<void> => {},
  configureWindow: async (_trayResident: boolean, _hideOnClose: boolean): Promise<void> => {},
};
export const configureSummon = (enabled: boolean, shortcut: string) => systemControl.configure(enabled, shortcut);
export const configureQuickCapture = async (_enabled: boolean, _shortcut: string): Promise<void> => {};
export const configureWindowBehavior = (trayResident: boolean, hideOnClose: boolean) =>
  systemControl.configureWindow(trayResident, hideOnClose);
export const listCssSnippets = async () => [];

// The editor component reaches for these; the UI harness mounts it with the backend absent.
export const copyToClipboard = async (req: import('../src/lib/types').CopyRequest) => ({
  plain: req.text, html: req.text, mode: req.mode,
  lineCount: req.text.split(String.fromCharCode(10)).length, charCount: req.text.length,
});
export const previewClipboardPayload = copyToClipboard;
export const pastePlainFromClipboard = async () => '';
export const savePastedImage = async (): Promise<[string, string]> => ['pasted.png', 'attachments/pasted.png'];
export const listSpellDictionaries = async () => [];
export const spellCheckWords = async () => [] as string[];
export const spellSuggest = async () => [] as string[];
export const spellAddWord = async () => {};
export const openSnippetsFolder = async () => {};

import type { AiConfig, AiRequest, AiResult, AiStatus } from '../src/lib/types';
export const aiControl = {
  run: async (req: AiRequest): Promise<AiResult> => ({
    operationId: req.operationId, text: req.text, provider: 'ollama', model: 'm',
    usage: {inputTokens: null, outputTokens: null},
  }),
  cancels: [] as string[],
  saveConfig: async (_config: AiConfig): Promise<AiStatus> => {throw Error('no AI in tests');},
};
export const aiRun = (req: AiRequest) => aiControl.run(req);
export const aiCancel = async (id: string) => {aiControl.cancels.push(id); return true;};
export const aiGetStatus = async () => {throw Error('no AI in tests');};
export const aiSaveConfig = (config: AiConfig) => aiControl.saveConfig(config);
export const aiSaveKey = aiGetStatus;
export const aiDeleteKey = aiGetStatus;
export async function readVaultImageDataUrl(_target: string): Promise<string> {
  throw new Error('Image loading is unavailable in store tests.');
}
export const settingsControl = {stored: null as string | null, saves: [] as string[], saveError: null as Error | null};
export const loadAppSettings = async () => settingsControl.stored;
export const saveAppSettings = async (json: string) => {
  if (settingsControl.saveError) throw settingsControl.saveError;
  settingsControl.stored = json; settingsControl.saves.push(json);
};
export const exportSettingsFile = async () => null;
export const importSettingsFile = async () => null;
export const configureTaskReminders = async () => {};

// The accessibility harness mounts the whole app, which reaches for these on startup.
export const listenQuitRequested = async (_handler: (quit: boolean) => void) => () => {};
export const listenVaultFsEvents = async (_handler: (payload: unknown) => void) => () => {};
export const listenCaptureRequests = async (_handler: (request: unknown) => Promise<string>) => () => {};
export const setTrayLabels = async (_show: string, _quit: string) => {};
export const configureVersionHistory = async (_enabled: boolean) => {};
export const quitApp = async (_quit?: boolean) => {};
export const keepAppOpen = async () => {};
export const renderNoteHtmlPreview = async (title: string, markdown: string) =>
  `<!DOCTYPE html><html><body><main><h1>${title}</h1><p>${markdown}</p></main></body></html>`;
export const exportActiveNoteHtml = async () => null;
export const getBacklinks = async () => [];
export const getAllTags = async () => [];
export const getNotesByTag = async () => [];
export const queryCalendarDays = async () => [];
export const queryCalendarDateNotes = async () => [];
export const queryDueTasks = async () => [];
export const listDeletedEntries = async () => [];
export const searchVault = async () => [];
export const getVaultHealth = async () => ({
  brokenLinks: {items: [], total: 0}, unusedAttachments: {items: [], total: 0},
  emptyNotes: {items: [], total: 0}, duplicateTitles: {items: [], total: 0},
});
export const versionControl = {
  text: '# Earlier version\n',
  restores: [] as string[],
};
export const listNoteVersions = async () => [{id:'1700000000000', savedAtMs:1700000000000, bytes:versionControl.text.length}];
export const readNoteVersion = async () => versionControl.text;
export const restoreNoteVersion = async (relPath: string) => {
  versionControl.restores.push(relPath);
  return {relPath, title:relPath, content:versionControl.text};
};
export const pickVaultFolder = async () => null;
export const openDictionariesFolder = async () => {};
