import { getNoteCards } from '../ipc';
import type { NoteCard } from '../types';
import { vaultStore } from './vault.svelte';

const OPENED_KEY = 'jotter.recentlyOpened';
const MAX_OPENED = 50;

/**
 * Note cards from the index, shared by the Home page and the tags-and-links view, plus each
 * vault's recently opened notes. The history lives in local storage, outside the vault.
 */
class NoteCardsStore {
  cards = $state<NoteCard[]>([]);
  loading = $state(false);
  error = $state('');
  private opened = $state<Record<string, string[]>>({});
  private loadedVersion = -1;
  private request = 0;

  constructor() {
    try {
      const saved = JSON.parse(localStorage.getItem(OPENED_KEY) ?? '{}');
      if (saved && typeof saved === 'object') this.opened = saved;
    } catch {
      this.opened = {};
    }
  }

  /** The open vault's notes in most-recently-opened order. */
  get openedPaths(): string[] {
    const root = vaultStore.vaultSnapshot?.rootPath;
    return root ? (this.opened[root] ?? []) : [];
  }

  recordOpened(relPath: string) {
    const root = vaultStore.vaultSnapshot?.rootPath;
    if (!root || !relPath) return;
    const list = (this.opened[root] ?? []).filter((p) => p !== relPath);
    this.opened = { ...this.opened, [root]: [relPath, ...list].slice(0, MAX_OPENED) };
    try {
      localStorage.setItem(OPENED_KEY, JSON.stringify(this.opened));
    } catch {
      // History lasts for this session only.
    }
  }

  /** Reloads the cards when the index changed since the last load. */
  async refresh(indexVersion: number) {
    if (!vaultStore.vaultSnapshot) {
      this.cards = [];
      return;
    }
    if (indexVersion === this.loadedVersion) return;
    const id = ++this.request;
    this.loading = true;
    try {
      const cards = await getNoteCards();
      if (id !== this.request) return;
      this.cards = cards;
      this.loadedVersion = indexVersion;
      this.error = '';
    } catch (err) {
      if (id !== this.request) return;
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      if (id === this.request) this.loading = false;
    }
  }
}

export const noteCardsStore = new NoteCardsStore();
