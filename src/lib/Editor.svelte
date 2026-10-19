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
  import { copyToClipboard, pastePlainFromClipboard } from './ipc';
  import type { ClipboardPayload, CopyMode } from './types';

  interface Props {
    initialDoc: string;
    copyMode: CopyMode;
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
    initialDoc,
    copyMode,
    onCopyComplete,
    onPastePlainComplete,
    onStatsChange,
    onError,
  }: Props = $props();

  let editorHost: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;

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
      fontSize: 'var(--text-base)',
      fontFamily: 'var(--font-ui)',
      color: 'var(--text-primary)',
      backgroundColor: 'transparent',
    },
    '.cm-scroller': {
      overflow: 'auto',
      padding: 'var(--space-6) var(--space-4) var(--space-8) var(--space-4)',
      fontFamily: 'var(--font-ui)',
      lineHeight: '1.65',
    },
    '.cm-content': {
      maxWidth: '700px',
      margin: '0 auto',
      padding: '0 0 var(--space-8) 0',
      caretColor: 'var(--accent-color, var(--focus-ring))',
    },
    '.cm-line': {
      padding: '0 var(--space-1)',
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

  function getSelectedOrCurrentLineText(v: EditorView, fallbackToWholeDoc = false): {
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

    const state = EditorState.create({
      doc: initialDoc,
      extensions: [
        history(),
        drawSelection(),
        highlightActiveLine(),
        EditorView.lineWrapping,
        placeholder('Write your Markdown note here...'),
        markdown(),
        syntaxHighlighting(markdownHighlight),
        editorTheme,
        keymap.of([
          {
            key: 'Mod-Shift-v',
            run: (v) => {
              void performPasteAsPlainText(v);
              return true;
            },
          },
          ...defaultKeymap,
          ...historyKeymap,
        ]),
        EditorView.domEventHandlers({
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
        }),
      ],
    });

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
