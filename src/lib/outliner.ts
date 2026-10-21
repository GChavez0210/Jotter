import { editorLocaleChanged, LocalizedWidget } from './editorLocale';
import { isMac, shortcutLabel } from './platform';
import { m } from './i18n.svelte';
import {
  codeFolding,
  foldEffect,
  foldKeymap,
  foldService,
  foldable,
  foldedRanges,
  unfoldEffect,
  syntaxTree,
} from '@codemirror/language';
import type { SyntaxNode } from '@lezer/common';
import {
  ChangeSet,
  EditorSelection,
  EditorState,
  Prec,
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
  keymap,
  ViewPlugin,
  type ViewUpdate,
} from '@codemirror/view';
import { moveLineDown, moveLineUp } from '@codemirror/commands';
import { parseFrontmatter } from './frontmatter';

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

// Frontmatter longer than this is not looked for; nobody writes that much YAML.
const MAX_FRONTMATTER_LINES = 2000;

/** The line number of the closing frontmatter fence, or 0 when the note has no frontmatter. */
function frontmatterLastLine(doc: Text): number {
  if (!/^---[ \t]*$/.test(doc.line(1).text)) return 0;
  for (let n = 2; n <= Math.min(doc.lines, MAX_FRONTMATTER_LINES); n++) {
    if (/^(---|\.\.\.)[ \t]*$/.test(doc.line(n).text)) {
      return parseFrontmatter(doc.sliceString(0, doc.line(n).to)) ? n : 0;
    }
  }
  return 0;
}

/**
 * The closing fence's line, kept per state so the fold service, which runs for every visible
 * list line, does not rescan the top of the note each time. An edit can change it only when it
 * touches the frontmatter or the line after it, or, while a note opens with `---` and has no
 * closing fence yet, any of the lines where that fence could be typed.
 */
const frontmatterEndField = StateField.define<number>({
  create: (state) => frontmatterLastLine(state.doc),
  update(value, tr) {
    if (!tr.docChanged) return value;
    const doc = tr.startState.doc;
    const opensWithFence = /^---[ \t]*$/.test(doc.line(1).text);
    const lastLine = value > 0 ? value + 1 : opensWithFence ? MAX_FRONTMATTER_LINES : 1;
    const limit = doc.line(Math.min(lastLine, doc.lines)).to;
    let touches = false;
    tr.changes.iterChangedRanges((fromA) => {
      if (fromA <= limit) touches = true;
    });
    return touches ? frontmatterLastLine(tr.state.doc) : value;
  },
});

/** The closing frontmatter fence's line, from the cache when the outliner extension is loaded. */
export function frontmatterEndLine(state: EditorState): number {
  return state.field(frontmatterEndField, false) ?? frontmatterLastLine(state.doc);
}

/**
 * True when `- ` on this line is list syntax: not inside a fenced or indented code block, and
 * not inside YAML frontmatter, where `- a` is a YAML list item.
 */
function isListContext(state: EditorState, lineNo: number, fmLastLine = frontmatterEndLine(state)): boolean {
  if (lineNo <= fmLastLine) return false;
  const line = state.doc.line(lineNo);
  for (let node: SyntaxNode | null = syntaxTree(state).resolveInner(line.from, 1); node; node = node.parent) {
    if (node.name === 'FencedCode' || node.name === 'CodeBlock') return false;
  }
  return true;
}

/** The item at `lineNo` when it is a real list item, outside code and frontmatter. */
function listItemAt(state: EditorState, lineNo: number): ListItem | null {
  const item = itemAt(state.doc, lineNo);
  return item && isListContext(state, item.line) ? item : null;
}

const ORDERED_RE = /^([ \t]*)(\d{1,9})([.)])/;

type NumberChange = { from: number; to: number; insert: string };

/**
 * The ordered sibling run that contains the item on `lineNo`, as each item's line and number.
 * Items with the other delimiter (`.` versus `)`) start a new list in CommonMark, so they end
 * the run.
 */
