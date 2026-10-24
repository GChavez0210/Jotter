import { loadAppSettings, saveAppSettings } from './ipc';
import { isTransferableKey } from './settingsTransfer';

const SAVE_DELAY_MS = 200;

/**
 * Key-value storage for app settings, with the `localStorage` calling shape.
 *
 * After `load()` succeeds, values live in memory and in `settings.json` in the app data folder,
 * which survives a reinstall and can move between computers. Before that, and wherever the
 * Tauri backend is missing (a browser preview, the unit tests), calls pass through to
 * `localStorage` so the app keeps working.
 */
class AppStorage {
  private map: Map<string, string> | null = null;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private pending: Promise<void> = Promise.resolve();

  /** Reads the settings file. The first run copies the old `localStorage` values into it. */
  async load(): Promise<void> {
    try {
      const raw = await loadAppSettings();
      if (raw !== null) {
        this.map = parseStored(raw);
        return;
      }
      const migrated = new Map<string, string>();
      for (let i = 0; i < localStorage.length; i++) {
        const key = localStorage.key(i);
        const value = key?.startsWith('jotter.') ? localStorage.getItem(key) : null;
        if (key && value !== null) migrated.set(key, value);
      }
      await saveAppSettings(serialize(migrated));
      this.map = migrated;
      // The file now owns these values. Leaving copies behind would resurrect them if it is deleted.
      for (const key of migrated.keys()) localStorage.removeItem(key);
    } catch {
      this.map = null;
    }
  }

  getItem(key: string): string | null {
    return this.map ? (this.map.get(key) ?? null) : localStorage.getItem(key);
  }

  setItem(key: string, value: string): void {
    if (!this.map) return localStorage.setItem(key, value);
    this.map.set(key, value);
    this.scheduleSave();
  }

  removeItem(key: string): void {
    if (!this.map) return localStorage.removeItem(key);
    this.map.delete(key);
    this.scheduleSave();
  }

  /** Every stored setting, for export. */
  snapshot(): Record<string, string> {
    if (this.map) return Object.fromEntries(this.map);
    const all: Record<string, string> = {};
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      const value = key ? localStorage.getItem(key) : null;
      if (key && value !== null) all[key] = value;
    }
    return all;
  }

  /**
   * Swaps the transferable settings for `incoming` and saves at once. This computer's own
   * entries, such as the last vault, stay as they are.
   */
  async replaceTransferable(incoming: Record<string, string>): Promise<void> {
    if (!this.map) throw new Error('Settings storage is not available');
    for (const key of [...this.map.keys()]) {
      if (isTransferableKey(key)) this.map.delete(key);
    }
    for (const [key, value] of Object.entries(incoming)) this.map.set(key, value);
    await this.flush();
  }

  /** Writes any pending change now. Saves run one after another so the last write wins. */
  flush(): Promise<void> {
    clearTimeout(this.timer);
    this.timer = undefined;
    const map = this.map;
    if (!map) return this.pending;
    const json = serialize(map);
    this.pending = this.pending.catch(() => undefined).then(() => saveAppSettings(json));
    return this.pending;
  }

  private scheduleSave() {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.flush().catch((err) => console.warn('Could not save settings', err));
    }, SAVE_DELAY_MS);
  }
}

function serialize(map: Map<string, string>): string {
  return JSON.stringify(Object.fromEntries(map), null, 2);
}

function parseStored(raw: string): Map<string, string> {
  const map = new Map<string, string>();
  try {
    const data: unknown = JSON.parse(raw);
    if (typeof data === 'object' && data !== null && !Array.isArray(data)) {
      for (const [key, value] of Object.entries(data)) {
        if (typeof value === 'string') map.set(key, value);
      }
    }
  } catch {
    // A damaged file starts the app on defaults. The next change writes a clean one.
  }
  return map;
}

export const appStorage = new AppStorage();

if (typeof window !== 'undefined') {
  window.addEventListener('pagehide', () => void appStorage.flush());
}
