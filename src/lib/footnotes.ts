import { Decoration, EditorView, WidgetType } from '@codemirror/view';
import type { Range } from '@codemirror/state';

/** Markdown footnotes: `[^id]` references and `[^id]: text` definitions. */

const REF_RE = /\[\^([^\]\s^]+)\]/g;
const DEFINITION_RE = /^\s{0,3}\[\^([^\]\s^]+)\]:/;

export interface FootnoteRef {
  from: number;
  to: number;
  id: string;
}

/** Finds `[^id]` references in one line. A reference at the start of a definition is skipped. */
export function findFootnoteRefs(text: string): FootnoteRef[] {
  const refs: FootnoteRef[] = [];
  for (const match of text.matchAll(REF_RE)) {
    const from = match.index ?? 0;
    const to = from + match[0].length;
    if (text[to] === ':' && /^\s{0,3}$/.test(text.slice(0, from))) continue;
    // An odd run of backslashes escapes the bracket; `\\[^1]` is a literal backslash and a reference.
    let slashes = 0;
    while (from - slashes > 0 && text[from - slashes - 1] === '\\') slashes++;
    if (slashes % 2 === 1) continue;
    refs.push({ from, to, id: match[1] });
  }
  return refs;
}

/** Parses `[^id]: text`. Returns the id and the length of the label through the colon. */
export function parseFootnoteDefinition(text: string): { id: string; labelEnd: number } | null {
  const match = DEFINITION_RE.exec(text);
  return match ? { id: match[1], labelEnd: match[0].length } : null;
}

/** Footnote ids match case-insensitively, as in comrak. */
export function footnoteKey(id: string): string {
  return id.toLowerCase();
}

class FootnoteRefWidget extends WidgetType {
  constructor(
    readonly id: string,
    readonly number: number,
    readonly target: number
  ) {
    super();
  }

  eq(other: FootnoteRefWidget): boolean {
    return other.id === this.id && other.number === this.number && other.target === this.target;
  }

  toDOM(view: EditorView): HTMLElement {
    const sup = document.createElement('sup');
    sup.className = 'cm-live-footnote-ref';
    // Numbered in order of first use, like the export, so both show the same labels.
    sup.textContent = String(this.number);
    sup.title = this.id;
    sup.addEventListener('mousedown', (e) => {
      e.preventDefault();
      view.dispatch({ selection: { anchor: this.target }, scrollIntoView: true });
      view.focus();
    });
    return sup;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

/** Replaces `[^id]` with its number; clicking it moves the cursor to the definition at `target`. */
export function footnoteRefDecoration(
  id: string,
  number: number,
  target: number,
  from: number,
  to: number
): Range<Decoration> {
  return Decoration.replace({ widget: new FootnoteRefWidget(id, number, target) }).range(from, to);
}

/** Footnote styling for exported files; comrak emits a `section.footnotes` list at the end. */
export const EXPORT_FOOTNOTE_CSS = `.footnote-ref{font-size:0.75em;}
.footnote-ref a{text-decoration:none;}
.footnotes{margin-top:2em;padding-top:0.5em;border-top:1px solid currentColor;font-size:0.9em;}
.footnotes ol{padding-left:1.4em;}
.footnotes p{margin-bottom:0.4em;}`;
