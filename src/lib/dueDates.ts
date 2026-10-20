import { type Extension, type Range } from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
} from '@codemirror/view';

/**
 * Due dates on checklist items, matching the indexer in `src-tauri/src/tasks.rs`:
 * `📅 2026-10-30`, `due:2026-10-30`, or `[due:: 2026-10-30]`, each with an optional `HH:MM`.
 * Live mode shows them as chips that turn red when the task is overdue.
 */

const TASK_RE = /^\s*(?:[-*+]|\d{1,9}[.)])\s+\[([ xX])\]/;
// Hours 00-23 and minutes 00-59, as the Rust parser checks. An invalid time such as `25:00` is
// not part of the match, so the date stands alone, and `[due:: 2026-10-30 25:00]` has no date.
const TIME = '((?:[01]\\d|2[0-3]):[0-5]\\d)';
const DUE_PATTERNS = [
  new RegExp(`📅️?\\s*(\\d{4}-\\d{2}-\\d{2})(?:[ T]${TIME})?(?!\\d)`, 'u'),
  new RegExp(`\\[due::\\s*(\\d{4}-\\d{2}-\\d{2})(?:[ T]${TIME})?\\s*\\]`, 'i'),
  new RegExp(`(?<![A-Za-z0-9])due:(\\d{4}-\\d{2}-\\d{2})(?:[ T]${TIME})?(?!\\d)`, 'i'),
];

export interface DueDateMatch {
  from: number;
  to: number;
  date: string;
  time: string | null;
  done: boolean;
}

function validDate(date: string): boolean {
  const [y, m, d] = date.split('-').map(Number);
  const parsed = new Date(y, m - 1, d);
  return parsed.getFullYear() === y && parsed.getMonth() === m - 1 && parsed.getDate() === d;
}

/** Finds the due date on a task line, or null for other lines and tasks without one. */
export function findDueDate(line: string): DueDateMatch | null {
  const task = TASK_RE.exec(line);
  if (!task) return null;
  // Pair runs of equal length, as Markdown does. Preserve UTF-16 offsets for decorations.
  const ticks = [...line.matchAll(/`+/g)];
  let masked = line;
  for (let i = 0; i < ticks.length; i++) {
    const close = ticks.findIndex((tick, j) => j > i && tick[0].length === ticks[i][0].length);
    if (close < 0) continue;
    const from = ticks[i].index!;
    const to = ticks[close].index! + ticks[close][0].length;
    masked = masked.slice(0, from) + ' '.repeat(to - from) + masked.slice(to);
    i = close;
  }
  for (const re of DUE_PATTERNS) {
    const m = re.exec(masked);
    if (m && validDate(m[1])) {
      return { from: m.index, to: m.index + m[0].length, date: m[1], time: m[2] ?? null, done: task[1] !== ' ' };
    }
  }
  return null;
}

function localDate(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** A short label such as "Today", "Tomorrow", or "Oct 30", plus whether the date has passed. */
export function describeDueDate(date: string, now = new Date()): { label: string; overdue: boolean } {
  const today = localDate(now);
  const tomorrow = new Date(now);
  tomorrow.setDate(now.getDate() + 1);
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  const [y, m, d] = date.split('-').map(Number);
  const when = new Date(y, m - 1, d);
  let label: string;
  if (date === today) label = 'Today';
  else if (date === localDate(tomorrow)) label = 'Tomorrow';
  else if (date === localDate(yesterday)) label = 'Yesterday';
  else {
    label = when.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      ...(y !== now.getFullYear() ? { year: 'numeric' } : {}),
    });
  }
  return { label, overdue: date < today };
}

class DueChipWidget extends WidgetType {
  constructor(
    readonly date: string,
    readonly time: string | null,
    readonly done: boolean,
    readonly editAt: number
  ) {
    super();
  }

  eq(other: DueChipWidget): boolean {
    return (
      other.date === this.date &&
      other.time === this.time &&
      other.done === this.done &&
      other.editAt === this.editAt
    );
  }

  toDOM(view: EditorView): HTMLElement {
    const { label, overdue } = describeDueDate(this.date);
    const chip = document.createElement('span');
    chip.className = 'cm-due-chip';
    if (overdue && !this.done) chip.classList.add('cm-due-overdue');
    if (this.done) chip.classList.add('cm-due-done');
    chip.textContent = `📅 ${label}${this.time ? ` ${this.time}` : ''}`;
    chip.title = `Due ${this.date}${this.time ? ` ${this.time}` : ''}. Click to edit.`;
    chip.addEventListener('mousedown', (e) => {
      e.preventDefault();
      view.dispatch({ selection: { anchor: this.editAt } });
      view.focus();
    });
    return chip;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

function buildChips(view: EditorView): DecorationSet {
  const ranges: Range<Decoration>[] = [];
  const { state } = view;
  const cursorLines = new Set(state.selection.ranges.map((r) => state.doc.lineAt(r.head).number));
  for (const { from, to } of view.visibleRanges) {
    for (let pos = from; pos <= to; ) {
      const line = state.doc.lineAt(pos);
      pos = line.to + 1;
      // Show the raw syntax on the line being edited.
      if (cursorLines.has(line.number)) continue;
      const due = findDueDate(line.text);
      if (!due) continue;
      ranges.push(
        Decoration.replace({
          widget: new DueChipWidget(due.date, due.time, due.done, line.from + due.to),
        }).range(line.from + due.from, line.from + due.to)
      );
    }
  }
  return Decoration.set(ranges, true);
}

export function createDueDateExtension(): Extension {
  return [
    ViewPlugin.fromClass(
      class {
        decorations: DecorationSet;
        constructor(view: EditorView) {
          this.decorations = buildChips(view);
        }
        update(update: ViewUpdate) {
          if (update.docChanged || update.selectionSet || update.viewportChanged) {
            this.decorations = buildChips(update.view);
          }
        }
      },
      { decorations: (v) => v.decorations }
    ),
    EditorView.baseTheme({
      '.cm-due-chip': {
        padding: '0 6px',
        borderRadius: '8px',
        background: 'var(--accent-soft)',
        color: 'var(--text-secondary)',
        fontFamily: 'var(--font-ui)',
        fontSize: '0.85em',
        whiteSpace: 'nowrap',
        cursor: 'pointer',
      },
      '.cm-due-overdue': { color: 'var(--status-danger)', fontWeight: '600' },
      '.cm-due-done': { opacity: '0.6' },
    }),
  ];
}

/** Adds ` 📅 <today>` to the end of the task at the cursor, or selects the existing date. */
export function addDueDate(view: EditorView): boolean {
  const line = view.state.doc.lineAt(view.state.selection.main.head);
  if (!TASK_RE.test(line.text)) return false;
  const existing = findDueDate(line.text);
  if (existing) {
    const dateStart = line.from + existing.from + line.text.slice(existing.from, existing.to).indexOf(existing.date);
    view.dispatch({ selection: { anchor: dateStart, head: dateStart + 10 } });
    return true;
  }
  const insert = `${/\s$/.test(line.text) ? '' : ' '}📅 ${localDate(new Date())}`;
  const end = line.to;
  // Select the date so typing replaces it.
  view.dispatch({
    changes: { from: end, insert },
    selection: { anchor: end + insert.length - 10, head: end + insert.length },
  });
  return true;
}
