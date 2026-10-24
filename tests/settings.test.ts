import { test } from 'node:test';
import assert from 'node:assert/strict';
import { SettingsStore } from '../src/lib/stores/settings.svelte';
import { windowControl } from './window.mock';

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
