import { syntaxTree } from '@codemirror/language';
import {
  Compartment,
  EditorState,
  Range,
  type Extension,
} from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
} from '@codemirror/view';
import { readVaultImageDataUrl } from './ipc';
import type { EditingMode } from './types';

const imageDataUrlCache = new Map<string, string>();

export function clearImagePreviewCache() {
  imageDataUrlCache.clear();
}

export interface LivePreviewConfig {
  editingMode: EditingMode;
  dimInactiveParagraphs: boolean;
  typewriterScrolling: boolean;
  onWikiLinkClick?: (noteTarget: string, heading?: string) => void;
}

const VIEWPORT_MARGIN_LINES = 20;

interface VisibleLineWindow {
  startLine: number;
  endLine: number;
  from: number;
  to: number;
}

class BulletPointWidget extends WidgetType {
  toDOM(): HTMLElement {
    const span = document.createElement('span');
    span.className = 'cm-live-bullet-widget';
    span.textContent = '•';
    span.setAttribute('aria-hidden', 'true');
    return span;
  }

  eq(): boolean {
    return true;
  }
}

class TaskCheckboxWidget extends WidgetType {
  constructor(
    readonly checked: boolean,
    readonly markerFrom: number,
    readonly markerTo: number
  ) {
    super();
  }

  eq(other: TaskCheckboxWidget): boolean {
    return (
      this.checked === other.checked &&
      this.markerFrom === other.markerFrom &&
      this.markerTo === other.markerTo
    );
  }

