import { test } from 'node:test';
import assert from 'node:assert/strict';
import { SettingsStore } from '../src/lib/stores/settings.svelte';
import { windowControl } from './window.mock';
import { settingsControl } from './ipc.mock';
import { appStorage } from '../src/lib/appStorage';
import { buildSettingsExport, parseSettingsImport } from '../src/lib/settingsTransfer';
import { saveBeforeLeaving } from '../src/lib/saveBeforeLeaving';
import { settingsErrorMessage } from '../src/lib/settingsErrors';
import { i18n } from '../src/lib/i18n.svelte';

function browser(saved: Record<string, string> = {}) {
  const storage = new Map(Object.entries(saved));
  const values = {
    localStorage: { getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
      removeItem: (key: string) => storage.delete(key) },
    window: { innerWidth: 500, innerHeight: 760,
      matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
      addEventListener() {}, removeEventListener() {} },
    document: { documentElement: { style: { setProperty() {}, removeProperty() {} },
      dataset: {}, setAttribute() {}, removeAttribute() {} } },
  };
  const previous = Object.keys(values).map((key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)] as const);
  for (const [key, value] of Object.entries(values)) Object.defineProperty(globalThis, key, { configurable: true, value });
  return { storage, restore() {
    for (const [key, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else Reflect.deleteProperty(globalThis, key);
    }
  } };
}

test('fresh-install defaults persist before summon is enabled and survive restart', () => {
  const env = browser();
  try {
    const first = new SettingsStore();
    first.initFromStorageAndSystem()();
    first.setSummon(true);
    const restarted = new SettingsStore();
    restarted.initFromStorageAndSystem()();
    assert.equal(restarted.summonEnabled, true);
    assert.equal(restarted.trayEnabled, false);
    assert.equal(restarted.closeAction, 'quit');
  } finally { env.restore(); }
});

test('legacy summon behavior migrates once and partial settings fail safely', () => {
  for (const saved of [
    { 'jotter.summonEnabled': 'true' },
    { 'jotter.summonEnabled': 'true', 'jotter.closeAction': 'tray' },
    { 'jotter.summonEnabled': 'true', 'jotter.trayEnabled': 'false' },
  ]) {
    const env = browser(saved);
    try {
      const first = new SettingsStore();
      first.initFromStorageAndSystem()();
      const legacy = Object.keys(saved).length === 1;
      assert.equal(first.trayEnabled, legacy);
      assert.equal(first.closeAction, legacy ? 'tray' : 'quit');
      first.setSummon(false);
      const restarted = new SettingsStore();
      restarted.initFromStorageAndSystem()();
      assert.equal(restarted.trayEnabled, legacy);
      assert.equal(restarted.closeAction, legacy ? 'tray' : 'quit');
    } finally { env.restore(); }
  }
});

test('opening the right panel reserves the sidebar that reappears after resizing', async () => {
  const env = browser();
  try {
    for (const sidebarWidth of [180, 248, 400, 480]) {
      const store = new SettingsStore();
      store.sidebarWidth = sidebarWidth;
      store.showRightPanel = false;
      windowControl.sizes = [];
      store.toggleRightPanel();
      await Promise.resolve();
      const size = windowControl.sizes.at(-1)!;
      assert.equal(size.width, sidebarWidth + 248 + 360);
      store.setWindowWidth(size.width);
      assert.equal(store.sidebarVisible, true);
      assert.equal(store.rightPanelVisible, true);
      for (let width = 480; width <= 1200; width++) {
        store.setWindowWidth(width);
        if (store.rightPanelVisible) assert.equal(store.sidebarVisible, true);
      }
      store.sidebarCollapsed = true;
      store.setWindowWidth(500);
      store.showRightPanel = false;
      store.toggleRightPanel();
      await Promise.resolve();
      assert.equal(windowControl.sizes.at(-1)!.width, 608);
      store.setWindowWidth(608);
      assert.equal(store.sidebarVisible, false);
      assert.equal(store.rightPanelVisible, true);
    }
  } finally { env.restore(); }
});

