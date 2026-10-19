import {
  EditorState,
  RangeSet,
  StateField,
  Transaction,
  type Extension,
  type TransactionSpec,
} from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView } from '@codemirror/view';
import { parseFrontmatter } from './frontmatter';

/**
 * Live mode hides the YAML frontmatter, because the properties form in the note header shows
 * and edits it. Source mode shows the YAML as text. The hidden range is atomic, so the cursor
 * skips it instead of landing inside invisible text.
 */

// Frontmatter longer than this stays visible; nobody edits that much YAML through a form.
const MAX_FRONTMATTER = 64 * 1024;

function hiddenRange(state: EditorState): DecorationSet {
  const fm = parseFrontmatter(state.doc.sliceString(0, MAX_FRONTMATTER));
  if (!fm) return Decoration.none;
  // Block replacements must span whole lines, so stop before the closing fence's newline.
  const closing = state.doc.lineAt(Math.max(0, fm.end - 1));
  return Decoration.set([Decoration.replace({ block: true }).range(0, closing.to)]);
}

/** The first position after hidden frontmatter, or 0 when nothing is hidden. */
export function positionAfterFrontmatter(doc: string): number {
  const fm = parseFrontmatter(doc.slice(0, MAX_FRONTMATTER));
  if (!fm) return 0;
  return Math.min(doc[fm.end - 1] === '\n' ? fm.end : fm.end + 1, doc.length);
}

export function createFrontmatterHideExtension(): Extension {
  const field = StateField.define<DecorationSet>({
    create: hiddenRange,
    update(value, tr) {
      if (!tr.docChanged) return value;
      // Only edits that touch the top of the note can change the frontmatter.
      let touchesTop = false;
      tr.changes.iterChangedRanges((fromA) => {
        if (fromA <= MAX_FRONTMATTER) touchesTop = true;
      });
      return touchesTop ? hiddenRange(tr.state) : value.map(tr.changes);
    },
    provide: (f) => [
      EditorView.decorations.from(f),
      EditorView.atomicRanges.of((view) => view.state.field(f) ?? RangeSet.empty),
    ],
  });
  // Keep the cursor out of the hidden lines, so typing never lands above the closing fence.
  const keepCursorOut = EditorState.transactionFilter.of((tr) => {
    if (!tr.selection && !tr.docChanged) return tr;
    const typed = typedAfterClosingFence(tr);
    if (typed) return typed;
    const sel = tr.newSelection.main;
    if (!sel.empty) return tr;
    // Read the field instead of reparsing, since this runs on every cursor move.
    const hidden = tr.state.field(field).iter();
    if (hidden.value === null) return tr;
    const after = Math.min(hidden.to + 1, tr.state.doc.length);
    if (sel.head >= after) return tr;
    return [tr, { selection: { anchor: after }, sequential: true }];
  });

  /**
   * In a note that is only frontmatter with no final newline, the cursor can only sit at the
   * end of the closing fence. Typing there would turn `---` into `---text` and break the block,
   * so the input moves to a new line below the fence.
   */
  function typedAfterClosingFence(tr: Transaction): TransactionSpec | null {
    if (!tr.docChanged || !tr.isUserEvent('input')) return null;
    const end = tr.startState.doc.length;
    const hidden = tr.startState.field(field).iter();
    if (hidden.value === null || hidden.to !== end) return null;
    let changes = 0;
    // Asserted so TypeScript does not narrow it to null; the callback below assigns it.
    let text = null as string | null;
    tr.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
      changes++;
      if (fromA === end && toA === end) text = inserted.toString();
    });
    if (changes !== 1 || text === null || text.startsWith('\n')) return null;
    return {
      changes: { from: end, insert: `\n${text}` },
      selection: { anchor: end + 1 + text.length },
      userEvent: tr.annotation(Transaction.userEvent),
      scrollIntoView: true,
    };
  }

  return [field, keepCursorOut];
}
