import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
import { clampKeep, DEFAULT_BACKUP_KEEP, MAX_BACKUP_KEEP } from '../src/lib/stores/backups.svelte';

test('the copy count stays a whole number between 1 and the limit', () => {
  assert.equal(clampKeep(7), 7);
  assert.equal(clampKeep(0), 1);
  assert.equal(clampKeep(-3), 1);
  assert.equal(clampKeep(2.6), 3);
  assert.equal(clampKeep(5000), MAX_BACKUP_KEEP);
});

test('an invalid copy count falls back to the default', () => {
  assert.equal(clampKeep(Number.NaN), DEFAULT_BACKUP_KEEP);
  assert.equal(clampKeep(Number.POSITIVE_INFINITY), DEFAULT_BACKUP_KEEP);
});

import { BackupStore } from '../src/lib/stores/backups.svelte';
import { backupControl } from './ipc.mock';
import { appStorage } from '../src/lib/appStorage';
import { vaultStore } from '../src/lib/stores/vault.svelte';

const storage = new Map<string, string>();
Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: {
  getItem: (key: string) => storage.get(key) ?? null,
  setItem: (key: string, value: string) => { storage.set(key, value); },
  removeItem: (key: string) => { storage.delete(key); },
} });

const flush = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };

test('backup status keeps updating after slow and later scheduled backups', async () => {
  mock.timers.enable({ apis: ['setInterval'] });
  appStorage.setItem('jotter.backups', JSON.stringify({ A: { folder: 'Backups', intervalHours: 24, keep: 7 } }));
  vaultStore.vaultSnapshot = { rootPath: 'A', name: 'A', tree: [], noteCount: 0 };
  let lastBackupMs: number | null = null;
  backupControl.status = async () => ({ lastBackupMs, lastError: null });
  const store = new BackupStore();
  try {
    await store.apply();
    mock.timers.tick(4000); await flush();
    assert.equal(store.status.lastBackupMs, null);
    lastBackupMs = 100;
    mock.timers.tick(4000); await flush();
    assert.equal(store.status.lastBackupMs, 100);
    lastBackupMs = 200;
    mock.timers.tick(4000); await flush();
    assert.equal(store.status.lastBackupMs, 200);
    await store.turnOff();
    lastBackupMs = 300;
    mock.timers.tick(12000); await flush();
    assert.equal(store.status.lastBackupMs, 200);
  } finally { store.stop(); mock.timers.reset(); }
});

test('a late backup status cannot overwrite the next vault', async () => {
  appStorage.setItem('jotter.backups', '{}');
  vaultStore.vaultSnapshot = { rootPath: 'A', name: 'A', tree: [], noteCount: 0 };
  let resolve!: (value: { lastBackupMs: number; lastError: null }) => void;
  backupControl.status = () => new Promise(r => { resolve = r; });
  const store = new BackupStore();
  const pending = store.apply();
  await flush();
  vaultStore.vaultSnapshot = { rootPath: 'B', name: 'B', tree: [], noteCount: 0 };
  backupControl.status = async () => ({ lastBackupMs: 200, lastError: null });
  await store.apply();
  resolve({ lastBackupMs: 100, lastError: null });
  await pending;
  assert.equal(store.status.lastBackupMs, 200);
  store.stop();
});
