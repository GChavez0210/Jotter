export const control = {
  save: async (path: string, content: string) => ({relPath:path,title:path,content}),
  openCalls: 0,
  closeCalls: 0,
  noteCardCalls: 0,
};
export const getNoteCards = async () => {control.noteCardCalls++; return [];};
export const calendarControl = {pick: async (_name: string): Promise<string | null> => null, writes: [] as string[]};
export const pickTaskCalendarPath = (name: string) => calendarControl.pick(name);
export const writeTaskCalendar = async (path: string) => {calendarControl.writes.push(path); return true;};
export const saveNote = (path: string, content: string) => control.save(path, content);
export const readNote = async (path: string) => ({relPath:path,title:path,content:'disk'});
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
export const renameEntry = async () => {};

export const systemControl = { configure: async (_enabled: boolean, _shortcut: string): Promise<void> => {} };
export const configureSummon = (enabled: boolean, shortcut: string) => systemControl.configure(enabled, shortcut);
export const listCssSnippets = async () => [];
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
