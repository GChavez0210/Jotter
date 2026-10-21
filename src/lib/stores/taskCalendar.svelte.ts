import { m } from '../i18n.svelte';
import { pickTaskCalendarPath, writeTaskCalendar } from '../ipc';
import { clipboardStore } from './clipboard.svelte';
import { vaultStore } from './vault.svelte';

const STORAGE_KEY = 'jotter.taskCalendars';
// Coalesces bursts of saves into one write; calendar apps poll far less often than this.
const WRITE_DELAY_MS = 2000;

/**
 * Keeps an `.ics` file of each vault's due-dated tasks up to date. The file path is per vault
 * and lives in local storage, because it points outside the vault and differs per computer.
 */
class TaskCalendarStore {
  private paths = $state<Record<string, string>>({});
  private timer: ReturnType<typeof setTimeout> | undefined;
  lastError = $state('');

  constructor() {
    try {
      const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}');
      if (saved && typeof saved === 'object') this.paths = saved;
    } catch {
      this.paths = {};
    }
  }

  /** The calendar file for the open vault, or an empty string when export is off. */
  get path(): string {
    const root = vaultStore.vaultSnapshot?.rootPath;
    return root ? (this.paths[root] ?? '') : '';
  }

  private persist() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.paths));
    } catch {
      // The setting lasts for this session only.
    }
  }

  async choosePath() {
    const snapshot = vaultStore.vaultSnapshot;
    if (!snapshot) return;
    let picked: string | null;
    try {
      picked = await pickTaskCalendarPath(m.task_calendar_file_name({ vault: snapshot.name }), {
        title: m.dialog_task_calendar_title(),
        filter: m.dialog_task_calendar_filter(),
      });
    } catch (err) {
      // Show picker errors separately from user cancellation.
      this.lastError = err instanceof Error ? err.message : String(err);
      return;
    }
    if (!picked) return;
    this.paths = { ...this.paths, [snapshot.rootPath]: picked };
    this.persist();
    await this.writeNow();
  }

  turnOff() {
    const root = vaultStore.vaultSnapshot?.rootPath;
    if (!root) return;
    const { [root]: _removed, ...rest } = this.paths;
    this.paths = rest;
    this.lastError = '';
    this.persist();
  }

  async writeNow() {
    clearTimeout(this.timer);
    const target = this.path;
    if (!target) return;
    try {
      await writeTaskCalendar(target);
      this.lastError = '';
    } catch (err) {
      this.lastError = err instanceof Error ? err.message : String(err);
      clipboardStore.setStatus(m.status_task_calendar_failed({ error: this.lastError }));
    }
  }

  /** Called whenever the index changes; writes after edits pause. */
  scheduleWrite() {
    if (!this.path) return;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.writeNow(), WRITE_DELAY_MS);
  }
}

export const taskCalendarStore = new TaskCalendarStore();
