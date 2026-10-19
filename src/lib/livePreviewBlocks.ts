import { StateField, type EditorState, type Range, type Extension } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, WidgetType } from '@codemirror/view';
import {
  cachedDiagram,
  cachedMath,
  renderDiagram,
  renderMath,
} from './richBlocks';

/**
 * Block-level Live Preview: `$$` math blocks and ```` ```mermaid ```` diagrams.
 *
 * CodeMirror only accepts block widgets from a state field, not from a view plugin, so these
 * live here instead of in `livePreview.ts`. The block list is rebuilt from a line scan on
 * document changes only; selection changes reuse it.
 */

interface RichBlock {
  kind: 'math' | 'diagram';
  from: number;
  to: number;
  source: string;
}

const FENCE_RE = /^\s{0,3}(`{3,}|~{3,})\s*([^\s`]*)/;

/** Finds `$$` blocks and mermaid fences. Text inside other fenced code is skipped. */
export function findRichBlocks(state: EditorState): RichBlock[] {
  const doc = state.doc;
  const blocks: RichBlock[] = [];
  let ln = 1;
  while (ln <= doc.lines) {
    const line = doc.line(ln);
    const fence = FENCE_RE.exec(line.text);
    if (fence) {
      const marker = fence[1];
      const isMermaid = fence[2].toLowerCase() === 'mermaid';
      let end = ln + 1;
      while (end <= doc.lines) {
        const closing = doc.line(end).text.trim();
        if (closing.startsWith(marker[0].repeat(marker.length)) && /^[`~]+$/.test(closing)) break;
        end++;
      }
      if (isMermaid && end <= doc.lines) {
        const body = end > ln + 1 ? doc.sliceString(doc.line(ln + 1).from, doc.line(end - 1).to) : '';
        blocks.push({ kind: 'diagram', from: line.from, to: doc.line(end).to, source: body });
      }
      ln = end + 1;
      continue;
    }

    const trimmed = line.text.trim();
    if (trimmed.startsWith('$$')) {
      // One-line block: $$ x^2 $$
      if (trimmed.length > 4 && trimmed.endsWith('$$')) {
        blocks.push({ kind: 'math', from: line.from, to: line.to, source: trimmed.slice(2, -2).trim() });
        ln++;
        continue;
      }
      let end = ln + 1;
      while (end <= doc.lines && !doc.line(end).text.trim().endsWith('$$')) end++;
      if (end <= doc.lines) {
        const lastLine = doc.line(end);
        const tex = doc
          .sliceString(line.from, lastLine.to)
          .trim()
          .replace(/^\$\$/, '')
          .replace(/\$\$$/, '')
          .trim();
        blocks.push({ kind: 'math', from: line.from, to: lastLine.to, source: tex });
        ln = end + 1;
        continue;
      }
    }
    ln++;
  }
  return blocks;
}

class RichBlockWidget extends WidgetType {
  constructor(
    readonly kind: RichBlock['kind'],
    readonly source: string,
    readonly editFrom: number | null
  ) {
    super();
  }

  eq(other: RichBlockWidget): boolean {
    return (
      other.kind === this.kind &&
      other.source === this.source &&
      other.editFrom === this.editFrom
    );
  }

  get estimatedHeight(): number {
    return this.kind === 'math' ? 48 : 160;
  }

  toDOM(view: EditorView): HTMLElement {
    const el = document.createElement('div');
    el.className =
      this.kind === 'math' ? 'cm-rich-block cm-math-block' : 'cm-rich-block cm-diagram-block';
    if (this.editFrom !== null) {
      el.title = 'Click to edit';
      const editAt = this.editFrom;
      el.addEventListener('mousedown', (e) => {
        e.preventDefault();
        view.dispatch({ selection: { anchor: editAt } });
        view.focus();
      });
    } else {
      el.classList.add('cm-rich-block-preview');
    }

    const cached =
      this.kind === 'math' ? cachedMath(this.source, true) : cachedDiagram(this.source);
    if (cached !== undefined) {
      el.innerHTML = cached;
      return el;
    }
    el.classList.add('cm-rich-block-loading');
    const pending =
      this.kind === 'math' ? renderMath(this.source, true) : renderDiagram(this.source);
    void pending.then((html) => {
      el.innerHTML = html;
      el.classList.remove('cm-rich-block-loading');
      view.requestMeasure();
    });
    return el;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

function touchesSelection(state: EditorState, from: number, to: number): boolean {
  return state.selection.ranges.some((r) => r.from <= to && r.to >= from);
}

function buildDecorations(state: EditorState, blocks: RichBlock[]): DecorationSet {
  const ranges: Range<Decoration>[] = [];
  for (const block of blocks) {
    if (touchesSelection(state, block.from, block.to)) {
      // While editing, show the source and a live preview below it.
      if (block.source.trim()) {
        ranges.push(
          Decoration.widget({
            widget: new RichBlockWidget(block.kind, block.source, null),
            block: true,
            side: 1,
          }).range(block.to)
        );
      }
    } else {
      ranges.push(
        Decoration.replace({
          widget: new RichBlockWidget(block.kind, block.source, block.from),
          block: true,
        }).range(block.from, block.to)
      );
    }
  }
  return Decoration.set(ranges, true);
}

interface RichBlockState {
  blocks: RichBlock[];
  decorations: DecorationSet;
}

export function createRichBlockExtension(): Extension {
  return StateField.define<RichBlockState>({
    create(state) {
      const blocks = findRichBlocks(state);
      return { blocks, decorations: buildDecorations(state, blocks) };
    },
    update(value, tr) {
      if (!tr.docChanged && !tr.selection) return value;
      const blocks = tr.docChanged ? findRichBlocks(tr.state) : value.blocks;
      return { blocks, decorations: buildDecorations(tr.state, blocks) };
    },
    provide: (field) => EditorView.decorations.from(field, (value) => value.decorations),
  });
}

class InlineMathWidget extends WidgetType {
  constructor(
    readonly tex: string,
    readonly editAt: number
  ) {
    super();
  }

  eq(other: InlineMathWidget): boolean {
    return other.tex === this.tex && other.editAt === this.editAt;
  }

  toDOM(view: EditorView): HTMLElement {
    const span = document.createElement('span');
    span.className = 'cm-math-inline';
    const cached = cachedMath(this.tex, false);
    if (cached !== undefined) {
      span.innerHTML = cached;
    } else {
      span.textContent = this.tex;
      void renderMath(this.tex, false).then((html) => {
        span.innerHTML = html;
      });
    }
    span.addEventListener('mousedown', (e) => {
      e.preventDefault();
      view.dispatch({ selection: { anchor: this.editAt } });
      view.focus();
    });
    return span;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

/**
 * Finds `$…$` inline math in one line, following Pandoc's rules so prices stay text: the
 * opening `$` is not followed by a space, the closing `$` is not preceded by a space or followed
 * by a digit, and neither is escaped or part of `$$`.
 */
export function findInlineMath(text: string): Array<{ from: number; to: number; tex: string }> {
  const found: Array<{ from: number; to: number; tex: string }> = [];
  const isEscaped = (i: number) => {
    let n = 0;
    for (let j = i - 1; j >= 0 && text[j] === '\\'; j--) n++;
    return n % 2 === 1;
  };
  let i = 0;
  while (i < text.length) {
    if (text[i] !== '$' || isEscaped(i) || text[i + 1] === '$' || text[i - 1] === '$') {
      i++;
      continue;
    }
    if (!text[i + 1] || /\s/.test(text[i + 1])) {
      i++;
      continue;
    }
    let close = -1;
    for (let j = i + 1; j < text.length; j++) {
      if (text[j] !== '$' || isEscaped(j)) continue;
      if (text[j + 1] === '$') break;
      if (!/\s/.test(text[j - 1]) && !/\d/.test(text[j + 1] ?? '')) close = j;
      break;
    }
    if (close === -1) {
      i++;
      continue;
    }
    found.push({ from: i, to: close + 1, tex: text.slice(i + 1, close) });
    i = close + 1;
  }
  return found;
}

export function inlineMathDecoration(tex: string, from: number, to: number): Range<Decoration> {
  return Decoration.replace({ widget: new InlineMathWidget(tex, from + 1) }).range(from, to);
}