function orderedRun(doc: Text, lineNo: number): Array<{ line: number; digitsFrom: number; number: number; width: number }> {
  const item = itemAt(doc, lineNo);
  const match = item && ORDERED_RE.exec(doc.line(item.line).text);
  if (!item || !match) return [];
  const ordered = (it: ListItem | null) => {
    const m = it && ORDERED_RE.exec(doc.line(it.line).text);
    return m && m[3] === match[3] ? m : null;
  };
  let first = item;
  for (let prev = previousSibling(doc, first); ordered(prev); prev = previousSibling(doc, prev!)) first = prev!;
  const run = [];
  for (let it: ListItem | null = first; it && ordered(it); it = nextSibling(doc, it)) {
    const m = ordered(it)!;
    run.push({ line: it.line, digitsFrom: doc.line(it.line).from + m[1].length, number: Number(m[2]), width: m[2].length });
  }
  return run;
}

/** The first number in the ordered run at `lineNo`, or undefined when there is none. */
function runStart(doc: Text, lineNo: number | undefined): number | undefined {
  const run = lineNo === undefined ? [] : orderedRun(doc, lineNo);
  return run[0]?.number;
}

/** Renumbers the ordered run at `lineNo` from `start`, or from its first number. */
function renumberRun(doc: Text, lineNo: number, start?: number): NumberChange[] {
  const run = orderedRun(doc, lineNo);
  let next = start ?? run[0]?.number ?? 1;
  const changes: NumberChange[] = [];
  for (const r of run) {
    if (r.number !== next) changes.push({ from: r.digitsFrom, to: r.digitsFrom + r.width, insert: String(next) });
    next++;
  }
  return changes;
}

/**
 * Applies `spec`, then renumbers the ordered runs that contain the given lines of the result.
 * Returns one spec whose changes and selection cover both steps.
 */
function withRenumbering(
  state: EditorState,
  spec: TransactionSpec,
  runs: (doc: Text, selectionHead: number) => Array<{ line: number; start?: number }>
): TransactionSpec {
  const tr = state.update({ ...spec, filter: false });
  const doc = tr.state.doc;
  // Keyed by position: two lines can name the same run, and the first request wins.
  const changes = new Map<number, NumberChange>();
  const seen = new Set<number>();
  for (const { line, start } of runs(doc, tr.newSelection.main.head)) {
    if (line < 1 || line > doc.lines) continue;
    const first = orderedRun(doc, line)[0];
    if (!first || seen.has(first.digitsFrom)) continue;
    seen.add(first.digitsFrom);
    const run = renumberRun(doc, line, start);
    for (const change of run) changes.set(change.from, change);
  }
  if (changes.size === 0) return spec;
  const renumber = ChangeSet.of([...changes.values()], doc.length);
  return {
    ...spec,
    changes: tr.changes.compose(renumber),
    selection: tr.newSelection.map(renumber),
  };
}

/** Swaps the item at the cursor with its previous or next sibling, children included. */
export function moveItem(state: EditorState, direction: -1 | 1): TransactionSpec | null {
  const doc = state.doc;
  const head = state.selection.main.head;
  const item = listItemAt(state, doc.lineAt(head).number);
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
  const spec: TransactionSpec = {
    changes: { from: a.from, to: b.to, insert },
    selection: EditorSelection.create(
      state.selection.ranges.map((r) => EditorSelection.range(r.anchor + shift, r.head + shift))
    ),
    scrollIntoView: true,
    userEvent: 'move.line',
  };
  // The two swapped items share one run, which contains the moved item at the cursor.
  return withRenumbering(state, spec, (newDoc, newHead) => [
    { line: newDoc.lineAt(newHead).number, start: runStart(doc, item.line) },
  ]);
}

