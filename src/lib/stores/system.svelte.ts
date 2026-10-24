import {
  configureSummon,
  configureWindowBehavior,
  listCssSnippets,
  openSnippetsFolder,
} from '../ipc';
import type { CssSnippet } from '../types';
import { settingsStore } from './settings.svelte';

const SNIPPET_ATTR = 'data-jotter-snippet';

/** CSS snippets, the summon hotkey, and the tray: settings that reach outside the editor. */
class SystemStore {
  snippets = $state<CssSnippet[]>([]);
  snippetError = $state('');
  summonError = $state('');
  trayError = $state('');

  /** Re-reads the snippets folder and applies the enabled snippets. */
  async reloadSnippets() {
    try {
      this.snippets = await listCssSnippets();
      this.snippetError = '';
    } catch (err) {
      this.snippets = [];
      this.snippetError = err instanceof Error ? err.message : String(err);
    }
    this.applySnippets();
  }

  /**
   * Appends one `<style>` per enabled snippet at the end of `<head>`, after the theme, so
   * snippets win ties with the theme's selectors.
   */
  applySnippets() {
    for (const el of document.head.querySelectorAll(`style[${SNIPPET_ATTR}]`)) el.remove();
    const enabled = new Set(settingsStore.enabledSnippets);
    for (const snippet of this.snippets) {
      if (!enabled.has(snippet.name)) continue;
      const style = document.createElement('style');
      style.setAttribute(SNIPPET_ATTR, snippet.name);
      style.textContent = snippet.css;
      document.head.appendChild(style);
    }
  }

  async openSnippetsFolder() {
    try {
      await openSnippetsFolder();
    } catch (err) {
      this.snippetError = err instanceof Error ? err.message : String(err);
    }
  }

  private summonQueue: Promise<void> = Promise.resolve();
  private appliedSummon = { enabled: false, shortcut: settingsStore.summonShortcut };

  /** Serialize registrations and restore the last working setting when a change fails. */
  syncSummon(): Promise<void> {
    const enabled = settingsStore.summonEnabled;
    const shortcut = settingsStore.summonShortcut;
    this.summonQueue = this.summonQueue.then(async () => {
      if (enabled === this.appliedSummon.enabled && shortcut === this.appliedSummon.shortcut) return;
      try {
        await configureSummon(enabled, shortcut);
        this.appliedSummon = { enabled, shortcut };
        this.summonError = '';
      } catch (err) {
        this.summonError = err instanceof Error ? err.message : String(err);
        // A queued older request must not replace a newer user choice.
        if (settingsStore.summonEnabled === enabled && settingsStore.summonShortcut === shortcut) {
          settingsStore.setSummon(this.appliedSummon.enabled, this.appliedSummon.shortcut);
        }
      }
    });
    return this.summonQueue;
  }

  private windowQueue: Promise<void> = Promise.resolve();
  private appliedWindow = { tray: false, hide: false };

  /** Pushes the tray icon and close-button settings to the backend, one call at a time. */
  syncWindowBehavior(): Promise<void> {
    const tray = settingsStore.trayEnabled;
    const hide = settingsStore.closeAction === 'tray';
    this.windowQueue = this.windowQueue.then(async () => {
      if (tray === this.appliedWindow.tray && hide === this.appliedWindow.hide) return;
      try {
        await configureWindowBehavior(tray, hide);
        this.appliedWindow = { tray, hide };
        this.trayError = '';
      } catch (err) {
        this.trayError = err instanceof Error ? err.message : String(err);
        if (settingsStore.trayEnabled === tray && (settingsStore.closeAction === 'tray') === hide) {
          settingsStore.setCloseAction(this.appliedWindow.hide ? 'tray' : 'quit');
          settingsStore.setTrayEnabled(this.appliedWindow.tray);
        }
      }
    });
    return this.windowQueue;
  }
}

export const systemStore = new SystemStore();
