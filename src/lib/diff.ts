/**
 * Line diff for the version history timeline.
 *
 * Jotter has no diff dependency: notes are plain text, and a line diff is enough to show what
 * a version changed. Identical lines at the start and the end are matched off first, so the
 * expensive part runs only over the lines that actually differ.
 */

export type DiffKind = 'same' | 'add' | 'del';

export interface DiffLine {
  kind: DiffKind;
  text: string;
  /** 1-based line in the older text, for `same` and `del`. */
  oldLine?: number;
  /** 1-based line in the newer text, for `same` and `add`. */
  newLine?: number;
}

/** Stands for lines the view hides between two changes. */
export interface DiffGap {
  kind: 'gap';
  count: number;
}

export type DiffRow = DiffLine | DiffGap;

/**
 * Longest-common-subsequence table size this is willing to build. Beyond it the diff marks the
 * whole differing middle as replaced, which still tells the reader where the change is.
 */
const MAX_TABLE_CELLS = 4_000_000;

export function diffLines(oldText: string, newText: string): DiffLine[] {
  const oldLines = splitLines(oldText);
  const newLines = splitLines(newText);

  let head = 0;
  while (head < oldLines.length && head < newLines.length && oldLines[head] === newLines[head]) {
    head++;
  }
  let tail = 0;
  while (
    tail < oldLines.length - head &&
    tail < newLines.length - head &&
    oldLines[oldLines.length - 1 - tail] === newLines[newLines.length - 1 - tail]
  ) {
    tail++;
  }

  const rows: DiffLine[] = [];
  for (let i = 0; i < head; i++) {
    rows.push({ kind: 'same', text: oldLines[i], oldLine: i + 1, newLine: i + 1 });
  }

  const oldMiddle = oldLines.slice(head, oldLines.length - tail);
  const newMiddle = newLines.slice(head, newLines.length - tail);
  for (const row of diffMiddle(oldMiddle, newMiddle, head)) rows.push(row);

  for (let i = 0; i < tail; i++) {
    const oldIndex = oldLines.length - tail + i;
    rows.push({
      kind: 'same',
      text: oldLines[oldIndex],
      oldLine: oldIndex + 1,
      newLine: newLines.length - tail + i + 1,
    });
  }
  return rows;
}

/** Added and removed line counts, for the timeline's summary. */
export function diffSummary(lines: DiffLine[]): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  for (const line of lines) {
    if (line.kind === 'add') added++;
    else if (line.kind === 'del') removed++;
  }
  return { added, removed };
}

/** Hides runs of unchanged lines, keeping `context` of them around every change. */
export function collapseUnchanged(lines: DiffLine[], context = 3): DiffRow[] {
  const keep = new Array<boolean>(lines.length).fill(false);
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].kind === 'same') continue;
    for (let j = Math.max(0, i - context); j <= Math.min(lines.length - 1, i + context); j++) {
      keep[j] = true;
    }
  }

  const rows: DiffRow[] = [];
  let hidden = 0;
  for (let i = 0; i < lines.length; i++) {
    if (keep[i]) {
      if (hidden > 0) {
        rows.push({ kind: 'gap', count: hidden });
        hidden = 0;
      }
      rows.push(lines[i]);
    } else {
      hidden++;
    }
  }
  if (hidden > 0) rows.push({ kind: 'gap', count: hidden });
  return rows;
}

function splitLines(text: string): string[] {
  const normalized = text.replace(/\r\n/g, '\n');
  // A trailing newline ends the last line rather than starting an empty one.
  const body = normalized.endsWith('\n') ? normalized.slice(0, -1) : normalized;
  return body === '' ? [] : body.split('\n');
}

/** Diffs the part that differs, with `offset` lines already matched before it. */
function diffMiddle(oldLines: string[], newLines: string[], offset: number): DiffLine[] {
  if (oldLines.length === 0 && newLines.length === 0) return [];
  if (oldLines.length === 0 || newLines.length === 0) {
    return [...removed(oldLines, offset), ...addedLines(newLines, offset)];
  }
  if (oldLines.length * newLines.length > MAX_TABLE_CELLS) {
    return [...removed(oldLines, offset), ...addedLines(newLines, offset)];
  }

  // table[i][j] is the length of the longest common subsequence of the two tails.
  const width = newLines.length + 1;
  const table = new Uint32Array((oldLines.length + 1) * width);
  for (let i = oldLines.length - 1; i >= 0; i--) {
    for (let j = newLines.length - 1; j >= 0; j--) {
      table[i * width + j] =
        oldLines[i] === newLines[j]
          ? table[(i + 1) * width + j + 1] + 1
          : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
    }
  }

  const rows: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < oldLines.length && j < newLines.length) {
    if (oldLines[i] === newLines[j]) {
      rows.push({ kind: 'same', text: oldLines[i], oldLine: offset + i + 1, newLine: offset + j + 1 });
      i++;
      j++;
    } else if (table[(i + 1) * width + j] >= table[i * width + j + 1]) {
      rows.push({ kind: 'del', text: oldLines[i], oldLine: offset + i + 1 });
      i++;
    } else {
      rows.push({ kind: 'add', text: newLines[j], newLine: offset + j + 1 });
      j++;
    }
  }
  for (; i < oldLines.length; i++) {
    rows.push({ kind: 'del', text: oldLines[i], oldLine: offset + i + 1 });
  }
  for (; j < newLines.length; j++) {
    rows.push({ kind: 'add', text: newLines[j], newLine: offset + j + 1 });
  }
  return rows;
}

function removed(lines: string[], offset: number): DiffLine[] {
  return lines.map((text, i) => ({ kind: 'del' as const, text, oldLine: offset + i + 1 }));
}

function addedLines(lines: string[], offset: number): DiffLine[] {
  return lines.map((text, i) => ({ kind: 'add' as const, text, newLine: offset + i + 1 }));
}
