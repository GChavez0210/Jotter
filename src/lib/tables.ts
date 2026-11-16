import { ensureSyntaxTree } from '@codemirror/language';
import {
  Prec,
  type ChangeSpec,
  type EditorState,
  type Extension,
  type TransactionSpec,
} from '@codemirror/state';
import { keymap, type EditorView, type KeyBinding } from '@codemirror/view';
import type { SyntaxNode } from '@lezer/common';

/**
 * Markdown table editing: Tab, Shift+Tab, and Enter move between cells and reformat the
 * table, and commands insert, delete, and reorder rows and columns or set column alignment.
 *
 * Every edit parses the table under the cursor, changes the model, and writes the model back
 * as one transaction, so one `Ctrl+Z` reverts one operation. Cell text is never rewritten
 * beyond trimming the padding around it, which keeps escaped pipes, inline code, and HTML
 * intact. A table with a row wider than its header is left alone, because reformatting it
 * would either drop text or add columns the author did not write.
 */

export type Align = 'none' | 'left' | 'center' | 'right';

export type TableCommandId =
  | 'format'
  | 'row_above'
  | 'row_below'
  | 'row_delete'
  | 'row_up'
  | 'row_down'
  | 'col_left'
  | 'col_right'
  | 'col_delete'
  | 'col_move_left'
  | 'col_move_right'
  | 'align_none'
  | 'align_left'
  | 'align_center'
  | 'align_right';

export interface TableModel {
  header: string[];
  align: Align[];
  body: string[][];
}

/** A cell as it sits in the source line, with offsets relative to the line start. */
interface SourceCell {
  text: string;
  /** Offset of the first character of the cell's text, after the padding. */
  contentStart: number;
  /** Offset just after the cell's raw span, which is where the next pipe sits. */
  rawEnd: number;
}

export interface ParsedTable extends TableModel {
  /** 1-based line numbers of the header and the last row. */
  firstLine: number;
  lastLine: number;
  /** Indentation and blockquote markers that every line of the table starts with. */
  prefix: string;
  /** Cursor cell. `line` is 0 for the header, 1 for the delimiter row, 2+ for body rows. */
  line: number;
  col: number;
  /** Cursor offset inside the cell's text, clamped to it. */
  offset: number;
}

interface Focus {
  line: number;
  col: number;
  offset: number;
}

const PREFIX_RE = /^(?:[ \t]*>)*[ \t]*/;
const DELIMITER_CELL_RE = /^[ \t]*(:?)-+(:?)[ \t]*$/;
const MIN_COLUMN_WIDTH = 3;

// Parsing -------------------------------------------------------------------------------

/**
 * Finds the unescaped pipes in a row. Like GitHub and the editor's own Markdown parser, it
 * treats a pipe inside an inline code span as a cell boundary unless a backslash escapes it,
 * so `\|` is the way to write one in code.
 */
function delimiterPipes(text: string): number[] {
  const pipes: number[] = [];
  for (let i = 0; i < text.length; i++) {
    if (text[i] === '\\') i++;
    else if (text[i] === '|') pipes.push(i);
  }
  return pipes;
}

/** Splits one row, given without its prefix, into cells. Offsets are relative to `text`. */
export function splitRow(text: string): SourceCell[] {
  const pipes = delimiterPipes(text);
  const firstNonSpace = text.length - text.trimStart().length;
  const lastNonSpace = text.trimEnd().length;
  const leading = pipes.length > 0 && pipes[0] === firstNonSpace;
  const trailing = pipes.length > 0 && pipes[pipes.length - 1] === lastNonSpace - 1;

  // Each cell runs from just after one pipe to the next pipe, or to the end of the text.
  const rest = leading ? pipes.slice(1) : pipes;
  const ends = trailing && rest.length > 0 ? rest : [...rest, text.length];
  const cells: SourceCell[] = [];
  let start = leading ? pipes[0] + 1 : 0;
  for (const end of ends) {
    const raw = text.slice(start, end);
    const lead = raw.length - raw.trimStart().length;
    const content = raw.trim();
    cells.push({ text: content, contentStart: start + (content ? lead : 0), rawEnd: end });
    start = end + 1;
  }
  return cells;
}

