import type { TextEdit } from './types';

/**
 * Edits that put the original text back after `edits` were applied. Positions count editor units,
 * and the inserted text holds no line breaks, so its JavaScript length is its length in the editor.
 */
export function reverseEdits(edits: readonly TextEdit[]): TextEdit[] {
  const sorted = [...edits].sort((a, b) => a.from - b.from);
  let delta = 0;
  return sorted.map((edit) => {
    const from = edit.from + delta;
    delta += edit.insert.length - (edit.to - edit.from);
    return { from, to: from + edit.insert.length, expected: edit.insert, insert: edit.expected };
  });
}

/** True when every edit still finds the text it expects, so the whole batch can apply. */
export function editsMatch(doc: { sliceString(from: number, to: number): string; length: number }, edits: readonly TextEdit[]): boolean {
  let end = 0;
  for (const edit of [...edits].sort((a, b) => a.from - b.from)) {
    if (edit.from < end || edit.to < edit.from || edit.to > doc.length) return false;
    if (doc.sliceString(edit.from, edit.to) !== edit.expected) return false;
    end = edit.to;
  }
  return true;
}

type EditApplier = (path: string, edits: TextEdit[]) => boolean;

let applier: EditApplier | null = null;

/** The editor registers itself here so the vault store can edit the note on screen. */
export function setOpenNoteEditApplier(next: EditApplier | null) {
  applier = next;
}

/** Applies `edits` in the editor as one undo step. False when that note is not the one on screen. */
export function applyToOpenNote(path: string, edits: TextEdit[]): boolean {
  return applier?.(path, edits) ?? false;
}
