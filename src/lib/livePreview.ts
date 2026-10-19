import { syntaxTree } from '@codemirror/language';
import {
  Compartment,
  EditorState,
  Range,
  StateField,
  type Extension,
} from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  EditorView,
  WidgetType,
} from '@codemirror/view';
import type { EditingMode } from './types';

export interface LivePreviewConfig {
  editingMode: EditingMode;
  dimInactiveParagraphs: boolean;
  typewriterScrolling: boolean;
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

class WikiLinkWidget extends WidgetType {
  constructor(
    readonly displayText: string,
    readonly target: string,
    readonly posFrom: number
  ) {
    super();
  }

  eq(other: WikiLinkWidget): boolean {
    return (
      this.displayText === other.displayText &&
      this.target === other.target &&
      this.posFrom === other.posFrom
    );
  }

  toDOM(view: EditorView): HTMLElement {
    const span = document.createElement('span');
    span.className = 'cm-live-wikilink-widget';
    span.textContent = this.displayText;
    span.title = `Wiki link: [[${this.target}]] (click to edit)`;
    span.addEventListener('mousedown', (e) => {
      e.preventDefault();
      view.dispatch({
        selection: { anchor: this.posFrom + 2 },
      });
      view.focus();
    });
    return span;
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

function formatWikiLinkDisplay(inner: string): {
  display: string;
  target: string;
} {
  const pipeIdx = inner.indexOf('|');
  if (pipeIdx >= 0) {
    const target = inner.slice(0, pipeIdx).trim();
    const alias = inner.slice(pipeIdx + 1).trim();
    return {
      display: alias || target,
      target: target || inner,
    };
  }
  const hashIdx = inner.indexOf('#');
  if (hashIdx >= 0) {
    const note = inner.slice(0, hashIdx).trim();
    const heading = inner.slice(hashIdx + 1).trim();
    if (note && heading) {
      return { display: `${note} › ${heading}`, target: inner };
    }
    return { display: heading || note || inner, target: inner };
  }
  return { display: inner.trim(), target: inner.trim() };
}

function buildLivePreviewDecorations(
  state: EditorState,
  config: LivePreviewConfig
): DecorationSet {
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

  // 1. Optional paragraph dimming in Focus Mode / Editor settings
  if (config.dimInactiveParagraphs) {
    const { startLine, endLine } = getActiveParagraphLineRange(state);
    for (let ln = 1; ln <= state.doc.lines; ln++) {
      if (ln < startLine || ln > endLine) {
        const line = state.doc.line(ln);
        addLineClass(line.from, 'cm-dimmed-paragraph-line');
      }
    }
  }

  // 2. If in Live Mode, walk syntax tree and build inline/line/widget decorations
  if (config.editingMode === 'live') {
    const codeRanges: Array<{ from: number; to: number }> = [];
    const handledTaskItemLines = new Set<number>();
    const tree = syntaxTree(state);

    tree.iterate({
      enter: (node) => {
        const name = node.name;

        // ATX Headings (# .. ######)
        if (name.startsWith('ATXHeading')) {
          const level = Number(name.replace('ATXHeading', '')) || 1;
          const line = state.doc.lineAt(node.from);
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

        // Blockquotes (> )
        if (name === 'Blockquote') {
          const startLine = state.doc.lineAt(node.from).number;
          const endLine = state.doc.lineAt(node.to).number;
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

        // Fenced Code Blocks (```lang ... ```)
        if (name === 'FencedCode') {
          codeRanges.push({ from: node.from, to: node.to });
          const startLine = state.doc.lineAt(node.from);
          const endLine = state.doc.lineAt(node.to);

          for (let ln = startLine.number; ln <= endLine.number; ln++) {
            const line = state.doc.line(ln);
            addLineClass(line.from, 'cm-live-codeblock-line');
            if (ln === startLine.number) {
              addLineClass(line.from, 'cm-live-codeblock-start');
            }
            if (ln === endLine.number) {
              addLineClass(line.from, 'cm-live-codeblock-end');
            }
          }

          if (!isLineTouchedBySelection(state, startLine.from, startLine.to)) {
            const fenceMatch = /^(\s*(?:`{3,}|~{3,}))(.*)$/.exec(
              startLine.text
            );
            if (fenceMatch && startLine.to > startLine.from) {
              const lang = fenceMatch[2].trim();
              decorations.push(
                Decoration.replace({
                  widget: new CodeBlockHeaderWidget(lang),
                }).range(startLine.from, startLine.to)
              );
            }
          }

          if (
            endLine.number > startLine.number &&
            !isLineTouchedBySelection(state, endLine.from, endLine.to)
          ) {
            if (/^\s*(?:`{3,}|~{3,})\s*$/.test(endLine.text) && endLine.to > endLine.from) {
              decorations.push(
                Decoration.replace({
                  widget: new CodeBlockHeaderWidget(''),
                }).range(endLine.from, endLine.to)
              );
            }
          }
          return;
        }

        // Inline Code (`code`)
        if (name === 'InlineCode') {
          codeRanges.push({ from: node.from, to: node.to });
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

          // Replace from the list bullet (`- [ ]`) through `TaskMarker.to` when cursor isn't inside the prefix
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
          // Check if this line actually has a task marker
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

        // GFM Tables (Table, TableHeader, TableRow, TableDelimiter)
        if (name === 'Table') {
          const startLine = state.doc.lineAt(node.from).number;
          const endLine = state.doc.lineAt(node.to).number;

          for (let ln = startLine; ln <= endLine; ln++) {
            const line = state.doc.line(ln);
            const trimmed = line.text.trim();
            if (!trimmed) continue;

            const isSeparatorLine =
              /^\|?[\s:]*-{2,}[\s:]*(\|[\s:]*-{2,}[\s:]*)*\|?$/.test(trimmed);
            const isHeaderLine = ln === startLine;

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
                // Replace pipe delimiters on non-active table rows with clean column dividers
                const text = line.text;
                const firstPipe = text.indexOf('|');
                const lastPipe = text.lastIndexOf('|');
                for (let i = 0; i < text.length; i++) {
                  if (text[i] === '|') {
                    const isEdge = i === firstPipe || i === lastPipe;
                    decorations.push(
                      Decoration.replace({
                        widget: new TablePipeWidget(isEdge),
                      }).range(line.from + i, line.from + i + 1)
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

    // 3. Scan for Obsidian [[wiki links]] outside inline/fenced code ranges
    const isInsideCode = (pos: number) =>
      codeRanges.some((r) => pos >= r.from && pos <= r.to);

    for (let ln = 1; ln <= state.doc.lines; ln++) {
      const line = state.doc.line(ln);
      if (!line.text.includes('[[')) continue;

      const regex = /\[\[([^\][\n]+)\]\]/g;
      let match: RegExpExecArray | null;
      while ((match = regex.exec(line.text)) !== null) {
        const matchFrom = line.from + match.index;
        const matchTo = matchFrom + match[0].length;
        if (isInsideCode(matchFrom)) continue;

        if (!isRangeTouchedBySelection(state, matchFrom, matchTo)) {
          const { display, target } = formatWikiLinkDisplay(match[1]);
          decorations.push(
            Decoration.replace({
              widget: new WikiLinkWidget(display, target, matchFrom),
            }).range(matchFrom, matchTo)
          );
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

  // Push accumulated line decorations
  for (const [lineFrom, classes] of lineClasses.entries()) {
    decorations.push(
      Decoration.line({ class: classes.join(' ') }).range(lineFrom)
    );
  }

  return Decoration.set(decorations, true);
}

/**
 * Creates a CodeMirror 6 extension that provides Live Preview decorations,
 * paragraph dimming, and typewriter scrolling according to `config`.
 */
export function createLivePreviewExtension(
  config: LivePreviewConfig
): Extension {
  const extensions: Extension[] = [];

  if (config.editingMode === 'live' || config.dimInactiveParagraphs) {
    const field = StateField.define<DecorationSet>({
      create(state) {
        return buildLivePreviewDecorations(state, config);
      },
      update(prev, tr) {
        if (tr.docChanged || tr.selection) {
          return buildLivePreviewDecorations(tr.state, config);
        }
        return prev;
      },
      provide: (f) => EditorView.decorations.from(f),
    });
    extensions.push(field);
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
