import { isolateHistory } from '@codemirror/commands';
import type { TransactionSpec } from '@codemirror/state';

/**
 * Extract selection to a linked note. The flow keeps the source text safe:
 *
 * 1. Capture the selection and check it does not cut through protected Markdown.
 * 2. Create the new note and read it back from disk.
 * 3. Only then replace the selection with a link, as one undo step, and only if the source
 *    note has not changed since step 1.
 *
 * A failure in step 2 leaves the source untouched. A change during step 3 leaves the original
 * text in place next to the new note. Nothing here rewrites text the user typed later.
 */

/** Characters that break a note name or a `[[wiki link]]` to it. */
const UNSAFE_NAME_CHARS = /[<>:"/\\|?*\u0000-\u001f[\]#^]/g;
const MAX_NAME = 60;

/**
 * A note name from the first line of the text: list, heading, and quote markers, emphasis,
 * and link syntax are dropped, unsafe characters become spaces, and the result is cut at a
 * word boundary. Returns an empty string when nothing usable is left.
 */
export function suggestExtractName(text: string): string {
  const firstLine = text.split(/\r?\n/).find((line) => line.trim() !== '') ?? '';
  const plain = firstLine
    .replace(/^\s*(?:#{1,6}\s+|>\s*|[-*+]\s+(?:\[[ xX]\]\s+)?|\d{1,9}[.)]\s+)+/, '')
    .replace(/!?\[\[([^\]|]*)(?:\|([^\]]*))?\]\]/g, (_, target: string, alias?: string) => alias || target)
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[*_`~]+/g, '')
    .replace(UNSAFE_NAME_CHARS, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/^\.+/, '')
    .replace(/[.\s]+$/, '');
  if (plain.length <= MAX_NAME) return plain;
  const cut = plain.slice(0, MAX_NAME);
  const space = cut.lastIndexOf(' ');
  return (space > MAX_NAME / 2 ? cut.slice(0, space) : cut).replace(/[.\s]+$/, '');
}

/** The extra rule for names a wiki link points to: `[ ] # ^ |` would end or split the link. */
export function hasLinkUnsafeChars(name: string): boolean {
  return /[[\]#^|]/.test(name);
}

/** The selection split into its outer whitespace and the text between. */
export function splitPadding(text: string): { lead: string; core: string; trail: string } {
  const lead = /^\s*/.exec(text)![0];
  const rest = text.slice(lead.length);
  const core = rest.trimEnd();
  return { lead, core, trail: rest.slice(core.length) };
}

function startsWithH1(body: string): boolean {
  const first = body.split(/\r?\n/).find((line) => line.trim() !== '') ?? '';
  return /^#\s+\S/.test(first);
}

/**
 * The new note's text: the selection under a title heading, or as written when it already
 * starts with a level-one heading, and an optional line linking back to the source.
 */
export function buildExtractedNote(opts: {
  title: string;
  body: string;
  /** The back-link line, already written in the interface language. Null leaves it out. */
  backLinkLine: string | null;
}): string {
  const body = opts.body.replace(/\r\n/g, '\n').trim();
  const parts = [startsWithH1(body) ? body : `# ${opts.title}\n\n${body}`];
  if (opts.backLinkLine) parts.push(opts.backLinkLine);
  return `${parts.join('\n\n')}\n`;
}

/** The note's path without `.md`, for a link that cannot be ambiguous. */
function withoutMd(path: string): string {
  return path.replace(/\.md$/i, '');
}

/**
 * The text between the brackets of the link to `destRel`. A bare name when it resolves to the
 * new note, and the full path when another note with the same name would win.
 */
export function linkTargetFor(
  destRel: string,
  resolve: (target: string) => { relPath: string } | null
): string {
  const name = withoutMd(destRel.split('/').pop() ?? destRel);
  const found = resolve(name);
  if (found && found.relPath.toLowerCase() === destRel.toLowerCase()) return name;
  return withoutMd(destRel);
}

/**
 * The edit that swaps the selection for a link as one undo step. The whitespace around the
 * selection stays, so a selected paragraph keeps the blank lines that separate it.
 */
export function extractionChange(
  range: { from: number; to: number; text: string },
  link: string
): TransactionSpec {
  const { lead, trail } = splitPadding(range.text);
  const from = range.from + lead.length;
  const to = range.to - trail.length;
  return {
    changes: { from, to, insert: link },
    selection: { anchor: from + link.length },
    userEvent: 'input.extract',
    annotations: isolateHistory.of('full'),
    scrollIntoView: true,
  };
}

// Recovery record ---------------------------------------------------------------------

export const PENDING_EXTRACTION_KEY = 'jotter.pendingExtraction';

export interface PendingExtraction {
  source: string;
  dest: string;
  startedMs: number;
}

export function parsePendingExtraction(raw: string | null): PendingExtraction | null {
  if (!raw) return null;
  try {
    const value: unknown = JSON.parse(raw);
    if (typeof value !== 'object' || value === null) return null;
    const { source, dest, startedMs } = value as Record<string, unknown>;
    return typeof source === 'string' && typeof dest === 'string' && typeof startedMs === 'number'
      ? { source, dest, startedMs }
      : null;
  } catch {
    return null;
  }
}
