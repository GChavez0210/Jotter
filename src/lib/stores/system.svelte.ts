import { configureSummon, listCssSnippets, openSnippetsFolder } from '../ipc';
import type { CssSnippet } from '../types';
import { settingsStore } from './settings.svelte';

const SNIPPET_ATTR = 'data-jotter-snippet';

/** CSS snippets and the summon hotkey: settings that reach outside the editor. */
class SystemStore {
  snippets = $state<CssSnippet[]>([]);
  snippetError = $state('');
  summonError = $state('');

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

  /** Registers or clears the summon hotkey to match settings. Turns the setting off on failure. */
  async syncSummon() {
    const enabled = settingsStore.summonEnabled;
    try {
      await configureSummon(enabled, settingsStore.summonShortcut);
      // Keep a registration error visible after the toggle flips back off.
      if (enabled) this.summonError = '';
    } catch (err) {
      if (!enabled) return;
      this.summonError = err instanceof Error ? err.message : String(err);
      settingsStore.setSummon(false);
    }
  }
}

export const systemStore = new SystemStore();
