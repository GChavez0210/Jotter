import {
  codeFolding,
  foldEffect,
  foldKeymap,
  foldService,
  foldable,
  foldedRanges,
  unfoldEffect,
} from '@codemirror/language';
import {
  EditorSelection,
  EditorState,
  RangeSet,
  StateEffect,
  StateField,
  type Extension,
  type Range,
  type Text,
  type TransactionSpec,
} from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  EditorView,
  type KeyBinding,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
} from '@codemirror/view';
import { moveLineDown, moveLineUp } from '@codemirror/commands';

/**
 * Outliner commands for Markdown lists: fold an item's children, move an item with its
 * children (`Alt+Up`/`Alt+Down`), indent and outdent (`Tab`/`Shift+Tab`), and zoom into one
 * item (`Ctrl+.`, pressed again on that item to zoom out). An item's block is its line plus every following line indented deeper,
 * including blank lines between them.
 */

const ITEM_RE = /^([ \t]*)([-*+]|\d{1,9}[.)])([ \t]+|$)/;

export interface ListItem {
  /** 1-based line numbers of the item line and the last line of its block. */
  line: number;
  end: number;
  indent: number;
  /** Column where the item's text starts, after the marker and its spacing. */
  contentColumn: number;
  indentText: string;
}

function columns(whitespace: string): number {
  let col = 0;
  for (const ch of whitespace) col = ch === '\t' ? col + 4 - (col % 4) : col + 1;
  return col;
}

function lineIndent(text: string): number {
  return columns(/^[ \t]*/.exec(text)![0]);
}

function isBlank(text: string): boolean {
  return text.trim() === '';
}

/** Returns the list item that starts on `lineNo`, or null when that line is not an item. */
export function itemOnLine(doc: Text, lineNo: number): ListItem | null {
  const text = doc.line(lineNo).text;
  const match = ITEM_RE.exec(text);
  if (!match) return null;
  const indent = columns(match[1]);
  let end = lineNo;
  for (let n = lineNo + 1; n <= doc.lines; n++) {
    const next = doc.line(n).text;
    if (isBlank(next)) continue;
    if (lineIndent(next) <= indent) break;
    end = n;
  }
  return {
    line: lineNo,
    end,
    indent,
    contentColumn: columns(match[1]) + match[2].length + Math.max(1, columns(match[3] || ' ')),
    indentText: match[1],
  };
}

/** Finds the item whose block contains `lineNo`: the line itself or the nearest item above. */
export function itemAt(doc: Text, lineNo: number): ListItem | null {
  for (let n = lineNo; n >= 1; n--) {
    const item = itemOnLine(doc, n);
    if (item) return item.end >= lineNo ? item : null;
    const text = doc.line(n).text;
    // A non-indented, non-blank line ends the search: the cursor is outside any list.
    if (!isBlank(text) && lineIndent(text) === 0) return null;
  }
  return null;
}

/** The previous item at the same indent inside the same parent, if any. */
export function previousSibling(doc: Text, item: ListItem): ListItem | null {
  for (let n = item.line - 1; n >= 1; n--) {
    const text = doc.line(n).text;
    if (isBlank(text)) continue;
    const indent = lineIndent(text);
    if (indent > item.indent) continue;
    if (indent < item.indent) return null;
    return itemOnLine(doc, n);
  }
  return null;
}

/** The next item at the same indent inside the same parent, if any. */
export function nextSibling(doc: Text, item: ListItem): ListItem | null {
  for (let n = item.end + 1; n <= doc.lines; n++) {
    const text = doc.line(n).text;
    if (isBlank(text)) continue;
    if (lineIndent(text) !== item.indent) return null;
    return itemOnLine(doc, n);
  }
  return null;
}

/** The item that contains `item` as a child, if any. */
export function parentItem(doc: Text, item: ListItem): ListItem | null {
  for (let n = item.line - 1; n >= 1; n--) {
    const text = doc.line(n).text;
    if (isBlank(text)) continue;
    if (lineIndent(text) < item.indent) {
      const parent = itemOnLine(doc, n);
      return parent && parent.end >= item.line ? parent : null;
    }
  }
  return null;
}

function blockRange(doc: Text, item: ListItem) {
  return { from: doc.line(item.line).from, to: doc.line(item.end).to };
}

