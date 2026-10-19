import { EditorSelection, type TransactionSpec } from '@codemirror/state';
import type { EditorView, KeyBinding } from '@codemirror/view';
import { foldAll, unfoldAll } from '@codemirror/language';
import { toggleZoom } from './outliner';
import { addDueDate } from './dueDates';

export type MarkdownFormatCommandId =
  | 'bold'
  | 'italic'
  | 'strikethrough'
  | 'inline_code'
  | 'h1'
  | 'h2'
  | 'h3'
  | 'h4'
  | 'h5'
  | 'h6'
  | 'paragraph'
  | 'bullet_list'
  | 'ordered_list'
  | 'task_list'
  | 'blockquote'
  | 'code_block'
  | 'table'
  | 'link'
  | 'wikilink'
  | 'hr'
  | 'zoom_toggle'
  | 'fold_all'
  | 'unfold_all'
  | 'due_date';

/**
 * Wraps or unwraps the current selection (or word at cursor) with a symmetric marker
 * such as `**`, `*`, `~~`, or `` ` ``.
 */
export function toggleInlineMarker(
  view: EditorView,
  marker: string,
  placeholderText: string
): boolean {
  const { state } = view;
  const len = marker.length;

  const tx: TransactionSpec = state.changeByRange((range) => {
    let from = range.from;
    let to = range.to;

    // If selection is empty, expand to word at cursor if available
    if (from === to) {
      const word = state.wordAt(from);
      if (word) {
        from = word.from;
        to = word.to;
      }
    }

    const before = state.sliceDoc(Math.max(0, from - len), from);
    const after = state.sliceDoc(to, Math.min(state.doc.length, to + len));

    // Case 1: Surrounding text already has the marker -> unwrap
    if (before === marker && after === marker) {
      // Avoid unwrapping `**` when testing `*` if `**` is bold and marker is `*`
      const beforeExtra = state.sliceDoc(Math.max(0, from - len - 1), from - len);
      const afterExtra = state.sliceDoc(to + len, Math.min(state.doc.length, to + len + 1));
      const isDoubleCollision =
        marker === '*' && (beforeExtra === '*' || afterExtra === '*');

      if (!isDoubleCollision) {
        return {
          changes: [
            { from: from - len, to: from, insert: '' },
            { from: to, to: to + len, insert: '' },
          ],
          range: EditorSelection.range(from - len, to - len),
        };
      }
    }

    const selected = state.sliceDoc(from, to);

    // Case 2: Selection itself starts and ends with the marker -> unwrap inside
    if (
      selected.length >= len * 2 &&
      selected.startsWith(marker) &&
      selected.endsWith(marker)
    ) {
      const inner = selected.slice(len, selected.length - len);
      return {
        changes: { from, to, insert: inner },
        range: EditorSelection.range(from, from + inner.length),
      };
    }

    // Case 3: Empty selection (no word) -> insert marker + placeholder + marker and select placeholder
    if (from === to) {
      const insert = `${marker}${placeholderText}${marker}`;
      return {
        changes: { from, to, insert },
        range: EditorSelection.range(
          from + len,
          from + len + placeholderText.length
        ),
      };
    }

    // Case 4: Wrap selection with marker
    const insert = `${marker}${selected}${marker}`;
    return {
      changes: { from, to, insert },
      range: EditorSelection.range(from + len, to + len),
    };
  });

  view.dispatch(tx);
  view.focus();
  return true;
}

/**
 * Sets or toggles ATX heading level (`#` .. `######`) on all selected lines.
 * Passing `level = 0` strips any heading prefix.
 */
export function setHeadingLevel(
  view: EditorView,
  level: 0 | 1 | 2 | 3 | 4 | 5 | 6
): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const startLine = state.doc.lineAt(sel.from);
  const endLine = state.doc.lineAt(sel.to);

  const changes: { from: number; to: number; insert: string }[] = [];
  const targetPrefix = level > 0 ? `${'#'.repeat(level)} ` : '';

  for (let lineNo = startLine.number; lineNo <= endLine.number; lineNo++) {
    const line = state.doc.line(lineNo);
    const match = /^(#{1,6})\s+/.exec(line.text);
    const existingLevel = match ? match[1].length : 0;
    const prefixLen = match ? match[0].length : 0;

    if (level === 0 || existingLevel === level) {
      // Remove heading prefix
      if (prefixLen > 0) {
        changes.push({
          from: line.from,
          to: line.from + prefixLen,
          insert: '',
        });
      }
    } else {
      changes.push({
        from: line.from,
        to: line.from + prefixLen,
        insert: targetPrefix,
      });
    }
  }

  if (changes.length > 0) {
    view.dispatch({ changes });
  }
  view.focus();
  return true;
}

