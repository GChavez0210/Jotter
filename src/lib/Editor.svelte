<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { EditorState } from '@codemirror/state';
  import {
    EditorView,
    keymap,
    placeholder,
    drawSelection,
    highlightActiveLine,
  } from '@codemirror/view';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import { markdown } from '@codemirror/lang-markdown';
  import { syntaxHighlighting, HighlightStyle } from '@codemirror/language';
  import { tags } from '@lezer/highlight';
  import {
    copyToClipboard,
    pastePlainFromClipboard,
    savePastedImage,
  } from './ipc';
  import {
    createLivePreviewExtension,
    findInlineCodeSpansInLine,
    livePreviewCompartment,
    parseWikiLinkInner,
  } from './livePreview';
  import {
    markdownFormattingKeymap,
    runMarkdownCommand,
    type MarkdownFormatCommandId,
  } from './markdownCommands';
  import { settingsStore } from './stores/settings.svelte';
  import type {
    ClipboardPayload,
    CopyMode,
    EditingMode,
    VaultNode,
  } from './types';
  import { createWikiLinkAutocompleteExtension } from './wikiLinkAutocomplete';

  interface Props {
    docId: string;
    initialDoc: string;
    copyMode: CopyMode;
    editingMode?: EditingMode;
    dimInactiveParagraphs?: boolean;
    typewriterScrolling?: boolean;
    vaultFiles?: VaultNode[];
    onWikiLinkClick?: (noteTarget: string, heading?: string) => void;
    onImagePasted?: (fileName: string, relPath: string) => void;
    onDocChange?: (docId: string, newContent: string) => void;
    onCopyComplete?: (payload: ClipboardPayload, source: 'copy' | 'cut') => void;
    onPastePlainComplete?: (charCount: number, lineCount: number) => void;
    onStatsChange?: (stats: {
      lines: number;
      words: number;
      chars: number;
      selectedChars: number;
      selectedLines: number;
    }) => void;
    onError?: (err: string) => void;
  }

  let {
    docId,
    initialDoc,
    copyMode,
    editingMode = 'live',
    dimInactiveParagraphs = false,
    typewriterScrolling = false,
    vaultFiles = [],
    onWikiLinkClick,
    onImagePasted,
    onDocChange,
    onCopyComplete,
    onPastePlainComplete,
    onStatsChange,
    onError,
  }: Props = $props();

  let editorHost: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;
  let currentLoadedDocId = '';
  let suppressChangeEmit = false;

  const markdownHighlight = HighlightStyle.define([
    {
      tag: tags.heading1,
      fontFamily: 'var(--font-display)',
      fontSize: 'var(--text-xl)',
      fontWeight: '600',
      color: 'var(--syn-heading)',
    },
    {
      tag: [tags.heading2, tags.heading3],
      fontFamily: 'var(--font-display)',
      fontSize: 'var(--text-lg)',
      fontWeight: '600',
      color: 'var(--syn-heading)',
    },
    {
      tag: [tags.heading4, tags.heading5, tags.heading6],
      fontWeight: '600',
      color: 'var(--syn-heading)',
    },
    {
      tag: tags.strong,
      fontWeight: '600',
      color: 'var(--syn-emphasis)',
    },
    {
      tag: tags.emphasis,
      fontStyle: 'italic',
      color: 'var(--syn-emphasis)',
    },
    {
      tag: tags.strikethrough,
      textDecoration: 'line-through',
      color: 'var(--text-muted)',
    },
    {
      tag: [tags.monospace, tags.processingInstruction],
      fontFamily: 'var(--font-mono)',
      fontSize: 'var(--text-sm)',
      color: 'var(--syn-code)',
    },
    {
      tag: [tags.link, tags.url],
      color: 'var(--syn-link)',
      textDecoration: 'underline',
    },
    {
      tag: tags.quote,
      color: 'var(--syn-quote)',
      fontStyle: 'italic',
    },
    {
      tag: [tags.meta, tags.comment],
      color: 'var(--syn-meta)',
    },
  ]);

  const editorTheme = EditorView.theme({
    '&': {
      height: '100%',
      fontSize: 'var(--editor-font-size, var(--text-base))',
      fontFamily: 'var(--font-ui)',
      color: 'var(--text-primary)',
      backgroundColor: 'transparent',
    },
    '.cm-scroller': {
      overflow: 'auto',
      padding: 'var(--space-6) var(--space-4) var(--space-8) var(--space-4)',
      fontFamily: 'var(--font-ui)',
      lineHeight: 'var(--leading-relaxed)',
    },
    '.cm-content': {
      maxWidth: 'var(--editor-max-width)',
      margin: '0 auto',
      padding: '0 0 var(--space-8) 0',
      caretColor: 'var(--accent-color, var(--focus-ring))',
    },
    '.cm-content.cm-typewriter-content': {
      paddingTop: '42vh',
      paddingBottom: '42vh',
    },
    '.cm-line': {
      padding: '0 var(--space-2)',
      transition: 'opacity var(--transition-fast)',
    },
    '.cm-activeLine': {
      backgroundColor: 'var(--bg-subtle)',
      borderRadius: 'var(--radius-control)',
    },
    '&.cm-focused': {
      outline: 'none',
    },
    '.cm-selectionBackground, &.cm-focused .cm-selectionBackground, ::selection': {
      backgroundColor: 'var(--selection-bg) !important',
    },
    '.cm-cursor, .cm-dropCursor': {
      borderLeftColor: 'var(--accent-color, var(--focus-ring))',
      borderLeftWidth: '2px',
    },
    /* Live Preview Inline & Block Styles */
    '.cm-line.cm-live-h1': {
      fontFamily: 'var(--font-display)',
      fontSize: 'var(--text-xl)',
      fontWeight: '600',
      lineHeight: 'var(--leading-tight)',
      paddingTop: 'var(--space-3)',
      paddingBottom: 'var(--space-1)',
      color: 'var(--syn-heading)',
    },
    '.cm-line.cm-live-h2': {
      fontFamily: 'var(--font-display)',
      fontSize: 'var(--text-lg)',
      fontWeight: '600',
      lineHeight: 'var(--leading-tight)',
      paddingTop: 'var(--space-2)',
      paddingBottom: '2px',
      color: 'var(--syn-heading)',
    },
    '.cm-line.cm-live-h3': {
      fontFamily: 'var(--font-display)',
      fontSize: 'var(--text-base)',
      fontWeight: '600',
      color: 'var(--syn-heading)',
    },
    '.cm-line.cm-live-h4, .cm-line.cm-live-h5, .cm-line.cm-live-h6': {
      fontSize: 'var(--text-sm)',
      fontWeight: '600',
      color: 'var(--syn-heading)',
    },
    '.cm-live-bold': {
      fontWeight: '600',
      color: 'var(--syn-emphasis)',
    },
    '.cm-live-italic': {
      fontStyle: 'italic',
      color: 'var(--syn-emphasis)',
    },
    '.cm-live-strike': {
      textDecoration: 'line-through',
      color: 'var(--text-muted)',
    },
    '.cm-live-inline-code': {
      fontFamily: 'var(--font-mono)',
      fontSize: '0.9em',
      backgroundColor: 'var(--bg-code)',
      color: 'var(--syn-code)',
      padding: '1px 5px',
      borderRadius: 'var(--radius-control)',
      border: '1px solid var(--border-subtle)',
    },
    '.cm-live-link': {
      color: 'var(--syn-link)',
      textDecoration: 'underline',
      textUnderlineOffset: '2px',
    },
    '.cm-live-wikilink-widget': {
      display: 'inline-flex',
      alignItems: 'center',
      color: 'var(--syn-link)',
      backgroundColor: 'var(--bg-subtle)',
      border: '1px solid var(--border-subtle)',
      borderRadius: 'var(--radius-control)',
      padding: '0 6px',
      fontSize: '0.95em',
      fontWeight: '500',
      cursor: 'pointer',
      textDecoration: 'none',
    },
    '.cm-live-wikilink-widget:hover': {
      backgroundColor: 'var(--bg-control-hover)',
      borderColor: 'var(--accent-color, var(--accent-fallback))',
    },
    '.cm-live-wikilink-raw': {
      color: 'var(--syn-link)',
    },
    '.cm-live-image-embed': {
      display: 'inline-block',
      margin: '4px 0',
      maxWidth: '100%',
      cursor: 'pointer',
    },
    '.cm-live-image-el': {
      display: 'block',
      maxWidth: '100%',
      maxHeight: '360px',
      borderRadius: 'var(--radius-card)',
      border: '1px solid var(--border-subtle)',
    },
    '.cm-live-image-missing': {
      display: 'inline-flex',
      alignItems: 'center',
      padding: '2px 8px',
      borderRadius: 'var(--radius-control)',
      border: '1px dashed var(--border-control)',
      backgroundColor: 'var(--bg-subtle)',
      color: 'var(--text-muted)',
      fontSize: 'var(--text-xs)',
    },
    '.cm-live-bullet-widget': {
      display: 'inline-block',
      width: '14px',
      textAlign: 'center',
      color: 'var(--accent-color, var(--accent-fallback))',
      fontWeight: '700',
    },
    '.cm-live-ordered-mark': {
      color: 'var(--accent-color, var(--accent-fallback))',
      fontWeight: '600',
    },
    '.cm-live-task-widget': {
      display: 'inline-flex',
      alignItems: 'center',
      verticalAlign: 'middle',
      marginRight: '6px',
    },
    '.cm-live-task-checkbox': {
      width: '15px',
      height: '15px',
      margin: '0',
      cursor: 'pointer',
      accentColor: 'var(--accent-color, var(--accent-fallback))',
    },
    '.cm-line.cm-live-task-done-line': {
      color: 'var(--text-muted)',
      textDecoration: 'line-through',
    },
    '.cm-line.cm-live-blockquote-line': {
      borderLeft: '3px solid var(--accent-color, var(--accent-fallback))',
      paddingLeft: 'var(--space-3)',
      backgroundColor: 'var(--bg-subtle)',
      color: 'var(--syn-quote)',
      fontStyle: 'italic',
    },
    '.cm-line.cm-live-codeblock-line': {
      fontFamily: 'var(--font-mono)',
      fontSize: 'var(--text-sm)',
      backgroundColor: 'var(--bg-code)',
      borderLeft: '1px solid var(--border-subtle)',
      borderRight: '1px solid var(--border-subtle)',
      paddingLeft: 'var(--space-3)',
      paddingRight: 'var(--space-3)',
    },
    '.cm-line.cm-live-codeblock-start': {
      borderTop: '1px solid var(--border-subtle)',
      borderTopLeftRadius: 'var(--radius-control)',
      borderTopRightRadius: 'var(--radius-control)',
      paddingTop: 'var(--space-1)',
    },
    '.cm-line.cm-live-codeblock-end': {
      borderBottom: '1px solid var(--border-subtle)',
      borderBottomLeftRadius: 'var(--radius-control)',
      borderBottomRightRadius: 'var(--radius-control)',
      paddingBottom: 'var(--space-1)',
    },
    '.cm-live-codeblock-badge': {
      display: 'inline-block',
      fontFamily: 'var(--font-mono)',
      fontSize: '11px',
      color: 'var(--text-muted)',
      textTransform: 'uppercase',
      letterSpacing: '0.04em',
      userSelect: 'none',
    },
    '.cm-line.cm-live-table-line': {
      fontFamily: 'var(--font-mono)',
      fontSize: 'var(--text-sm)',
      backgroundColor: 'var(--bg-subtle)',
      borderLeft: '1px solid var(--border-subtle)',
      borderRight: '1px solid var(--border-subtle)',
      paddingLeft: 'var(--space-3)',
      paddingRight: 'var(--space-3)',
    },
    '.cm-line.cm-live-table-header-line': {
      fontWeight: '600',
      borderTop: '1px solid var(--border-subtle)',
      borderTopLeftRadius: 'var(--radius-control)',
      borderTopRightRadius: 'var(--radius-control)',
      color: 'var(--syn-heading)',
    },
    '.cm-line.cm-live-table-row-line': {
      borderBottom: '1px solid var(--border-subtle)',
    },
    '.cm-live-table-pipe': {
      display: 'inline-block',
      padding: '0 var(--space-2)',
      color: 'var(--border-control)',
      userSelect: 'none',
    },
    '.cm-live-table-edge-pipe': {
      display: 'inline-block',
      width: '2px',
    },
    '.cm-live-table-sep-widget': {
      display: 'block',
      width: '100%',
      height: '1px',
      backgroundColor: 'var(--border-control)',
      margin: '6px 0',
    },
    '.cm-live-hr-widget': {
      display: 'block',
      width: '100%',
      height: '1px',
      backgroundColor: 'var(--border-control)',
      margin: '10px 0',
    },
    '.cm-line.cm-dimmed-paragraph-line': {
      opacity: '0.32',
    },
    '.cm-tooltip.cm-tooltip-autocomplete': {
      backgroundColor: 'var(--bg-card-solid)',
      border: '1px solid var(--border-control)',
      borderRadius: 'var(--radius-card)',
      fontFamily: 'var(--font-ui)',
      fontSize: 'var(--text-xs)',
      color: 'var(--text-primary)',
      padding: 'var(--space-1)',
      overflow: 'hidden',
    },
    '.cm-tooltip.cm-tooltip-autocomplete > ul': {
      fontFamily: 'var(--font-ui)',
      maxHeight: '220px',
    },
    '.cm-tooltip.cm-tooltip-autocomplete > ul > li': {
      padding: '4px 8px',
      borderRadius: 'var(--radius-control)',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      gap: 'var(--space-2)',
    },
    '.cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]': {
      backgroundColor: 'var(--selection-bg)',
      color: 'var(--text-primary)',
    },
    '.cm-completionDetail': {
      color: 'var(--text-muted)',
      fontSize: '11px',
      fontStyle: 'normal',
      marginLeft: 'auto',
    },
  });

  function emitStats(state: EditorState) {
    if (!onStatsChange) return;
    const docText = state.doc.toString();
    const lines = state.doc.lines;
    const chars = docText.length;
    const trimmed = docText.trim();
    const words = trimmed ? trimmed.split(/\s+/).length : 0;

    const mainSel = state.selection.main;
    const selectedText = mainSel.empty ? '' : state.sliceDoc(mainSel.from, mainSel.to);
    const selectedChars = selectedText.length;
    const selectedLines = selectedText ? selectedText.split('\n').length : 0;

    onStatsChange({
      lines,
      words,
      chars,
      selectedChars,
      selectedLines,
    });
  }

  function getSelectedOrCurrentLineText(
    v: EditorView,
    fallbackToWholeDoc = false
  ): {
    text: string;
    from: number;
    to: number;
  } {
    const sel = v.state.selection.main;
    if (!sel.empty) {
      return {
        text: v.state.sliceDoc(sel.from, sel.to),
        from: sel.from,
        to: sel.to,
      };
    }
    if (fallbackToWholeDoc) {
      return {
        text: v.state.doc.toString(),
        from: 0,
        to: v.state.doc.length,
      };
    }
    const line = v.state.doc.lineAt(sel.head);
    return {
      text: line.text,
      from: line.from,
      to: line.to,
    };
  }

  async function performCustomCopy(
    v: EditorView,
    source: 'copy' | 'cut',
    fallbackToWholeDoc = false
  ): Promise<boolean> {
    const target = getSelectedOrCurrentLineText(v, fallbackToWholeDoc);
    if (!target.text) return false;

    try {
      const payload = await copyToClipboard({
        text: target.text,
        mode: copyMode,
      });
      if (source === 'cut') {
        v.dispatch({
          changes: { from: target.from, to: target.to, insert: '' },
        });
      }
      onCopyComplete?.(payload, source);
      return true;
    } catch (err) {
      onError?.(err instanceof Error ? err.message : String(err));
      return false;
    }
  }

  async function performPasteAsPlainText(v: EditorView): Promise<boolean> {
    try {
      const plainText = await pastePlainFromClipboard();
      if (!plainText) return true;

      const sel = v.state.selection.main;
      v.dispatch({
        changes: { from: sel.from, to: sel.to, insert: plainText },
        selection: { anchor: sel.from + plainText.length },
        scrollIntoView: true,
      });
      const lineCount = plainText.split('\n').length;
      onPastePlainComplete?.(plainText.length, lineCount);
      return true;
    } catch (err) {
      onError?.(err instanceof Error ? err.message : String(err));
      return false;
    }
  }

  async function insertSavedImageAtCursor(
    v: EditorView,
    pngBytes: number[] | null
  ): Promise<boolean> {
    try {
      const [fileName, relPath] = await savePastedImage(pngBytes);
      const insertSyntax =
        settingsStore.imageLinkFormat === 'markdown'
          ? `![${fileName}](${relPath.replace(/ /g, '%20')})`
          : `![[${fileName}]]`;
      const sel = v.state.selection.main;
      v.dispatch({
        changes: { from: sel.from, to: sel.to, insert: insertSyntax },
        selection: { anchor: sel.from + insertSyntax.length },
        scrollIntoView: true,
      });
      onImagePasted?.(fileName, relPath);
      return true;
    } catch (err) {
      onError?.(err instanceof Error ? err.message : String(err));
      return false;
    }
  }

  function tryPasteImageFromEvent(
    event: ClipboardEvent,
    v: EditorView
  ): boolean {
    const items = event.clipboardData?.items;
    if (items) {
      for (let i = 0; i < items.length; i++) {
        const item = items[i];
        if (item.type.startsWith('image/')) {
          const blob = item.getAsFile();
          if (blob) {
            event.preventDefault();
            void blob
              .arrayBuffer()
              .then((buf) => {
                const bytes = Array.from(new Uint8Array(buf));
                return insertSavedImageAtCursor(v, bytes);
              })
              .catch(() => {
                void insertSavedImageAtCursor(v, null);
              });
            return true;
          }
        }
      }
    }

    const plainText = event.clipboardData?.getData('text/plain') ?? '';
    if (!plainText) {
      event.preventDefault();
      void insertSavedImageAtCursor(v, null);
      return true;
    }

    return false;
  }

  function tryCtrlClickRawWikiLink(event: MouseEvent, v: EditorView): boolean {
    if (!(event.ctrlKey || event.metaKey) || !onWikiLinkClick) return false;
    const pos = v.posAtCoords({ x: event.clientX, y: event.clientY });
    if (pos === null) return false;
    const line = v.state.doc.lineAt(pos);
    if (!line.text.includes('[[')) return false;

    const inlineSpans = findInlineCodeSpansInLine(line.text);
    const offsetInLine = pos - line.from;
    if (inlineSpans.some((s) => offsetInLine >= s.from && offsetInLine < s.to)) {
      return false;
    }

    const regex = /\[\[([^\][\n]+)\]\]/g;
    let match: RegExpExecArray | null;
    while ((match = regex.exec(line.text)) !== null) {
      const start = match.index;
      const end = start + match[0].length;
      if (offsetInLine >= start && offsetInLine <= end) {
        event.preventDefault();
        const parsed = parseWikiLinkInner(match[1]);
        onWikiLinkClick(parsed.noteTarget, parsed.heading);
        return true;
      }
    }
    return false;
  }

  function createEditorState(content: string): EditorState {
    return EditorState.create({
      doc: content,
      extensions: [
        history(),
        drawSelection(),
        highlightActiveLine(),
        EditorView.lineWrapping,
        placeholder('Write your Markdown note here...'),
        markdown(),
        syntaxHighlighting(markdownHighlight),
        createWikiLinkAutocompleteExtension(() => vaultFiles),
        livePreviewCompartment.of(
          createLivePreviewExtension({
            editingMode,
            dimInactiveParagraphs,
            typewriterScrolling,
            onWikiLinkClick: (target, heading) =>
              onWikiLinkClick?.(target, heading),
          })
        ),
        editorTheme,
        keymap.of([
          {
            key: 'Mod-Shift-v',
            run: (v) => {
              void performPasteAsPlainText(v);
              return true;
            },
          },
          ...markdownFormattingKeymap,
          ...defaultKeymap,
          ...historyKeymap,
        ]),
        EditorView.domEventHandlers({
          mousedown: (event, v) => {
            return tryCtrlClickRawWikiLink(event, v);
          },
          paste: (event, v) => {
            return tryPasteImageFromEvent(event, v);
          },
          copy: (event, v) => {
            event.preventDefault();
            void performCustomCopy(v, 'copy', false);
            return true;
          },
          cut: (event, v) => {
            event.preventDefault();
            void performCustomCopy(v, 'cut', false);
            return true;
          },
        }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged || update.selectionSet) {
            emitStats(update.state);
          }
          if (update.docChanged && !suppressChangeEmit) {
            onDocChange?.(docId, update.state.doc.toString());
          }
        }),
      ],
    });
  }

  $effect(() => {
    if (view && docId !== currentLoadedDocId) {
      currentLoadedDocId = docId;
      suppressChangeEmit = true;
      view.setState(createEditorState(initialDoc));
      suppressChangeEmit = false;
      emitStats(view.state);
    }
  });

  $effect(() => {
    const mode = editingMode;
    const dim = dimInactiveParagraphs;
    const typewriter = typewriterScrolling;
    if (view) {
      view.dispatch({
        effects: livePreviewCompartment.reconfigure(
          createLivePreviewExtension({
            editingMode: mode,
            dimInactiveParagraphs: dim,
            typewriterScrolling: typewriter,
            onWikiLinkClick: (target, heading) =>
              onWikiLinkClick?.(target, heading),
          })
        ),
      });
    }
  });

  function normalizeHeadingSlug(text: string): string {
    return text
      .trim()
      .replace(/^#{1,6}\s+/, '')
      .replace(/\s+#+\s*$/, '')
      .trim()
      .toLowerCase();
  }

  export function scrollToHeading(heading: string) {
    if (!view || !heading.trim()) return;
    const targetNorm = normalizeHeadingSlug(heading);
    const doc = view.state.doc;

    for (let ln = 1; ln <= doc.lines; ln++) {
      const line = doc.line(ln);
      const m = /^#{1,6}\s+(.+)$/.exec(line.text.trim());
      if (m && normalizeHeadingSlug(m[1]) === targetNorm) {
        view.dispatch({
          selection: { anchor: line.from },
          effects: EditorView.scrollIntoView(line.from, { y: 'center' }),
        });
        view.focus();
        return;
      }
    }
  }

  export function scrollToLine(lineNumber: number) {
    if (!view) return;
    const doc = view.state.doc;
    const clamped = Math.max(1, Math.min(doc.lines, Math.floor(lineNumber)));
    const line = doc.line(clamped);
    view.dispatch({
      selection: { anchor: line.from },
      effects: EditorView.scrollIntoView(line.from, { y: 'center' }),
    });
    view.focus();
  }

  export function replaceContentFromDisk(newContent: string) {
    if (!view) return;
    const currentText = view.state.doc.toString();
    if (currentText === newContent) return;
    const cursorOffset = Math.min(view.state.selection.main.head, newContent.length);
    suppressChangeEmit = true;
    view.dispatch({
      changes: { from: 0, to: currentText.length, insert: newContent },
      selection: { anchor: cursorOffset },
    });
    suppressChangeEmit = false;
    emitStats(view.state);
  }

  export function executeMarkdownCommand(cmd: MarkdownFormatCommandId) {
    if (!view) return;
    runMarkdownCommand(view, cmd);
  }

  export function focusEditor() {
    view?.focus();
  }

  export async function triggerCopyFromToolbar() {
    if (!view) return;
    await performCustomCopy(view, 'copy', true);
    view.focus();
  }

  export async function triggerPastePlainFromToolbar() {
    if (!view) return;
    await performPasteAsPlainText(view);
    view.focus();
  }

  export function getCurrentCopyTargetText(): string {
    if (!view) return initialDoc;
    return getSelectedOrCurrentLineText(view, true).text;
  }

  onMount(() => {
    if (!editorHost) return;
    currentLoadedDocId = docId;
    const state = createEditorState(initialDoc);
    view = new EditorView({
      state,
      parent: editorHost,
    });
    emitStats(view.state);
    view.focus();
  });

  onDestroy(() => {
    view?.destroy();
  });
</script>

<div class="editor-surface" bind:this={editorHost} aria-label="Markdown note editor"></div>

<style>
  .editor-surface {
    width: 100%;
    height: 100%;
    overflow: hidden;
  }
</style>
