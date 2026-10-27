<script lang="ts">
  /**
   * One editor pane: a note's text with its header above it. Split view renders two of these,
   * each with its own CodeMirror view, scroll position, and cursor.
   */
  import Editor from './Editor.svelte';
  import NoteHeader from './NoteHeader.svelte';
  import { clipboardStore } from './stores/clipboard.svelte';
  import { settingsStore } from './stores/settings.svelte';
  import { tabsStore, type OpenTab } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import { m } from './i18n.svelte';
  import type { PaneId } from './types';

  interface Props {
    pane: PaneId;
    /** The note's path, or `__scratchpad__` for the sample note. */
    docId: string;
    initialDoc: string;
    /** The open tab this pane shows, or `null` for the scratchpad. */
    tab: OpenTab | null;
    /** Hands the editor to the app, which routes commands to the pane the user works in. */
    onEditor: (editor: Editor | undefined) => void;
    /** Opens a wiki link that names another note. A link to a heading stays in this pane. */
    onOpenNote: (noteTarget: string, heading?: string) => void;
    /** Runs after an edit to the scratchpad, which feeds the clipboard inspector. */
    onScratchpadEdit?: () => void;
  }

  let { pane, docId, initialDoc, tab, onEditor, onOpenNote, onScratchpadEdit }: Props = $props();

  let editor: Editor | undefined = $state();
  let stats = $state({ lines: 0, words: 0, chars: 0, selectedChars: 0, selectedLines: 0, cursorLine: 1 });

  $effect(() => {
    onEditor(editor);
    return () => onEditor(undefined);
  });

  $effect(() => {
    if (settingsStore.effectivePane === pane) tabsStore.docStats = stats;
  });
</script>

<Editor
  bind:this={editor}
  {docId}
  {initialDoc}
  autoFocus={pane === 'primary'}
  copyMode={settingsStore.copyMode}
  editingMode={settingsStore.editingMode}
  dimInactiveParagraphs={settingsStore.dimInactiveParagraphs}
  typewriterScrolling={settingsStore.typewriterScrolling}
  vaultFiles={vaultStore.allFiles}
  onWikiLinkClick={(target, heading) => {
    if (!target.trim() && heading) editor?.scrollToHeading(heading);
    else onOpenNote(target, heading);
  }}
  onImagePasted={(_fileName, relPath) =>
    clipboardStore.setStatus(m.status_saved_pasted_image({ path: relPath }))}
  onDocChange={(changedId, content) =>
    tabsStore.handleEditorDocChange(changedId, content, onScratchpadEdit)}
  onCopyComplete={(payload, source) => clipboardStore.handleCopyComplete(payload, source)}
  onPastePlainComplete={(chars, lines) => clipboardStore.handlePastePlainComplete(chars, lines)}
  onStatsChange={(next) => {
    stats = next;
    // The status bar and the outline follow the pane the user is working in.
    if (settingsStore.effectivePane === pane) tabsStore.docStats = next;
  }}
  onError={(err) => clipboardStore.setStatus(m.status_clipboard_error({ error: String(err) }))}
>
  {#snippet header()}
    <NoteHeader
      {tab}
      words={stats.words}
      onEdit={(change) => editor?.applyChange(change)}
    />
  {/snippet}
</Editor>