test('automatic hiding preserves sidebar preference through focus mode and restart', () => {
  const env = browser();
  try {
    const store = new SettingsStore();
    store.initFromStorageAndSystem()();
    assert.equal(store.sidebarVisible, false);
    store.setFocusMode('minimal');
    store.setFocusMode('off');
    assert.equal(store.sidebarCollapsed, false);
    const restarted = new SettingsStore();
    restarted.initFromStorageAndSystem()();
    restarted.setWindowWidth(1120);
    assert.equal(restarted.sidebarVisible, true);
    restarted.toggleSidebar();
    const collapsed = new SettingsStore();
    collapsed.initFromStorageAndSystem()();
    collapsed.setWindowWidth(1120);
    assert.equal(collapsed.sidebarVisible, false);
  } finally { env.restore(); }
});

test('export leaves out entries that belong to one computer and import round-trips the rest', () => {
  const text = buildSettingsExport({
    'jotter.themePref': 'dark',
    'jotter.lastVaultPath': 'C:\Notes',
    'jotter.taskCalendars': '{}',
    'other.key': 'x',
  });
  const parsed = parseSettingsImport(text);
  assert.deepEqual(parsed, { ok: true, settings: { 'jotter.themePref': 'dark' }, skipped: 0 });
});

test('import rejects foreign, newer, and empty files and drops malformed entries', () => {
  assert.deepEqual(parseSettingsImport('nope'), { ok: false, reason: 'invalid' });
  assert.deepEqual(parseSettingsImport('{"settings":{}}'), { ok: false, reason: 'invalid' });
  const file = (version: number, settings: unknown) =>
    JSON.stringify({ format: 'jotter-settings', version, settings });
  assert.deepEqual(parseSettingsImport(file(99, {})), { ok: false, reason: 'newer-version' });
  assert.deepEqual(parseSettingsImport(file(1, {})), { ok: false, reason: 'empty' });
  assert.deepEqual(
    parseSettingsImport(file(1, { 'jotter.copyMode': 'plain', 'jotter.lastVaultPath': 'x', 'jotter.bad': 5 })),
    { ok: true, settings: { 'jotter.copyMode': 'plain' }, skipped: 2 }
  );
});

test('the settings file takes over from local storage once and then persists changes', async () => {
  const local = new Map([['jotter.themePref', 'dark'], ['unrelated', 'keep']]);
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'localStorage');
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: {
    get length() { return local.size; },
    key: (i: number) => [...local.keys()][i] ?? null,
    getItem: (key: string) => local.get(key) ?? null,
    setItem: (key: string, value: string) => local.set(key, value),
    removeItem: (key: string) => local.delete(key),
  } });
  settingsControl.stored = null;
  settingsControl.saves.length = 0;
  try {
    await appStorage.load();
    assert.equal(appStorage.getItem('jotter.themePref'), 'dark');
    assert.deepEqual(JSON.parse(settingsControl.stored ?? '{}'), { 'jotter.themePref': 'dark' });
    assert.equal(local.has('jotter.themePref'), false);
    assert.equal(local.has('unrelated'), true);

    appStorage.setItem('jotter.copyMode', 'plain');
    await appStorage.flush();
    assert.deepEqual(JSON.parse(settingsControl.stored ?? '{}'), { 'jotter.themePref': 'dark', 'jotter.copyMode': 'plain' });

    await appStorage.replaceTransferable({ 'jotter.themePref': 'light' });
    assert.deepEqual(JSON.parse(settingsControl.stored ?? '{}'), { 'jotter.themePref': 'light' });
  } finally {
    if (previous) Object.defineProperty(globalThis, 'localStorage', previous);
    else Reflect.deleteProperty(globalThis, 'localStorage');
  }
});

test('task reminders default to on and remember being turned off', () => {
  const env = browser();
  try {
    const first = new SettingsStore();
    first.initFromStorageAndSystem()();
    assert.equal(first.taskReminders, true);
    first.setTaskReminders(false);
    const restarted = new SettingsStore();
    restarted.initFromStorageAndSystem()();
    assert.equal(restarted.taskReminders, false);
  } finally { env.restore(); }
});

test('exit waits for both note saves and settings writes, including in-flight work', async () => {
  const calls: string[] = [];
  let finishNotes!: (ok: boolean) => void;
  let finishSettings!: () => void;
  const ready = saveBeforeLeaving(
    () => new Promise<boolean>((resolve) => { calls.push('notes'); finishNotes = resolve; }),
    () => new Promise<void>((resolve) => { calls.push('settings'); finishSettings = resolve; }),
  );
  let finished = false;
  void ready.then(() => { finished = true; });
  assert.deepEqual(calls, ['notes']);
  finishNotes(true);
  await Promise.resolve();
  assert.deepEqual(calls, ['notes', 'settings']);
  assert.equal(finished, false);
  finishSettings();
  assert.equal(await ready, true);
});