/**
 * Toggles unordered list (`- `), ordered list (`1. `), task list (`- [ ] `), or blockquote (`> `)
 * on all lines intersecting the main selection.
 */
export function toggleLinePrefix(
  view: EditorView,
  kind: 'bullet' | 'ordered' | 'task' | 'quote'
): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const startLine = state.doc.lineAt(sel.from);
  const endLine = state.doc.lineAt(sel.to);

  const lines: { number: number; from: number; to: number; text: string }[] = [];
  for (let n = startLine.number; n <= endLine.number; n++) {
    lines.push(state.doc.line(n));
  }

  const patternByKind: Record<typeof kind, RegExp> = {
    bullet: /^(\s*)[-*+]\s+(?!\[[ xX]\])/,
    ordered: /^(\s*)\d+\.\s+/,
    task: /^(\s*)[-*+]\s+\[[ xX]\]\s+/,
    quote: /^(\s*)>\s?/,
  };

  const targetRegex = patternByKind[kind];
  const allMatch = lines.every((l) => targetRegex.test(l.text));

  const changes: { from: number; to: number; insert: string }[] = [];

  lines.forEach((line, idx) => {
    const indentMatch = /^(\s*)/.exec(line.text);
    const indent = indentMatch ? indentMatch[1] : '';
    const rest = line.text.slice(indent.length);

    if (allMatch) {
      const m = targetRegex.exec(line.text);
      if (m) {
        changes.push({
          from: line.from,
          to: line.from + m[0].length,
          insert: indent,
        });
      }
    } else {
      // Strip existing list marker if switching list kind
      let cleanRest = rest;
      if (kind === 'bullet' || kind === 'ordered' || kind === 'task') {
        cleanRest = rest
          .replace(/^[-*+]\s+\[[ xX]\]\s+/, '')
          .replace(/^[-*+]\s+/, '')
          .replace(/^\d+\.\s+/, '');
      } else if (kind === 'quote') {
        cleanRest = rest.replace(/^>\s?/, '');
      }

      const newPrefix =
        kind === 'bullet'
          ? `${indent}- `
          : kind === 'ordered'
            ? `${indent}${idx + 1}. `
            : kind === 'task'
              ? `${indent}- [ ] `
              : `${indent}> `;

      changes.push({
        from: line.from,
        to: line.to,
        insert: `${newPrefix}${cleanRest}`,
      });
    }
  });

  if (changes.length > 0) {
    view.dispatch({ changes });
  }
  view.focus();
  return true;
}

/**
 * Inserts or wraps selection in a fenced code block (```).
 */
export function insertCodeBlock(view: EditorView): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const selected = state.sliceDoc(sel.from, sel.to);
  const line = state.doc.lineAt(sel.from);
  const needsLeadingNewline = sel.from > line.from;

  const prefix = needsLeadingNewline ? '\n```text\n' : '```text\n';
  const body = selected || 'code';
  const suffix = '\n```';
  const insert = `${prefix}${body}${suffix}`;

  const bodyStart = sel.from + prefix.length;
  const bodyEnd = bodyStart + body.length;

  view.dispatch({
    changes: { from: sel.from, to: sel.to, insert },
    selection: EditorSelection.range(bodyStart, bodyEnd),
  });
  view.focus();
  return true;
}

/**
 * Inserts a GFM Markdown table and selects the first header cell.
 */
export function insertTable(view: EditorView): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const line = state.doc.lineAt(sel.from);
  const leading = line.text.trim().length > 0 ? '\n\n' : '';
  const headerCell = 'Column 1';
  const tableTemplate = `${leading}| ${headerCell} | Column 2 | Column 3 |\n| --- | --- | --- |\n| Value 1 | Value 2 | Value 3 |\n`;

  const cellStart = sel.from + leading.length + 2;
  const cellEnd = cellStart + headerCell.length;

  view.dispatch({
    changes: { from: sel.from, to: sel.to, insert: tableTemplate },
    selection: EditorSelection.range(cellStart, cellEnd),
  });
  view.focus();
  return true;
}

/**
 * Wraps selection in `[text](url)` or `[[Note Title]]`.
 */
export function insertMarkdownLink(view: EditorView, wiki = false): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const selected = state.sliceDoc(sel.from, sel.to);

  if (wiki) {
    const inner = selected || 'Note Title';
    const insert = `[[${inner}]]`;
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert },
      selection: EditorSelection.range(sel.from + 2, sel.from + 2 + inner.length),
    });
    view.focus();
    return true;
  }

  const label = selected || 'link text';
  const url = 'https://';
  const insert = `[${label}](${url})`;
  const urlStart = sel.from + label.length + 3;
  const urlEnd = urlStart + url.length;

  view.dispatch({
    changes: { from: sel.from, to: sel.to, insert },
    selection: EditorSelection.range(urlStart, urlEnd),
  });
  view.focus();
  return true;
}

