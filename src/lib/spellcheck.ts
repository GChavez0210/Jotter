import { syntaxTree } from '@codemirror/language';
import { Facet, StateEffect, type EditorState, type Extension, type Range } from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
} from '@codemirror/view';
import { parseFrontmatter, type Frontmatter } from './frontmatter';
import { findInlineMath } from './livePreviewBlocks';

/**
 * Spell checking with Jotter's Hunspell dictionaries. The plugin collects the words on visible
 * lines, asks the backend about words it has not seen, and underlines the misspelled ones.
 * Results are cached per language set, so scrolling and switching notes rarely call the backend.
 */

export interface WordRange {
  from: number;
  to: number;
  word: string;
}

export interface SpellcheckConfig {
  /** Dictionary ids for notes without a `lang` property. */
  vaultLanguages: string[];
  /** Ids of every installed dictionary, used to resolve a note's `lang` tags. */
  available: string[];
  /** Returns the words the dictionaries reject. */
  check: (languages: string[], words: string[]) => Promise<string[]>;
}

// Runs of letters, digits, `_` and apostrophes, joined by `.`, `@`, `/`, `\` or `:` so file
// names, addresses, and paths come out as one chunk and can be skipped whole.
const CHUNK_RE = /[\p{L}\p{M}\p{N}_'’]+(?:[.@/\\:][\p{L}\p{M}\p{N}_'’]+)*/gu;
const JOINER_RE = /[.@/\\:]/;
const DIGIT_OR_UNDERSCORE_RE = /[\p{N}_]/u;
const UPPER_RE = /\p{Lu}/u;
const LOWER_RE = /\p{Ll}/u;
const WIKILINK_RE = /!?\[\[[^\]\n]*\]\]/g;
// Matches the inline tags Live Preview styles.
const TAG_RE = /(?<=^|\s)#[\p{L}\p{N}_/-]+/gu;

/** Syntax nodes whose text is code, markup, or an address rather than prose. */
const SKIPPED_NODES = new Set([
  'InlineCode',
  'FencedCode',
  'CodeBlock',
  'URL',
  'Autolink',
  'HTMLTag',
  'HTMLBlock',
  'Comment',
  'CommentBlock',
  'ProcessingInstruction',
  'ProcessingInstructionBlock',
  'LinkLabel',
]);

/**
 * Finds the words worth checking in `text`, skipping anything that looks like an identifier:
 * chunks with digits, underscores, or `.`/`@`/`/` joiners, all-caps acronyms, words with inner
 * capitals, LaTeX commands, and HTML entities. `offset` is added to every position.
 */