  toDOM(view: EditorView): HTMLElement {
    const wrap = document.createElement('span');
    wrap.className = 'cm-live-task-widget';

    const input = document.createElement('input');
    input.type = 'checkbox';
    input.className = 'cm-live-task-checkbox';
    input.checked = this.checked;
    input.setAttribute(
      'aria-label',
      this.checked ? 'Completed task item' : 'Incomplete task item'
    );

    input.addEventListener('mousedown', (e) => {
      e.preventDefault();
      e.stopPropagation();
      const replacement = this.checked ? '[ ]' : '[x]';
      view.dispatch({
        changes: {
          from: this.markerFrom,
          to: this.markerTo,
          insert: replacement,
        },
      });
    });

    wrap.appendChild(input);
    return wrap;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

class CodeBlockHeaderWidget extends WidgetType {
  constructor(readonly lang: string) {
    super();
  }

  eq(other: CodeBlockHeaderWidget): boolean {
    return this.lang === other.lang;
  }

  toDOM(): HTMLElement {
    const badge = document.createElement('span');
    badge.className = 'cm-live-codeblock-badge';
    badge.textContent = this.lang || 'code';
    return badge;
  }
}

class TablePipeWidget extends WidgetType {
  constructor(readonly isEdge: boolean) {
    super();
  }

  eq(other: TablePipeWidget): boolean {
    return this.isEdge === other.isEdge;
  }

  toDOM(): HTMLElement {
    const span = document.createElement('span');
    span.className = this.isEdge
      ? 'cm-live-table-edge-pipe'
      : 'cm-live-table-pipe';
    span.textContent = this.isEdge ? '' : '│';
    span.setAttribute('aria-hidden', 'true');
    return span;
  }
}

class TableSeparatorWidget extends WidgetType {
  constructor(readonly colCount: number) {
    super();
  }

  eq(other: TableSeparatorWidget): boolean {
    return this.colCount === other.colCount;
  }

  toDOM(): HTMLElement {
    const hr = document.createElement('span');
    hr.className = 'cm-live-table-sep-widget';
    hr.setAttribute('aria-hidden', 'true');
    return hr;
  }
}

class HorizontalRuleWidget extends WidgetType {
  toDOM(): HTMLElement {
    const hr = document.createElement('span');
    hr.className = 'cm-live-hr-widget';
    hr.setAttribute('aria-hidden', 'true');
    return hr;
  }

  eq(): boolean {
    return true;
  }
}

class ImageEmbedWidget extends WidgetType {
  constructor(
    readonly target: string,
    readonly altText: string,
    readonly posFrom: number
  ) {
    super();
  }

  eq(other: ImageEmbedWidget): boolean {
    return (
      this.target === other.target &&
      this.altText === other.altText &&
      this.posFrom === other.posFrom
    );
  }

  toDOM(view: EditorView): HTMLElement {
    const container = document.createElement('span');
    container.className = 'cm-live-image-embed';
    container.title = `${this.altText || this.target} (Click to edit syntax)`;

    const img = document.createElement('img');
    img.className = 'cm-live-image-el';
    img.alt = this.altText || this.target;

    const cleanTarget = this.target.split('|')[0].trim();
    if (
      cleanTarget.startsWith('http://') ||
      cleanTarget.startsWith('https://') ||
      cleanTarget.startsWith('data:')
    ) {
      img.src = cleanTarget;
    } else {
      const cached = imageDataUrlCache.get(cleanTarget);
      if (cached) {
        img.src = cached;
      } else {
        void readVaultImageDataUrl(cleanTarget)
          .then((dataUrl) => {
            imageDataUrlCache.set(cleanTarget, dataUrl);
            img.src = dataUrl;
          })
          .catch(() => {
            container.textContent = `🖼 ${this.altText || cleanTarget}`;
            container.classList.add('cm-live-image-missing');
          });
      }
    }

    container.addEventListener('mousedown', (e) => {
      e.preventDefault();
      e.stopPropagation();
      view.dispatch({
        selection: { anchor: this.posFrom + 2 },
      });
      view.focus();
    });

    container.appendChild(img);
    return container;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

class WikiLinkWidget extends WidgetType {
  constructor(
    readonly displayText: string,
    readonly rawTarget: string,
    readonly noteTarget: string,
    readonly heading: string | undefined,
    readonly posFrom: number,
    readonly onWikiLinkClick?: (noteTarget: string, heading?: string) => void
  ) {
    super();
  }

  eq(other: WikiLinkWidget): boolean {
    return (
      this.displayText === other.displayText &&
      this.rawTarget === other.rawTarget &&
      this.noteTarget === other.noteTarget &&
      this.heading === other.heading &&
      this.posFrom === other.posFrom
    );
  }

  toDOM(view: EditorView): HTMLElement {
    const span = document.createElement('span');
    span.className = 'cm-live-wikilink-widget';
    span.textContent = this.displayText;
    span.title = `Open [[${this.rawTarget}]] (Alt+Click to edit syntax)`;
    span.addEventListener('mousedown', (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.altKey || !this.onWikiLinkClick) {
        view.dispatch({
          selection: { anchor: this.posFrom + 2 },
        });
        view.focus();
        return;
      }
      this.onWikiLinkClick(this.noteTarget, this.heading);
    });
    return span;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

function isRangeTouchedBySelection(
  state: EditorState,
  from: number,
  to: number
): boolean {
  for (const r of state.selection.ranges) {
    if (r.from <= to && r.to >= from) {
      return true;
    }
  }
  return false;
}

function isLineTouchedBySelection(
  state: EditorState,
  lineFrom: number,
  lineTo: number
): boolean {
  for (const r of state.selection.ranges) {
    if (r.from <= lineTo && r.to >= lineFrom) {
      return true;
    }
  }
  return false;
}

function getActiveParagraphLineRange(state: EditorState): {
  startLine: number;
  endLine: number;
} {
  const headLine = state.doc.lineAt(state.selection.main.head).number;
  let startLine = headLine;
  let endLine = headLine;

  while (startLine > 1) {
    const prev = state.doc.line(startLine - 1);
    if (prev.text.trim().length === 0) break;
    startLine--;
  }
  while (endLine < state.doc.lines) {
    const next = state.doc.line(endLine + 1);
    if (next.text.trim().length === 0) break;
    endLine++;
  }
  return { startLine, endLine };
}

/**
 * Computes merged visible line windows from `view.visibleRanges`, expanded by
 * `VIEWPORT_MARGIN_LINES` (~20 lines) above and below each visible range.
 */
function computeVisibleLineWindows(view: EditorView): VisibleLineWindow[] {
  const { state } = view;
  const totalLines = state.doc.lines;
  const rawRanges =
    view.visibleRanges.length > 0
      ? view.visibleRanges
      : [{ from: 0, to: state.doc.length }];

  const windows: VisibleLineWindow[] = [];

  for (const r of rawRanges) {
    const startLine = Math.max(
      1,
      state.doc.lineAt(r.from).number - VIEWPORT_MARGIN_LINES
    );
    const endLine = Math.min(
      totalLines,
      state.doc.lineAt(r.to).number + VIEWPORT_MARGIN_LINES
    );
    const from = state.doc.line(startLine).from;
    const to = state.doc.line(endLine).to;

    const prev = windows[windows.length - 1];
    if (prev && startLine <= prev.endLine + 1) {
      prev.endLine = Math.max(prev.endLine, endLine);
      prev.to = Math.max(prev.to, to);
    } else {
      windows.push({ startLine, endLine, from, to });
    }
  }

  return windows;
}

/**
 * Returns true if the character at `index` in `text` is escaped by an odd number of preceding `\`.
 */
export function isEscapedAt(text: string, index: number): boolean {
  let backslashes = 0;
  for (let i = index - 1; i >= 0 && text[i] === '\\'; i--) {
    backslashes++;
  }
  return backslashes % 2 === 1;
}

/**
 * Finds inline code spans (delimited by matching backtick runs `` `...` ``) within a single line.
 * Escaped backticks (`\``) are ignored when opening/closing spans.
 */
export function findInlineCodeSpansInLine(
  lineText: string
): Array<{ from: number; to: number }> {
  const spans: Array<{ from: number; to: number }> = [];
  let i = 0;
  const n = lineText.length;

  while (i < n) {
    if (lineText[i] === '`' && !isEscapedAt(lineText, i)) {
      let openLen = 0;
      const start = i;
      while (i < n && lineText[i] === '`') {
        openLen++;
        i++;
      }

      // Find matching closing run of `openLen` backticks
      let foundClose = -1;
      let j = i;
      while (j < n) {
        if (lineText[j] === '`' && !isEscapedAt(lineText, j)) {
          let closeLen = 0;
          const closeStart = j;
          while (j < n && lineText[j] === '`') {
            closeLen++;
            j++;
          }
          if (closeLen === openLen) {
            foundClose = closeStart + closeLen;
            break;
          }
        } else {
          j++;
        }
      }

      if (foundClose !== -1) {
        spans.push({ from: start, to: foundClose });
        i = foundClose;
      }
    } else {
      i++;
    }
  }

  return spans;
}

export function parseWikiLinkInner(inner: string): {
  display: string;
  rawTarget: string;
  noteTarget: string;
  heading?: string;
} {
  const pipeIdx = inner.indexOf('|');
  const targetPart =
    pipeIdx >= 0 ? inner.slice(0, pipeIdx).trim() : inner.trim();
  const aliasPart = pipeIdx >= 0 ? inner.slice(pipeIdx + 1).trim() : '';

  const hashIdx = targetPart.indexOf('#');
  const noteTarget =
    hashIdx >= 0 ? targetPart.slice(0, hashIdx).trim() : targetPart;
  const heading =
    hashIdx >= 0 ? targetPart.slice(hashIdx + 1).trim() || undefined : undefined;

  let display = targetPart;
  if (aliasPart) {
    display = aliasPart;
  } else if (hashIdx >= 0) {
    if (noteTarget && heading) {
      display = `${noteTarget} › ${heading}`;
    } else {
      display = heading || noteTarget || inner.trim();
    }
  }

  return {
    display,
    rawTarget: inner.trim(),
    noteTarget,
    heading,
  };
}

function buildLivePreviewDecorations(
  view: EditorView,
  config: LivePreviewConfig
): DecorationSet {
  const { state } = view;
  const windows = computeVisibleLineWindows(view);
  const decorations: Range<Decoration>[] = [];
  const lineClasses = new Map<number, string[]>();

  const addLineClass = (lineFrom: number, cls: string) => {
    const list = lineClasses.get(lineFrom);
    if (list) {
      if (!list.includes(cls)) list.push(cls);
    } else {
      lineClasses.set(lineFrom, [cls]);
    }
  };

  // 1. Optional paragraph dimming — runs ONLY on visible lines
  if (config.dimInactiveParagraphs) {
    const { startLine, endLine } = getActiveParagraphLineRange(state);
    for (const win of windows) {
      for (let ln = win.startLine; ln <= win.endLine; ln++) {
        if (ln < startLine || ln > endLine) {
          const line = state.doc.line(ln);
          addLineClass(line.from, 'cm-dimmed-paragraph-line');
        }
      }
    }
  }

  // 2. If in Live Mode, iterate syntax tree ONLY over visible windows
  if (config.editingMode === 'live') {
    const codeRanges: Array<{ from: number; to: number }> = [];
    const handledTaskItemLines = new Set<number>();
    const visitedNodes = new Set<string>();
    const tree = syntaxTree(state);

    // Pre-collect InlineCode and FencedCode ranges within the visible windows so
    // table pipe replacement and wiki-link scans can check `isInsideCode` accurately.
    for (const win of windows) {
      tree.iterate({
        from: win.from,
        to: win.to,
        enter: (node) => {
          if (node.name === 'InlineCode' || node.name === 'FencedCode') {
            codeRanges.push({ from: node.from, to: node.to });
          }
        },
      });
    }

    const isInsideSyntaxCode = (pos: number) =>
      codeRanges.some((r) => pos >= r.from && pos < r.to);

    for (const win of windows) {
      tree.iterate({
        from: win.from,
        to: win.to,
        enter: (node) => {
          const nodeKey = `${node.name}:${node.from}:${node.to}`;
          if (visitedNodes.has(nodeKey)) return;
          visitedNodes.add(nodeKey);

          const name = node.name;

          // ATX Headings (# .. ######)
          if (name.startsWith('ATXHeading')) {
            const level = Number(name.replace('ATXHeading', '')) || 1;
            const line = state.doc.lineAt(node.from);
            if (line.number < win.startLine || line.number > win.endLine) {
              return;
            }
            addLineClass(line.from, `cm-live-h${level}`);

            if (!isLineTouchedBySelection(state, line.from, line.to)) {
              const m = /^(#{1,6})\s+/.exec(line.text);
              if (m) {
                decorations.push(
                  Decoration.replace({}).range(
                    line.from,
                    line.from + m[0].length
                  )
                );
              }
            }
            return;
          }

          // Blockquotes (> ) — clamped to visible window lines
          if (name === 'Blockquote') {
            const startLine = Math.max(
              win.startLine,
              state.doc.lineAt(node.from).number
            );
            const endLine = Math.min(
              win.endLine,
              state.doc.lineAt(node.to).number
            );
            for (let ln = startLine; ln <= endLine; ln++) {
              const line = state.doc.line(ln);
              addLineClass(line.from, 'cm-live-blockquote-line');
              if (!isLineTouchedBySelection(state, line.from, line.to)) {
                const m = /^(\s*>+\s?)/.exec(line.text);
                if (m) {
                  decorations.push(
                    Decoration.replace({}).range(
                      line.from,
                      line.from + m[1].length
                    )
                  );
                }
              }
            }
            return;
          }

          // Fenced Code Blocks (```lang ... ```) — clamped to visible window lines
          if (name === 'FencedCode') {
            const blockStartLine = state.doc.lineAt(node.from);
            const blockEndLine = state.doc.lineAt(node.to);
            const visStart = Math.max(win.startLine, blockStartLine.number);
            const visEnd = Math.min(win.endLine, blockEndLine.number);

            for (let ln = visStart; ln <= visEnd; ln++) {
              const line = state.doc.line(ln);
              addLineClass(line.from, 'cm-live-codeblock-line');
              if (ln === blockStartLine.number) {
                addLineClass(line.from, 'cm-live-codeblock-start');
              }
              if (ln === blockEndLine.number) {
                addLineClass(line.from, 'cm-live-codeblock-end');
              }
            }

            if (
              blockStartLine.number >= win.startLine &&
              blockStartLine.number <= win.endLine &&
              !isLineTouchedBySelection(
                state,
                blockStartLine.from,
                blockStartLine.to
              )
            ) {
              const fenceMatch = /^(\s*(?:`{3,}|~{3,}))(.*)$/.exec(
                blockStartLine.text
              );
              if (fenceMatch && blockStartLine.to > blockStartLine.from) {
                const lang = fenceMatch[2].trim();
                decorations.push(
                  Decoration.replace({
                    widget: new CodeBlockHeaderWidget(lang),
                  }).range(blockStartLine.from, blockStartLine.to)
                );
              }
            }

            if (
              blockEndLine.number > blockStartLine.number &&
              blockEndLine.number >= win.startLine &&
              blockEndLine.number <= win.endLine &&
              !isLineTouchedBySelection(
                state,
                blockEndLine.from,
                blockEndLine.to
              )
            ) {
              if (
                /^\s*(?:`{3,}|~{3,})\s*$/.test(blockEndLine.text) &&
                blockEndLine.to > blockEndLine.from
              ) {
                decorations.push(
                  Decoration.replace({
                    widget: new CodeBlockHeaderWidget(''),
                  }).range(blockEndLine.from, blockEndLine.to)
                );
              }
            }
            return;
          }

          // Inline Code (`code`)
          if (name === 'InlineCode') {
            const lineStart = state.doc.lineAt(node.from);
            const lineEnd = state.doc.lineAt(node.to);
            if (lineStart.number === lineEnd.number) {
              decorations.push(
                Decoration.mark({ class: 'cm-live-inline-code' }).range(
                  node.from,
                  node.to
                )
              );
              if (!isRangeTouchedBySelection(state, node.from, node.to)) {
                const text = state.sliceDoc(node.from, node.to);
                const openTicks = /^(`+)/.exec(text)?.[1].length ?? 1;
                const closeTicks = /(`+)$/.exec(text)?.[1].length ?? 1;
                if (node.to - node.from > openTicks + closeTicks) {
                  decorations.push(
                    Decoration.replace({}).range(
                      node.from,
                      node.from + openTicks
                    )
                  );
                  decorations.push(
                    Decoration.replace({}).range(
                      node.to - closeTicks,
                      node.to
                    )
                  );
                }
              }
            }
            return;
          }

          // Bold (**text** or __text__)
          if (name === 'StrongEmphasis') {
            const lineStart = state.doc.lineAt(node.from);
            const lineEnd = state.doc.lineAt(node.to);
            if (lineStart.number === lineEnd.number && node.to - node.from > 4) {
              decorations.push(
                Decoration.mark({ class: 'cm-live-bold' }).range(
                  node.from,
                  node.to
                )
              );
              if (!isRangeTouchedBySelection(state, node.from, node.to)) {
                decorations.push(
                  Decoration.replace({}).range(node.from, node.from + 2)
                );
                decorations.push(
                  Decoration.replace({}).range(node.to - 2, node.to)
                );
              }
            }
            return;
          }

          // Italic (*text* or _text_)
          if (name === 'Emphasis') {
            const lineStart = state.doc.lineAt(node.from);
            const lineEnd = state.doc.lineAt(node.to);
            if (lineStart.number === lineEnd.number && node.to - node.from > 2) {
              decorations.push(
                Decoration.mark({ class: 'cm-live-italic' }).range(
                  node.from,
                  node.to
                )
              );
              if (!isRangeTouchedBySelection(state, node.from, node.to)) {
                decorations.push(
                  Decoration.replace({}).range(node.from, node.from + 1)
                );
                decorations.push(
                  Decoration.replace({}).range(node.to - 1, node.to)
                );
              }
            }
            return;
          }

          // Strikethrough (~~text~~)
          if (name === 'Strikethrough') {
            const lineStart = state.doc.lineAt(node.from);
            const lineEnd = state.doc.lineAt(node.to);
            if (lineStart.number === lineEnd.number && node.to - node.from > 4) {
              decorations.push(
                Decoration.mark({ class: 'cm-live-strike' }).range(
                  node.from,
                  node.to
                )
              );
              if (!isRangeTouchedBySelection(state, node.from, node.to)) {
                decorations.push(
                  Decoration.replace({}).range(node.from, node.from + 2)
                );
                decorations.push(
                  Decoration.replace({}).range(node.to - 2, node.to)
                );
              }
            }
            return;
          }

          // Standard Markdown Images ![alt](url)
          if (name === 'Image') {
            const lineStart = state.doc.lineAt(node.from);
            const lineEnd = state.doc.lineAt(node.to);
            if (lineStart.number === lineEnd.number) {
              const raw = state.sliceDoc(node.from, node.to);
              const m = /^!\[([^\]]*)\]\(([^)]+)\)$/.exec(raw);
              if (m && !isRangeTouchedBySelection(state, node.from, node.to)) {
                decorations.push(
                  Decoration.replace({
                    widget: new ImageEmbedWidget(m[2].trim(), m[1].trim(), node.from),
                  }).range(node.from, node.to)
                );
              }
            }
            return;
          }

          // Standard Markdown Links [label](url)
          if (name === 'Link') {
            const lineStart = state.doc.lineAt(node.from);
            const lineEnd = state.doc.lineAt(node.to);
            if (lineStart.number === lineEnd.number) {
              const raw = state.sliceDoc(node.from, node.to);
              const m = /^\[([^\]]+)\]\(([^)]*)\)$/.exec(raw);
              if (m) {
                const labelStart = node.from + 1;
                const labelEnd = labelStart + m[1].length;
                decorations.push(
                  Decoration.mark({ class: 'cm-live-link' }).range(
                    labelStart,
                    labelEnd
                  )
                );
                if (!isRangeTouchedBySelection(state, node.from, node.to)) {
                  decorations.push(
                    Decoration.replace({}).range(node.from, labelStart)
                  );
                  decorations.push(
                    Decoration.replace({}).range(labelEnd, node.to)
                  );
                }
              }
            }
            return;
          }

          // Task List Marker ([ ] or [x])
          if (name === 'TaskMarker') {
            const line = state.doc.lineAt(node.from);
            handledTaskItemLines.add(line.number);
            const markerText = state.sliceDoc(node.from, node.to);
            const checked = /\[[xX]\]/.test(markerText);
            if (checked) {
              addLineClass(line.from, 'cm-live-task-done-line');
            }

            const prefixMatch = /^(\s*)([-*+]\s+\[[ xX]\])/.exec(line.text);
            const replaceFrom = prefixMatch
              ? line.from + prefixMatch[1].length
              : node.from;
            const replaceTo = node.to;

            if (!isRangeTouchedBySelection(state, replaceFrom, replaceTo)) {
              decorations.push(
                Decoration.replace({
                  widget: new TaskCheckboxWidget(checked, node.from, node.to),
                }).range(replaceFrom, replaceTo)
              );
            }
            return;
          }

          // Unordered / Ordered List Marks (-, *, +, 1.)
          if (name === 'ListMark') {
            const line = state.doc.lineAt(node.from);
            if (handledTaskItemLines.has(line.number)) return;
            if (/^\s*[-*+]\s+\[[ xX]\]/.test(line.text)) return;

            const markText = state.sliceDoc(node.from, node.to);
            if (markText === '-' || markText === '*' || markText === '+') {
              if (!isRangeTouchedBySelection(state, node.from, node.to + 1)) {
                decorations.push(
                  Decoration.replace({
                    widget: new BulletPointWidget(),
                  }).range(node.from, node.to)
                );
              }
            } else {
              decorations.push(
                Decoration.mark({ class: 'cm-live-ordered-mark' }).range(
                  node.from,
                  node.to
                )
              );
            }
            return;
          }

          // GFM Tables (Table, TableHeader, TableRow, TableDelimiter) — clamped to visible lines
          if (name === 'Table') {
            const tableStartLine = state.doc.lineAt(node.from).number;
            const tableEndLine = state.doc.lineAt(node.to).number;
            const visStart = Math.max(win.startLine, tableStartLine);
            const visEnd = Math.min(win.endLine, tableEndLine);

            for (let ln = visStart; ln <= visEnd; ln++) {
              const line = state.doc.line(ln);
              const trimmed = line.text.trim();
              if (!trimmed) continue;

              const isSeparatorLine =
                /^\|?[\s:]*-{2,}[\s:]*(\|[\s:]*-{2,}[\s:]*)*\|?$/.test(
                  trimmed
                );
              const isHeaderLine = ln === tableStartLine;

              addLineClass(line.from, 'cm-live-table-line');
              if (isHeaderLine) {
                addLineClass(line.from, 'cm-live-table-header-line');
              } else if (isSeparatorLine) {
                addLineClass(line.from, 'cm-live-table-sep-line');
              } else {
                addLineClass(line.from, 'cm-live-table-row-line');
              }

              if (!isLineTouchedBySelection(state, line.from, line.to)) {
                if (isSeparatorLine && line.to > line.from) {
                  const colCount = Math.max(
                    1,
                    trimmed.split('|').filter((c) => c.trim().length > 0).length
                  );
                  decorations.push(
                    Decoration.replace({
                      widget: new TableSeparatorWidget(colCount),
                    }).range(line.from, line.to)
                  );
                } else {
                  // Replace only unescaped table delimiter pipes outside inline code spans
                  const text = line.text;
                  const inlineSpans = findInlineCodeSpansInLine(text);
                  const isInsideLineCode = (offset: number) =>
                    isInsideSyntaxCode(line.from + offset) ||
                    inlineSpans.some(
                      (s) => offset >= s.from && offset < s.to
                    );

                  const validPipeIndices: number[] = [];
                  for (let i = 0; i < text.length; i++) {
                    if (
                      text[i] === '|' &&
                      !isEscapedAt(text, i) &&
                      !isInsideLineCode(i)
                    ) {
                      validPipeIndices.push(i);
                    }
                  }

                  if (validPipeIndices.length > 0) {
                    const firstPipe = validPipeIndices[0];
                    const lastPipe =
                      validPipeIndices[validPipeIndices.length - 1];
                    for (const pipeIdx of validPipeIndices) {
                      const isEdge =
                        pipeIdx === firstPipe || pipeIdx === lastPipe;
                      decorations.push(
                        Decoration.replace({
                          widget: new TablePipeWidget(isEdge),
                        }).range(line.from + pipeIdx, line.from + pipeIdx + 1)
                      );
                    }
                  }
                }
              }
            }
            return;
          }

          // Horizontal Rule (---, ***, ___)
          if (name === 'HorizontalRule') {
            const line = state.doc.lineAt(node.from);
            if (line.number < win.startLine || line.number > win.endLine) {
              return;
            }
            addLineClass(line.from, 'cm-live-hr-line');
            if (
              !isLineTouchedBySelection(state, line.from, line.to) &&
              line.to > line.from
            ) {
              decorations.push(
                Decoration.replace({
                  widget: new HorizontalRuleWidget(),
                }).range(line.from, line.to)
              );
            }
            return;
          }
        },
      });
    }

    // 3. Scan for Obsidian ![[image]] embeds and [[wiki links]] ONLY on visible lines, outside inline/fenced code
    for (const win of windows) {
      for (let ln = win.startLine; ln <= win.endLine; ln++) {
        const line = state.doc.line(ln);
        if (!line.text.includes('[[')) continue;

        const inlineSpans = findInlineCodeSpansInLine(line.text);
        const regex = /(!?)\[\[([^\][\n]+)\]\]/g;
        let match: RegExpExecArray | null;
        while ((match = regex.exec(line.text)) !== null) {
          const isEmbed = match[1] === '!';
          const innerContent = match[2];
          const offset = match.index;
          const matchFrom = line.from + offset;
          const matchTo = matchFrom + match[0].length;
          if (
            isInsideSyntaxCode(matchFrom) ||
            inlineSpans.some((s) => offset >= s.from && offset < s.to)
          ) {
            continue;
          }

          if (!isRangeTouchedBySelection(state, matchFrom, matchTo)) {
            if (isEmbed) {
              const parsed = parseWikiLinkInner(innerContent);
              decorations.push(
                Decoration.replace({
                  widget: new ImageEmbedWidget(
                    parsed.noteTarget,
                    parsed.display,
                    matchFrom
                  ),
                }).range(matchFrom, matchTo)
              );
            } else {
              const parsed = parseWikiLinkInner(innerContent);
              decorations.push(
                Decoration.replace({
                  widget: new WikiLinkWidget(
                    parsed.display,
                    parsed.rawTarget,
                    parsed.noteTarget,
                    parsed.heading,
                    matchFrom,
                    config.onWikiLinkClick
                  ),
                }).range(matchFrom, matchTo)
              );
            }
          } else {
            decorations.push(
              Decoration.mark({ class: 'cm-live-wikilink-raw' }).range(
                matchFrom,
                matchTo
              )
            );
          }
        }
      }
    }
  }

  // Push accumulated line decorations
  for (const [lineFrom, classes] of lineClasses.entries()) {
    decorations.push(
      Decoration.line({ class: classes.join(' ') }).range(lineFrom)
    );
  }

  return Decoration.set(decorations, true);
}

/**
 * Creates a CodeMirror 6 extension that provides viewport-bounded Live Preview decorations,
 * paragraph dimming, and typewriter scrolling according to `config`.
 */
export function createLivePreviewExtension(
  config: LivePreviewConfig
): Extension {
  const extensions: Extension[] = [];

  if (config.editingMode === 'live' || config.dimInactiveParagraphs) {
    const plugin = ViewPlugin.fromClass(
      class {
        decorations: DecorationSet;

        constructor(view: EditorView) {
          this.decorations = buildLivePreviewDecorations(view, config);
        }

        update(update: ViewUpdate) {
          const treeChanged =
            syntaxTree(update.startState) !== syntaxTree(update.state);
          if (
            update.docChanged ||
            update.selectionSet ||
            update.viewportChanged ||
            treeChanged
          ) {
            this.decorations = buildLivePreviewDecorations(update.view, config);
          }
        }
      },
      {
        decorations: (v) => v.decorations,
      }
    );
    extensions.push(plugin);
  }

  if (config.typewriterScrolling) {
    extensions.push(
      EditorView.contentAttributes.of({
        class: 'cm-typewriter-content',
      }),
      EditorView.updateListener.of((update) => {
        if (update.docChanged || update.selectionSet) {
          const head = update.state.selection.main.head;
          requestAnimationFrame(() => {
            if (!update.view.dom.isConnected) return;
            const coords = update.view.coordsAtPos(head);
            if (!coords) return;
            const scroller = update.view.scrollDOM;
            const scrollerRect = scroller.getBoundingClientRect();
            const targetTop =
              scrollerRect.top + scrollerRect.height / 2 - 14;
            const delta = coords.top - targetTop;
            if (Math.abs(delta) > 18) {
              scroller.scrollBy({ top: delta, behavior: 'auto' });
            }
          });
        }
      })
    );
  }

  return extensions;
}

export const livePreviewCompartment = new Compartment();