/** Swaps the item at the cursor with its previous or next sibling, children included. */
export function moveItem(state: EditorState, direction: -1 | 1): TransactionSpec | null {
  const doc = state.doc;
  const head = state.selection.main.head;
  const item = itemAt(doc, doc.lineAt(head).number);
  if (!item) return null;
  const other = direction < 0 ? previousSibling(doc, item) : nextSibling(doc, item);
  if (!other) return { selection: state.selection };
  const [first, second] = direction < 0 ? [other, item] : [item, other];
  const a = blockRange(doc, first);
  const b = blockRange(doc, second);
  const between = doc.sliceString(a.to, b.from);
  const insert = doc.sliceString(b.from, b.to) + between + doc.sliceString(a.from, a.to);
  const moved = blockRange(doc, item);
  const shift = direction < 0 ? a.from - moved.from : b.to - moved.to;
  return {
    changes: { from: a.from, to: b.to, insert },
    selection: EditorSelection.create(
      state.selection.ranges.map((r) => EditorSelection.range(r.anchor + shift, r.head + shift))
    ),
    scrollIntoView: true,
    userEvent: 'move.line',
  };
}

/** Indents (`1`) or outdents (`-1`) the item at the cursor together with its children. */
export function shiftItem(state: EditorState, direction: -1 | 1): TransactionSpec | null {
  const doc = state.doc;
  const sel = state.selection.main;
  const first = itemAt(doc, doc.lineAt(sel.from).number);
  if (!first) return null;
  const lastLine = doc.lineAt(sel.to).number;

  let delta: number;
  let unit = ' ';
  if (direction > 0) {
    const sibling = previousSibling(doc, first);
    // The first child cannot become deeper: there is no item to nest it under.
    if (!sibling) return { selection: state.selection };
    delta = sibling.contentColumn - first.indent;
    if (doc.line(sibling.line).text.startsWith('\t') || first.indentText.includes('\t')) {
      unit = '\t';
      delta = 1;
    }
  } else {
    const parent = parentItem(doc, first);
    delta = parent ? first.indent - parent.indent : first.indent;
    if (delta === 0) return { selection: state.selection };
  }

  // Shift every item the selection touches, each with its children.
  const end = Math.max(first.end, itemAt(doc, lastLine)?.end ?? first.end);
  const changes = [];
  for (let n = first.line; n <= end; n++) {
    const line = doc.line(n);
    if (isBlank(line.text)) continue;
    if (direction > 0) {
      changes.push({ from: line.from, insert: unit.repeat(delta) });
    } else {
      let removeChars = 0;
      let removed = 0;
      while (removed < delta && removeChars < line.text.length) {
        const ch = line.text[removeChars];
        if (ch === ' ') removed++;
        else if (ch === '\t') removed += 4;
        else break;
        removeChars++;
      }
      if (removeChars > 0) changes.push({ from: line.from, to: line.from + removeChars });
    }
  }
  return { changes, userEvent: direction > 0 ? 'indent' : 'delete.dedent' };
}

function runSpec(view: EditorView, spec: TransactionSpec | null): boolean {
  if (!spec) return false;
  view.dispatch(spec);
  return true;
}

// Zoom ----------------------------------------------------------------------------------

interface ZoomRange {
  from: number;
  to: number;
}

const setZoom = StateEffect.define<ZoomRange | null>();

class ZoomBarWidget extends WidgetType {
  constructor(readonly label: string) {
    super();
  }

  eq(other: ZoomBarWidget): boolean {
    return other.label === this.label;
  }