test('failed notes prevent settings import and reload; failed settings prevent exit', async () => {
  let imported = false;
  const ready = await saveBeforeLeaving(async () => false, async () => { imported = true; });
  assert.equal(ready, false);
  assert.equal(imported, false);
  await assert.rejects(saveBeforeLeaving(async () => true, async () => { throw Error('disk full'); }), /disk full/);
});

test('import drops unknown keys and invalid values and rejects invalid versions', () => {
  const file = (settings: unknown, version = 1) => JSON.stringify({ format: 'jotter-settings', version, settings });
  assert.deepEqual(parseSettingsImport(file({ 'jotter.unknown': 'x' })), { ok: false, reason: 'empty' });
  assert.deepEqual(parseSettingsImport(file({ 'jotter.themePref': 'dark' }, -1)), { ok: false, reason: 'invalid' });
  assert.deepEqual(parseSettingsImport(file({ 'jotter.themePref': 'dark' }, 1.5)), { ok: false, reason: 'invalid' });
  const settings = JSON.parse('{"__proto__":"pollute","jotter.themePref":"dark"}');
  Object.assign(settings, { 'jotter.taskReminders': 'maybe', 'jotter.sidebarWidth': 'Infinity',
    'jotter.enabledSnippets': '{}', 'jotter.spellLanguages': '{"vault":3}',
    'jotter.homeSections': '{"vault":[{}]}', 'jotter.aiTargetLanguage': 'a'.repeat(100001),
    'jotter.copyMode': 3 });
  assert.deepEqual(parseSettingsImport(file(settings)), { ok: true, settings: { 'jotter.themePref': 'dark' }, skipped: 8 });
  assert.equal(({} as Record<string, unknown>).pollute, undefined);
});

test('settings transfer preserves valid preferences and structured per-vault settings', () => {
  const settings = { 'jotter.closeAction': 'tray', 'jotter.language': 'es', 'jotter.taskReminders': 'false',
    'jotter.spellLanguages': '{"vault":["en-US","es"]}',
    'jotter.homeSections': '{"vault":[{"id":"tag","kind":"tag","title":"Work","limit":6,"sort":"default","folder":"Work","tag":"work"}]}' };
  assert.deepEqual(parseSettingsImport(buildSettingsExport(settings)), { ok: true, settings, skipped: 0 });
});

test('native settings errors translate in Spanish without leaking English OS messages', () => {
  const previous = i18n.preference;
  try {
    i18n.setPreference('es');
    assert.match(settingsErrorMessage({ code: 'too_large', detail: 'Settings too large' }), /máximo 1 MiB/);
    assert.match(settingsErrorMessage({ code: 'read', detail: 'Access denied' }), /No se pudo leer/);
    assert.match(settingsErrorMessage({ code: 'write' }), /No se pudo guardar/);
    assert.match(settingsErrorMessage({ code: 'invalid_path' }), /archivo de configuración local/);
    assert.equal(settingsErrorMessage(Error('Access denied')), settingsErrorMessage({ code: 'unknown' }));
  } finally { i18n.setPreference(previous); }
});

test('failed imports retain the previous settings and rejected imports never clear them', async () => {
  settingsControl.stored = JSON.stringify({ 'jotter.themePref': 'dark', 'jotter.lastVaultPath': 'keep' });
  await appStorage.load();
  const before = appStorage.snapshot();
  await assert.rejects(appStorage.replaceTransferable({ 'jotter.unknown': 'x' }));
  assert.deepEqual(appStorage.snapshot(), before);
  settingsControl.saveError = Error('disk full');
  try {
    await assert.rejects(appStorage.replaceTransferable({ 'jotter.themePref': 'light' }), /disk full/);
    assert.deepEqual(appStorage.snapshot(), before);
  } finally { settingsControl.saveError = null; }
  await appStorage.flush();
  assert.deepEqual(JSON.parse(settingsControl.stored!), before);
});
