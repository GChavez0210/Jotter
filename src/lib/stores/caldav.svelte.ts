import { appStorage } from '../appStorage';
import { m } from '../i18n.svelte';
import { caldavForget, caldavHasPassword, caldavSetPassword, caldavSync } from '../ipc';
import type { CaldavReport } from '../types';
import { clipboardStore } from './clipboard.svelte';
import { vaultStore } from './vault.svelte';

const STORAGE_KEY = 'jotter.caldav';
// Pushing sends one request per changed task, so wait longer than the `.ics` file write.
const SYNC_DELAY_MS = 8000;

interface CaldavConfig {
  url: string;
  username: string;
}

/**
 * Pushes each vault's open tasks to a CalDAV calendar. The address and user name are per vault
 * and live in local storage, because they differ per computer. The password stays in the
 * system credential store and never reaches this side.
 */
class CaldavStore {
  private configs = $state<Record<string, CaldavConfig>>({});
  private timer: ReturnType<typeof setTimeout> | undefined;
  private syncing = false;
  private again = false;
  hasPassword = $state(false);
  busy = $state(false);
  lastError = $state('');
  lastReport = $state<CaldavReport | null>(null);

  constructor() {
    try {
      const saved = JSON.parse(appStorage.getItem(STORAGE_KEY) ?? '{}');
      if (saved && typeof saved === 'object') this.configs = saved;
    } catch {
      this.configs = {};
    }
  }

  /** The calendar settings for the open vault. */
  get config(): CaldavConfig {
    const root = vaultStore.vaultSnapshot?.rootPath;
    return (root && this.configs[root]) || { url: '', username: '' };
  }

  /** True once an address, a user name, and a password are all saved. */
  get enabled(): boolean {
    const { url, username } = this.config;
    return !!url && !!username && this.hasPassword;
  }

  private persist() {
    try {
      appStorage.setItem(STORAGE_KEY, JSON.stringify(this.configs));
    } catch {
      // The setting lasts for this session only.
    }
  }

  /** Reads whether the open vault has a saved password. Call when the vault changes. */
  async refresh() {
    this.hasPassword = false;
    if (!vaultStore.vaultSnapshot) return;
    try {
      this.hasPassword = await caldavHasPassword();
    } catch (err) {
      this.lastError = err instanceof Error ? err.message : String(err);
    }
  }

  /** Saves the address and user name, stores the password if one was typed, and syncs. */
  async connect(url: string, username: string, password: string) {
    const root = vaultStore.vaultSnapshot?.rootPath;
    if (!root) return;
    this.lastError = '';
    try {
      if (password) await caldavSetPassword(password);
      this.hasPassword = await caldavHasPassword();
    } catch (err) {
      this.lastError = err instanceof Error ? err.message : String(err);
      return;
    }
    this.configs = { ...this.configs, [root]: { url: url.trim(), username: username.trim() } };
    this.persist();
    await this.syncNow();
  }

  async turnOff() {
    const root = vaultStore.vaultSnapshot?.rootPath;
    if (!root) return;
    clearTimeout(this.timer);
    try {
      await caldavForget();
    } catch (err) {
      this.lastError = err instanceof Error ? err.message : String(err);
      return;
    }
    const { [root]: _removed, ...rest } = this.configs;
    this.configs = rest;
    this.hasPassword = false;
    this.lastError = '';
    this.lastReport = null;
    this.persist();
  }

  async syncNow() {
    clearTimeout(this.timer);
    if (!this.enabled) return;
    // A sync already in flight reads the tasks as they were; run once more when it ends.
    if (this.syncing) {
      this.again = true;
      return;
    }
    this.syncing = true;
    this.busy = true;
    const { url, username } = this.config;
    try {
      const report = await caldavSync(url, username);
      this.lastReport = report;
      this.lastError = report.firstError ?? '';
      if (report.firstError) clipboardStore.setStatus(m.status_caldav_failed({ error: report.firstError }));
    } catch (err) {
      this.lastError = err instanceof Error ? err.message : String(err);
      clipboardStore.setStatus(m.status_caldav_failed({ error: this.lastError }));
    } finally {
      this.syncing = false;
      this.busy = false;
    }
    if (this.again) {
      this.again = false;
      void this.syncNow();
    }
  }

  /** Called whenever the index changes; pushes after edits pause. */
  scheduleSync() {
    if (!this.enabled) return;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.syncNow(), SYNC_DELAY_MS);
  }
}

export const caldavStore = new CaldavStore();
