import { foldEffect, foldService, unfoldEffect } from '@codemirror/language';
import type { EditorState, Range } from '@codemirror/state';
import { Decoration, EditorView } from '@codemirror/view';
import { calloutTitle, parseCalloutMarker, type CalloutMarker } from './callouts';
import { LocalizedWidget } from './editorLocale';
import { m } from './i18n.svelte';
import { isFolded } from './outliner';

const QUOTE_LINE_RE = /^\s{0,3}>/;

/** The range a callout folds away: everything after its title line, through its last quote line. */
export function calloutFoldRange(
  state: EditorState,
  titleLineNumber: number
): { from: number; to: number } | null {
  const doc = state.doc;
  const title = doc.line(titleLineNumber);
  if (!parseCalloutMarker(title.text)) return null;
  if (titleLineNumber > 1 && QUOTE_LINE_RE.test(doc.line(titleLineNumber - 1).text)) return null;
  let last = titleLineNumber;
  while (last < doc.lines && QUOTE_LINE_RE.test(doc.line(last + 1).text)) last++;
  return last === titleLineNumber ? null : { from: title.to, to: doc.line(last).to };
}

/** Lets the fold commands and the title chevron fold callouts. */
export const calloutFolding = foldService.of((state, lineStart) =>
  calloutFoldRange(state, state.doc.lineAt(lineStart).number)
);

class CalloutTitleWidget extends LocalizedWidget {
  constructor(
    readonly title: string | null,
    readonly foldable: boolean,
    readonly folded: boolean,
    readonly lineFrom: number
  ) {
    super();
  }

  eq(other: CalloutTitleWidget): boolean {
    return (
      this.locale === other.locale &&
      this.title === other.title &&
      this.foldable === other.foldable &&
      this.folded === other.folded &&
      this.lineFrom === other.lineFrom
    );
  }

  toDOM(view: EditorView): HTMLElement {
    const wrap = document.createElement('span');
    wrap.className = 'cm-live-callout-head';
    if (this.foldable) {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'cm-live-callout-chevron';
      button.textContent = this.folded ? '▸' : '▾';
      button.setAttribute('aria-label', m.callout_toggle());
      button.setAttribute('aria-expanded', String(!this.folded));
      // Keep focus in the editor on mouse use; the toggle runs on click so Enter and Space work too.
      button.addEventListener('mousedown', (e) => e.preventDefault());
      button.addEventListener('keydown', (e) => e.stopPropagation());
      button.addEventListener('click', (e) => {
        e.preventDefault();
        const state = view.state;
        const range = calloutFoldRange(state, state.doc.lineAt(this.lineFrom).number);
        if (!range) return;
        view.dispatch({
          effects: isFolded(state, range.from, range.to)
            ? unfoldEffect.of(range)
            : foldEffect.of(range),
        });
      });
      wrap.appendChild(button);
    }
    if (this.title !== null) {
      const text = document.createElement('span');
      text.textContent = this.title;
      wrap.appendChild(text);
    }
    return wrap;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

/**
 * Replaces the `> [!type]+` marker on a callout's title line. With no custom title the whole line
 * becomes the localized default title, so the line shows a label instead of raw syntax.
 */
export function calloutTitleDecoration(
  state: EditorState,
  line: { from: number; to: number; text: string },
  marker: CalloutMarker
): Range<Decoration> {
  const spaces = /^[ \t]*/.exec(line.text.slice(marker.markerEnd))?.[0].length ?? 0;
  const to = marker.title ? line.from + marker.markerEnd + spaces : line.to;
  const range = marker.fold ? calloutFoldRange(state, state.doc.lineAt(line.from).number) : null;
  const folded = range ? isFolded(state, range.from, range.to) : false;
  return Decoration.replace({
    widget: new CalloutTitleWidget(
      marker.title ? null : calloutTitle(marker),
      marker.fold !== null && range !== null,
      folded,
      line.from
    ),
  }).range(line.from, to);
}