  toDOM(view: EditorView): HTMLElement {
    const bar = document.createElement('div');
    bar.className = 'cm-zoom-bar';
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = '← Show whole note';
    button.title = 'Show the whole note';
    button.addEventListener('mousedown', (e) => {
      e.preventDefault();
      view.dispatch({ effects: setZoom.of(null) });
      view.focus();
    });
    const crumb = document.createElement('span');
    crumb.textContent = this.label;
    bar.append(button, crumb);
    return bar;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

function zoomDecorations(state: EditorState, zoom: ZoomRange | null): DecorationSet {
  if (!zoom) return Decoration.none;
  const doc = state.doc;
  const startLine = doc.lineAt(zoom.from);
  const endLine = doc.lineAt(zoom.to);
  const crumb = startLine.text.replace(ITEM_RE, '').trim().slice(0, 80);
  const widget = new ZoomBarWidget(crumb);
  const ranges: Range<Decoration>[] = [];
  if (startLine.number > 1) {
    ranges.push(Decoration.replace({ block: true, widget }).range(0, startLine.from - 1));
  } else {
    ranges.push(Decoration.widget({ block: true, widget, side: -1 }).range(0));
  }
  if (endLine.number < doc.lines) {
    ranges.push(Decoration.replace({ block: true }).range(endLine.to + 1, doc.length));
  }
  return Decoration.set(ranges);
}

const zoomField = StateField.define<{ zoom: ZoomRange | null; decorations: DecorationSet }>({
  create: () => ({ zoom: null, decorations: Decoration.none }),
  update(value, tr) {
    let zoom = value.zoom;
    if (zoom && tr.docChanged) {
      zoom = { from: tr.changes.mapPos(zoom.from, -1), to: tr.changes.mapPos(zoom.to, 1) };
    }
    for (const effect of tr.effects) if (effect.is(setZoom)) zoom = effect.value;
    if (zoom === value.zoom) return value;
    // Recompute against the item at the zoom start, so new children typed at the end show.
    if (zoom) {
      const item = itemOnLine(tr.state.doc, tr.state.doc.lineAt(zoom.from).number);
      if (item) zoom = blockRange(tr.state.doc, item);
    }
    return { zoom, decorations: zoomDecorations(tr.state, zoom) };
  },
  provide: (f) => [
    EditorView.decorations.from(f, (v) => v.decorations),
    EditorView.atomicRanges.of((view) => view.state.field(f).decorations ?? RangeSet.empty),
  ],
});

// Keep the cursor inside the zoomed item, so typing never lands in hidden text.
const clampToZoom = EditorState.transactionFilter.of((tr) => {
  const zoom = tr.state.field(zoomField).zoom;
  if (!zoom || !tr.selection) return tr;
  const sel = tr.newSelection.main;
  if (sel.from >= zoom.from && sel.to <= zoom.to) return tr;
  const anchor = Math.min(Math.max(sel.head, zoom.from), zoom.to);
  return [tr, { selection: { anchor }, sequential: true }];
});

/** Zooms into the item at the cursor. On the zoomed item's own line, zooms out one level. */
export function toggleZoom(view: EditorView): boolean {
  const { state } = view;
  const zoom = state.field(zoomField, false)?.zoom;
  const cursorLine = state.doc.lineAt(state.selection.main.head);
  if (zoom && state.doc.lineAt(zoom.from).number === cursorLine.number) return zoomOut(view);
  const item = itemAt(state.doc, cursorLine.number);
  if (!item) return false;
  view.dispatch({
    effects: [setZoom.of(blockRange(state.doc, item)), EditorView.scrollIntoView(0)],
  });
  return true;
}

export function zoomOut(view: EditorView): boolean {
  const zoom = view.state.field(zoomField, false)?.zoom;
  if (!zoom) return false;
  // Step out to the parent item when there is one, otherwise show the whole note.
  const item = itemOnLine(view.state.doc, view.state.doc.lineAt(zoom.from).number);
  const parent = item ? parentItem(view.state.doc, item) : null;
  view.dispatch({
    effects: [
      setZoom.of(parent ? blockRange(view.state.doc, parent) : null),
      EditorView.scrollIntoView(view.state.selection.main.head, { y: 'center' }),
    ],
  });
  return true;
}

// Folding ---------------------------------------------------------------------------------

const listFolding = foldService.of((state, lineStart) => {
  const line = state.doc.lineAt(lineStart);
  if (!ITEM_RE.test(line.text)) return null;
  const item = itemOnLine(state.doc, line.number);
  if (!item || item.end === item.line) return null;
  return { from: line.to, to: state.doc.line(item.end).to };
});

function isFolded(state: EditorState, from: number, to: number): boolean {
  let found = false;
  foldedRanges(state).between(from, to, (a, b) => {
    if (a === from && b === to) found = true;
  });
  return found;
}

/**
 * A caret in the left margin of each foldable list item. It sits outside the text column, so
 * the note keeps its alignment with the header, which a fold gutter would shift.
 */
class FoldCaretWidget extends WidgetType {
  constructor(
    readonly from: number,
    readonly to: number,
    readonly folded: boolean
  ) {
    super();
  }

  eq(other: FoldCaretWidget): boolean {
    return other.from === this.from && other.to === this.to && other.folded === this.folded;
  }

  toDOM(view: EditorView): HTMLElement {
    const anchor = document.createElement('span');
    anchor.className = 'cm-fold-anchor';
    const button = document.createElement('span');
    button.className = this.folded ? 'cm-fold-caret cm-fold-closed' : 'cm-fold-caret';
    button.setAttribute('role', 'button');
    button.setAttribute('aria-label', this.folded ? 'Unfold list item' : 'Fold list item');
    button.title = this.folded ? 'Unfold' : 'Fold';
    button.innerHTML =
      '<svg viewBox="0 0 256 256" width="12" height="12" fill="currentColor" aria-hidden="true"><path d="M213.66,101.66l-80,80a8,8,0,0,1-11.32,0l-80-80A8,8,0,0,1,53.66,90.34L128,164.69l74.34-74.35a8,8,0,0,1,11.32,11.32Z"/></svg>';
    button.addEventListener('mousedown', (e) => {
      e.preventDefault();
      const range = { from: this.from, to: this.to };
      view.dispatch({ effects: this.folded ? unfoldEffect.of(range) : foldEffect.of(range) });
    });
    anchor.append(button);
    return anchor;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

function foldCarets(view: EditorView): DecorationSet {
  const ranges: Range<Decoration>[] = [];
  const { state } = view;
  for (const { from, to } of view.visibleRanges) {
    for (let pos = from; pos <= to; ) {
      const line = state.doc.lineAt(pos);
      if (ITEM_RE.test(line.text)) {
        const range = foldable(state, line.from, line.to);
        if (range) {
          const widget = new FoldCaretWidget(range.from, range.to, isFolded(state, range.from, range.to));
          // Place the caret just left of the marker, so nested items get their own column.
          const markerAt = line.from + /^\s*/.exec(line.text)![0].length;
          ranges.push(Decoration.widget({ widget, side: -1 }).range(markerAt));
        }
      }
      pos = line.to + 1;
    }
  }
  return Decoration.set(ranges);
}

const foldCaretPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = foldCarets(view);
    }
    update(update: ViewUpdate) {
      const foldsChanged = update.transactions.some((tr) =>
        tr.effects.some((e) => e.is(foldEffect) || e.is(unfoldEffect))
      );
      if (update.docChanged || update.viewportChanged || foldsChanged) {
        this.decorations = foldCarets(update.view);
      }
    }
  },
  { decorations: (v) => v.decorations }
);

export const outlinerKeymap: KeyBinding[] = [
  { key: 'Alt-ArrowUp', run: (v) => runSpec(v, moveItem(v.state, -1)) || moveLineUp(v) },
  { key: 'Alt-ArrowDown', run: (v) => runSpec(v, moveItem(v.state, 1)) || moveLineDown(v) },
  { key: 'Tab', run: (v) => runSpec(v, shiftItem(v.state, 1)) },
  { key: 'Shift-Tab', run: (v) => runSpec(v, shiftItem(v.state, -1)) },
  { key: 'Mod-.', run: toggleZoom },
  ...foldKeymap,
];

export function createOutlinerExtension(): Extension {
  return [
    listFolding,
    codeFolding({ placeholderText: '…' }),
    foldCaretPlugin,
    zoomField,
    clampToZoom,
    EditorView.baseTheme({
      '.cm-fold-anchor': { position: 'relative', display: 'inline-block', width: '0' },
      '.cm-fold-caret': {
        position: 'absolute',
        right: '6px',
        top: '-0.95em',
        display: 'inline-flex',
        padding: '2px',
        borderRadius: '3px',
        color: 'var(--text-muted)',
        opacity: '0',
        cursor: 'pointer',
        transition: 'opacity 120ms',
      },
      '.cm-line:hover .cm-fold-caret, .cm-fold-caret.cm-fold-closed': { opacity: '1' },
      '.cm-fold-caret:hover': { color: 'var(--text-primary)', background: 'var(--surface-app)' },
      '.cm-fold-closed svg': { transform: 'rotate(-90deg)' },
      '.cm-foldPlaceholder': {
        margin: '0 4px',
        padding: '0 6px',
        border: 'none',
        borderRadius: '8px',
        background: 'var(--accent-soft)',
        color: 'var(--text-secondary)',
      },
      '.cm-zoom-bar': {
        display: 'flex',
        alignItems: 'center',
        gap: '10px',
        padding: '4px 0 10px',
        color: 'var(--text-muted)',
        fontFamily: 'var(--font-ui)',
        fontSize: 'var(--fs-12)',
      },
      '.cm-zoom-bar button': {
        padding: '2px 8px',
        border: '1px solid var(--line-subtle)',
        borderRadius: '4px',
        background: 'transparent',
        color: 'var(--text-secondary)',
        font: 'inherit',
        cursor: 'pointer',
      },
    }),
  ];
}
