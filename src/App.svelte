<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import BacklinksPanel from './lib/BacklinksPanel.svelte';
  import ClipboardInspector from './lib/ClipboardInspector.svelte';
  import CommandBar from './lib/CommandBar.svelte';
  import CommandPalette from './lib/CommandPalette.svelte';
  import Editor from './lib/Editor.svelte';
  import FirstRunScreen from './lib/FirstRunScreen.svelte';
  import FormattingToolbar from './lib/FormattingToolbar.svelte';
  import QuickSwitcher from './lib/QuickSwitcher.svelte';
  import SettingsModal from './lib/SettingsModal.svelte';
  import Sidebar from './lib/Sidebar.svelte';
  import StatusBar from './lib/StatusBar.svelte';
  import TabBar from './lib/TabBar.svelte';
  import { fluentIcons } from './lib/icons';
  import {
    exportActiveNoteHtml,
    listenVaultFsEvents,
    renderNoteHtmlPreview,
  } from './lib/ipc';
  import type { MarkdownFormatCommandId } from './lib/markdownCommands';
  import { clipboardStore } from './lib/stores/clipboard.svelte';
  import { settingsStore } from './lib/stores/settings.svelte';
  import { tabsStore } from './lib/stores/tabs.svelte';
  import { vaultStore } from './lib/stores/vault.svelte';

  let editorRef: Editor | undefined = $state();
  let unlistenFsEvents: UnlistenFn | null = null;
  let missingWikiLinkPrompt = $state<{
    noteTarget: string;
    heading?: string;
  } | null>(null);
  let printArticleHtml = $state('');

  $effect(() => {
    const target = tabsStore.pendingScrollTarget;
    const activeRel = tabsStore.activeTabPath;
    if (
      target &&
      activeRel &&
      target.relPath.toLowerCase() === activeRel.toLowerCase() &&
      editorRef
    ) {
      requestAnimationFrame(() => {
        if (target.heading) {
          editorRef?.scrollToHeading(target.heading);
        } else if (target.lineNumber) {
          editorRef?.scrollToLine(target.lineNumber);
        }
        tabsStore.clearPendingScrollTarget();
      });
    }
  });

  function getActiveNoteTitleAndMarkdown(): { title: string; markdown: string } {
    if (tabsStore.activeTab) {
      const name = tabsStore.activeTab.title.replace(/\.md$/i, '');
      return {
        title: name || 'Untitled',
        markdown: tabsStore.activeTab.content,
      };
    }
    return {
      title: 'Scratchpad',
      markdown: tabsStore.scratchpadContent,
    };
  }

  async function handleExportHtml() {
    const { title, markdown } = getActiveNoteTitleAndMarkdown();
    try {
      const savedPath = await exportActiveNoteHtml(
        title,
        markdown,
        settingsStore.effectiveDark
      );
      if (savedPath) {
        clipboardStore.setStatus(`Exported HTML to ${savedPath}`);
      }
    } catch (e) {
      clipboardStore.setStatus(`HTML export failed: ${String(e)}`);
    }
  }

  async function handleExportPdf() {
    const { title, markdown } = getActiveNoteTitleAndMarkdown();
    try {
      const fullHtml = await renderNoteHtmlPreview(
        title,
        markdown,
        settingsStore.effectiveDark
      );
      const match = fullHtml.match(/<article[^>]*>([\s\S]*?)<\/article>/i);
      printArticleHtml = match ? match[0] : fullHtml;
      await tick();
      clipboardStore.setStatus('Opening print / Save as PDF dialog...');
      window.print();
    } catch (e) {
      clipboardStore.setStatus(`Print to PDF failed: ${String(e)}`);
    }
  }

  async function refreshInspectorPreview() {
    const sample =
      editorRef?.getCurrentCopyTargetText() ??
      tabsStore.activeTab?.content ??
      tabsStore.scratchpadContent;
    await clipboardStore.refreshPreview(sample, settingsStore.copyMode);
  }

  function handleFormatCommand(cmd: MarkdownFormatCommandId) {
    editorRef?.executeMarkdownCommand(cmd);
  }

  function handleWikiLinkClick(noteTarget: string, heading?: string) {
    const cleanNote = noteTarget.trim();
    if (!cleanNote && heading) {
      editorRef?.scrollToHeading(heading);
      return;
    }

    if (!vaultStore.vaultSnapshot) {
      if (heading) {
        editorRef?.scrollToHeading(heading);
      } else {
        clipboardStore.setStatus(
          `Open a vault to navigate to [[${cleanNote}]]`
        );
      }
      return;
    }

    const found = vaultStore.resolveWikiTarget(cleanNote);
    if (found) {
      void tabsStore.openNoteInTab(found.relPath, { heading });
      return;
    }

    missingWikiLinkPrompt = {
      noteTarget: cleanNote,
      heading,
    };
  }

  async function confirmCreateMissingWikiNote() {
    const prompt = missingWikiLinkPrompt;
    missingWikiLinkPrompt = null;
    if (!prompt) return;

    const normalized = prompt.noteTarget.replace(/\\/g, '/');
    const slashIdx = normalized.lastIndexOf('/');
    const parentFolder = slashIdx >= 0 ? normalized.slice(0, slashIdx) : '';
    const noteName = slashIdx >= 0 ? normalized.slice(slashIdx + 1) : normalized;
    const createdRel = await vaultStore.createNoteInVault(parentFolder, noteName);
    if (createdRel && prompt.heading) {
      await tabsStore.openNoteInTab(createdRel, { heading: prompt.heading });
    }
  }

  function handleGlobalMouseMove(e: MouseEvent) {
    if (settingsStore.focusMode === 'off') return;
    if (e.clientY <= 12) {
      settingsStore.topHoverReveal = true;
    } else if (e.clientY > 64 && !settingsStore.showSettings) {
      settingsStore.topHoverReveal = false;
    }
  }

  function handleGlobalKeyDown(e: KeyboardEvent) {
    const isMod = e.ctrlKey || e.metaKey;
    const keyLower = e.key.toLowerCase();

    // Settings screen (Ctrl+,)
    if (isMod && !e.shiftKey && e.key === ',') {
      e.preventDefault();
      settingsStore.showCommandPalette = false;
      settingsStore.showQuickSwitcher = false;
      settingsStore.showSettings = !settingsStore.showSettings;
      return;
    }

    // Command Palette (Ctrl+P) — works everywhere, including Zen mode
    if (isMod && !e.shiftKey && keyLower === 'p') {
      e.preventDefault();
      settingsStore.showQuickSwitcher = false;
      settingsStore.showCommandPalette = !settingsStore.showCommandPalette;
      return;
    }

    // Print / Export to PDF (Ctrl+Shift+P)
    if (isMod && e.shiftKey && keyLower === 'p') {
      e.preventDefault();
      void handleExportPdf();
      return;
    }

    // Quick Switcher (Ctrl+O)
    if (isMod && !e.shiftKey && keyLower === 'o') {
      if (vaultStore.vaultSnapshot) {
        e.preventDefault();
        settingsStore.showCommandPalette = false;
        settingsStore.showQuickSwitcher = !settingsStore.showQuickSwitcher;
      }
      return;
    }

    // Full-Text Vault Search (Ctrl+Shift+F)
    if (isMod && e.shiftKey && keyLower === 'f') {
      if (vaultStore.vaultSnapshot) {
        e.preventDefault();
        settingsStore.setLeftSidebarView('search');
      }
      return;
    }

    // Minimal Focus Mode (Ctrl+Shift+M)
    if (isMod && e.shiftKey && keyLower === 'm') {
      e.preventDefault();
      settingsStore.toggleMinimalFocus();
      return;
    }

    // Zen Fullscreen Focus Mode (F11)
    if (e.key === 'F11') {
      e.preventDefault();
      settingsStore.toggleZenFocus();
      return;
    }

    // Toggle Live / Source Mode (Ctrl+/)
    if (isMod && e.key === '/') {
      e.preventDefault();
      settingsStore.toggleEditingMode();
      return;
    }

    // Escape exits modals or Focus Mode (Minimal or Zen) to the previous layout
    if (e.key === 'Escape') {
      if (missingWikiLinkPrompt) {
        missingWikiLinkPrompt = null;
        editorRef?.focusEditor();
        return;
      }
      if (settingsStore.showQuickSwitcher) {
        settingsStore.showQuickSwitcher = false;
        editorRef?.focusEditor();
        return;
      }
      if (settingsStore.showCommandPalette) {
        settingsStore.showCommandPalette = false;
        editorRef?.focusEditor();
        return;
      }
      if (settingsStore.showSettings) {
        settingsStore.showSettings = false;
        editorRef?.focusEditor();
        return;
      }
      if (settingsStore.focusMode !== 'off') {
        e.preventDefault();
        settingsStore.exitFocusMode();
        editorRef?.focusEditor();
        return;
      }
    }

    if (isMod && keyLower === 's' && !e.shiftKey) {
      e.preventDefault();
      if (tabsStore.activeTabPath) {
        void tabsStore.flushAutosaveFor(tabsStore.activeTabPath);
      }
    } else if (isMod && keyLower === 'w') {
      if (tabsStore.activeTabPath) {
        e.preventDefault();
        void tabsStore.closeTab(tabsStore.activeTabPath);
      }
    } else if (isMod && keyLower === 'n') {
      if (vaultStore.vaultSnapshot) {
        e.preventDefault();
        const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
        void vaultStore.createNoteInVault('', `Note ${stamp}`);
      }
    } else if (isMod && e.key === '\\') {
      if (vaultStore.vaultSnapshot) {
        e.preventDefault();
        if (settingsStore.focusMode !== 'off') {
          settingsStore.exitFocusMode();
        } else {
          settingsStore.toggleSidebar();
        }
      }
    }
  }

  onMount(() => {
    const cleanupSettings = settingsStore.initFromStorageAndSystem();

    void vaultStore.refreshRecentVaults().then(() => {
      try {
        const lastVault = localStorage.getItem('jotter.lastVaultPath');
        if (lastVault) {
          void vaultStore.openVaultByPath(lastVault);
        }
      } catch {
        // ignore
      }
    });

    void listenVaultFsEvents((evt) => {
      void vaultStore.handleExternalFsEvent(evt.changedPaths, (newContent) => {
        editorRef?.replaceContentFromDisk(newContent);
      });
    })
      .then((unlisten) => {
        unlistenFsEvents = unlisten;
      })
      .catch(() => {
        // ignore in browser preview
      });

    return () => {
      cleanupSettings();
    };
  });

  onDestroy(() => {
    unlistenFsEvents?.();
    tabsStore.clearAllTimers();
  });
