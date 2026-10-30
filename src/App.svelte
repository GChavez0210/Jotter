<script lang="ts">
  import { shortcutLabel } from './lib/platform';
  import { onMount, onDestroy, tick, untrack } from 'svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import { appStorage } from './lib/appStorage';
  import { saveBeforeLeaving } from './lib/saveBeforeLeaving';
  import LazyLoadError from './lib/LazyLoadError.svelte';
  import Dialog from './lib/ui/Dialog.svelte';
  import { configureTaskReminders, configureVersionHistory } from './lib/ipc';
  import RightPanel from './lib/RightPanel.svelte';
  import ClipboardInspector from './lib/ClipboardInspector.svelte';
  import Editor from './lib/Editor.svelte';
  import NotePane from './lib/NotePane.svelte';
  import FirstRunScreen from './lib/FirstRunScreen.svelte';
  import Sidebar from './lib/Sidebar.svelte';
  import StatusBar from './lib/StatusBar.svelte';
  import TitleBar from './lib/TitleBar.svelte';
  import Button from './lib/ui/Button.svelte';
  import {
    exportActiveNoteHtml,
    listenQuitRequested,
    listenVaultFsEvents,
    quitApp,
    keepAppOpen,
    setTrayLabels,
    renderNoteHtmlPreview,
    listenCaptureRequests,
  } from './lib/ipc';
  import { enhanceRenderedHtml } from './lib/richBlocks';
  import { setOpenNoteEditApplier } from './lib/linkUpdates';
  import { clampSplitRatio } from './lib/layout';
  import { openInActivePane, toggleSplitNote } from './lib/panes';
  import IconButton from './lib/ui/IconButton.svelte';
  import type { MarkdownFormatCommandId } from './lib/markdownCommands';
  import { aiStore } from './lib/stores/ai.svelte';
  import { askStore } from './lib/stores/ask.svelte';
  import { updateStore } from './lib/stores/updates.svelte';
  import { clipboardStore } from './lib/stores/clipboard.svelte';
  import { settingsStore } from './lib/stores/settings.svelte';
  import { systemStore } from './lib/stores/system.svelte';
  import { tabsStore } from './lib/stores/tabs.svelte';
  import { vaultStore } from './lib/stores/vault.svelte';
  import { backupStore } from './lib/stores/backups.svelte';
  import { caldavStore } from './lib/stores/caldav.svelte';
  import { taskCalendarStore } from './lib/stores/taskCalendar.svelte';
  import { noteCardsStore } from './lib/stores/noteCards.svelte';
  import HomePage from './lib/HomePage.svelte';
  import { i18n, m, SLOT, splitSlots } from './lib/i18n.svelte';
  import { noteStamp } from './lib/templates';
  import { sameNotePath } from './lib/notePaths';

  let primaryEditor: Editor | undefined = $state();
  let secondaryEditor: Editor | undefined = $state();
  let paneRow: HTMLDivElement | undefined = $state();
  let draggingSash = $state(false);
  let paneWidth = $state(0);
  let displayedSplitRatio = $derived(clampSplitRatio(settingsStore.splitRatio, paneWidth));

  $effect(() => {
    if (!paneRow) return;
    const observer = new ResizeObserver(([entry]) => { paneWidth = entry.contentRect.width; });
    observer.observe(paneRow);
    return () => observer.disconnect();
  });
  let unlistenFsEvents: UnlistenFn | null = null;
  let unlistenQuit: UnlistenFn | null = null;
  let unlistenCapture: UnlistenFn | null = null;
  let missingWikiLinkPrompt = $state<{
    noteTarget: string;
    heading?: string;
  } | null>(null);
  let printArticleHtml = $state('');
  let closing = $state(false);

  /** The editor the user last worked in. Editor commands and the AI panel act on this one. */
  let activeEditor = $derived(
    settingsStore.effectivePane === 'secondary'
      ? secondaryEditor
      : primaryEditor
  );

  /** The note in the second pane, which both panes read from the same open tabs. */
  let splitTab = $derived(
    settingsStore.splitNotePath
      ? (tabsStore.openTabs.find((t) => t.relPath === settingsStore.splitNotePath) ?? null)
      : null
  );

  /** Open tabs the second pane can show: everything except the note already on the left. */
  let splitCandidates = $derived(
    tabsStore.openTabs.filter((t) => t.relPath !== tabsStore.activeTabPath)
  );

  /** Finds the pane that holds a note, so a reload or a link update reaches the right editor. */
  function editorForPath(relPath: string): Editor | undefined {
    if (settingsStore.splitVisible && settingsStore.splitMode === 'note' && sameNotePath(settingsStore.splitNotePath, relPath)) {
      return secondaryEditor;
    }
    return sameNotePath(tabsStore.activeTabPath, relPath) ? primaryEditor : undefined;
  }

  let sidebarVisible = $derived(
    Boolean(vaultStore.vaultSnapshot) &&
      settingsStore.focusMode === 'off' &&
      settingsStore.sidebarVisible
  );

  $effect(() => {
    const target = tabsStore.pendingScrollTarget;
    const targetEditor = target ? editorForPath(target.relPath) : undefined;
    if (
      target &&
      targetEditor
    ) {
      requestAnimationFrame(() => {
        if (!tabsStore.consumeScrollTarget(target)) return;
        if (target.heading) {
          targetEditor.scrollToHeading(target.heading);
        } else if (target.lineNumber) {
          targetEditor.scrollToLine(target.lineNumber);
        }
      });
    }
  });

  function getActiveNoteTitleAndMarkdown(): { title: string; markdown: string } {
    // In split view the focused pane decides, so an export matches the note on screen.
    const tab = tabsStore.focusedTab;
    if (tab) {
      const name = tab.title.replace(/\.md$/i, '');
      return {
        title: name || m.note_untitled(),
        markdown: tab.content,
      };
    }
    return {
      title: m.scratchpad_title(),
      markdown: tabsStore.scratchpadContent,
    };
  }

  async function handleExportHtml() {
    const { title, markdown } = getActiveNoteTitleAndMarkdown();
    try {
      const dark = settingsStore.effectiveDark;
      const rendered = await renderNoteHtmlPreview(title, markdown, dark);
      const html = await enhanceRenderedHtml(rendered, 'file', dark);
      const savedPath = await exportActiveNoteHtml(title, html, {
        title: m.dialog_export_html_title(),
        filter: m.dialog_export_html_filter(),
      });
      if (savedPath) {
        clipboardStore.setStatus(m.status_exported_html({ path: savedPath }));
      }
    } catch (e) {
      clipboardStore.setStatus(m.status_export_html_failed({ error: String(e) }));
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
      const printDoc = new DOMParser().parseFromString(fullHtml, 'text/html');
      // Keep export document styles out of the live app's document.
      printArticleHtml = printDoc.querySelector('main, article')?.innerHTML ?? printDoc.body.innerHTML;
      await tick();
      await document.fonts.ready;
      clipboardStore.setStatus(m.status_opening_print());
      window.print();
    } catch (e) {
      clipboardStore.setStatus(m.status_print_failed({ error: String(e) }));
    }
  }

  async function refreshInspectorPreview() {
    const sample =
      activeEditor?.getCurrentCopyTargetText() ??
      tabsStore.activeTab?.content ??
      tabsStore.scratchpadContent;
    await clipboardStore.refreshPreview(sample, settingsStore.copyMode);
  }

  /** Drags the sash between the two panes. The ratio is clamped to keep both readable. */
  function startSashDrag(e: PointerEvent) {
    e.preventDefault();
    draggingSash = true;

    const handleMove = (moveEvent: PointerEvent) => {
      const row = paneRow?.getBoundingClientRect();
      if (!row || row.width <= 0) return;
      const share = (moveEvent.clientX - row.left) / row.width;
      settingsStore.setSplitRatio(clampSplitRatio(share, row.width));
    };

    const handleUp = () => {
      draggingSash = false;
      window.removeEventListener('pointermove', handleMove);
      window.removeEventListener('pointerup', handleUp);
    };

    window.addEventListener('pointermove', handleMove);
    window.addEventListener('pointerup', handleUp);
  }

  function handleSashKeys(e: KeyboardEvent) {
    if (e.key === 'Home') {
      e.preventDefault();
      settingsStore.setSplitRatio(0.5);
      return;
    }
    const step = e.key === 'ArrowLeft' ? -0.02 : e.key === 'ArrowRight' ? 0.02 : 0;
    if (step === 0) return;
    e.preventDefault();
    const width = paneRow?.getBoundingClientRect().width ?? 0;
    settingsStore.setSplitRatio(clampSplitRatio(displayedSplitRatio + step, width));
  }

  function handleFormatCommand(cmd: MarkdownFormatCommandId) {
    activeEditor?.executeMarkdownCommand(cmd);
  }

  function handleWikiLinkClick(noteTarget: string, heading?: string) {
    const cleanNote = noteTarget.trim();
    if (!cleanNote && heading) {
      activeEditor?.scrollToHeading(heading);
      return;
    }

    if (!vaultStore.vaultSnapshot) {
      if (heading) {
        activeEditor?.scrollToHeading(heading);
      } else {
        clipboardStore.setStatus(m.status_open_vault_to_navigate({ note: cleanNote }));
      }
      return;
    }

    const found = vaultStore.resolveWikiTarget(cleanNote);
    if (found) {
      void openInActivePane(found.relPath, { heading });
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
    if (closing) { e.preventDefault(); return; }
    const isMod = e.ctrlKey || e.metaKey;
    const keyLower = e.key.toLowerCase();
    if (updateStore.dialogOpen) {
      if (e.key === 'Escape' && !updateStore.installing) {
        updateStore.closeDialog();
        activeEditor?.focusEditor();
      }
      if (e.key === 'Escape' || e.key === 'F11' || (isMod && keyLower !== 'c' && keyLower !== 'a')) e.preventDefault();
      return;
    }

    // Settings screen (Ctrl+,)
    if (isMod && !e.shiftKey && e.key === ',') {
      e.preventDefault();
      askStore.close();
      settingsStore.showCommandPalette = false;
      settingsStore.showQuickSwitcher = false;
      settingsStore.showSettings = !settingsStore.showSettings;
      return;
    }

    // Command Palette (Ctrl+P) — works everywhere, including Zen mode
    if (isMod && !e.shiftKey && keyLower === 'p') {
      e.preventDefault();
      askStore.close();
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
        askStore.close();
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
        activeEditor?.focusEditor();
        return;
      }
      if (settingsStore.versionHistoryNote) {
        settingsStore.openVersionHistory(null);
        activeEditor?.focusEditor();
        return;
      }
      if (settingsStore.showTemplatePicker) {
        settingsStore.showTemplatePicker = false;
        activeEditor?.focusEditor();
        return;
      }
      if (settingsStore.showQuickSwitcher) {
        settingsStore.showQuickSwitcher = false;
        activeEditor?.focusEditor();
        return;
      }
      if (settingsStore.showCommandPalette) {
        settingsStore.showCommandPalette = false;
        activeEditor?.focusEditor();
        return;
      }
      if (settingsStore.showSettings) {
        settingsStore.showSettings = false;
        activeEditor?.focusEditor();
        return;
      }
      if (askStore.show) {
        askStore.close();
        activeEditor?.focusEditor();
        return;
      }
      if (aiStore.showPanel) {
        aiStore.closePanel();
        activeEditor?.focusEditor();
        return;
      }
      if (settingsStore.focusMode !== 'off') {
        e.preventDefault();
        settingsStore.exitFocusMode();
        activeEditor?.focusEditor();
        return;
      }
    }

    if (isMod && keyLower === 's' && !e.shiftKey) {
      e.preventDefault();
      if (tabsStore.focusedTabPath) {
        void tabsStore.flushAutosaveFor(tabsStore.focusedTabPath);
      }
    } else if (isMod && keyLower === 'w') {
      if (tabsStore.focusedTabPath) {
        e.preventDefault();
        void tabsStore.closeTab(tabsStore.focusedTabPath);
      }
    } else if (isMod && keyLower === 'n') {
      if (vaultStore.vaultSnapshot) {
        e.preventDefault();
        const stamp = noteStamp();
        void vaultStore.createNoteInVault('', m.note_default_name({ stamp }));
      }
    } else if (isMod && (e.code === 'Backslash' || e.key === '\\' || e.key === '|')) {
      if (vaultStore.vaultSnapshot) {
        e.preventDefault();
        if (e.shiftKey) {
          void toggleSplitNote();
        } else if (settingsStore.focusMode !== 'off') {
          settingsStore.exitFocusMode();
        } else {
          settingsStore.toggleSidebar();
        }
      }
    }
  }

  $effect(() => {
    document.documentElement.lang = i18n.locale;
    void setTrayLabels(m.tray_show(), m.tray_quit()).catch(() => {
      // ignore in browser preview
    });
  });

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

  $effect(() => {
    void settingsStore.quickCaptureEnabled;
    void settingsStore.quickCaptureShortcut;
    void systemStore.syncQuickCapture();
  });

  $effect(() => {
    if (!settingsStore.autoUpdateCheck) return;
    return updateStore.startAutoCheck();
  });

  $effect(() => {
    // Snapshots are taken in the backend, as each save replaces the text on disk.
    void configureVersionHistory(settingsStore.versionHistory).catch(() => {
      // The backend is missing in a browser preview.
    });
  });

  $effect(() => {
    // Reminders run in the backend so they still fire while the window sits in the tray.
    void configureTaskReminders(settingsStore.taskReminders).catch(() => {
      // The backend is missing in a browser preview.
    });
  });

  $effect(() => {
    void settingsStore.trayEnabled;
    void settingsStore.closeAction;
    void systemStore.syncWindowBehavior();
  });

  function createUntitledNote() {
    const stamp = noteStamp();
    void vaultStore.createNoteInVault('', m.note_default_name({ stamp }));
  }

  // Feeds the Home page's "Recently opened" section.
  $effect(() => {
    const path = tabsStore.activeTabPath;
    if (path) untrack(() => noteCardsStore.recordOpened(path));
  });

  // Every save, external change, and vault switch bumps the index version.
  $effect(() => {
    void vaultStore.indexVersion;
    untrack(() => {
      taskCalendarStore.scheduleWrite();
      caldavStore.scheduleSync();
    });
  });

  // The password lives in the credential store, so ask which vault has one after a switch.
  $effect(() => {
    void vaultStore.vaultSnapshot?.rootPath;
    untrack(() => void caldavStore.refresh());
  });

  // The scheduler runs in Rust and needs to know which vault is open and where its zips go.
  $effect(() => {
    void vaultStore.vaultSnapshot?.rootPath;
    untrack(() => void backupStore.apply());
  });

  onMount(() => {
    setOpenNoteEditApplier((path, edits) => editorForPath(path)?.applyTextEdits(edits) ?? false);
    const cleanupSettings = settingsStore.initFromStorageAndSystem();
    void systemStore.reloadSnippets();
    void aiStore.ensureLoaded();

    // Tray "Quit" asks the page first so pending edits reach disk.
    void listenQuitRequested(async (quit) => {
      if (closing) return;
      closing = true;
      try {
        await tick();
        if (await saveBeforeLeaving(() => tabsStore.flushAllAutosaves(), () => appStorage.flush())) await quitApp(quit);
        else await keepAppOpen();
      } catch (err) {
        clipboardStore.setStatus(m.status_save_before_close_failed({ error: String(err) }));
        await keepAppOpen();
      } finally {
        closing = false;
      }
    })
      .then((unlisten) => {
        unlistenQuit = unlisten;
      })
      .catch(() => {
        // ignore in browser preview
      });

    // The quick capture window sends its entries here, where the open notes live.
    void listenCaptureRequests((request) => vaultStore.captureEntry(request.note, request.text))
      .then((unlisten) => {
        unlistenCapture = unlisten;
      })
      .catch(() => {
        // ignore in browser preview
      });

    void vaultStore.refreshRecentVaults().then(() => {
      try {
        const lastVault = appStorage.getItem('jotter.lastVaultPath');
        if (lastVault) {
          void vaultStore.openVaultByPath(lastVault);
        }
      } catch {
        // ignore
      }
    });

    void listenVaultFsEvents((evt) => {
      void vaultStore.handleExternalFsEvent(evt.changedPaths, (relPath, newContent) => {
        editorForPath(relPath)?.replaceContentFromDisk(newContent);
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
    backupStore.stop();
    unlistenFsEvents?.();
    unlistenQuit?.();
    unlistenCapture?.();
    tabsStore.clearAllTimers();
  });

  const appMissingLinkPromptParts = $derived(splitSlots(m.app_missing_link_prompt({ note: SLOT })));
  const appExternalChangeParts = $derived(splitSlots(m.app_external_change({ path: SLOT })));
  const appNothingOpenHintParts = $derived(splitSlots(m.app_nothing_open_hint({ shortcut: SLOT })));
</script>

<svelte:window
  onkeydown={handleGlobalKeyDown}
  onmousemove={handleGlobalMouseMove}
/>

<div
  inert={closing}
  class="app-shell"
  class:focus-minimal={settingsStore.focusMode === 'minimal'}
  class:focus-zen={settingsStore.focusMode === 'zen'}
  data-focus-mode={settingsStore.focusMode}
>
  {#if vaultStore.vaultSnapshot && settingsStore.focusMode === 'off'}
    <Sidebar
      snapshot={vaultStore.vaultSnapshot}
      recentVaults={vaultStore.recentVaults}
      onOpenVault={(path) => void vaultStore.openVaultByPath(path)}
      activeRelPath={tabsStore.activeTabPath}
      width={settingsStore.sidebarWidth}
      collapsed={!settingsStore.sidebarVisible}
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
        <div class="external-reload-banner" role="alertdialog" aria-label={m.app_missing_link_label()}>
          <span>
            {appMissingLinkPromptParts[0]}<strong>[[{missingWikiLinkPrompt.noteTarget}]]</strong>{appMissingLinkPromptParts[1]}
          </span>
          <div class="external-reload-actions">
            <Button
              variant="primary"
              onclick={() => void confirmCreateMissingWikiNote()}
            >
              {m.app_create_and_open()}
            </Button>
            <Button
              onclick={() => (missingWikiLinkPrompt = null)}
            >
              {m.common_cancel()}
            </Button>
          </div>
        </div>
      {/if}

      {#each tabsStore.openTabs.filter((tab) => tab.externalChangePending) as changedTab (changedTab.relPath)}
        <div class="external-reload-banner" role="alert">
          <span>
            {appExternalChangeParts[0]}<strong>{changedTab.relPath}</strong>{appExternalChangeParts[1]}
          </span>
          <div class="external-reload-actions">
            <Button
              variant="primary"
              onclick={() =>
                void tabsStore.reloadTabFromDisk(
                  changedTab.relPath,
                  (relPath, newContent) => editorForPath(relPath)?.replaceContentFromDisk(newContent)
                )}
            >
              {m.app_reload_from_disk()}
            </Button>
            <Button
              onclick={() =>
                tabsStore.dismissExternalChangeBanner(
                  changedTab.relPath
                )}
            >
              {m.app_keep_my_edits()}
            </Button>
          </div>
        </div>
      {/each}

      <div class="work-row">
        <div class="pane-row" bind:this={paneRow}>
          <section
            class="editor-column"
            aria-label={m.app_editor_label()}
            style:flex={settingsStore.splitVisible ? `${displayedSplitRatio} 1 0` : '1 1 0'}
            onfocusin={() => settingsStore.setActivePane('primary')}
          >
            <div class="editor-card">
              {#if tabsStore.scratchpadMode}
                <NotePane
                  pane="primary"
                  docId="__scratchpad__"
                  initialDoc={tabsStore.scratchpadContent}
                  tab={null}
                  onEditor={(editor) => (primaryEditor = editor)}
                  onOpenNote={handleWikiLinkClick}
                  onScratchpadEdit={() => {
                    if (settingsStore.showInspector) void refreshInspectorPreview();
                  }}
                />
              {:else if tabsStore.activeTab}
                <NotePane
                  pane="primary"
                  docId={tabsStore.activeTab.relPath}
                  initialDoc={tabsStore.activeTab.content}
                  tab={tabsStore.activeTab}
                  onEditor={(editor) => (primaryEditor = editor)}
                  onOpenNote={handleWikiLinkClick}
                />
              {:else if vaultStore.vaultSnapshot && settingsStore.showHomePage}
                <div class="home-scroll">
                  <HomePage onNewNote={createUntitledNote} />
                </div>
              {:else}
                <div class="no-open-note">
                  <div class="no-note-title">{m.app_nothing_open()}</div>
                  <div class="no-note-desc">
                    {appNothingOpenHintParts[0]}<kbd>{shortcutLabel('Ctrl O')}</kbd>{appNothingOpenHintParts[1]}
                  </div>
                  <Button variant="primary" icon="file-plus" onclick={createUntitledNote}>
                    {m.common_new_note()}
                  </Button>
                </div>
              {/if}
            </div>
          </section>

          {#if settingsStore.splitVisible}
            <!-- A focusable separator is the ARIA pattern for a window splitter, which the
                 a11y rules do not cover. Arrow keys move it. -->
            <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <div
              class="pane-sash"
              class:dragging={draggingSash}
              role="separator"
              aria-orientation="vertical"
              aria-label={m.split_resize()}
              aria-valuenow={Math.round(displayedSplitRatio * 100)}
              aria-valuemin={20}
              aria-valuemax={80}
              tabindex="0"
              onpointerdown={startSashDrag}
              onkeydown={handleSashKeys}
              ondblclick={() => settingsStore.setSplitRatio(0.5)}
            ></div>

            <section
              class="editor-column second"
              aria-label={settingsStore.splitMode === 'preview'
                ? m.split_preview_label()
                : m.split_second_pane()}
              style:flex={`${1 - displayedSplitRatio} 1 0`}
              onfocusin={() => settingsStore.setActivePane(settingsStore.splitMode === 'note' ? 'secondary' : 'primary')}
            >
              <div class="pane-bar">
                <span class="pane-bar-title">
                  {settingsStore.splitMode === 'preview'
                    ? m.split_preview_label()
                    : m.split_second_pane()}
                </span>
                <IconButton
                  icon="x"
                  size="sm"
                  label={m.split_close()}
                  onclick={() => settingsStore.closeSplit()}
                />
              </div>
              <div class="editor-card">
                {#if settingsStore.splitMode === 'preview'}
                  {#await import('./lib/PreviewPane.svelte') then { default: PreviewPane }}
                    <PreviewPane />
                  {:catch}
                    <LazyLoadError onClose={() => settingsStore.closeSplit()} />
                  {/await}
                {:else if splitTab}
                  <NotePane
                    pane="secondary"
                    docId={splitTab.relPath}
                    initialDoc={splitTab.content}
                    tab={splitTab}
                    onEditor={(editor) => (secondaryEditor = editor)}
                    onOpenNote={handleWikiLinkClick}
                  />
                {:else}
                  <div class="no-open-note">
                    <div class="no-note-desc">{m.split_pick_note()}</div>
                    {#each splitCandidates as candidate (candidate.relPath)}
                      <Button
                        icon="file-text"
                        onclick={() => settingsStore.openSplitNote(candidate.relPath)}
                      >
                        {candidate.title}
                      </Button>
                    {/each}
                  </div>
                {/if}
              </div>
            </section>
          {/if}
        </div>

        {#if vaultStore.vaultSnapshot && settingsStore.focusMode === 'off'}
          <RightPanel />
        {/if}
      </div>

      {#if settingsStore.focusMode === 'off'}
        <ClipboardInspector />
      {/if}
    {/if}

    <StatusBar />
  </div>

  {#if vaultStore.linkUpdatePrompt}
    {#await import('./lib/LinkUpdateDialog.svelte') then { default: LinkUpdateDialog }}
      <LinkUpdateDialog />
    {:catch}
      <Dialog label={m.lazy_load_failed()} onClose={() => vaultStore.answerLinkUpdate('cancel')}>
        <LazyLoadError onClose={() => vaultStore.answerLinkUpdate('cancel')} />
      </Dialog>
    {/await}
  {/if}

  {#if updateStore.dialogOpen}
    {#await import('./lib/UpdateDialog.svelte') then { default: UpdateDialog }}
      <UpdateDialog
        saveFirst={() => saveBeforeLeaving(() => tabsStore.flushAllAutosaves(), () => appStorage.flush())}
        onClose={() => {
          if (updateStore.installing) return;
          updateStore.closeDialog();
          activeEditor?.focusEditor();
        }}
      />
    {:catch}
      <Dialog label={m.lazy_load_failed()} onClose={() => updateStore.closeDialog()}>
        <LazyLoadError onClose={() => updateStore.closeDialog()} />
      </Dialog>
    {/await}
  {/if}

  {#if settingsStore.versionHistoryNote}
    {#await import('./lib/VersionHistoryDialog.svelte') then { default: VersionHistoryDialog }}
      <VersionHistoryDialog
        relPath={settingsStore.versionHistoryNote}
        onRestored={(relPath, content) => {
          tabsStore.applyRestoredContent(relPath, content);
          editorForPath(relPath)?.replaceContentFromDisk(content);
        }}
        onClose={() => {
          settingsStore.openVersionHistory(null);
          activeEditor?.focusEditor();
        }}
      />
    {:catch}
      <Dialog label={m.lazy_load_failed()} onClose={() => settingsStore.openVersionHistory(null)}>
        <LazyLoadError onClose={() => settingsStore.openVersionHistory(null)} />
      </Dialog>
    {/await}
  {/if}

  {#if settingsStore.showTemplatePicker}
    {#await import('./lib/TemplatePicker.svelte') then { default: TemplatePicker }}
      <TemplatePicker
        onClose={() => {
          settingsStore.showTemplatePicker = false;
          activeEditor?.focusEditor();
        }}
      />
    {:catch}
      <Dialog label={m.lazy_load_failed()} onClose={() => { settingsStore.showTemplatePicker = false; activeEditor?.focusEditor(); }}>
        <LazyLoadError onClose={() => { settingsStore.showTemplatePicker = false; activeEditor?.focusEditor(); }} />
      </Dialog>
    {/await}
  {/if}

  {#if settingsStore.showQuickSwitcher}
    {#await import('./lib/QuickSwitcher.svelte') then { default: QuickSwitcher }}
      <QuickSwitcher
        onClose={() => {
          settingsStore.showQuickSwitcher = false;
          activeEditor?.focusEditor();
        }}
      />
    {:catch}
      <Dialog label={m.lazy_load_failed()} onClose={() => { settingsStore.showQuickSwitcher = false; activeEditor?.focusEditor(); }}>
        <LazyLoadError onClose={() => { settingsStore.showQuickSwitcher = false; activeEditor?.focusEditor(); }} />
      </Dialog>
    {/await}
  {/if}

  {#if settingsStore.showCommandPalette}
    {#await import('./lib/CommandPalette.svelte') then { default: CommandPalette }}
      <CommandPalette
        onFormat={handleFormatCommand}
        onCopyTrigger={() => activeEditor?.triggerCopyFromToolbar()}
        onPastePlainTrigger={() => activeEditor?.triggerPastePlainFromToolbar()}
        onRefreshInspector={() => void refreshInspectorPreview()}
        onExportHtml={() => void handleExportHtml()}
        onExportPdf={() => void handleExportPdf()}
        onClose={() => {
          settingsStore.showCommandPalette = false;
          activeEditor?.focusEditor();
        }}
      />
    {:catch}
      <Dialog label={m.lazy_load_failed()} onClose={() => { settingsStore.showCommandPalette = false; activeEditor?.focusEditor(); }}>
        <LazyLoadError onClose={() => { settingsStore.showCommandPalette = false; activeEditor?.focusEditor(); }} />
      </Dialog>
    {/await}
  {/if}

  {#if aiStore.showPanel && activeEditor}
    {#await import('./lib/AiPanel.svelte') then { default: AiPanel }}
      <AiPanel
        hasSelection={activeEditor.hasSelection()}
        captureTarget={(scope) => activeEditor?.captureAiTarget(scope) ?? null}
        applyResult={(target, text, placement) =>
          activeEditor?.applyAiResult(target, text, placement) ?? false}
        onOpenSettings={() => {
          aiStore.closePanel();
          settingsStore.openSettings('ai');
        }}
        onClose={() => {
          aiStore.closePanel();
          activeEditor?.focusEditor();
        }}
      />
    {:catch}
      <Dialog label={m.lazy_load_failed()} onClose={() => { aiStore.closePanel(); activeEditor?.focusEditor(); }}>
        <LazyLoadError onClose={() => { aiStore.closePanel(); activeEditor?.focusEditor(); }} />
      </Dialog>
    {/await}
  {/if}

  {#if askStore.show}
    {#await import('./lib/AskPanel.svelte') then { default: AskPanel }}
      <AskPanel
        onOpenNote={(path, line) => {
          askStore.close();
          void tabsStore.openNoteInTab(path, { lineNumber: line });
        }}
        onOpenSettings={() => {
          askStore.close();
          settingsStore.openSettings('ai');
        }}
        onClose={() => {
          askStore.close();
          activeEditor?.focusEditor();
        }}
      />
    {:catch}
      <Dialog label={m.lazy_load_failed()} onClose={() => askStore.close()}>
        <LazyLoadError onClose={() => askStore.close()} />
      </Dialog>
    {/await}
  {/if}

  {#if settingsStore.showSettings}
    {#await import('./lib/SettingsModal.svelte') then { default: SettingsModal }}
      <SettingsModal
        onRefreshInspector={() => void refreshInspectorPreview()}
        onClose={() => {
          settingsStore.showSettings = false;
          activeEditor?.focusEditor();
        }}
      />
    {:catch}
      <Dialog label={m.lazy_load_failed()} onClose={() => { settingsStore.showSettings = false; activeEditor?.focusEditor(); }}>
        <LazyLoadError onClose={() => { settingsStore.showSettings = false; activeEditor?.focusEditor(); }} />
      </Dialog>
    {/await}
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

  /* Holds the one or two editor panes, with the right panel outside it, so the sash
     measures only the text columns. */
  .pane-row {
    flex: 1;
    min-width: 0;
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

  .editor-column.second {
    border-left: 1px solid var(--line-subtle);
  }

  .pane-sash {
    flex: none;
    width: 5px;
    cursor: col-resize;
    background: transparent;
    transition: background-color var(--transition-fast);
  }

  .pane-sash:hover,
  .pane-sash:focus-visible,
  .pane-sash.dragging {
    background: var(--accent);
  }

  /* Names the second pane and closes it; the first pane needs no bar of its own. */
  .pane-bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--row-height);
    padding: 0 var(--space-1) 0 var(--space-3);
    border-bottom: 1px solid var(--line-subtle);
    color: var(--text-muted);
    font-size: var(--fs-12);
  }

  .pane-bar-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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

  .home-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
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
