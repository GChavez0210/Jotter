import { editorLocaleChanged, LocalizedWidget } from './editorLocale';
import { syntaxTree } from '@codemirror/language';
import { m } from './i18n.svelte';
import { type EditorState, type Extension, type Range } from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
} from '@codemirror/view';
import { evaluateCalcBlock, inlineCalculation } from './calc';

/**
 * Shows results for inline `12*4 =` expressions and for each line of ```` ```calc ```` blocks.
 * Results are widgets, so the file only changes when the user accepts an inline result with
 * `Tab` or a click. Work is bounded to the visible ranges, like Live Preview.
 */

class InlineResultWidget extends LocalizedWidget {
  constructor(
    readonly result: string,
    readonly insertAt: number
  ) {
    super();
  }

  eq(other: InlineResultWidget): boolean {
    return this.locale === other.locale && other.result === this.result && other.insertAt === this.insertAt;
  }

  toDOM(view: EditorView): HTMLElement {
    const span = document.createElement('span');
    span.className = 'cm-calc-inline';
    span.textContent = this.result;
    span.title = m.calc_insert_hint();
    span.addEventListener('mousedown', (e) => {
      e.preventDefault();
      acceptAt(view, this.insertAt, this.result);
    });
    return span;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

class BlockResultWidget extends LocalizedWidget {
  constructor(
    readonly text: string,
    readonly error: boolean
  ) {
    super();
  }

  eq(other: BlockResultWidget): boolean {
    return this.locale === other.locale && other.text === this.text && other.error === this.error;
  }

  toDOM(): HTMLElement {
    const span = document.createElement('span');
    span.className = this.error ? 'cm-calc-result cm-calc-error' : 'cm-calc-result';
    span.textContent = this.error ? '?' : `= ${this.text}`;
    if (this.error) span.title = this.text;
    return span;
  }
}

function acceptAt(view: EditorView, pos: number, result: string) {
  const line = view.state.doc.lineAt(pos);
  const before = line.text.slice(0, pos - line.from);
  const insert = /\s$/.test(before) ? result : ` ${result}`;
  view.dispatch({
    changes: { from: pos, to: line.to, insert },
    selection: { anchor: pos + insert.length },
    userEvent: 'input.calc',
  });
  view.focus();
}

/** True when `pos` sits inside a fenced or indented code block or inline code span. */
function inCode(state: EditorState, pos: number): boolean {
  for (let node = syntaxTree(state).resolveInner(pos, -1); node.parent; node = node.parent) {
    if (/^(FencedCode|CodeBlock|InlineCode|CodeText)$/.test(node.name)) return true;
  }
  return false;
}

function calcBlockLines(state: EditorState, node: { from: number; to: number }) {
  const doc = state.doc;
  const first = doc.lineAt(node.from).number + 1;
  const last = doc.lineAt(node.to).number;
  // Without a closing fence, the block runs to the end of the document.
  const closed = /^\s*(```|~~~)\s*$/.test(doc.line(last).text) && last >= first;
  const end = closed ? last - 1 : last;
  const lines = [];
  for (let n = first; n <= end; n++) lines.push(doc.line(n));
  return lines;
}

function buildDecorations(view: EditorView): DecorationSet {
  const { state } = view;
  const ranges: Range<Decoration>[] = [];
  const seenBlocks = new Set<number>();
  for (const { from, to } of view.visibleRanges) {
    syntaxTree(state).iterate({
      from,
      to,
      enter(ref) {
        if (ref.name !== 'FencedCode') return;
        if (seenBlocks.has(ref.from)) return false;
        seenBlocks.add(ref.from);
        const info = ref.node.getChild('CodeInfo');
        if (!info || state.doc.sliceString(info.from, info.to).trim().toLowerCase() !== 'calc') {
          return false;
        }
        const lines = calcBlockLines(state, ref);
        for (const r of evaluateCalcBlock(lines.map((l) => l.text))) {
          const line = lines[r.line];
          const widget = new BlockResultWidget(r.result ?? r.error ?? '', r.error !== undefined);
          ranges.push(Decoration.widget({ widget, side: 1 }).range(line.to));
        }
        return false;
      },
    });

    for (let pos = from; pos <= to; ) {
      const line = state.doc.lineAt(pos);
      if (line.text.trimEnd().endsWith('=') && !inCode(state, line.to)) {
        const found = inlineCalculation(line.text);
        if (found) {
          const end = line.from + line.text.trimEnd().length;
          ranges.push(
            Decoration.widget({ widget: new InlineResultWidget(found.result, end), side: 1 }).range(end)
          );
        }
      }
      pos = line.to + 1;
    }
  }
  return Decoration.set(ranges, true);
}

/** Inserts the pending inline result when the cursor sits right after `=`. */
export function acceptInlineCalculation(view: EditorView): boolean {
  const sel = view.state.selection.main;
  if (!sel.empty) return false;
  const line = view.state.doc.lineAt(sel.head);
  const end = line.from + line.text.trimEnd().length;
  if (sel.head < end || inCode(view.state, line.to)) return false;
  const found = inlineCalculation(line.text);
  if (!found) return false;
  acceptAt(view, end, found.result);
  return true;
}

export function createCalcExtension(): Extension {
  return [
    ViewPlugin.fromClass(
      class {
        decorations: DecorationSet;
        constructor(view: EditorView) {
          this.decorations = buildDecorations(view);
        }
        update(update: ViewUpdate) {
          if (
            update.docChanged ||
            update.transactions.some(editorLocaleChanged) ||
            update.viewportChanged ||
            syntaxTree(update.startState) !== syntaxTree(update.state)
          ) {
            this.decorations = buildDecorations(update.view);
          }
        }
      },
      { decorations: (v) => v.decorations }
    ),
    EditorView.baseTheme({
      '.cm-calc-inline': {
        marginLeft: '0.35em',
        color: 'var(--accent)',
        opacity: '0.75',
        cursor: 'pointer',
      },
      '.cm-calc-inline:hover': { opacity: '1', textDecoration: 'underline' },
      '.cm-calc-result': {
        marginLeft: '1.5em',
        color: 'var(--accent)',
        fontVariantNumeric: 'tabular-nums',
      },
      '.cm-calc-error': { color: 'var(--text-muted)', cursor: 'help' },
    }),
  ];
}
