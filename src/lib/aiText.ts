import type { EditorState, TransactionSpec } from '@codemirror/state';
import { isolateHistory } from '@codemirror/commands';
import type { captureEditorOperation } from './editorOperations';
import { positionAfterFrontmatter } from './frontmatterPreview';
import { markdownLanguage } from '@codemirror/lang-markdown';
import { findInlineMath, findRichBlocks } from './livePreviewBlocks';

export type AiScope = 'selection' | 'note';
export type AiPlacement = 'replace' | 'below';

export interface AiRange {
  from: number;
  to: number;
  text: string;
}

/** A captured range plus a handle that tells whether the note changed since. */
export interface AiTarget extends AiRange {
  operation: ReturnType<typeof captureEditorOperation>;
}

/**
 * The text an AI command works on. A whole note skips YAML frontmatter, so properties are
 * never sent or rewritten. Returns null when the range holds only whitespace.
 */
export function aiTargetRange(state: EditorState, scope: AiScope): AiRange | null {
  const doc = state.doc.toString();
  const { from, to } =
    scope === 'selection'
      ? state.selection.main
      : { from: positionAfterFrontmatter(doc), to: doc.length };
  if (scope === 'selection' && from !== to) validateSelection(state, from, to);
  const text = doc.slice(from, to);
  return text.trim() ? { from, to, text } : null;
}

/** Check the full note before slicing away the syntax that identifies protected content. */
function validateSelection(state: EditorState, from: number, to: number) {
  const doc = state.doc.toString();
  if (from < positionAfterFrontmatter(doc)) {
    throw new Error('The selection includes note properties. Select text below the properties first.');
  }
  const check = (start: number, end: number) => {
    if (from < end && to > start && !(from <= start && to >= end)) {
      throw new Error('The selection cuts through protected Markdown. Include the complete code block, link, math, or tag, or select prose outside it.');
    }
  };
  const blocks = new Set(['FencedCode', 'CodeBlock', 'LinkReference', 'HTMLBlock']);
  const inline = new Set(['InlineCode', 'Link', 'Image', 'Autolink', 'HTMLTag']);
  // Parse the entire document; the editor's incremental tree may stop before the selection.
  markdownLanguage.parser.parse(doc).iterate({
    enter(node) {
      if (blocks.has(node.name)) {
        check(state.doc.lineAt(node.from).from, state.doc.lineAt(node.to).to);
        return false;
      }
      if (inline.has(node.name)) {
        check(node.from, node.to);
        return false;
      }
    },
  });
  for (const block of findRichBlocks(state)) check(block.from, block.to);
  for (const match of doc.matchAll(/!?\[\[[^\]\n]*\]\]|(?<=^|\s)#[\p{L}\p{N}_/-]+|https?:\/\/[^\s]+/gu)) {
    check(match.index, match.index + match[0].length);
  }
  for (let n = 1; n <= state.doc.lines; n++) {
    const line = state.doc.line(n);
    for (const math of findInlineMath(line.text)) check(line.from + math.from, line.from + math.to);
  }
}

/**
 * The edit that applies an AI result as one undo step. `below` inserts the result as a new
 * paragraph after the range and leaves the original text in place.
 */
export function aiResultChange(
  state: EditorState,
  range: Pick<AiRange, 'from' | 'to'>,
  result: string,
  placement: AiPlacement
): TransactionSpec {
  const annotations = isolateHistory.of('full');
  if (placement === 'replace') {
    return {
      changes: { from: range.from, to: range.to, insert: result },
      selection: { anchor: range.from, head: range.from + result.length },
      userEvent: 'input.ai',
      annotations,
      scrollIntoView: true,
    };
  }
  const doc = state.doc;
  // Insert after the last line of the range, separated by one blank line.
  const at = doc.lineAt(Math.max(range.from, range.to - (range.to > range.from ? 1 : 0))).to;
  const before = doc.sliceString(0, at);
  const lead = before.length === 0 ? '' : before.endsWith('\n\n') ? '' : before.endsWith('\n') ? '\n' : '\n\n';
  const body = result.trim();
  const after = doc.sliceString(at, at + 2);
  const atEnd = doc.sliceString(at).trim() === '';
  const trail = atEnd || after.startsWith('\n\n') ? '' : after.startsWith('\n') ? '\n' : '\n\n';
  const insert = `${lead}${body}${trail}`;
  const start = at + lead.length;
  return {
    changes: { from: at, insert },
    selection: { anchor: start, head: start + body.length },
    userEvent: 'input.ai',
    annotations,
    scrollIntoView: true,
  };
}