function parseAlignment(cells: SourceCell[], columns: number): Align[] | null {
  if (cells.length !== columns) return null;
  const align: Align[] = [];
  for (const cell of cells) {
    const match = DELIMITER_CELL_RE.exec(cell.text);
    if (!match) return null;
    const [, left, right] = match;
    align.push(left && right ? 'center' : left ? 'left' : right ? 'right' : 'none');
  }
  return align;
}

/** The 1-based line range of the GFM table that contains `pos`, from the syntax tree. */
function tableLineRange(state: EditorState, pos: number): [number, number] | null {
  const line = state.doc.lineAt(pos);
  if (line.length === 0) return null;
  const tree = ensureSyntaxTree(state, line.to, 25);
  if (!tree) return null;
  for (let node: SyntaxNode | null = tree.resolveInner(line.to, -1); node; node = node.parent) {
    if (node.name === 'Table') {
      return [state.doc.lineAt(node.from).number, state.doc.lineAt(node.to).number];
    }
  }
  return null;
}

/**
 * Parses the table under the main cursor. Returns `null` when the cursor is outside a
 * table, the selection spans several lines, or the table is irregular.
 */
export function parseTableAt(state: EditorState): ParsedTable | null {
  const sel = state.selection.main;
  const cursorLine = state.doc.lineAt(sel.head);
  if (state.doc.lineAt(sel.anchor).number !== cursorLine.number) return null;
  const range = tableLineRange(state, sel.head);
  if (!range) return null;
  const [firstLine, lastLine] = range;
  if (lastLine - firstLine < 1) return null;

  const rows: SourceCell[][] = [];
  let prefix: string | null = null;
  for (let n = firstLine; n <= lastLine; n++) {
    const text = state.doc.line(n).text;
    const linePrefix = PREFIX_RE.exec(text)![0];
    if (prefix === null) prefix = linePrefix;
    else if (prefix !== linePrefix) return null;
    rows.push(splitRow(text.slice(linePrefix.length)).map((cell) => shift(cell, linePrefix.length)));
  }

  const columns = rows[0].length;
  const align = parseAlignment(rows[1], columns);
  if (!align) return null;
  const body: string[][] = [];
  for (let i = 2; i < rows.length; i++) {
    if (rows[i].length > columns) return null;
    const cells = rows[i].map((c) => c.text);
    while (cells.length < columns) cells.push('');
    body.push(cells);
  }

  const line = cursorLine.number - firstLine;
  const column = sel.head - cursorLine.from;
  const cells = rows[line];
  let col = cells.findIndex((cell) => column <= cell.rawEnd);
  if (col === -1) col = cells.length - 1;
  col = Math.min(col, columns - 1);
  const cell = cells[col];
  const offset = Math.min(Math.max(column - cell.contentStart, 0), cell.text.length);

  return {
    header: rows[0].map((c) => c.text),
    align,
    body,
    firstLine,
    lastLine,
    prefix: prefix ?? '',
    line,
    col,
    offset,
  };
}

function shift(cell: SourceCell, by: number): SourceCell {
  return { text: cell.text, contentStart: cell.contentStart + by, rawEnd: cell.rawEnd + by };
}

// Rendering -----------------------------------------------------------------------------

/**
 * Display width in monospace columns: East Asian wide characters and emoji take two,
 * combining marks and zero-width characters take none.
 */
