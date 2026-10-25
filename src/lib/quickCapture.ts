import type { TextEdit } from './types';

/**
 * Quick capture hands each entry to the main window, which owns the open notes. When the inbox
 * note is on screen the entry goes into the editor, so unsaved typing there is kept; otherwise
 * the main window saves its tabs first and then appends on disk.
 */

export const CAPTURE_REQUEST_EVENT = 'quick-capture-request';
export const CAPTURE_RESULT_EVENT = 'quick-capture-result';

export interface CaptureRequest {
  id: string;
  note: string;
  text: string;
}

export type CaptureResult = { id: string; path: string } | { id: string; error: string };

/** The inbox path as the vault stores it: forward slashes, no leading slash, ending in `.md`. */
export function normalizeCaptureNote(note: string): string {
  const clean = note.trim().replace(/\\/g, '/').replace(/^\/+/, '') || 'Inbox.md';
  return /\.md$/i.test(clean) ? clean : `${clean}.md`;
}

/**
 * The edit that appends `text` to a note, matching the backend: trailing whitespace goes, then
 * one blank line, the trimmed entry, and a final line break. Null when the entry is empty.
 */
export function captureEdit(doc: string, text: string): TextEdit | null {
  const entry = text.trim();
  if (!entry) return null;
  const from = doc.trimEnd().length;
  return {
    from,
    to: doc.length,
    expected: doc.slice(from),
    insert: `${from > 0 ? '\n\n' : ''}${entry}\n`,
  };
}