export function findCheckableWords(text: string, offset = 0): WordRange[] {
  const words: WordRange[] = [];
  for (const match of text.matchAll(CHUNK_RE)) {
    const chunk = match[0];
    const start = match.index;
    const before = text[start - 1];
    if (before === '\\' || before === '&') continue;
    if (JOINER_RE.test(chunk) || DIGIT_OR_UNDERSCORE_RE.test(chunk)) continue;

    const lead = chunk.length - chunk.replace(/^['’]+/, '').length;
    const word = chunk.replace(/^['’]+/, '').replace(/['’]+$/, '');
    if ([...word].length < 2) continue;
    if (!LOWER_RE.test(word)) continue; // all caps: an acronym
    if (UPPER_RE.test(word.slice(1))) continue; // camelCase or a brand name
    words.push({ from: offset + start + lead, to: offset + start + lead + word.length, word });
  }
  return words;
}

/** Ranges within one line that hold wiki links, tags, or inline math. */
export function inlineSkipRanges(text: string): Array<{ from: number; to: number }> {
  const ranges: Array<{ from: number; to: number }> = [];
  for (const m of text.matchAll(WIKILINK_RE)) ranges.push({ from: m.index, to: m.index + m[0].length });
  for (const m of text.matchAll(TAG_RE)) ranges.push({ from: m.index, to: m.index + m[0].length });
  for (const m of findInlineMath(text)) ranges.push({ from: m.from, to: m.to });
  return ranges;
}

/**
 * The note's language tags from its `lang` property, `false` when `spellcheck: false` turns
 * checking off, or `null` when the note does not say.
 */
export function noteLanguageTags(frontmatter: Frontmatter | null): string[] | false | null {
  if (!frontmatter) return null;
  const prop = (key: string) =>
    frontmatter.properties.find((p) => p.key.toLowerCase() === key)?.value;
  const spellcheck = prop('spellcheck');
  if (spellcheck === false || spellcheck === 'false' || spellcheck === 'off') return false;
  const lang = prop('lang');
  if (typeof lang === 'string' && lang.trim()) return lang.split(/[\s,]+/).filter(Boolean);
  if (Array.isArray(lang)) return lang.filter((t) => t.trim());
  return null;
}

/**
 * Maps BCP 47 tags to installed dictionary ids. An exact id wins (`en-GB`); otherwise the first
 * installed dictionary with the same primary language does (`es-MX` → `es`, `en` → `en-US`).
 */
export function matchDictionaries(tags: readonly string[], available: readonly string[]): string[] {
  const norm = (s: string) => s.trim().replace(/_/g, '-').toLowerCase();
  const out: string[] = [];
  for (const tag of tags) {
    const wanted = norm(tag);
    if (!wanted) continue;
    const primary = wanted.split('-')[0];
    const id =
      available.find((a) => norm(a) === wanted) ??
      available.find((a) => norm(a).split('-')[0] === primary);
    if (id && !out.includes(id)) out.push(id);
  }
  return out;
}

/** The dictionaries a note uses: its own `lang`, else the vault's languages. */
export function resolveNoteLanguages(state: EditorState, config: SpellcheckConfig): string[] {
  const tags = noteLanguageTags(readFrontmatter(state));
  if (tags === false) return [];
  if (tags === null) return config.vaultLanguages;
  // A note in a language with no installed dictionary gets no underlines rather than all of them.
  return matchDictionaries(tags, config.available);
}

function readFrontmatter(state: EditorState): Frontmatter | null {
  if (!/^---[ \t]*$/.test(state.doc.line(1).text)) return null;
  return parseFrontmatter(state.doc.sliceString(0, 64 * 1024));
}

/**
 * The prose words on the lines that `ranges` touch, minus frontmatter, code, link addresses,
 * wiki links, tags, math, and the word under `cursor` (pass -1 to keep it).
 */
export function collectCheckableWords(
  state: EditorState,
  ranges: readonly { from: number; to: number }[],
  cursor = -1
): WordRange[] {
  const skipped: Array<{ from: number; to: number }> = [];
  const fm = readFrontmatter(state);
  if (fm) skipped.push({ from: 0, to: fm.end });
  for (const { from, to } of ranges) {
    syntaxTree(state).iterate({
      from,
      to,
      enter: (node) => {
        if (!SKIPPED_NODES.has(node.name)) return;
        skipped.push({ from: node.from, to: node.to });
        return false;
      },
    });
  }

  const words: WordRange[] = [];
  let lastLine = 0;
  for (const { from, to } of ranges) {
    for (let pos = from; pos <= to; ) {
      const line = state.doc.lineAt(pos);
      pos = line.to + 1;
      // Ranges can split one line, for example around a folded block.
      if (line.number <= lastLine) continue;
      lastLine = line.number;
      const inline = inlineSkipRanges(line.text);
      for (const w of findCheckableWords(line.text, line.from)) {
        if (w.from <= cursor && cursor <= w.to) continue;
        const rel = w.from - line.from;
        if (inline.some((r) => rel >= r.from && rel < r.to)) continue;
        if (skipped.some((r) => w.from < r.to && w.to > r.from)) continue;
        words.push(w);
      }
    }
  }
  return words;
}

// word → misspelled, per language set. Shared across editors and notes.
let cacheGeneration = 0;
const resultCache = new Map<string, Map<string, boolean>>();
const MAX_CACHED_WORDS = 50_000;

function cacheFor(languages: string[]): Map<string, boolean> {
  const key = languages.join('|');
  let cache = resultCache.get(key);
  if (!cache || cache.size > MAX_CACHED_WORDS) {
    cache = new Map();
    resultCache.set(key, cache);
  }
  return cache;
}

/** Discard results after dictionaries or the personal word list change. */
export function clearSpellingCache() {
  cacheGeneration++;
  resultCache.clear();
}

const spellConfig = Facet.define<SpellcheckConfig, SpellcheckConfig | null>({
  combine: (values) => values[0] ?? null,
});

const resultsArrived = StateEffect.define<null>();
const misspelledMark = Decoration.mark({ class: 'cm-misspelled' });
const CHECK_DELAY_MS = 200;

const spellPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet = Decoration.none;
    languages: string[] = [];
    pending = new Set<string>();
    timer: ReturnType<typeof setTimeout> | undefined;
    destroyed = false;
    /** True after typing, until the cursor moves some other way, such as a click. */
    typing = false;

    constructor(readonly view: EditorView) {
      this.languages = this.resolveLanguages();
      this.decorations = this.build();
    }

    resolveLanguages(): string[] {
      const config = this.view.state.facet(spellConfig);
      return config ? resolveNoteLanguages(this.view.state, config) : [];
    }

    update(update: ViewUpdate) {
      const reconfigured = update.startState.facet(spellConfig) !== update.state.facet(spellConfig);
      if (update.docChanged || reconfigured) {
        const next = this.resolveLanguages();
        if (next.join('|') !== this.languages.join('|')) {
          this.languages = next;
          this.pending.clear();
        }
      }
      if (update.docChanged) {
        this.typing = update.transactions.some((tr) => tr.isUserEvent('input') || tr.isUserEvent('delete'));
      } else if (update.selectionSet) {
        // A click or arrow key, including the caret move a right-click makes, ends typing so
        // the word under the cursor keeps its underline for the spelling menu.
        this.typing = false;
      }
      const arrived = update.transactions.some((tr) => tr.effects.some((e) => e.is(resultsArrived)));
      if (
        update.docChanged ||
        update.viewportChanged ||
        update.selectionSet ||
        update.focusChanged ||
        reconfigured ||
        arrived
      ) {
        this.decorations = this.build();
      }
    }

    build(): DecorationSet {
      if (this.languages.length === 0) return Decoration.none;
      const { state } = this.view;
      const cache = cacheFor(this.languages);
      // Leave the word being typed alone until the user moves on from it.
      const sel = state.selection.main;
      const cursor = this.typing && this.view.hasFocus && sel.empty ? sel.head : -1;
      const marks: Range<Decoration>[] = [];
      for (const w of collectCheckableWords(state, this.view.visibleRanges, cursor)) {
        const known = cache.get(w.word);
        if (known === undefined) this.pending.add(w.word);
        else if (known) marks.push(misspelledMark.range(w.from, w.to));
      }
      if (this.pending.size > 0) this.schedule();
      return Decoration.set(marks, true);
    }

    schedule() {
      if (this.timer !== undefined) return;
      this.timer = setTimeout(() => void this.flush(), CHECK_DELAY_MS);
    }

    async flush() {
      this.timer = undefined;
      const config = this.view.state.facet(spellConfig);
      const languages = this.languages;
      const words = [...this.pending];
      this.pending.clear();
      if (!config || words.length === 0 || languages.length === 0) return;
      const generation = cacheGeneration;
      let misspelled: string[];
      try {
        misspelled = await config.check(languages, words);
      } catch (err) {
        console.warn('Spell check failed', err);
        return;
      }
      if (generation !== cacheGeneration || this.destroyed) return;
      const cache = cacheFor(languages);
      const wrong = new Set(misspelled);
      for (const word of words) cache.set(word, wrong.has(word));
      if (!this.destroyed) this.view.dispatch({ effects: resultsArrived.of(null) });
    }

    destroy() {
      this.destroyed = true;
      clearTimeout(this.timer);
    }
  },
  { decorations: (plugin) => plugin.decorations }
);

export function createSpellcheckExtension(config: SpellcheckConfig): Extension {
  return [
    spellConfig.of(config),
    spellPlugin,
    EditorView.contentAttributes.of({ spellcheck: 'false' }),
  ];
}

/** The underlined word at `pos` and the dictionaries its note uses, or null. */
export function misspelledWordAt(
  view: EditorView,
  pos: number
): { from: number; to: number; word: string; languages: string[] } | null {
  const plugin = view.plugin(spellPlugin);
  if (!plugin) return null;
  let found: { from: number; to: number } | null = null;
  plugin.decorations.between(pos, pos, (from, to) => {
    found = { from, to };
    return false;
  });
  const hit = found as { from: number; to: number } | null;
  if (!hit) return null;
  const { from, to } = hit;
  return { from, to, word: view.state.sliceDoc(from, to), languages: plugin.languages };
}

/** Re-runs the plugin so cache changes, such as a word added to the dictionary, show at once. */
export function refreshSpellcheck(view: EditorView) {
  view.dispatch({ effects: resultsArrived.of(null) });
}
