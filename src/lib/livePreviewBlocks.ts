import { markdownLanguage } from '@codemirror/lang-markdown';
import { StateEffect, StateField, type EditorState, type Range, type Extension } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, ViewPlugin, WidgetType } from '@codemirror/view';
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
 * live here instead of in `livePreview.ts`. Unchanged blocks map through edits; changed Markdown reparses after typing pauses.
 */

interface RichBlock {
  kind: 'math' | 'diagram';
  from: number;
  to: number;
  source: string;
}

/** Use Markdown block context so indented code and container markers stay distinct. */
export function findRichBlocks(state: EditorState): RichBlock[] {
  const doc = state.doc;
  const blocks: RichBlock[] = [];
  const tree = markdownLanguage.parser.parse(doc.toString());
  tree.iterate({
    enter(ref) {
      const node = ref.node;
      if (node.name === 'CodeBlock') return false;
      if (node.name === 'FencedCode') {
        const info = node.getChild('CodeInfo');
        if (info && doc.sliceString(info.from, info.to).trim().toLowerCase() === 'mermaid' &&
            node.getChildren('CodeMark').length === 2) {
          const source = node.getChildren('CodeText').map((n) => doc.sliceString(n.from, n.to)).join('');
          blocks.push({ kind: 'diagram', from: doc.lineAt(node.from).from, to: doc.lineAt(node.to).to, source });
        }
        return false;
      }
      if (node.name !== 'Paragraph') return;
      if (!doc.sliceString(node.from, node.to).includes('$$')) return false;
      const lines: { from: number; to: number; text: string }[] = [];
      for (let n = doc.lineAt(node.from).number; n <= doc.lineAt(node.to).number; n++) {
        const line = doc.line(n);
        const text = n === doc.lineAt(node.from).number
          ? line.text.slice(node.from - line.from)
          : line.text.replace(/^\s{0,3}(?:>\s?)+/, '');
        lines.push({ from: line.from, to: line.to, text: text.trim() });
      }
      for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        if (!line.text.startsWith('$$')) continue;
        if (line.text.length > 4 && line.text.endsWith('$$')) {
          blocks.push({kind:'math',from:line.from,to:line.to,source:line.text.slice(2,-2).trim()});
          continue;
        }
        let end = i + 1;
        while (end < lines.length && !lines[end].text.endsWith('$$')) end++;
        if (end === lines.length) continue;
        const source = lines.slice(i, end + 1).map((l) => l.text).join('\n').slice(2,-2).trim();
        blocks.push({kind:'math',from:line.from,to:lines[end].to,source});
        i = end;
      }
      return false;
    },
  });
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

const updateBlocks = StateEffect.define<RichBlock[]>();

export function createRichBlockExtension(): Extension {
  const field = StateField.define<RichBlockState>({
    create(state) {
      const blocks = findRichBlocks(state);
      return { blocks, decorations: buildDecorations(state, blocks) };
    },
    update(value, tr) {
      let blocks = value.blocks;
      if (tr.docChanged) {
        // Keep unaffected widgets in place while the changed Markdown is reparsed after typing.
        blocks = blocks.filter((block) => {
          let touched = false;
          tr.changes.iterChangedRanges((from, to) => {
            if (from <= block.to && to >= block.from) touched = true;
          });
          return !touched;
        }).map((block) => ({...block, from:tr.changes.mapPos(block.from, 1), to:tr.changes.mapPos(block.to, -1)}));
      }
      for (const effect of tr.effects) if (effect.is(updateBlocks)) blocks = effect.value;
      if (!tr.docChanged && !tr.selection && blocks === value.blocks) return value;
      return { blocks, decorations: buildDecorations(tr.state, blocks) };
    },
    provide: (field) => EditorView.decorations.from(field, (value) => value.decorations),
  });
  const parser = ViewPlugin.fromClass(class {
    timer: ReturnType<typeof setTimeout> | undefined;
    update(update: import('@codemirror/view').ViewUpdate) {
      if (!update.docChanged) return;
      clearTimeout(this.timer);
      this.timer = setTimeout(() => {
        update.view.dispatch({effects:updateBlocks.of(findRichBlocks(update.view.state))});
      }, 100);
    }
    destroy() { clearTimeout(this.timer); }
  });
  return [field, parser];
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
