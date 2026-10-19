<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import BacklinksPanel from './lib/BacklinksPanel.svelte';
  import ClipboardInspector from './lib/ClipboardInspector.svelte';
  import CommandPalette from './lib/CommandPalette.svelte';
  import Editor from './lib/Editor.svelte';
  import FirstRunScreen from './lib/FirstRunScreen.svelte';
  import QuickSwitcher from './lib/QuickSwitcher.svelte';
  import SettingsModal from './lib/SettingsModal.svelte';
  import NoteHeader from './lib/NoteHeader.svelte';
  import Sidebar from './lib/Sidebar.svelte';
  import StatusBar from './lib/StatusBar.svelte';
  import TitleBar from './lib/TitleBar.svelte';
  import Button from './lib/ui/Button.svelte';
  import {
    exportActiveNoteHtml,
    listenQuitRequested,
    listenVaultFsEvents,
    quitApp,
    renderNoteHtmlPreview,
  } from './lib/ipc';
  import { enhanceRenderedHtml } from './lib/richBlocks';
  import type { MarkdownFormatCommandId } from './lib/markdownCommands';
  import { clipboardStore } from './lib/stores/clipboard.svelte';
  import { settingsStore } from './lib/stores/settings.svelte';
  import { systemStore } from './lib/stores/system.svelte';
  import { tabsStore } from './lib/stores/tabs.svelte';
  import { vaultStore } from './lib/stores/vault.svelte';

  let editorRef: Editor | undefined = $state();
  let unlistenFsEvents: UnlistenFn | null = null;
  let unlistenQuit: UnlistenFn | null = null;
  let missingWikiLinkPrompt = $state<{
    noteTarget: string;
    heading?: string;
  } | null>(null);
  let printArticleHtml = $state('');

  let sidebarVisible = $derived(
    Boolean(vaultStore.vaultSnapshot) &&
      settingsStore.focusMode === 'off' &&
      !settingsStore.sidebarCollapsed
  );

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
      const dark = settingsStore.effectiveDark;
      const rendered = await renderNoteHtmlPreview(title, markdown, dark);
      const html = await enhanceRenderedHtml(rendered, 'file', dark);
      const savedPath = await exportActiveNoteHtml(title, html);
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
      const rendered = await renderNoteHtmlPreview(
        title,
        markdown,
        settingsStore.effectiveDark
      );
      const fullHtml = await enhanceRenderedHtml(
        rendered,
        'print',
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

    // Today's daily note (Ctrl+Shift+D)
    if (isMod && e.shiftKey && keyLower === 'd') {
      if (vaultStore.vaultSnapshot) {
        e.preventDefault();
        void vaultStore.openDailyNote();
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

  // Re-apply snippets when the user toggles one in Settings.
  $effect(() => {
    void settingsStore.enabledSnippets;
    systemStore.applySnippets();
  });

  $effect(() => {
    void settingsStore.summonEnabled;
    void settingsStore.summonShortcut;
    void systemStore.syncSummon();
  });

  onMount(() => {
    const cleanupSettings = settingsStore.initFromStorageAndSystem();
    void systemStore.reloadSnippets();

    // Tray "Quit" asks the page first so pending edits reach disk.
    void listenQuitRequested(async () => {
      try {
        await tabsStore.flushAllAutosaves();
      } finally {
        await quitApp();
      }
    })
      .then((unlisten) => {
        unlistenQuit = unlisten;
      })
      .catch(() => {
        // ignore in browser preview
      });

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
    unlistenQuit?.();
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

  <div class="main-column">
    <TitleBar {sidebarVisible} />

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
      {#if missingWikiLinkPrompt}
        <div class="external-reload-banner" role="alertdialog" aria-label="Create missing wiki link note">
          <span>
            Note <strong>[[{missingWikiLinkPrompt.noteTarget}]]</strong> does not exist in this vault yet. Would you like to create it?
          </span>
          <div class="external-reload-actions">
            <Button
              variant="primary"
              onclick={() => void confirmCreateMissingWikiNote()}
            >
              Create & Open
            </Button>
            <Button
              onclick={() => (missingWikiLinkPrompt = null)}
            >
              Cancel
            </Button>
          </div>
        </div>
      {/if}

      {#if tabsStore.activeTab?.externalChangePending}
        <div class="external-reload-banner" role="alert">
          <span>
            <strong>{tabsStore.activeTab.relPath}</strong> was modified outside Jotter while you have unsaved edits.
          </span>
          <div class="external-reload-actions">
            <Button
              variant="primary"
              onclick={() =>
                void tabsStore.reloadTabFromDisk(
                  tabsStore.activeTab!.relPath,
                  (newContent) => editorRef?.replaceContentFromDisk(newContent)
                )}
            >
              Reload from disk
            </Button>
            <Button
              onclick={() =>
                tabsStore.dismissExternalChangeBanner(
                  tabsStore.activeTab!.relPath
                )}
            >
              Keep my edits
            </Button>
          </div>
        </div>
      {/if}

      <div class="work-row">
        <section class="editor-column" aria-label="Editor">
          <div class="editor-card">

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
              >
                {#snippet header()}<NoteHeader />{/snippet}
              </Editor>
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
              >
                {#snippet header()}<NoteHeader />{/snippet}
              </Editor>
            {:else}
              <div class="no-open-note">
                <div class="no-note-title">Nothing open</div>
                <div class="no-note-desc">
                  Pick a note in the sidebar, press <kbd>Ctrl O</kbd> to find one by name, or start a new one.
                </div>
                <Button
                  variant="primary"
                  icon="file-plus"
                  onclick={() => {
                    const stamp = new Date()
                      .toISOString()
                      .slice(0, 19)
                      .replace(/[:T]/g, '-');
                    void vaultStore.createNoteInVault('', `Note ${stamp}`);
                  }}
                >
                  New note
                </Button>
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
    {/if}

    <StatusBar />
  </div>

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
  /* One flat window: sidebar | (title bar, editor, status bar). Hairlines split regions. */
  .app-shell {
    display: flex;
    width: 100vw;
    height: 100vh;
    background-color: transparent;
  }

  .app-shell.solid-backdrop {
    background-color: var(--surface-app);
  }

  .main-column {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface-editor);
  }

  .work-row {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .editor-column {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .external-reload-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--line-subtle);
    background: var(--accent-soft);
    font-size: var(--fs-13);
  }

  .external-reload-actions {
    display: flex;
    gap: var(--space-2);
  }

  .editor-card {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
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

  .no-note-title {
    font: 600 var(--fs-24) / var(--leading-tight) var(--font-display);
    color: var(--text-primary);
  }

  .no-note-desc {
    font-size: var(--fs-13);
    max-width: 320px;
  }
</style>