export function displayWidth(text: string): number {
  let width = 0;
  for (const char of text) {
    const code = char.codePointAt(0)!;
    if (
      (code >= 0x0300 && code <= 0x036f) ||
      (code >= 0x200b && code <= 0x200f) ||
      (code >= 0xfe00 && code <= 0xfe0f)
    ) {
      continue;
    }
    const wide =
      (code >= 0x1100 && code <= 0x115f) ||
      (code >= 0x2e80 && code <= 0xa4cf) ||
      (code >= 0xac00 && code <= 0xd7a3) ||
      (code >= 0xf900 && code <= 0xfaff) ||
      (code >= 0xfe30 && code <= 0xfe6f) ||
      (code >= 0xff00 && code <= 0xff60) ||
      (code >= 0xffe0 && code <= 0xffe6) ||
      (code >= 0x1f300 && code <= 0x1faff) ||
      (code >= 0x20000 && code <= 0x3fffd);
    width += wide ? 2 : 1;
  }
  return width;
}

function padCell(text: string, width: number, align: Align): { text: string; lead: number } {
  const fill = Math.max(width - displayWidth(text), 0);
  if (align === 'right') return { text: ' '.repeat(fill) + text, lead: fill };
  if (align === 'center') {
    const lead = Math.floor(fill / 2);
    return { text: ' '.repeat(lead) + text + ' '.repeat(fill - lead), lead };
  }
  return { text: text + ' '.repeat(fill), lead: 0 };
}

function delimiterCell(width: number, align: Align): string {
  if (align === 'left') return ':' + '-'.repeat(width - 1);
  if (align === 'right') return '-'.repeat(width - 1) + ':';
  if (align === 'center') return ':' + '-'.repeat(width - 2) + ':';
  return '-'.repeat(width);
}

export interface RenderedTable {
  lines: string[];
  /** For each line and column, the offset in that line where the cell's text starts. */
  cellStarts: number[][];
}

/** Writes the model as aligned Markdown. Lines 0 and 1 are the header and delimiter row. */
export function renderTable(model: TableModel, prefix: string): RenderedTable {
  const columns = model.header.length;
  const widths: number[] = [];
  for (let c = 0; c < columns; c++) {
    let width = MIN_COLUMN_WIDTH;
    width = Math.max(width, displayWidth(model.header[c]));
    for (const row of model.body) width = Math.max(width, displayWidth(row[c]));
    widths.push(width);
  }

  const lines: string[] = [];
  const cellStarts: number[][] = [];
  const writeRow = (cells: string[]) => {
    let line = prefix + '|';
    const starts: number[] = [];
    for (let c = 0; c < columns; c++) {
      const padded = padCell(cells[c], widths[c], model.align[c]);
      starts.push(line.length + 1 + padded.lead);
      line += ' ' + padded.text + ' |';
    }
    lines.push(line);
    cellStarts.push(starts);
  };

  writeRow(model.header);
  // The delimiter row is dashes, so it needs its own layout.
  {
    let line = prefix + '|';
    const starts: number[] = [];
    for (let c = 0; c < columns; c++) {
      starts.push(line.length + 1);
      line += ' ' + delimiterCell(widths[c], model.align[c]) + ' |';
    }
    lines.push(line);
    cellStarts.push(starts);
  }
  for (const row of model.body) writeRow(row);
  return { lines, cellStarts };
}

// Applying edits ------------------------------------------------------------------------

function rowToLine(row: number): number {
  return row === 0 ? 0 : row + 1;
}

