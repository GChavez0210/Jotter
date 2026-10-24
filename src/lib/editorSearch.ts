import { EditorState, type Extension } from '@codemirror/state';
import { EditorView, keymap, type Command } from '@codemirror/view';
import { openSearchPanel, search, searchKeymap } from '@codemirror/search';
import { m } from './i18n.svelte';

/** The labels `@codemirror/search` looks up by their English text. Rebuilt on every locale change. */
export function searchPhrases(): Record<string, string> {
  return {
    Find: m.editor_search_find(),
    Replace: m.editor_search_replace(),
    next: m.editor_search_next(),
    previous: m.editor_search_previous(),
    all: m.editor_search_all(),
    'match case': m.editor_search_match_case(),
    regexp: m.editor_search_regexp(),
    'by word': m.editor_search_by_word(),
    replace: m.editor_search_replace_one(),
    'replace all': m.editor_search_replace_all(),
    close: m.editor_search_close(),
    'current match': m.editor_search_current_match(),
    'on line': m.editor_search_on_line(),
    'replaced $ matches': m.editor_search_replaced_matches(),
    'replaced match on line $': m.editor_search_replaced_on_line(),
    'Go to line': m.editor_search_go_to_line(),
    go: m.editor_search_go(),
  };
}

/** Opens the find panel and moves focus to the replace field, like Ctrl+H elsewhere. */
export const openReplacePanel: Command = (view) => {
  openSearchPanel(view);
  const field = view.dom.querySelector<HTMLInputElement>('.cm-search input[name=replace]');
  field?.focus();
  field?.select();
  return true;
};

/** The search panel, its keymap, and the translated labels for the current locale. */
export function searchExtension(): Extension {
  return [
    search({ top: true }),
    EditorState.phrases.of(searchPhrases()),
    keymap.of([{ key: 'Mod-h', run: openReplacePanel, scope: 'editor search-panel' }, ...searchKeymap]),
  ];
}

/** Panel and match styling that follows the app theme. */
export const searchTheme: Extension = EditorView.theme({
  '.cm-panels': {
    zIndex: '20',
    backgroundColor: 'var(--surface-raised)',
    color: 'var(--text-primary)',
    fontFamily: 'var(--font-ui)',
    fontSize: 'var(--fs-13)',
  },
  '.cm-panels-top': {
    borderBottom: '1px solid var(--line-subtle)',
  },
  '.cm-search': {
    display: 'flex',
    flexWrap: 'wrap',
    alignItems: 'center',
    gap: 'var(--space-2)',
    padding: 'var(--space-2) var(--space-6) var(--space-2) var(--space-3)',
  },
  '.cm-search br': { display: 'none' },
  '.cm-search label': {
    display: 'inline-flex',
    alignItems: 'center',
    gap: '4px',
    color: 'var(--text-secondary)',
  },
  '.cm-search .cm-textfield': {
    padding: '4px var(--space-2)',
    border: '1px solid var(--line-subtle)',
    borderRadius: 'var(--radius-control)',
    backgroundColor: 'var(--surface-app)',
    color: 'var(--text-primary)',
    font: 'inherit',
  },
  '.cm-search .cm-button': {
    padding: '4px var(--space-2)',
    border: '1px solid var(--line-subtle)',
    borderRadius: 'var(--radius-control)',
    backgroundImage: 'none',
    backgroundColor: 'var(--surface-app)',
    color: 'var(--text-primary)',
    font: 'inherit',
  },
  '.cm-search .cm-button:hover': { backgroundColor: 'var(--surface-hover)' },
  '.cm-search button[name=close]': {
    position: 'absolute',
    top: 'var(--space-2)',
    right: 'var(--space-2)',
    border: '0',
    background: 'transparent',
    color: 'var(--text-muted)',
    cursor: 'pointer',
  },
  '.cm-searchMatch': {
    backgroundColor: 'var(--accent-soft)',
    outline: '1px solid var(--accent)',
  },
  '.cm-searchMatch.cm-searchMatch-selected': {
    backgroundColor: 'var(--accent)',
    color: 'var(--on-accent)',
  },
});
