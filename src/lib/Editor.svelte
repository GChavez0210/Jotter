<script lang="ts">
  import { onMount, onDestroy, untrack, type Snippet } from 'svelte';
  import { Compartment, EditorState } from '@codemirror/state';
  import {
    EditorView,
    keymap,
    placeholder,
    drawSelection,
    highlightActiveLine,
  } from '@codemirror/view';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
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
  import Icon from './ui/Icon.svelte';
  import type { IconName } from './ui/icons';
  import type {
    ClipboardPayload,
    CopyMode,
    EditingMode,
    VaultNode,
  } from './types';
  import { createWikiLinkAutocompleteExtension } from './wikiLinkAutocomplete';
  import { captureEditorOperation } from './editorOperations';
  import { acceptInlineCalculation, createCalcExtension } from './calcPreview';
  import { positionAfterFrontmatter } from './frontmatterPreview';
  import type { TextChange } from './frontmatter';

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
    /** Rendered above the text and scrolled with it, such as the note title. */
    header?: Snippet;
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
    header,
  }: Props = $props();

  let editorHost: HTMLDivElement | undefined = $state();
  let scrollHost: HTMLDivElement | undefined = $state();

  // Floating format toolbar shown above a non-empty selection.
  let selectionBar = $state<{ left: number; top: number } | null>(null);

  const SELECTION_ACTIONS: { cmd: MarkdownFormatCommandId; icon: IconName; label: string }[] = [
    { cmd: 'bold', icon: 'text-b', label: 'Bold (Ctrl+B)' },
    { cmd: 'italic', icon: 'text-italic', label: 'Italic (Ctrl+I)' },
    { cmd: 'strikethrough', icon: 'text-strikethrough', label: 'Strikethrough' },
    { cmd: 'inline_code', icon: 'code', label: 'Inline code' },
    { cmd: 'link', icon: 'link-simple', label: 'Link' },
    { cmd: 'wikilink', icon: 'notebook', label: 'Wiki link' },
    { cmd: 'blockquote', icon: 'quotes', label: 'Quote' },
    { cmd: 'task_list', icon: 'list-checks', label: 'Task list' },
  ];

  function updateSelectionBar(target: EditorView) {
    const sel = target.state.selection.main;
    if (sel.empty || !target.hasFocus) {
      selectionBar = null;
      return;
    }
    const start = target.coordsAtPos(sel.from);
    const end = target.coordsAtPos(sel.to);
    if (!start || !end) {
      selectionBar = null;
      return;
    }
    // Centre over a single-line selection; anchor to the start of a multi-line one.
    const left = start.top === end.top ? (start.left + end.right) / 2 : start.left + 120;
    selectionBar = { left, top: Math.min(start.top, end.top) };
  }

  function runSelectionAction(cmd: MarkdownFormatCommandId) {
    if (!view) return;
    runMarkdownCommand(view, cmd);
    view.focus();
    updateSelectionBar(view);
  }
  let view: EditorView | undefined;
  let currentLoadedDocId = '';
  let editorGeneration = 0;
  let statsDoc: EditorState['doc'] | undefined;
  let totalStats = { lines: 0, words: 0, chars: 0 };
  let statsTimer: ReturnType<typeof setTimeout> | undefined;
  const spellcheckCompartment = new Compartment();

  // WebView2 and WebKitGTK check spelling against the languages installed in the OS.
  function spellcheckExtension(enabled: boolean) {
    return EditorView.contentAttributes.of({
      spellcheck: enabled ? 'true' : 'false',
      autocorrect: 'off',
      autocapitalize: 'off',
    });
  }
  let suppressChangeEmit = false;

  const markdownHighlight = HighlightStyle.define([
    {
      tag: tags.heading1,
      fontFamily: 'var(--font-display)',
      fontSize: 'var(--fs-24)',
      fontWeight: '600',
      color: 'var(--syn-heading)',
    },
    {
      tag: [tags.heading2, tags.heading3],
      fontFamily: 'var(--font-display)',
      fontSize: 'var(--fs-17)',
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
      fontSize: 'var(--fs-13)',
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
      minHeight: '100%',
      fontSize: 'var(--editor-font-size, var(--fs-17))',
      fontFamily: 'var(--font-body)',
      color: 'var(--text-primary)',
      backgroundColor: 'transparent',
    },
    '.cm-scroller': {
      // The outer .editor-scroll element scrolls so the note header moves with the text.
      overflow: 'visible',
      padding: '0 var(--space-6) var(--space-12)',
      fontFamily: 'var(--font-body)',
      lineHeight: 'var(--leading-relaxed)',
    },
    '.cm-content': {
      maxWidth: 'var(--editor-max-width)',
      margin: '0 auto',
      padding: '0 0 var(--space-12) 0',
      caretColor: 'var(--accent)',
    },
    '.cm-content.cm-typewriter-content': {
      paddingTop: '42vh',
      paddingBottom: '42vh',
    },
    '.cm-line': {
      padding: '0',
      transition: 'opacity var(--transition-fast)',
    },
    '.cm-activeLine': {
      backgroundColor: 'transparent',
    },
    '&.cm-focused': {
      outline: 'none',
    },
    '.cm-selectionBackground, &.cm-focused .cm-selectionBackground, ::selection': {
      backgroundColor: 'var(--selection) !important',
    },
    '.cm-cursor, .cm-dropCursor': {
      borderLeftColor: 'var(--accent)',
      borderLeftWidth: '2px',
    },
    /* Live Preview Inline & Block Styles */
    '.cm-line.cm-live-h1': {
      fontFamily: 'var(--font-display)',
      fontSize: 'var(--fs-32)',
      fontWeight: '600',
      lineHeight: 'var(--leading-tight)',
      letterSpacing: '-0.01em',
      paddingTop: 'var(--space-6)',
      paddingBottom: 'var(--space-2)',
      color: 'var(--syn-heading)',
    },
    '.cm-line.cm-live-h2': {
      fontFamily: 'var(--font-display)',
      fontSize: 'var(--fs-24)',
      fontWeight: '600',
      lineHeight: 'var(--leading-tight)',
      paddingTop: 'var(--space-6)',
      paddingBottom: 'var(--space-1)',
      color: 'var(--syn-heading)',
    },
    '.cm-line.cm-live-h3': {
      fontFamily: 'var(--font-ui)',
      fontSize: 'var(--fs-17)',
      fontWeight: '600',
      paddingTop: 'var(--space-4)',
      color: 'var(--syn-heading)',
    },
    '.cm-line.cm-live-h4, .cm-line.cm-live-h5, .cm-line.cm-live-h6': {
      fontSize: 'var(--fs-15)',
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
      fontSize: '0.86em',
      backgroundColor: 'var(--surface-code)',
      color: 'var(--syn-code)',
      padding: '1px 5px',
      borderRadius: 'var(--radius-control)',
    },
    '.cm-live-link': {
      color: 'var(--syn-link)',
      textDecoration: 'underline',
      textDecorationColor: 'color-mix(in srgb, var(--accent) 40%, transparent)',
      textUnderlineOffset: '3px',
    },
    '.cm-live-wikilink-widget': {
      color: 'var(--syn-link)',
      textDecoration: 'underline',
      textDecorationColor: 'color-mix(in srgb, var(--accent) 40%, transparent)',
      textUnderlineOffset: '3px',
      cursor: 'pointer',
    },
    '.cm-live-wikilink-widget:hover': {
      textDecorationColor: 'currentColor',
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
      border: '1px solid var(--line-subtle)',
    },
    '.cm-live-image-missing': {
      display: 'inline-flex',
      alignItems: 'center',
      padding: '2px 8px',
      borderRadius: 'var(--radius-control)',
      border: '1px dashed var(--line-control)',
      backgroundColor: 'var(--surface-subtle)',
      color: 'var(--text-muted)',
      fontSize: 'var(--fs-12)',
    },
    '.cm-live-bullet-widget': {
      display: 'inline-block',
      width: '1em',
      color: 'var(--text-muted)',
    },
    '.cm-live-ordered-mark': {
      color: 'var(--text-muted)',
      fontVariantNumeric: 'tabular-nums',
    },
    '.cm-live-task-widget': {
      display: 'inline-flex',
      alignItems: 'center',
      verticalAlign: '-2px',
      marginRight: '0.5em',
    },
    '.cm-live-task-checkbox': {
      width: '16px',
      height: '16px',
      margin: '0',
      cursor: 'pointer',
      accentColor: 'var(--accent)',
    },
    '.cm-line.cm-live-task-done-line': {
      color: 'var(--text-muted)',
      textDecoration: 'line-through',
      textDecorationColor: 'var(--line-strong)',
    },
    '.cm-line.cm-live-blockquote-line': {
      borderLeft: '2px solid var(--line-strong)',
      paddingLeft: 'var(--space-4)',
      color: 'var(--syn-quote)',
      fontStyle: 'italic',
    },
    '.cm-line.cm-live-codeblock-line': {
      fontFamily: 'var(--font-mono)',
      fontSize: 'var(--fs-13)',
      lineHeight: '1.6',
      backgroundColor: 'var(--surface-code)',
      paddingLeft: 'var(--space-4)',
      paddingRight: 'var(--space-4)',
    },
    '.cm-line.cm-live-codeblock-start': {
      borderTopLeftRadius: 'var(--radius-card)',
      borderTopRightRadius: 'var(--radius-card)',
      paddingTop: 'var(--space-2)',
    },
    '.cm-line.cm-live-codeblock-end': {
      borderBottomLeftRadius: 'var(--radius-card)',
      borderBottomRightRadius: 'var(--radius-card)',
      paddingBottom: 'var(--space-2)',
    },
    '.cm-live-codeblock-badge': {
      display: 'inline-block',
      fontFamily: 'var(--font-ui)',
      fontSize: 'var(--fs-11)',
      fontWeight: '500',
      color: 'var(--text-muted)',
      userSelect: 'none',
    },
    '.cm-line.cm-live-table-line': {
      fontFamily: 'var(--font-ui)',
      fontSize: 'var(--fs-15)',
      fontVariantNumeric: 'tabular-nums',
      borderBottom: '1px solid var(--line-subtle)',
      paddingTop: '4px',
      paddingBottom: '4px',
    },
    '.cm-line.cm-live-table-header-line': {
      fontSize: 'var(--fs-12)',
      fontWeight: '600',
      letterSpacing: '0.04em',
      textTransform: 'uppercase',
      color: 'var(--text-secondary)',
    },
    '.cm-line.cm-live-table-row-line': {
      borderBottom: '1px solid var(--line-subtle)',
    },
    '.cm-live-table-pipe': {
      display: 'inline-block',
      padding: '0 var(--space-3)',
      color: 'var(--line-control)',
      userSelect: 'none',
    },
    '.cm-live-table-edge-pipe': {
      display: 'inline-block',
      width: '2px',
    },
    '.cm-live-table-sep-widget': {
      display: 'none',
    },
    '.cm-live-hr-widget': {
      display: 'block',
      width: '100%',
      height: '1px',
      backgroundColor: 'var(--line-subtle)',
      margin: 'var(--space-3) 0',
    },
    '.cm-live-tag': {
      fontFamily: 'var(--font-ui)',
      fontSize: '0.78em',
      fontWeight: '500',
      color: 'var(--accent)',
      backgroundColor: 'var(--accent-soft)',
      padding: '0.15em 0.5em',
      borderRadius: '10px',
      whiteSpace: 'nowrap',
    },
    '.cm-line.cm-dimmed-paragraph-line': {
      opacity: '0.32',
    },
    '.cm-tooltip.cm-tooltip-autocomplete': {
      backgroundColor: 'var(--surface-raised)',
      border: '1px solid var(--line-control)',
      borderRadius: 'var(--radius-card)',
      boxShadow: 'var(--shadow-popover)',
      fontFamily: 'var(--font-ui)',
      fontSize: 'var(--fs-13)',
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
      backgroundColor: 'var(--accent-soft)',
      color: 'var(--accent)',
    },
    '.cm-completionDetail': {
      color: 'var(--text-muted)',
      fontSize: 'var(--fs-11)',
      fontStyle: 'normal',
      marginLeft: 'auto',
    },
  });

  function emitStats(state: EditorState) {
    if (!onStatsChange) return;
    if (statsDoc !== state.doc) {
      statsDoc = state.doc;
      const trimmed = state.doc.toString().trim();
      totalStats = { lines: state.doc.lines, chars: state.doc.length, words: trimmed ? trimmed.split(/\s+/).length : 0 };
    }

    const mainSel = state.selection.main;
    const selectedText = mainSel.empty ? '' : state.sliceDoc(mainSel.from, mainSel.to);
    const selectedChars = selectedText.length;
    const selectedLines = selectedText ? selectedText.split('\n').length : 0;

    onStatsChange({
      ...totalStats,
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
    const operation = captureEditorOperation(v, editorGeneration);
    if (!target.text) return false;

    try {
      const payload = await copyToClipboard({
        text: target.text,
        mode: copyMode,
      });
      if (source === 'cut') {
        if (!operation.isCurrent(v, editorGeneration)) {
          onError?.('Cut cancelled because the note changed. The copied text is on the clipboard.');
          return false;
        }
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
    const operation = captureEditorOperation(v, editorGeneration);
    try {
      const plainText = await pastePlainFromClipboard();
      if (!plainText) return true;
      if (!operation.isCurrent(v, editorGeneration)) {
        onError?.('Paste cancelled because the note changed. Paste again in the intended note.');
        return false;
      }
      const sel = operation;
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
    pngBytes: number[] | null,
    operation = captureEditorOperation(v, editorGeneration)
  ): Promise<boolean> {
    try {
      if (!operation.isCurrent(v, editorGeneration)) return false;
      const [fileName, relPath] = await savePastedImage(pngBytes);
      if (!operation.isCurrent(v, editorGeneration)) {
        onError?.('Image paste cancelled because the note changed. The image remains in attachments.');
        return false;
      }
      const insertSyntax =
        settingsStore.imageLinkFormat === 'markdown'
          ? `![${fileName}](${relPath.replace(/ /g, '%20')})`
          : `![[${fileName}]]`;
      const sel = operation;
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
    const operation = captureEditorOperation(v, editorGeneration);
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
                return insertSavedImageAtCursor(v, bytes, operation);
              })
              .catch(() => {
                void insertSavedImageAtCursor(v, null, operation);
              });
            return true;
          }
        }
      }
    }

    const plainText = event.clipboardData?.getData('text/plain') ?? '';
    if (!plainText) {
      event.preventDefault();
      void insertSavedImageAtCursor(v, null, operation);
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
      // Live mode hides frontmatter, so start below it.
      selection: { anchor: editingMode === 'live' ? positionAfterFrontmatter(content) : 0 },
      extensions: [
        history(),
        drawSelection(),
        highlightActiveLine(),
        EditorView.lineWrapping,
        placeholder('Write your Markdown note here...'),
        markdown({ base: markdownLanguage }),
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
        spellcheckCompartment.of(spellcheckExtension(settingsStore.spellCheck)),
        createCalcExtension(),
        editorTheme,
        keymap.of([
          { key: 'Tab', run: acceptInlineCalculation },
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
          if (update.docChanged) {
            clearTimeout(statsTimer);
            statsTimer = setTimeout(() => { if (view) emitStats(view.state); }, 120);
          } else if (update.selectionSet) {
            emitStats(update.state);
          }
          if (update.selectionSet || update.focusChanged || update.docChanged) {
            updateSelectionBar(update.view);
          }
          if (update.docChanged && !suppressChangeEmit) {
            onDocChange?.(docId, update.state.doc.toString());
          }
        }),
      ],
    });
  }

  $effect(() => {
    // Read docId before the view check: this effect first runs before onMount creates
    // the view, and a dependency read only after the check would never be tracked.
    const nextDocId = docId;
    if (!view || nextDocId === currentLoadedDocId) return;
    currentLoadedDocId = nextDocId;
    const nextDoc = untrack(() => initialDoc);
    suppressChangeEmit = true;
    editorGeneration++;
    view.setState(createEditorState(nextDoc));
    suppressChangeEmit = false;
    emitStats(view.state);
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

  $effect(() => {
    const enabled = settingsStore.spellCheck;
    view?.dispatch({
      effects: spellcheckCompartment.reconfigure(spellcheckExtension(enabled)),
    });
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

  /** Applies an edit made outside the editor, such as from the properties form, as one undo step. */
  export function applyChange(change: TextChange) {
    if (!view) return;
    view.dispatch({ changes: change, userEvent: 'input.properties' });
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
    editorGeneration++;
    clearTimeout(statsTimer);
    view?.destroy();
  });
</script>

<div
  class="editor-scroll"
  data-editor-scroll
  bind:this={scrollHost}
  onscroll={() => (selectionBar = null)}
>
  {#if header}
    <div class="note-header">{@render header()}</div>
  {/if}
  <div class="editor-surface" bind:this={editorHost} aria-label="Markdown note editor"></div>
</div>

{#if selectionBar}
  <div
    class="selection-bar"
    role="toolbar"
    aria-label="Format selection"
    style:left="{selectionBar.left}px"
    style:top="{selectionBar.top}px"
  >
    {#each SELECTION_ACTIONS as action, i (action.cmd)}
      {#if i === 5}<span class="sep" aria-hidden="true"></span>{/if}
      <button
        type="button"
        title={action.label}
        aria-label={action.label}
        onmousedown={(e) => {
          e.preventDefault();
          runSelectionAction(action.cmd);
        }}
      >
        <Icon name={action.icon} />
      </button>
    {/each}
  </div>
{/if}

<style>
  .editor-scroll {
    height: 100%;
    overflow-y: auto;
  }

  .note-header {
    max-width: var(--editor-max-width);
    margin: 0 auto;
    padding: var(--space-12) var(--space-6) var(--space-6);
    box-sizing: content-box;
  }

  .editor-surface {
    width: 100%;
  }

  .selection-bar {
    position: fixed;
    z-index: 60;
    display: flex;
    gap: 1px;
    padding: 3px;
    transform: translate(-50%, calc(-100% - 8px));
    border: 1px solid var(--line-control);
    border-radius: var(--radius-card);
    background: var(--surface-raised);
    box-shadow: var(--shadow-popover);
  }

  .selection-bar button {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .selection-bar button:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .sep {
    width: 1px;
    margin: 4px 2px;
    background: var(--line-subtle);
  }
</style>