/** Diffs the old table lines against the new ones and writes only the lines that differ. */
function lineChanges(state: EditorState, table: ParsedTable, next: string[]): ChangeSpec[] {
  const { doc } = state;
  const old: string[] = [];
  for (let n = table.firstLine; n <= table.lastLine; n++) old.push(doc.line(n).text);

  let head = 0;
  while (head < old.length && head < next.length && old[head] === next[head]) head++;
  let tail = 0;
  while (
    tail < old.length - head &&
    tail < next.length - head &&
    old[old.length - 1 - tail] === next[next.length - 1 - tail]
  ) {
    tail++;
  }
  const oldEnd = old.length - tail;
  const newEnd = next.length - tail;
  const replacement = next.slice(head, newEnd).join(state.lineBreak);

  if (head === oldEnd && head === newEnd) return [];
  if (head === oldEnd) {
    // Pure insertion.
    if (head < old.length) {
      return [{ from: doc.line(table.firstLine + head).from, insert: replacement + state.lineBreak }];
    }
    return [{ from: doc.line(table.lastLine).to, insert: state.lineBreak + replacement }];
  }
  if (head === newEnd) {
    // Pure deletion: take one adjacent line break with the lines.
    const from = doc.line(table.firstLine + head).from;
    const to = doc.line(table.firstLine + oldEnd - 1).to;
    if (oldEnd < old.length) return [{ from, to: to + 1 }];
    return [{ from: doc.line(table.firstLine + head - 1).to, to }];
  }
  return [
    {
      from: doc.line(table.firstLine + head).from,
      to: doc.line(table.firstLine + oldEnd - 1).to,
      insert: replacement,
    },
  ];
}

/** Builds the transaction that rewrites `table` as `model` and places the cursor on `focus`. */
function writeTable(
  state: EditorState,
  table: ParsedTable,
  model: TableModel,
  focus: Focus
): TransactionSpec {
  const rendered = renderTable(model, table.prefix);
  const line = Math.min(Math.max(focus.line, 0), rendered.lines.length - 1);
  const col = Math.min(Math.max(focus.col, 0), model.header.length - 1);

  let anchor = state.doc.line(table.firstLine).from;
  for (let i = 0; i < line; i++) anchor += rendered.lines[i].length + 1;
  const cellText = line === 0 ? model.header[col] : line === 1 ? '' : model.body[line - 2][col];
  anchor += rendered.cellStarts[line][col] + Math.min(focus.offset, cellText.length);

  return {
    changes: lineChanges(state, table, rendered.lines),
    selection: { anchor },
    scrollIntoView: true,
    userEvent: 'table.edit',
  };
}

function cloneModel(table: ParsedTable): TableModel {
  return {
    header: [...table.header],
    align: [...table.align],
    body: table.body.map((row) => [...row]),
  };
}

function swap<T>(items: T[], a: number, b: number) {
  [items[a], items[b]] = [items[b], items[a]];
}

function here(table: ParsedTable): Focus {
  return { line: table.line, col: table.col, offset: table.offset };
}

/** The cursor row as a model row: 0 for the header and the delimiter row, then body rows. */
function currentRow(table: ParsedTable): number {
  return table.line <= 1 ? 0 : table.line - 1;
}

/**
 * Builds the transaction for a table command, or `null` when the cursor is not in a
 * supported table or the command does not apply there, such as deleting the header.
 */
