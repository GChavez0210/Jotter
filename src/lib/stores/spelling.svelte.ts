import { listSpellDictionaries, spellAddWord } from '../ipc';
import { clearSpellingCache, matchDictionaries } from '../spellcheck';
import type { SpellDictionary } from '../types';
import { vaultStore } from './vault.svelte';

const ENGINE_KEY = 'jotter.spellEngine';
const LANGUAGES_KEY = 'jotter.spellLanguages';

/** `jotter` checks with bundled Hunspell dictionaries; `system` hands the job to the WebView. */
export type SpellEngine = 'jotter' | 'system';

/**
 * Installed dictionaries and each vault's spelling languages. Languages are stored per vault in
 * local storage rather than in the vault, because Jotter never writes vault settings files.
 */
class SpellingStore {
  engine = $state<SpellEngine>('jotter');
  dictionaries = $state<SpellDictionary[]>([]);
  loadError = $state('');
  revision = $state(0);
  private languagesByVault = $state<Record<string, string[]>>({});
  private loading: Promise<void> | null = null;

  constructor() {
    try {
      if (localStorage.getItem(ENGINE_KEY) === 'system') this.engine = 'system';
      const saved = JSON.parse(localStorage.getItem(LANGUAGES_KEY) ?? '{}');
      if (saved && typeof saved === 'object' && !Array.isArray(saved)) {
        this.languagesByVault = Object.fromEntries(
          Object.entries(saved).filter((entry): entry is [string, string[]] => {
            const ids = entry[1];
            return Array.isArray(ids) && ids.every((id) => typeof id === 'string');
          })
        );
      }
    } catch {
      this.languagesByVault = {};
    }
  }

  available = $derived(this.dictionaries.map((d) => d.id));

  /** The open vault's languages, defaulting to the dictionary that best matches the OS language. */
  get vaultLanguages(): string[] {
    const root = vaultStore.vaultSnapshot?.rootPath;
    const saved = root ? this.languagesByVault[root] : undefined;
    if (saved) return saved.filter((id) => this.available.includes(id));
    const preferred = typeof navigator === 'undefined' ? [] : [...navigator.languages];
    return matchDictionaries([...preferred, 'en-US'], this.available).slice(0, 1);
  }

  /** Lists installed dictionaries once; later calls reuse the first result. */
  ensureLoaded(): Promise<void> {
    this.loading ??= this.reload();
    return this.loading;
  }

  async reload() {
    try {
      this.dictionaries = await listSpellDictionaries();
      clearSpellingCache();
      this.revision++;
      this.loadError = '';
    } catch (err) {
      this.loadError = err instanceof Error ? err.message : String(err);
    }
  }

  setEngine(engine: SpellEngine) {
    this.engine = engine;
    this.persist(ENGINE_KEY, engine);
  }

  setVaultLanguages(ids: string[]) {
    const root = vaultStore.vaultSnapshot?.rootPath;
    if (!root) return;
    this.languagesByVault = { ...this.languagesByVault, [root]: ids };
    this.persist(LANGUAGES_KEY, JSON.stringify(this.languagesByVault));
  }

  toggleVaultLanguage(id: string, on: boolean) {
    const current = this.vaultLanguages.filter((l) => l !== id);
    // Keep Settings order so the list does not reshuffle as boxes are ticked.
    const next = on ? this.available.filter((a) => a === id || current.includes(a)) : current;
    this.setVaultLanguages(next);
  }

  async addWord(word: string) {
    await spellAddWord(word);
    clearSpellingCache();
    this.revision++;
  }

  private persist(key: string, value: string) {
    try {
      localStorage.setItem(key, value);
    } catch {
      // The setting lasts for this session only.
    }
  }
}

export const spellingStore = new SpellingStore();