/**
 * Inserts a horizontal rule (`---`).
 */
export function insertHorizontalRule(view: EditorView): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const line = state.doc.lineAt(sel.from);
  const leading = line.text.trim().length > 0 ? '\n\n' : '';
  const insert = `${leading}---\n\n`;

  view.dispatch({
    changes: { from: sel.from, to: sel.to, insert },
    selection: { anchor: sel.from + insert.length },
  });
  view.focus();
  return true;
}

/**
 * Executes any named Markdown formatting command on the given EditorView.
 */
export function runMarkdownCommand(
  view: EditorView,
  cmd: MarkdownFormatCommandId
): boolean {
  switch (cmd) {
    case 'bold':
      return toggleInlineMarker(view, '**', 'bold');
    case 'italic':
      return toggleInlineMarker(view, '*', 'italic');
    case 'strikethrough':
      return toggleInlineMarker(view, '~~', 'strikethrough');
    case 'inline_code':
      return toggleInlineMarker(view, '`', 'code');
    case 'h1':
      return setHeadingLevel(view, 1);
    case 'h2':
      return setHeadingLevel(view, 2);
    case 'h3':
      return setHeadingLevel(view, 3);
    case 'h4':
      return setHeadingLevel(view, 4);
    case 'h5':
      return setHeadingLevel(view, 5);
    case 'h6':
      return setHeadingLevel(view, 6);
    case 'paragraph':
      return setHeadingLevel(view, 0);
    case 'bullet_list':
      return toggleLinePrefix(view, 'bullet');
    case 'ordered_list':
      return toggleLinePrefix(view, 'ordered');
    case 'task_list':
      return toggleLinePrefix(view, 'task');
    case 'blockquote':
      return toggleLinePrefix(view, 'quote');
    case 'code_block':
      return insertCodeBlock(view);
    case 'table':
      return insertTable(view);
    case 'link':
      return insertMarkdownLink(view, false);
    case 'wikilink':
      return insertMarkdownLink(view, true);
    case 'hr':
      return insertHorizontalRule(view);
    case 'zoom_toggle':
      return toggleZoom(view);
    case 'fold_all':
      return foldAll(view);
    case 'unfold_all':
      return unfoldAll(view);
    case 'due_date':
      return addDueDate(view);
  }
}

/**
 * Standard Markdown formatting keymap for CodeMirror 6.
 */
export const markdownFormattingKeymap: readonly KeyBinding[] = [
  {
    key: 'Mod-b',
    run: (v) => toggleInlineMarker(v, '**', 'bold'),
  },
  {
    key: 'Mod-i',
    run: (v) => toggleInlineMarker(v, '*', 'italic'),
  },
  {
    key: 'Mod-Shift-s',
    run: (v) => toggleInlineMarker(v, '~~', 'strikethrough'),
  },
  {
    key: 'Mod-e',
    run: (v) => toggleInlineMarker(v, '`', 'code'),
  },
  {
    key: 'Mod-k',
    run: (v) => insertMarkdownLink(v, false),
  },
  {
    key: 'Mod-Shift-k',
    run: (v) => insertCodeBlock(v),
  },
  {
    key: 'Mod-1',
    run: (v) => setHeadingLevel(v, 1),
  },
  {
    key: 'Mod-2',
    run: (v) => setHeadingLevel(v, 2),
  },
  {
    key: 'Mod-3',
    run: (v) => setHeadingLevel(v, 3),
  },
  {
    key: 'Mod-4',
    run: (v) => setHeadingLevel(v, 4),
  },
  {
    key: 'Mod-5',
    run: (v) => setHeadingLevel(v, 5),
  },
  {
    key: 'Mod-6',
    run: (v) => setHeadingLevel(v, 6),
  },
  {
    key: 'Mod-0',
    run: (v) => setHeadingLevel(v, 0),
  },
  {
    key: 'Mod-Shift-8',
    run: (v) => toggleLinePrefix(v, 'bullet'),
  },
  {
    key: 'Mod-Shift-7',
    run: (v) => toggleLinePrefix(v, 'ordered'),
  },
  {
    key: 'Mod-Shift-9',
    run: (v) => toggleLinePrefix(v, 'task'),
  },
  {
    key: 'Mod-l',
    run: (v) => toggleLinePrefix(v, 'task'),
  },
  {
    key: 'Mod-Shift-.',
    run: (v) => toggleLinePrefix(v, 'quote'),
  },
];