/** Indents (`1`) or outdents (`-1`) the item at the cursor together with its children. */
export function shiftItem(state: EditorState, direction: -1 | 1): TransactionSpec | null {
  const doc = state.doc;
  const sel = state.selection.main;
  const first = listItemAt(state, doc.lineAt(sel.from).number);
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
  // Shifting changes only indentation, so line numbers stay valid in the new document. Renumber
  // the run the items join, and the old siblings they leave, which keep their old first number.
  const oldStart = runStart(doc, first.line);
  const oldPrevious = previousSibling(doc, first)?.line;
  const oldNext = nextSibling(doc, { ...first, end })?.line;
  return withRenumbering(state, { changes, userEvent: direction > 0 ? 'indent' : 'delete.dedent' }, (newDoc) => {
    const moved = itemOnLine(newDoc, first.line);
    const joined = moved && previousSibling(newDoc, moved);
    // An item that becomes the first of its list, such as one indented into a new sublist,
    // starts it at 1.
    const startsSublist = !(joined && ORDERED_RE.test(newDoc.line(joined.line).text));
    const runs: Array<{ line: number; start?: number }> = [{ line: first.line, start: startsSublist ? 1 : undefined }];
    if (oldPrevious !== undefined) runs.push({ line: oldPrevious, start: oldStart });
    // After an indent, the next siblings stay in the previous sibling's run, handled above.
    // After an outdent, they become the moved item's children: a new list, unless they were
    // the whole run.
    if (oldNext !== undefined) runs.push({ line: oldNext, start: oldPrevious === undefined ? oldStart : 1 });
    return runs;
  });
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

class ZoomBarWidget extends LocalizedWidget {
  constructor(readonly label: string) {
    super();
  }

  eq(other: ZoomBarWidget): boolean {
    return this.locale === other.locale && other.label === this.label;
  }

  toDOM(view: EditorView): HTMLElement {
    const bar = document.createElement('div');
    bar.className = 'cm-zoom-bar';
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = `← ${m.outline_show_whole()}`;
    button.title = m.outline_show_whole_title();
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
    if (zoom === value.zoom && !editorLocaleChanged(tr)) return value;
    // Recompute against the item at the zoom start, so new children typed at the end show.
    // Never shrink below the mapped range: an edit can turn a child into a sibling (an outdent,
    // or Enter on the zoomed line), and hiding it would hide text the user is working on.
    if (zoom) {
      const doc = tr.state.doc;
      const item = itemOnLine(doc, doc.lineAt(zoom.from).number);
      if (item) {
        const block = blockRange(doc, item);
        zoom = { from: block.from, to: Math.max(block.to, doc.lineAt(zoom.to).to) };
      }
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

/** The zoomed range, or null when the whole note shows. */
export function zoomedRange(state: EditorState): ZoomRange | null {
  return state.field(zoomField, false)?.zoom ?? null;
}

/** Zooms into the item at the cursor, or returns null when the cursor is not in a list. */
export function zoomIntoItem(state: EditorState): TransactionSpec | null {
  const item = itemAt(state.doc, state.doc.lineAt(state.selection.main.head).number);
  if (!item) return null;
  return { effects: [setZoom.of(blockRange(state.doc, item)), EditorView.scrollIntoView(0)] };
}

/**
 * Enter at the end of the zoomed item's line adds a first child instead of a sibling. A
 * sibling would sit outside the zoomed item, and the existing children would move under it.
 */
export function insertZoomChild(state: EditorState): TransactionSpec | null {
  const zoom = zoomedRange(state);
  const sel = state.selection.main;
  if (!zoom || !sel.empty || state.selection.ranges.length > 1) return null;
  const line = state.doc.lineAt(zoom.from);
  if (sel.head !== line.to) return null;
  const item = itemOnLine(state.doc, line.number);
  if (!item) return null;
  const marker = ITEM_RE.exec(line.text)![2];
  const childMarker = /\d/.test(marker) ? `1${marker.slice(-1)}` : marker;
  const indent = item.indentText.includes('\t') ? `${item.indentText}\t` : ' '.repeat(item.contentColumn);
  const insert = `\n${indent}${childMarker} `;
  return {
    changes: { from: line.to, insert },
    selection: { anchor: line.to + insert.length },
    scrollIntoView: true,
    userEvent: 'input',
  };
}

/** Zooms into the item at the cursor. On the zoomed item's own line, zooms out one level. */
export function toggleZoom(view: EditorView): boolean {
  const { state } = view;
  const zoom = state.field(zoomField, false)?.zoom;
  const cursorLine = state.doc.lineAt(state.selection.main.head);
  if (zoom && state.doc.lineAt(zoom.from).number === cursorLine.number) return zoomOut(view);
  return runSpec(view, zoomIntoItem(state));
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
  if (!ITEM_RE.test(line.text) || !isListContext(state, line.number)) return null;
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

export function foldCaretAttributes(folded: boolean): Record<string, string> {
  const action = folded ? m.outline_unfold() : m.outline_fold();
  const shortcut = isMac
    ? (folded ? 'Cmd+Option+]' : 'Cmd+Option+[')
    : shortcutLabel(folded ? 'Ctrl+Shift+]' : 'Ctrl+Shift+[');
  return {
    type: 'button',
    'aria-label': action,
    'aria-expanded': String(!folded),
    title: m.outline_action_shortcut({ action, shortcut }),
  };
}

/**
 * A caret in the left margin of each foldable list item. It sits outside the text column, so
 * the note keeps its alignment with the header, which a fold gutter would shift.
 */
export class FoldCaretWidget extends LocalizedWidget {
  constructor(
    readonly from: number,
    readonly to: number,
    readonly folded: boolean
  ) {
    super();
  }

  eq(other: FoldCaretWidget): boolean {
    return this.locale === other.locale && other.from === this.from && other.to === this.to && other.folded === this.folded;
  }

  toDOM(view: EditorView): HTMLElement {
    const anchor = document.createElement('span');
    anchor.className = 'cm-fold-anchor';
    const button = document.createElement('button');
    button.className = this.folded ? 'cm-fold-caret cm-fold-closed' : 'cm-fold-caret';
    for (const [name, value] of Object.entries(foldCaretAttributes(this.folded))) button.setAttribute(name, value);
    button.innerHTML =
      '<svg viewBox="0 0 256 256" width="12" height="12" fill="currentColor" aria-hidden="true"><path d="M213.66,101.66l-80,80a8,8,0,0,1-11.32,0l-80-80A8,8,0,0,1,53.66,90.34L128,164.69l74.34-74.35a8,8,0,0,1,11.32,11.32Z"/></svg>';
    button.addEventListener('click', (e) => {
      e.stopPropagation();
      const range = { from: this.from, to: this.to };
      view.dispatch({ effects: this.folded ? unfoldEffect.of(range) : foldEffect.of(range) });
    });
    // Keep editor Tab/Space bindings from intercepting native button activation and traversal.
    button.addEventListener('keydown', (e) => e.stopPropagation());
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
  const fmLastLine = frontmatterEndLine(state);
  for (const { from, to } of view.visibleRanges) {
    for (let pos = from; pos <= to; ) {
      const line = state.doc.lineAt(pos);
      if (ITEM_RE.test(line.text) && isListContext(state, line.number, fmLastLine)) {
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
      if (update.docChanged || update.viewportChanged || foldsChanged || update.transactions.some(editorLocaleChanged)) {
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
  // Windows reports AltGr as Ctrl+Alt, so `Ctrl-Alt-[` would swallow `[` and `]` on German,
  // French, and other layouts that type brackets with AltGr.
  ...foldKeymap.filter((b) => !b.key?.startsWith('Ctrl-Alt-')),
];

export function createOutlinerExtension(): Extension {
  return [
    frontmatterEndField,
    listFolding,
    codeFolding({ placeholderText: '…' }),
    foldCaretPlugin,
    zoomField,
    clampToZoom,
    // Above the Markdown keymap, whose Enter continues the list with a sibling.
    Prec.highest(keymap.of([{ key: 'Enter', run: (v) => runSpec(v, insertZoomChild(v.state)) }])),
    EditorView.baseTheme({
      '.cm-fold-anchor': { position: 'relative', display: 'inline-block', width: '0' },
      '.cm-fold-caret': {
        border: '0',
        background: 'transparent',
        font: 'inherit',
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
      '.cm-line:hover .cm-fold-caret, .cm-fold-caret.cm-fold-closed, .cm-fold-caret:focus-visible': { opacity: '1' },
      '.cm-fold-caret:focus-visible': { outline: '2px solid var(--text-primary)' },
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