export function tableCommand(state: EditorState, id: TableCommandId): TransactionSpec | null {
  const table = parseTableAt(state);
  if (!table) return null;
  const model = cloneModel(table);
  const row = currentRow(table);
  const columns = model.header.length;
  const blank = () => new Array<string>(columns).fill('');

  switch (id) {
    case 'format':
      return writeTable(state, table, model, here(table));
    case 'row_above':
      if (row === 0) return null;
      model.body.splice(row - 1, 0, blank());
      return writeTable(state, table, model, { line: rowToLine(row), col: table.col, offset: 0 });
    case 'row_below':
      model.body.splice(row, 0, blank());
      return writeTable(state, table, model, { line: rowToLine(row + 1), col: table.col, offset: 0 });
    case 'row_delete': {
      if (row === 0) return null;
      model.body.splice(row - 1, 1);
      const target = Math.min(row, model.body.length);
      return writeTable(state, table, model, { line: rowToLine(target), col: table.col, offset: 0 });
    }
    case 'row_up':
      if (row < 2) return null;
      swap(model.body, row - 1, row - 2);
      return writeTable(state, table, model, { ...here(table), line: rowToLine(row - 1) });
    case 'row_down':
      if (row === 0 || row >= model.body.length) return null;
      swap(model.body, row - 1, row);
      return writeTable(state, table, model, { ...here(table), line: rowToLine(row + 1) });
    case 'col_left':
    case 'col_right': {
      const at = id === 'col_left' ? table.col : table.col + 1;
      model.header.splice(at, 0, '');
      model.align.splice(at, 0, 'none');
      for (const cells of model.body) cells.splice(at, 0, '');
      return writeTable(state, table, model, { line: table.line, col: at, offset: 0 });
    }
    case 'col_delete': {
      if (columns < 2) return null;
      model.header.splice(table.col, 1);
      model.align.splice(table.col, 1);
      for (const cells of model.body) cells.splice(table.col, 1);
      return writeTable(state, table, model, { line: table.line, col: table.col, offset: 0 });
    }
    case 'col_move_left':
    case 'col_move_right': {
      const to = id === 'col_move_left' ? table.col - 1 : table.col + 1;
      if (to < 0 || to >= columns) return null;
      swap(model.header, table.col, to);
      swap(model.align, table.col, to);
      for (const cells of model.body) swap(cells, table.col, to);
      return writeTable(state, table, model, { ...here(table), col: to });
    }
    case 'align_none':
    case 'align_left':
    case 'align_center':
    case 'align_right':
      model.align[table.col] = id.slice('align_'.length) as Align;
      return writeTable(state, table, model, here(table));
  }
}

/** Tab (`1`) and Shift+Tab (`-1`): reformat, then move to the next or previous cell. */
export function tableStep(state: EditorState, direction: -1 | 1): TransactionSpec | null {
  const table = parseTableAt(state);
  if (!table) return null;
  const model = cloneModel(table);
  const columns = model.header.length;
  const row = currentRow(table);

  let nextRow = row;
  let nextCol = table.col + direction;
  if (nextCol >= columns) {
    nextCol = 0;
    nextRow = row + 1;
  } else if (nextCol < 0) {
    nextCol = columns - 1;
    nextRow = row - 1;
  }
  if (nextRow < 0) {
    // Shift+Tab in the first cell stays put but still formats the table.
    nextRow = 0;
    nextCol = 0;
  }
  if (nextRow > model.body.length) model.body.push(new Array<string>(columns).fill(''));
  return writeTable(state, table, model, { line: rowToLine(nextRow), col: nextCol, offset: 0 });
}

/** Enter: reformat, then move to the same column one row down, adding a row at the end. */
export function tableEnter(state: EditorState): TransactionSpec | null {
  if (!state.selection.main.empty) return null;
  const table = parseTableAt(state);
  if (!table) return null;
  const model = cloneModel(table);
  const nextRow = currentRow(table) + 1;
  if (nextRow > model.body.length) model.body.push(new Array<string>(model.header.length).fill(''));
  return writeTable(state, table, model, { line: rowToLine(nextRow), col: table.col, offset: 0 });
}

// Wiring --------------------------------------------------------------------------------

function run(view: EditorView, spec: TransactionSpec | null): boolean {
  // Leave Tab and Enter to an input method while it is composing text.
  if (!spec || view.composing) return false;
  view.dispatch(spec);
  return true;
}

export function runTableCommand(view: EditorView, id: TableCommandId): boolean {
  return run(view, tableCommand(view.state, id));
}

/** Tab and Shift+Tab. Place it after the inline-calculation Tab and before the outliner. */
export const tableKeymap: KeyBinding[] = [
  { key: 'Tab', run: (v) => run(v, tableStep(v.state, 1)) },
  { key: 'Shift-Tab', run: (v) => run(v, tableStep(v.state, -1)) },
];

export function createTableExtension(): Extension {
  // Above the Markdown keymap, whose Enter continues a list or quote.
  return Prec.highest(keymap.of([{ key: 'Enter', run: (v) => run(v, tableEnter(v.state)) }]));
}