</script>

<svelte:window
  onkeydown={handleGlobalKeyDown}
  onmousemove={handleGlobalMouseMove}
/>

<div
  class="app-shell"
  class:solid-backdrop={settingsStore.effectiveSolidBackdrop}
  class:focus-minimal={settingsStore.focusMode === 'minimal'}
  class:focus-zen={settingsStore.focusMode === 'zen'}
  data-backdrop={settingsStore.effectiveSolidBackdrop ? 'solid' : 'mica'}
  data-focus-mode={settingsStore.focusMode}
>
  <CommandBar
    onCopyTrigger={() => editorRef?.triggerCopyFromToolbar()}
    onPastePlainTrigger={() => editorRef?.triggerPastePlainFromToolbar()}
    onRefreshInspector={() => void refreshInspectorPreview()}
    onExportHtml={() => void handleExportHtml()}
    onExportPdf={() => void handleExportPdf()}
  />

  {#if !vaultStore.vaultSnapshot && !tabsStore.scratchpadMode}
    <FirstRunScreen
      recentVaults={vaultStore.recentVaults}
      errorMessage={vaultStore.onboardingError}
      onOpenVaultPath={(path) => void vaultStore.openVaultByPath(path)}
      onCreateVault={(parent, name) => void vaultStore.createVault(parent, name)}
      onRemoveRecentVault={(path) => void vaultStore.removeRecent(path)}
      onOpenScratchpad={() => (tabsStore.scratchpadMode = true)}
    />
  {:else}
    <main class="workspace">
      <div class="workspace-split">
        {#if vaultStore.vaultSnapshot && settingsStore.focusMode === 'off'}
          <Sidebar
            snapshot={vaultStore.vaultSnapshot}
            recentVaults={vaultStore.recentVaults}
            onOpenVault={(path) => void vaultStore.openVaultByPath(path)}
            activeRelPath={tabsStore.activeTabPath}
            width={settingsStore.sidebarWidth}
            collapsed={settingsStore.sidebarCollapsed}
            onResize={(w) => settingsStore.setSidebarWidth(w)}
            onSelectFile={(relPath) => void tabsStore.openNoteInTab(relPath)}
            onCreateNote={(parent, name) =>
              void vaultStore.createNoteInVault(parent, name)}
            onCreateFolder={(parent, name) =>
              void vaultStore.createFolderInVault(parent, name)}
            onRenameEntry={(oldRel, newRel) =>
              void vaultStore.renameEntryInVault(oldRel, newRel)}
            onDeleteEntry={(rel) => void vaultStore.deleteEntryInVault(rel)}
            onSwitchVault={() => void vaultStore.switchVault()}
          />
        {/if}

        <section class="editor-column" aria-label="Editor workspace">
          {#if vaultStore.vaultSnapshot && settingsStore.focusMode === 'off'}
            <TabBar />
          {/if}

          {#if missingWikiLinkPrompt}
            <div class="external-reload-banner" role="alertdialog" aria-label="Create missing wiki link note">
              <span>
                Note <strong>[[{missingWikiLinkPrompt.noteTarget}]]</strong> does not exist in this vault yet. Would you like to create it?
              </span>
              <div class="external-reload-actions">
                <button
                  type="button"
                  class="fluent-btn primary-btn"
                  onclick={() => void confirmCreateMissingWikiNote()}
                >
                  Create & Open
                </button>
                <button
                  type="button"
                  class="fluent-btn"
                  onclick={() => (missingWikiLinkPrompt = null)}
                >
                  Cancel
                </button>
              </div>
            </div>
          {/if}

          {#if tabsStore.activeTab?.externalChangePending}
            <div class="external-reload-banner" role="alert">
              <span>
                <strong>{tabsStore.activeTab.relPath}</strong> was modified outside Jotter while you have unsaved edits.
              </span>
              <div class="external-reload-actions">
                <button
                  type="button"
                  class="fluent-btn primary-btn"
                  onclick={() =>
                    void tabsStore.reloadTabFromDisk(
                      tabsStore.activeTab!.relPath,
                      (newContent) => editorRef?.replaceContentFromDisk(newContent)
                    )}
                >
                  Reload from disk
                </button>
                <button
                  type="button"
                  class="fluent-btn"
                  onclick={() =>
                    tabsStore.dismissExternalChangeBanner(
                      tabsStore.activeTab!.relPath
                    )}
                >
                  Keep my edits
                </button>
              </div>
            </div>
          {/if}

          <div class="editor-card">
            {#if (tabsStore.scratchpadMode || tabsStore.activeTab) && settingsStore.focusMode === 'off'}
              <FormattingToolbar onFormat={handleFormatCommand} />
            {/if}

            {#if tabsStore.scratchpadMode}
              <Editor
                bind:this={editorRef}
                docId="__scratchpad__"
                initialDoc={tabsStore.scratchpadContent}
                copyMode={settingsStore.copyMode}
                editingMode={settingsStore.editingMode}
                dimInactiveParagraphs={settingsStore.dimInactiveParagraphs}
                typewriterScrolling={settingsStore.typewriterScrolling}
                vaultFiles={vaultStore.allFiles}
                onWikiLinkClick={handleWikiLinkClick}
                onImagePasted={(_fileName, relPath) =>
                  clipboardStore.setStatus(`Saved pasted image to ${relPath}`)}
                onDocChange={(docId, content) =>
                  tabsStore.handleEditorDocChange(docId, content, () => {
                    if (settingsStore.showInspector) void refreshInspectorPreview();
                  })}
                onCopyComplete={(payload, source) =>
                  clipboardStore.handleCopyComplete(payload, source)}
                onPastePlainComplete={(chars, lines) =>
                  clipboardStore.handlePastePlainComplete(chars, lines)}
                onStatsChange={(s) => (tabsStore.docStats = s)}
                onError={(err) => clipboardStore.setStatus(`Clipboard error: ${err}`)}
              />
            {:else if tabsStore.activeTab}
              <Editor
                bind:this={editorRef}
                docId={tabsStore.activeTab.relPath}
                initialDoc={tabsStore.activeTab.content}
                copyMode={settingsStore.copyMode}
                editingMode={settingsStore.editingMode}
                dimInactiveParagraphs={settingsStore.dimInactiveParagraphs}
                typewriterScrolling={settingsStore.typewriterScrolling}
                vaultFiles={vaultStore.allFiles}
                onWikiLinkClick={handleWikiLinkClick}
                onImagePasted={(_fileName, relPath) =>
                  clipboardStore.setStatus(`Saved pasted image to ${relPath}`)}
                onDocChange={(docId, content) =>
                  tabsStore.handleEditorDocChange(docId, content)}
                onCopyComplete={(payload, source) =>
                  clipboardStore.handleCopyComplete(payload, source)}
                onPastePlainComplete={(chars, lines) =>
                  clipboardStore.handlePastePlainComplete(chars, lines)}
                onStatsChange={(s) => (tabsStore.docStats = s)}
                onError={(err) => clipboardStore.setStatus(`Clipboard error: ${err}`)}
              />
            {:else}
              <div class="no-open-note">
                <div class="no-note-icon">{@html fluentIcons.note20}</div>
                <div class="no-note-title">No note selected</div>
                <div class="no-note-desc">
                  Select a note from the file tree on the left or create a new one.
                </div>
                <button
                  type="button"
                  class="fluent-btn primary-btn"
                  onclick={() => {
                    const stamp = new Date()
                      .toISOString()
                      .slice(0, 19)
                      .replace(/[:T]/g, '-');
                    void vaultStore.createNoteInVault('', `Note ${stamp}`);
                  }}
                >
                  {@html fluentIcons.documentAdd16}
                  <span>New Note (Ctrl+N)</span>
                </button>
              </div>
            {/if}
          </div>
        </section>

        {#if vaultStore.vaultSnapshot && settingsStore.focusMode === 'off'}
          <BacklinksPanel />
        {/if}
      </div>

      {#if settingsStore.focusMode === 'off'}
        <ClipboardInspector />
      {/if}
    </main>
  {/if}

  <StatusBar />

  {#if settingsStore.showQuickSwitcher}
    <QuickSwitcher
      onClose={() => {
        settingsStore.showQuickSwitcher = false;
        editorRef?.focusEditor();
      }}
    />
  {/if}

  {#if settingsStore.showCommandPalette}
    <CommandPalette
      onFormat={handleFormatCommand}
      onCopyTrigger={() => editorRef?.triggerCopyFromToolbar()}
      onPastePlainTrigger={() => editorRef?.triggerPastePlainFromToolbar()}
      onRefreshInspector={() => void refreshInspectorPreview()}
      onExportHtml={() => void handleExportHtml()}
      onExportPdf={() => void handleExportPdf()}
      onClose={() => {
        settingsStore.showCommandPalette = false;
        editorRef?.focusEditor();
      }}
    />
  {/if}

  {#if settingsStore.showSettings}
    <SettingsModal
      onRefreshInspector={() => void refreshInspectorPreview()}
      onClose={() => {
        settingsStore.showSettings = false;
        editorRef?.focusEditor();
      }}
    />
  {/if}
</div>

<div class="jotter-print-view" aria-hidden="true">
  {@html printArticleHtml}
</div>

<style>
  .app-shell {
    display: flex;
    flex-direction: column;
    width: 100vw;
    height: 100vh;
    background-color: var(--bg-mica-tint);
    transition: background-color var(--transition-fast);
  }

  .app-shell.solid-backdrop {
    background-color: var(--bg-solid-fallback);
  }

  .workspace {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    min-height: 0;
    overflow: hidden;
  }

  .app-shell.focus-minimal .workspace,
  .app-shell.focus-zen .workspace {
    padding: 0;
    gap: 0;
  }

  .workspace-split {
    flex: 1;
    display: flex;
    gap: var(--space-3);
    min-height: 0;
    overflow: hidden;
  }

  .editor-column {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .external-reload-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    background: var(--bg-card-solid);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-card);
    font-size: var(--text-xs);
  }

  .external-reload-actions {
    display: flex;
    gap: var(--space-2);
  }

  .editor-card {
    flex: 1;
    min-height: 0;
    background: var(--bg-card);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-card);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    transition: background-color var(--transition-fast);
  }

  .app-shell.focus-minimal .editor-card,
  .app-shell.focus-zen .editor-card {
    border: none;
    border-radius: 0;
  }

  .solid-backdrop .editor-card {
    background: var(--bg-card-solid);
  }

  .no-open-note {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    padding: var(--space-6);
    text-align: center;
    color: var(--text-secondary);
  }

  .no-note-icon {
    color: var(--accent-color, var(--accent-fallback));
  }

  .no-note-title {
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--text-primary);
  }

  .no-note-desc {
    font-size: var(--text-xs);
    max-width: 320px;
  }

  .fluent-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    min-height: var(--control-min-height);
    min-width: var(--control-min-height);
    padding: 0 var(--space-3);
    border-radius: var(--radius-control);
    border: 1px solid var(--border-control);
    background: var(--bg-control);
    color: var(--text-primary);
    font-size: var(--text-xs);
    font-weight: 500;
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      border-color var(--transition-fast);
  }

  .fluent-btn:hover {
    background: var(--bg-control-hover);
  }

  .fluent-btn.primary-btn {
    background: var(--accent-color, var(--accent-fallback));
    color: var(--accent-text, #ffffff);
    border-color: transparent;
  }

  .fluent-btn.primary-btn:hover {
    filter: brightness(1.06);
  }
</style>
