<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import ClipboardInspector from './lib/ClipboardInspector.svelte';
  import CommandBar from './lib/CommandBar.svelte';
  import Editor from './lib/Editor.svelte';
  import FirstRunScreen from './lib/FirstRunScreen.svelte';
  import Sidebar from './lib/Sidebar.svelte';
  import StatusBar from './lib/StatusBar.svelte';
  import TabBar from './lib/TabBar.svelte';
  import { fluentIcons } from './lib/icons';
  import { listenVaultFsEvents } from './lib/ipc';
  import { clipboardStore } from './lib/stores/clipboard.svelte';
  import { settingsStore } from './lib/stores/settings.svelte';
  import { tabsStore } from './lib/stores/tabs.svelte';
  import { vaultStore } from './lib/stores/vault.svelte';

  let editorRef: Editor | undefined = $state();
  let unlistenFsEvents: UnlistenFn | null = null;

  async function refreshInspectorPreview() {
    const sample =
      editorRef?.getCurrentCopyTargetText() ??
      tabsStore.activeTab?.content ??
      tabsStore.scratchpadContent;
    await clipboardStore.refreshPreview(sample, settingsStore.copyMode);
  }

  function handleGlobalKeyDown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      if (tabsStore.activeTabPath) {
        void tabsStore.flushAutosaveFor(tabsStore.activeTabPath);
      }
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'w') {
      if (tabsStore.activeTabPath) {
        e.preventDefault();
        void tabsStore.closeTab(tabsStore.activeTabPath);
      }
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n') {
      if (vaultStore.vaultSnapshot) {
        e.preventDefault();
        const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
        void vaultStore.createNoteInVault('', `Note ${stamp}`);
      }
    } else if ((e.ctrlKey || e.metaKey) && e.key === '\\') {
      if (vaultStore.vaultSnapshot) {
        e.preventDefault();
        settingsStore.toggleSidebar();
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

<svelte:window onkeydown={handleGlobalKeyDown} />

<div
  class="app-shell"
  class:solid-backdrop={settingsStore.effectiveSolidBackdrop}
  data-backdrop={settingsStore.effectiveSolidBackdrop ? 'solid' : 'mica'}
>
  <CommandBar
    onCopyTrigger={() => editorRef?.triggerCopyFromToolbar()}
    onPastePlainTrigger={() => editorRef?.triggerPastePlainFromToolbar()}
    onRefreshInspector={() => void refreshInspectorPreview()}
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
        {#if vaultStore.vaultSnapshot}
          <Sidebar
            snapshot={vaultStore.vaultSnapshot}
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
          {#if vaultStore.vaultSnapshot}
            <TabBar />
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
            {#if tabsStore.scratchpadMode}
              <Editor
                bind:this={editorRef}
                docId="__scratchpad__"
                initialDoc={tabsStore.scratchpadContent}
                copyMode={settingsStore.copyMode}
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
      </div>

      <ClipboardInspector />
    </main>
  {/if}

  <StatusBar />
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
