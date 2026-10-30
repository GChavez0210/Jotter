import { checkForUpdate, installUpdate, listenUpdateProgress } from '../ipc';
import { m } from '../i18n.svelte';
import type { UpdateInfo, UpdateProgress } from '../types';

/** Wait this long after startup so the check never competes with opening the vault. */
export const FIRST_CHECK_DELAY_MS = 15_000;
export const CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

function installError(err: unknown): string {
  const code = err instanceof Error ? err.message : String(err);
  if (code === 'update_offer_changed') return m.update_offer_changed();
  if (code === 'update_busy') return m.update_busy();
  if (code === 'update_cannot_install') return m.update_cannot_install();
  return m.update_install_failed();
}

/** The newest update the release feed offered, and the state of installing it. */
export class UpdateStore {
  /** The update the last check found, or `null`. */
  info = $state<UpdateInfo | null>(null);
  checking = $state(false);
  /** True after a check that found nothing newer. */
  upToDate = $state(false);
  installing = $state(false);
  /** 0 to 1 while downloading, `null` when the size is unknown or no download is running. */
  progress = $state<number | null>(null);
  stage = $state<'idle' | 'downloading' | 'installing'>('idle');
  /** The last failure a person should read: a manual check or an install that went wrong. */
  error = $state('');
  dialogOpen = $state(false);

  /**
   * Asks the feed for a newer version. An automatic check (`manual` false) stays quiet when
   * the network or the feed fails, because nobody asked for it.
   */
  async check(manual: boolean): Promise<UpdateInfo | null> {
    if (this.checking || this.installing || this.dialogOpen) return this.info;
    this.checking = true;
    if (manual) this.error = '';
    try {
      const found = await checkForUpdate();
      this.info = found;
      this.upToDate = found === null;
      return found;
    } catch {
      if (manual) this.error = m.update_check_failed();
      return this.info;
    } finally {
      this.checking = false;
    }
  }

  /**
   * Downloads and installs the update, then Jotter restarts. `saveFirst` writes open notes and
   * settings to disk, and the install does not start if that fails.
   */
  async install(saveFirst: () => Promise<boolean>): Promise<void> {
    if (!this.info || !this.info.canInstall || this.installing || this.checking) return;
    const expectedVersion = this.info.version;
    this.installing = true;
    this.error = '';
    this.progress = null;
    this.stage = 'downloading';
    let unlisten: (() => void) | undefined;
    let saved = false;
    try {
      if (!(await saveFirst())) {
        this.error = m.update_save_failed();
        return;
      }
      saved = true;
      unlisten = await listenUpdateProgress((event) => this.onProgress(event));
      await installUpdate(expectedVersion);
    } catch (err) {
      this.error = saved ? installError(err) : m.update_save_failed();
    } finally {
      unlisten?.();
      this.installing = false;
      this.stage = 'idle';
      this.progress = null;
    }
  }

  private onProgress(event: UpdateProgress) {
    if (event.phase === 'installing') {
      this.stage = 'installing';
      this.progress = null;
      return;
    }
    this.stage = 'downloading';
    this.progress = event.total ? Math.min(1, event.downloaded / event.total) : null;
  }

  openDialog() {
    if (this.info) this.dialogOpen = true;
  }

  closeDialog() {
    // Closing mid-install would hide the only progress display.
    if (!this.installing) this.dialogOpen = false;
  }

  /** Checks once after a short delay and then daily. Returns a function that stops the schedule. */
  startAutoCheck(): () => void {
    const first = setTimeout(() => void this.check(false), FIRST_CHECK_DELAY_MS);
    const daily = setInterval(() => void this.check(false), CHECK_INTERVAL_MS);
    return () => {
      clearTimeout(first);
      clearInterval(daily);
    };
  }
}

export const updateStore = new UpdateStore();
