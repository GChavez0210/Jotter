import { appStorage } from '../appStorage';
import { m } from '../i18n.svelte';
import { configureBackups, getBackupStatus, pickBackupFolder, runBackupNow } from '../ipc';
import type { BackupStatus } from '../types';
import { clipboardStore } from './clipboard.svelte';
import { vaultStore } from './vault.svelte';

const STORAGE_KEY = 'jotter.backups';

export const BACKUP_INTERVAL_HOURS = [6, 24, 168] as const;
export const DEFAULT_BACKUP_KEEP = 7;
export const MAX_BACKUP_KEEP = 100;

interface BackupSettings {
  folder: string;
  intervalHours: number;
  keep: number;
}

/** Clamps a typed copy count to a whole number from 1 to the limit. */
export function clampKeep(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_BACKUP_KEEP;
  return Math.min(MAX_BACKUP_KEEP, Math.max(1, Math.round(value)));
}

/**
 * Chooses where and how often the open vault is zipped. The schedule is per vault and lives in
 * local storage, because the folder differs per computer. The Rust side runs the schedule, so
 * backups continue while the window sits in the tray.
 */
export class BackupStore {
  private poll: ReturnType<typeof setInterval> | undefined;
  private generation = 0;
  private all = $state<Record<string, BackupSettings>>({});
  status = $state<BackupStatus>({ lastBackupMs: null, lastError: null });
  busy = $state(false);
  lastError = $state('');

  constructor() {
    try {
      const saved = JSON.parse(appStorage.getItem(STORAGE_KEY) ?? '{}');
      if (saved && typeof saved === 'object') this.all = saved;
    } catch {
      this.all = {};
    }
  }

  /** The schedule for the open vault. An empty folder means backups are off. */
  get settings(): BackupSettings {
    const root = vaultStore.vaultSnapshot?.rootPath;
    return (root && this.all[root]) || { folder: '', intervalHours: 24, keep: DEFAULT_BACKUP_KEEP };
  }

  private persist() {
    try {
      appStorage.setItem(STORAGE_KEY, JSON.stringify(this.all));
    } catch {
      // The setting lasts for this session only.
    }
  }

  /** Cancels status polling and invalidates responses for the previous vault. */
  stop() {
    clearInterval(this.poll);
    this.poll = undefined;
    this.generation++;
  }

  /** Sends the open vault's schedule and keeps its displayed status current. */
  async apply() {
    this.stop();
    const generation = this.generation;
    const root = vaultStore.vaultSnapshot?.rootPath;
    this.status = { lastBackupMs: null, lastError: null };
    this.lastError = '';
    try {
      const { folder, intervalHours, keep } = this.settings;
      await configureBackups(root && folder ? { vault: root, folder, intervalHours, keep } : null);
      if (generation !== this.generation) return;
      if (root && folder) {
        this.poll = setInterval(() => void this.refreshStatus(root, generation), 4000);
      }
      if (root) await this.refreshStatus(root, generation);
    } catch (err) {
      if (generation === this.generation) this.lastError = err instanceof Error ? err.message : String(err);
    }
  }

  private async refreshStatus(root: string, generation: number) {
    try {
      const status = await getBackupStatus();
      if (generation === this.generation && root === vaultStore.vaultSnapshot?.rootPath) this.status = status;
    } catch {
      // Keep polling after a transient IPC failure.
    }
  }

  private async update(patch: Partial<BackupSettings>) {
    const root = vaultStore.vaultSnapshot?.rootPath;
    if (!root) return;
    this.all = { ...this.all, [root]: { ...this.settings, ...patch } };
    this.persist();
    await this.apply();
  }

  async chooseFolder() {
    let picked: string | null;
    try {
      picked = await pickBackupFolder(m.dialog_backup_folder_title());
    } catch (err) {
      this.lastError = err instanceof Error ? err.message : String(err);
      return;
    }
    if (!picked) return;
    this.lastError = '';
    await this.update({ folder: picked });
  }

  setInterval(hours: number) {
    return this.update({ intervalHours: hours });
  }

  setKeep(count: number) {
    return this.update({ keep: clampKeep(count) });
  }

  async turnOff() {
    const root = vaultStore.vaultSnapshot?.rootPath;
    if (!root) return;
    const { [root]: _removed, ...rest } = this.all;
    this.all = rest;
    this.lastError = '';
    this.persist();
    await this.apply();
  }

  async backupNow() {
    if (!this.settings.folder || this.busy) return;
    this.busy = true;
    try {
      await runBackupNow();
      this.lastError = '';
    } catch (err) {
      this.lastError = err instanceof Error ? err.message : String(err);
      clipboardStore.setStatus(m.status_backup_failed({ error: this.lastError }));
    } finally {
      this.busy = false;
    }
    const root = vaultStore.vaultSnapshot?.rootPath;
    if (root) await this.refreshStatus(root, this.generation);
  }
}

export const backupStore = new BackupStore();
