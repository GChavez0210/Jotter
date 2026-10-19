<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import Editor from './lib/Editor.svelte';
  import FirstRunScreen from './lib/FirstRunScreen.svelte';
  import Sidebar from './lib/Sidebar.svelte';
  import { designTokens } from './lib/designTokens';
  import { fluentIcons } from './lib/icons';
  import {
    closeVault,
    createFolder,
    createNewVault,
    createNote,
    deleteEntry,
    getBackdropState,
    getRecentVaults,
    getVaultSnapshot,
    listenVaultFsEvents,
    openVault,
    previewClipboardPayload,
    readNote,
    removeRecentVaultEntry,
    renameEntry,
    saveNote,
    syncWindowTheme,
  } from './lib/ipc';
  import type {
    BackdropState,
    ClipboardPayload,
    CopyMode,
    RecentVault,
    ThemePreference,
    VaultNode,
    VaultSnapshot,
  } from './lib/types';

  interface OpenTab {
    relPath: string;
    title: string;
    content: string;
    savedContent: string;
    dirty: boolean;
    externalChangePending: boolean;
  }

  const SAMPLE_NOTE = `# Jotter Clipboard Spike — Line Break Verification

Copy any block below with Ctrl+C (or the Copy button) and paste it into Google Chat, Slack, Gmail, Microsoft Word, or Windows Notepad.

## 1. Consecutive Single Line Breaks
Line A: First line with **bold text** and *italic text*.
Line B: Immediately below Line A (single newline, never collapsed into a space).
Line C: Immediately below Line B with \`inline code\` and ~~strikethrough~~.

## 2. Preserved Blank Lines (Single and Double Gaps)
Paragraph 1: Separated from Paragraph 2 by one blank line.

Paragraph 2: Separated from Paragraph 3 by two blank lines.


Paragraph 3: Exact blank line count stays intact just like Windows Notepad.

## 3. Obsidian Vault Syntax & Lists
See [[Product Spec|Jotter Product Spec]] and [[Architecture#Clipboard Engine]] for #windows11 details.
- [x] Write text/plain with real \\n line breaks
- [x] Write text/html where every newline becomes <br>
- [ ] Run manual paste checks in target apps
> Tip: Press Ctrl+Shift+V anywhere in the editor to paste clipboard contents strictly as plain text.`;

  const COPY_MODE_LABELS: Record<CopyMode, { label: string; desc: string }> = {
    markdown_source: {
      label: 'Markdown source',
      desc: 'Default: raw Markdown with <br> & &nbsp; HTML plus plain text',
    },
    plain: {
      label: 'Plain',
      desc: 'Opt-in clean text: text/plain only (arboard set_text, Markdown stripped)',
    },
    rich: {
      label: 'Rich',
      desc: 'HTML (<br> newlines, &nbsp; indent, formatting) plus plain text',
    },
  };

  let copyMode: CopyMode = $state('markdown_source');
  let themePref: ThemePreference = $state('system');
  let systemIsDark = $state(false);
  let forceSolidBackdrop = $state(false);
  let backdropState: BackdropState = $state({
    micaApplied: true,
    backdropKind: 'mica',
    darkMode: null,
  });

  // Vault & Workspace state
  let vaultSnapshot: VaultSnapshot | null = $state(null);
  let recentVaults: RecentVault[] = $state([]);
  let scratchpadMode = $state(false);
  let onboardingError = $state('');

  // Sidebar state
  let sidebarWidth: number = $state(designTokens.layout.sidebarDefaultWidth);
  let sidebarCollapsed = $state(false);

  // Tabs & Autosave state
  let openTabs: OpenTab[] = $state([]);
  let activeTabPath: string | null = $state(null);
  let scratchpadContent = $state(SAMPLE_NOTE);
  let autosaveTimers = new Map<string, ReturnType<typeof setTimeout>>();

  // UI popovers & drawers
  let showSettings = $state(false);
  let showInspector = $state(false);
  let lastPayload: ClipboardPayload | null = $state(null);
  let statusBanner = $state('Ready');
  let statusTimestamp = $state('');

  let docStats = $state({
    lines: 0,
    words: 0,
    chars: 0,
    selectedChars: 0,
    selectedLines: 0,
  });

  let editorRef: Editor | undefined = $state();
  let unlistenFsEvents: UnlistenFn | null = null;

  let activeTab = $derived(
    openTabs.find((t) => t.relPath === activeTabPath) ?? null
  );

  let effectiveDark = $derived(
    themePref === 'system' ? systemIsDark : themePref === 'dark'
  );

  let effectiveSolidBackdrop = $derived(
    forceSolidBackdrop || !backdropState.micaApplied
  );

  function setStatus(msg: string) {
    statusBanner = msg;
    statusTimestamp = new Date().toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  }

  function setCopyMode(next: CopyMode) {
    copyMode = next;
    try {
      localStorage.setItem('jotter.copyMode', next);
    } catch {
      // ignore storage errors
    }
    if (showInspector) void refreshInspectorPreview();
  }

  function setThemePreference(next: ThemePreference) {
    themePref = next;
    try {
      localStorage.setItem('jotter.themePref', next);
    } catch {
      // ignore
    }
    applyThemeToDocument(next, systemIsDark);
  }

  function applyThemeToDocument(pref: ThemePreference, sysDark: boolean) {
    const root = document.documentElement;
    if (pref === 'system') {
      root.removeAttribute('data-theme');
    } else {
      root.setAttribute('data-theme', pref);
    }
    const isDark = pref === 'system' ? sysDark : pref === 'dark';
    void syncWindowTheme(isDark)
      .then((res) => {
        backdropState = res;
      })
      .catch(() => {
        backdropState = {
          micaApplied: false,
          backdropKind: 'solid_fallback',
          darkMode: isDark,
        };
      });
  }

  function findFirstFileNode(nodes: VaultNode[]): VaultNode | null {
    for (const node of nodes) {
      if (node.kind === 'file') return node;
      if (node.children.length > 0) {
        const found = findFirstFileNode(node.children);
        if (found) return found;
      }
    }
    return null;
  }

  async function refreshRecentVaults() {
    try {
      recentVaults = await getRecentVaults();
    } catch {
      recentVaults = [];
    }
  }

  async function handleOpenVaultPath(path: string) {
    onboardingError = '';
    try {
      await flushAllAutosaves();
      const snap = await openVault(path);
      vaultSnapshot = snap;
      scratchpadMode = false;
      openTabs = [];
      activeTabPath = null;
      try {
        localStorage.setItem('jotter.lastVaultPath', snap.rootPath);
      } catch {
        // ignore
      }
      await refreshRecentVaults();
      const firstFile = findFirstFileNode(snap.tree);
      if (firstFile) {
        await openNoteInTab(firstFile.relPath);
      }
      setStatus(`Opened vault "${snap.name}" (${snap.noteCount} notes)`);
    } catch (err) {
      onboardingError = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleCreateVault(parentPath: string, vaultName: string) {
    onboardingError = '';
    try {
      await flushAllAutosaves();
      const snap = await createNewVault(parentPath, vaultName);
      vaultSnapshot = snap;
      scratchpadMode = false;
      openTabs = [];
      activeTabPath = null;
      try {
        localStorage.setItem('jotter.lastVaultPath', snap.rootPath);
      } catch {
        // ignore
      }
      await refreshRecentVaults();
      const firstFile = findFirstFileNode(snap.tree);
      if (firstFile) {
        await openNoteInTab(firstFile.relPath);
      }
      setStatus(`Created vault "${snap.name}"`);
    } catch (err) {
      onboardingError = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleSwitchVault() {
    await flushAllAutosaves();
    try {
      await closeVault();
    } catch {
      // ignore
    }
    vaultSnapshot = null;
    scratchpadMode = false;
    openTabs = [];
    activeTabPath = null;
    try {
      localStorage.removeItem('jotter.lastVaultPath');
    } catch {
      // ignore
    }
    await refreshRecentVaults();
  }

  async function handleRemoveRecentVault(path: string) {
    try {
      recentVaults = await removeRecentVaultEntry(path);
    } catch (err) {
      onboardingError = err instanceof Error ? err.message : String(err);
    }
  }

  async function openNoteInTab(relPath: string) {
    const existing = openTabs.find((t) => t.relPath === relPath);
    if (existing) {
      activeTabPath = existing.relPath;
      return;
    }

    try {
      const doc = await readNote(relPath);
      const tab: OpenTab = {
        relPath: doc.relPath,
        title: doc.title,
        content: doc.content,
        savedContent: doc.content,
        dirty: false,
        externalChangePending: false,
      };
      openTabs = [...openTabs, tab];
      activeTabPath = tab.relPath;
      setStatus(`Opened ${doc.relPath}`);
    } catch (err) {
      setStatus(`Error opening note: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  async function closeTab(relPath: string, e?: MouseEvent) {
    e?.stopPropagation();
    await flushAutosaveFor(relPath);
    const idx = openTabs.findIndex((t) => t.relPath === relPath);
    if (idx < 0) return;

    const nextTabs = openTabs.filter((t) => t.relPath !== relPath);
    openTabs = nextTabs;

    if (activeTabPath === relPath) {
      if (nextTabs.length === 0) {
        activeTabPath = null;
      } else {
        const nextIdx = Math.min(idx, nextTabs.length - 1);
        activeTabPath = nextTabs[nextIdx].relPath;
      }
    }
  }

  function handleEditorDocChange(docId: string, newContent: string) {
    if (docId === '__scratchpad__') {
      scratchpadContent = newContent;
      if (showInspector) void refreshInspectorPreview();
      return;
    }

    openTabs = openTabs.map((t) =>
      t.relPath === docId
        ? {
            ...t,
            content: newContent,
            dirty: newContent !== t.savedContent,
          }
        : t
    );

    scheduleAutosave(docId);
  }

  function scheduleAutosave(relPath: string) {
    const prev = autosaveTimers.get(relPath);
    if (prev) clearTimeout(prev);
    const timer = setTimeout(() => {
      autosaveTimers.delete(relPath);
      void flushAutosaveFor(relPath);
    }, 380);
    autosaveTimers.set(relPath, timer);
  }

  async function flushAutosaveFor(relPath: string) {
    const timer = autosaveTimers.get(relPath);
    if (timer) {
      clearTimeout(timer);
      autosaveTimers.delete(relPath);
    }

    const tab = openTabs.find((t) => t.relPath === relPath);
    if (!tab || !tab.dirty) return;

    const contentToSave = tab.content;
    try {
      const savedDoc = await saveNote(relPath, contentToSave);
      openTabs = openTabs.map((t) =>
        t.relPath === relPath
          ? {
              ...t,
              savedContent: contentToSave,
              dirty: t.content !== contentToSave,
              title: savedDoc.title,
            }
          : t
      );
      setStatus(`Saved ${savedDoc.relPath}`);
    } catch (err) {
      setStatus(`Autosave failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  async function flushAllAutosaves() {
    const paths = openTabs.filter((t) => t.dirty).map((t) => t.relPath);
    for (const p of paths) {
      await flushAutosaveFor(p);
    }
  }

  async function handleCreateNote(parentFolderRel: string, noteName: string) {
    const targetRel = parentFolderRel ? `${parentFolderRel}/${noteName}` : noteName;
    try {
      const titleBase = noteName.replace(/\.md$/i, '');
      const [snap, doc] = await createNote(targetRel, `# ${titleBase}\n\n`);
      vaultSnapshot = snap;
      const newTab: OpenTab = {
        relPath: doc.relPath,
        title: doc.title,
        content: doc.content,
        savedContent: doc.content,
        dirty: false,
        externalChangePending: false,
      };
      openTabs = [...openTabs.filter((t) => t.relPath !== doc.relPath), newTab];
      activeTabPath = doc.relPath;
      setStatus(`Created ${doc.relPath}`);
    } catch (err) {
      setStatus(`Failed to create note: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  async function handleCreateFolder(parentFolderRel: string, folderName: string) {
    const targetRel = parentFolderRel
      ? `${parentFolderRel}/${folderName}`
      : folderName;
    try {
      vaultSnapshot = await createFolder(targetRel);
      setStatus(`Created folder ${targetRel}`);
    } catch (err) {
      setStatus(`Failed to create folder: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  async function handleRenameEntry(oldRelPath: string, newRelPath: string) {
    await flushAllAutosaves();
    try {
      const res = await renameEntry(oldRelPath, newRelPath);
      vaultSnapshot = res.snapshot;
      const oldPrefix = `${res.oldRelPath}/`;

      openTabs = openTabs.map((t) => {
        if (t.relPath === res.oldRelPath) {
          const base = res.newRelPath.split('/').pop() ?? res.newRelPath;
          const title = base.toLowerCase().endsWith('.md') ? base.slice(0, -3) : base;
          return { ...t, relPath: res.newRelPath, title };
        }
        if (t.relPath.startsWith(oldPrefix)) {
          const updatedRel = `${res.newRelPath}/${t.relPath.slice(oldPrefix.length)}`;
          return { ...t, relPath: updatedRel };
        }
        return t;
      });

      if (activeTabPath === res.oldRelPath) {
        activeTabPath = res.newRelPath;
      } else if (activeTabPath?.startsWith(oldPrefix)) {
        activeTabPath = `${res.newRelPath}/${activeTabPath.slice(oldPrefix.length)}`;
      }

      setStatus(`Renamed ${res.oldRelPath} → ${res.newRelPath}`);
    } catch (err) {
      setStatus(`Rename failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  async function handleDeleteEntry(relPath: string) {
    const prefix = `${relPath}/`;
    for (const [k, timer] of autosaveTimers.entries()) {
      if (k === relPath || k.startsWith(prefix)) {
        clearTimeout(timer);
        autosaveTimers.delete(k);
      }
    }

    try {
      vaultSnapshot = await deleteEntry(relPath);
      const remaining = openTabs.filter(
        (t) => t.relPath !== relPath && !t.relPath.startsWith(prefix)
      );
      openTabs = remaining;
      if (
        activeTabPath &&
        (activeTabPath === relPath || activeTabPath.startsWith(prefix))
      ) {
        activeTabPath = remaining.length > 0 ? remaining[remaining.length - 1].relPath : null;
      }
      setStatus(`Deleted ${relPath}`);
    } catch (err) {
      setStatus(`Delete failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  async function handleExternalFsEvent(changedPaths: string[]) {
    if (!vaultSnapshot) return;
    try {
      const snap = await getVaultSnapshot();
      if (snap) vaultSnapshot = snap;
    } catch {
      // ignore
    }

    for (const rel of changedPaths) {
      const tab = openTabs.find(
        (t) => t.relPath.toLowerCase() === rel.toLowerCase()
      );
      if (!tab) continue;

      if (!tab.dirty) {
        await reloadTabFromDisk(tab.relPath);
      } else {
        openTabs = openTabs.map((t) =>
          t.relPath === tab.relPath ? { ...t, externalChangePending: true } : t
        );
        setStatus(`External edit detected on ${tab.relPath}`);
      }
    }
  }

  async function reloadTabFromDisk(relPath: string) {
    try {
      const doc = await readNote(relPath);
      openTabs = openTabs.map((t) =>
        t.relPath === relPath
          ? {
              ...t,
              content: doc.content,
              savedContent: doc.content,
              dirty: false,
              externalChangePending: false,
            }
          : t
      );
      if (activeTabPath === relPath) {
        editorRef?.replaceContentFromDisk(doc.content);
      }
      setStatus(`Reloaded external changes: ${relPath}`);
    } catch {
      // File may have been deleted externally; close tab
      openTabs = openTabs.filter((t) => t.relPath !== relPath);
      if (activeTabPath === relPath) {
        activeTabPath = openTabs.length > 0 ? openTabs[openTabs.length - 1].relPath : null;
      }
    }
  }

  async function refreshInspectorPreview() {
    const sample =
      editorRef?.getCurrentCopyTargetText() ??
      activeTab?.content ??
      scratchpadContent;
    try {
      lastPayload = await previewClipboardPayload({
        text: sample,
        mode: copyMode,
      });
    } catch {
      // ignore in browser-only preview
    }
  }

  function handleCopyComplete(payload: ClipboardPayload, source: 'copy' | 'cut') {
    lastPayload = payload;
    const verb = source === 'cut' ? 'Cut' : 'Copied';
    const modeLabel = COPY_MODE_LABELS[payload.mode].label;
    setStatus(
      `${verb} ${payload.lineCount} ${
        payload.lineCount === 1 ? 'line' : 'lines'
      } (${payload.charCount} chars) in ${modeLabel} mode`
    );
  }

  function handlePastePlainComplete(charCount: number, lineCount: number) {
    setStatus(
      `Pasted ${lineCount} ${
        lineCount === 1 ? 'line' : 'lines'
      } (${charCount} chars) as plain text (Ctrl+Shift+V)`
    );
  }

  function handleGlobalKeyDown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      if (activeTabPath) {
        void flushAutosaveFor(activeTabPath);
      }
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'w') {
      if (activeTabPath) {
        e.preventDefault();
        void closeTab(activeTabPath);
      }
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n') {
      if (vaultSnapshot) {
        e.preventDefault();
        const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
        void handleCreateNote('', `Note ${stamp}`);
      }
    } else if ((e.ctrlKey || e.metaKey) && e.key === '\\') {
      if (vaultSnapshot) {
        e.preventDefault();
        sidebarCollapsed = !sidebarCollapsed;
      }
    }
  }

  onMount(() => {
    try {
      const savedMode = localStorage.getItem('jotter.copyMode') as CopyMode | null;
      if (
        savedMode === 'markdown_source' ||
        savedMode === 'plain' ||
        savedMode === 'rich'
      ) {
        copyMode = savedMode;
      }
      const savedTheme = localStorage.getItem(
        'jotter.themePref'
      ) as ThemePreference | null;
      if (
        savedTheme === 'system' ||
        savedTheme === 'light' ||
        savedTheme === 'dark'
      ) {
        themePref = savedTheme;
      }
      const savedWidth = Number(localStorage.getItem('jotter.sidebarWidth'));
      if (savedWidth >= 180 && savedWidth <= 480) {
        sidebarWidth = savedWidth;
      }
    } catch {
      // ignore
    }

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    systemIsDark = mediaQuery.matches;
    applyThemeToDocument(themePref, systemIsDark);

    const handleMediaChange = (e: MediaQueryListEvent) => {
      systemIsDark = e.matches;
      applyThemeToDocument(themePref, systemIsDark);
    };
    mediaQuery.addEventListener('change', handleMediaChange);

    void getBackdropState()
      .then((s) => {
        backdropState = s;
      })
      .catch(() => {
        backdropState = {
          micaApplied: false,
          backdropKind: 'solid_fallback',
          darkMode: systemIsDark,
        };
      });

    void refreshRecentVaults().then(() => {
      try {
        const lastVault = localStorage.getItem('jotter.lastVaultPath');
        if (lastVault) {
          void handleOpenVaultPath(lastVault);
        }
      } catch {
        // ignore
      }
    });

    void listenVaultFsEvents((evt) => {
      void handleExternalFsEvent(evt.changedPaths);
    })
      .then((unlisten) => {
        unlistenFsEvents = unlisten;
      })
      .catch(() => {
        // ignore in browser preview
      });

    return () => {
      mediaQuery.removeEventListener('change', handleMediaChange);
    };
  });

  onDestroy(() => {
    unlistenFsEvents?.();
    for (const timer of autosaveTimers.values()) {
      clearTimeout(timer);
    }
  });
</script>

<svelte:window onkeydown={handleGlobalKeyDown} />

<div
  class="app-shell"
  class:solid-backdrop={effectiveSolidBackdrop}
  data-backdrop={effectiveSolidBackdrop ? 'solid' : 'mica'}
>
  <!-- Integrated Fluent Header & Toolbar -->
  <header class="command-bar" aria-label="Application toolbar">
    <div class="brand-group">
      {#if vaultSnapshot}
        <button
          type="button"
          class="fluent-btn icon-only"
          aria-pressed={!sidebarCollapsed}
          title="Toggle file tree sidebar (Ctrl+\)"
          aria-label="Toggle file tree sidebar"
          onclick={() => (sidebarCollapsed = !sidebarCollapsed)}
        >
          {@html fluentIcons.sidebar16}
        </button>
      {/if}
      <span class="brand-icon">{@html fluentIcons.note20}</span>
      <span class="brand-title">Jotter</span>
      {#if vaultSnapshot}
        <span class="note-pill" title={vaultSnapshot.rootPath}>
          {vaultSnapshot.name}
        </span>
      {:else if scratchpadMode}
        <span class="note-pill">Scratchpad</span>
        <button
          type="button"
          class="fluent-btn"
          onclick={() => (scratchpadMode = false)}
        >
          {@html fluentIcons.folderOpen20}
          <span>Open / Create Vault</span>
        </button>
      {/if}
    </div>

    <div class="toolbar-actions">
      <!-- Copy Mode Selector (Markdown source default / Plain / Rich) -->
      <div class="segmented-group" role="radiogroup" aria-label="Copy mode">
        <span class="segmented-label">Copy mode:</span>
        {#each ['markdown_source', 'plain', 'rich'] as const as mode}
          <button
            type="button"
            role="radio"
            aria-checked={copyMode === mode}
            class="seg-btn"
            class:active={copyMode === mode}
            title={COPY_MODE_LABELS[mode].desc}
            onclick={() => setCopyMode(mode)}
          >
            {COPY_MODE_LABELS[mode].label}
          </button>
        {/each}
      </div>

      <div class="divider" role="separator"></div>

      <button
        type="button"
        class="fluent-btn primary-btn"
        title="Copy selection or note (Ctrl+C)"
        onclick={() => editorRef?.triggerCopyFromToolbar()}
      >
        {@html fluentIcons.copy16}
        <span>Copy</span>
      </button>

      <button
        type="button"
        class="fluent-btn"
        title="Paste clipboard contents strictly as plain text (Ctrl+Shift+V)"
        onclick={() => editorRef?.triggerPastePlainFromToolbar()}
      >
        {@html fluentIcons.clipboardPaste16}
        <span>Paste Plain</span>
      </button>

      <button
        type="button"
        class="fluent-btn"
        class:active-toggle={showInspector}
        aria-pressed={showInspector}
        title="Toggle live clipboard payload inspector"
        onclick={() => {
          showInspector = !showInspector;
          if (showInspector) void refreshInspectorPreview();
        }}
      >
        {@html fluentIcons.code16}
        <span>Inspector</span>
      </button>

      <div class="settings-wrapper">
        <button
          type="button"
          class="fluent-btn icon-only"
          aria-expanded={showSettings}
          aria-label="Clipboard and appearance settings"
          title="Settings"
          onclick={() => (showSettings = !showSettings)}
        >
          {@html fluentIcons.settings16}
        </button>

        {#if showSettings}
          <div class="settings-popover" role="dialog" aria-label="Settings">
            <div class="popover-section">
              <div class="popover-heading">Copy Format Mode</div>
              {#each ['markdown_source', 'plain', 'rich'] as const as mode}
                <label class="radio-row">
                  <input
                    type="radio"
                    name="copy-mode-setting"
                    checked={copyMode === mode}
                    onchange={() => setCopyMode(mode)}
                  />
                  <div class="radio-text">
                    <span class="radio-title">{COPY_MODE_LABELS[mode].label}</span>
                    <span class="radio-desc">{COPY_MODE_LABELS[mode].desc}</span>
                  </div>
                </label>
              {/each}
            </div>

            <div class="popover-section">
              <div class="popover-heading">Theme</div>
              <div class="theme-pills">
                {#each ['system', 'light', 'dark'] as const as pref}
                  <button
                    type="button"
                    class="seg-btn"
                    class:active={themePref === pref}
                    onclick={() => setThemePreference(pref)}
                  >
                    {pref === 'system'
                      ? 'System'
                      : pref === 'light'
                        ? 'Light'
                        : 'Dark'}
                  </button>
                {/each}
              </div>
            </div>

            <div class="popover-section">
              <div class="popover-heading">Window Backdrop</div>
              <label class="checkbox-row">
                <input type="checkbox" bind:checked={forceSolidBackdrop} />
                <span>Use solid-color fallback (Win10 / transparency off)</span>
              </label>
            </div>
          </div>
        {/if}
      </div>
    </div>
  </header>

  <!-- Main Workspace Area -->
  {#if !vaultSnapshot && !scratchpadMode}
    <FirstRunScreen
      {recentVaults}
      errorMessage={onboardingError}
      onOpenVaultPath={handleOpenVaultPath}
      onCreateVault={handleCreateVault}
      onRemoveRecentVault={handleRemoveRecentVault}
      onOpenScratchpad={() => (scratchpadMode = true)}
    />
  {:else}
    <main class="workspace">
      <div class="workspace-split">
        {#if vaultSnapshot}
          <Sidebar
            snapshot={vaultSnapshot}
            activeRelPath={activeTabPath}
            width={sidebarWidth}
            collapsed={sidebarCollapsed}
            onResize={(w) => {
              sidebarWidth = w;
              try {
                localStorage.setItem('jotter.sidebarWidth', String(w));
              } catch {
                // ignore
              }
            }}
            onSelectFile={(relPath) => void openNoteInTab(relPath)}
            onCreateNote={(parent, name) => void handleCreateNote(parent, name)}
            onCreateFolder={(parent, name) => void handleCreateFolder(parent, name)}
            onRenameEntry={(oldRel, newRel) => void handleRenameEntry(oldRel, newRel)}
            onDeleteEntry={(rel) => void handleDeleteEntry(rel)}
            onSwitchVault={() => void handleSwitchVault()}
          />
        {/if}

        <section class="editor-column" aria-label="Editor workspace">
          {#if vaultSnapshot}
            <!-- Tab Bar -->
            <div class="tab-bar" role="tablist" aria-label="Open notes">
              <div class="tabs-scroll">
                {#each openTabs as tab (tab.relPath)}
                  <div
                    class="tab-item"
                    class:active={tab.relPath === activeTabPath}
                    role="tab"
                    tabindex="0"
                    aria-selected={tab.relPath === activeTabPath}
                    title={tab.relPath}
                    onclick={() => (activeTabPath = tab.relPath)}
                    onkeydown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        activeTabPath = tab.relPath;
                      }
                    }}
                    onauxclick={(e) => {
                      if (e.button === 1) void closeTab(tab.relPath, e);
                    }}
                  >
                    <span class="tab-icon">{@html fluentIcons.document16}</span>
                    <span class="tab-title">{tab.title}</span>
                    {#if tab.dirty}
                      <span
                        class="dirty-dot"
                        title="Unsaved changes (autosaving…)"
                        aria-label="Unsaved changes"
                      ></span>
                    {/if}
                    <button
                      type="button"
                      class="tab-close-btn"
                      title="Close tab (Ctrl+W)"
                      aria-label="Close {tab.title}"
                      onclick={(e) => void closeTab(tab.relPath, e)}
                    >
                      {@html fluentIcons.dismiss16}
                    </button>
                  </div>
                {/each}
              </div>

              <button
                type="button"
                class="new-tab-btn"
                title="New note (Ctrl+N)"
                aria-label="New note"
                onclick={() => {
                  const stamp = new Date()
                    .toISOString()
                    .slice(0, 19)
                    .replace(/[:T]/g, '-');
                  void handleCreateNote('', `Note ${stamp}`);
                }}
              >
                {@html fluentIcons.add20}
              </button>
            </div>
          {/if}

          {#if activeTab?.externalChangePending}
            <div class="external-reload-banner" role="alert">
              <span>
                <strong>{activeTab.relPath}</strong> was modified outside Jotter while you have unsaved edits.
              </span>
              <div class="external-reload-actions">
                <button
                  type="button"
                  class="fluent-btn primary-btn"
                  onclick={() => void reloadTabFromDisk(activeTab!.relPath)}
                >
                  Reload from disk
                </button>
                <button
                  type="button"
                  class="fluent-btn"
                  onclick={() => {
                    openTabs = openTabs.map((t) =>
                      t.relPath === activeTab!.relPath
                        ? { ...t, externalChangePending: false }
                        : t
                    );
                  }}
                >
                  Keep my edits
                </button>
              </div>
            </div>
          {/if}

          <div class="editor-card">
            {#if scratchpadMode}
              <Editor
                bind:this={editorRef}
                docId="__scratchpad__"
                initialDoc={scratchpadContent}
                {copyMode}
                onDocChange={handleEditorDocChange}
                onCopyComplete={handleCopyComplete}
                onPastePlainComplete={handlePastePlainComplete}
                onStatsChange={(s) => (docStats = s)}
                onError={(err) => setStatus(`Clipboard error: ${err}`)}
              />
            {:else if activeTab}
              <Editor
                bind:this={editorRef}
                docId={activeTab.relPath}
                initialDoc={activeTab.content}
                {copyMode}
                onDocChange={handleEditorDocChange}
                onCopyComplete={handleCopyComplete}
                onPastePlainComplete={handlePastePlainComplete}
                onStatsChange={(s) => (docStats = s)}
                onError={(err) => setStatus(`Clipboard error: ${err}`)}
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
                    void handleCreateNote('', `Note ${stamp}`);
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

      {#if showInspector && lastPayload}
        <aside class="inspector-card" aria-label="Clipboard format inspector">
          <div class="inspector-header">
            <div class="inspector-title">
              {@html fluentIcons.checkmark16}
              <span>
                {lastPayload.mode === 'plain'
                  ? 'arboard::Clipboard::set_text Plain-Only Output (Plain)'
                  : `arboard::Clipboard::set_html Dual-Format Output (${COPY_MODE_LABELS[lastPayload.mode].label})`}
              </span>
            </div>
            <span class="inspector-meta">
              {lastPayload.lineCount} lines · {lastPayload.charCount} chars
            </span>
          </div>

          <div class="inspector-grid">
            <div class="format-box">
              <div class="format-label">
                <span>text/plain (real <code>\n</code> line breaks)</span>
              </div>
              <pre class="format-code">{lastPayload.plain}</pre>
            </div>

            <div class="format-box">
              <div class="format-label">
                <span>text/html (every newline becomes <code>&lt;br&gt;</code>)</span>
              </div>
              <pre class="format-code">{lastPayload.mode === 'plain'
                  ? '(none — Plain mode writes text/plain only via set_text)'
                  : lastPayload.html}</pre>
            </div>
          </div>
        </aside>
      {/if}
    </main>
  {/if}

  <!-- Fluent Status Bar -->
  <footer class="status-bar" aria-label="Status bar">
    <div class="status-left">
      <span class="status-indicator">{statusBanner}</span>
      {#if statusTimestamp}
        <span class="status-time">({statusTimestamp})</span>
      {/if}
    </div>

    <div class="status-right">
      <span class="status-item" title="Active window backdrop">
        Backdrop: {effectiveSolidBackdrop ? 'Solid Fallback' : 'Windows 11 Mica'}
      </span>
      <span class="status-item" title="Active color theme">
        Theme: {effectiveDark ? 'Dark' : 'Light'} ({themePref})
      </span>
      {#if activeTab || scratchpadMode}
        <span class="status-item">
          {#if docStats.selectedChars > 0}
            Selected: {docStats.selectedLines} ln, {docStats.selectedChars} ch ·
          {/if}
          {docStats.lines} ln · {docStats.words} words
        </span>
      {/if}
    </div>
  </footer>
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

  .command-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--border-subtle);
    user-select: none;
    flex-shrink: 0;
  }

  .brand-group {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .brand-icon {
    display: inline-flex;
    align-items: center;
    color: var(--accent-color, var(--accent-fallback));
  }

  .brand-title {
    font-family: var(--font-display);
    font-size: var(--text-base);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .note-pill {
    font-size: var(--text-xs);
    color: var(--text-secondary);
    background: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-control);
    white-space: nowrap;
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .toolbar-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .segmented-group {
    display: inline-flex;
    align-items: center;
    background: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-control);
    padding: 2px;
    gap: 2px;
  }

  .segmented-label {
    font-size: var(--text-xs);
    color: var(--text-secondary);
    padding: 0 var(--space-2);
  }

  .seg-btn {
    min-height: 28px;
    padding: 0 var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font-size: var(--text-xs);
    font-weight: 500;
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      color var(--transition-fast),
      border-color var(--transition-fast);
  }

  .seg-btn:hover {
    background: var(--bg-control-hover);
    color: var(--text-primary);
  }

  .seg-btn.active {
    background: var(--bg-card-solid);
    color: var(--text-primary);
    border-color: var(--border-control);
    font-weight: 600;
  }

  .divider {
    width: 1px;
    height: 20px;
    background: var(--border-subtle);
    margin: 0 var(--space-1);
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

  .fluent-btn.icon-only {
    padding: 0 var(--space-2);
  }

  .fluent-btn:hover {
    background: var(--bg-control-hover);
  }

  .fluent-btn:active {
    background: var(--bg-control-active);
  }

  .fluent-btn.active-toggle {
    border-color: var(--accent-color, var(--accent-fallback));
    background: var(--selection-bg);
  }

  .fluent-btn.primary-btn {
    background: var(--accent-color, var(--accent-fallback));
    color: var(--accent-text, #ffffff);
    border-color: transparent;
  }

  .fluent-btn.primary-btn:hover {
    filter: brightness(1.06);
  }

  .settings-wrapper {
    position: relative;
  }

  .settings-popover {
    position: absolute;
    top: calc(100% + var(--space-2));
    right: 0;
    width: 320px;
    background: var(--bg-card-solid);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-card);
    padding: var(--space-4);
    z-index: 50;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .popover-section {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .popover-heading {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .radio-row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-2);
    border-radius: var(--radius-control);
    cursor: pointer;
  }

  .radio-row:hover {
    background: var(--bg-control-hover);
  }

  .radio-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .radio-title {
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .radio-desc {
    font-size: var(--text-xs);
    color: var(--text-secondary);
  }

  .theme-pills {
    display: flex;
    gap: var(--space-1);
    background: var(--bg-subtle);
    padding: 2px;
    border-radius: var(--radius-control);
    border: 1px solid var(--border-subtle);
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-min-height);
    font-size: var(--text-xs);
    cursor: pointer;
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

  .tab-bar {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-height: 36px;
    flex-shrink: 0;
  }

  .tabs-scroll {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    overflow-x: auto;
    scrollbar-width: none;
  }

  .tab-item {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-min-height);
    max-width: 220px;
    padding: 0 var(--space-2) 0 var(--space-3);
    border-radius: var(--radius-control);
    border: 1px solid var(--border-subtle);
    background: var(--bg-subtle);
    color: var(--text-secondary);
    font-size: var(--text-xs);
    cursor: pointer;
    user-select: none;
    transition:
      background-color var(--transition-fast),
      color var(--transition-fast),
      border-color var(--transition-fast);
  }

  .tab-item:hover {
    background: var(--bg-control-hover);
    color: var(--text-primary);
  }

  .tab-item.active {
    background: var(--bg-card-solid);
    color: var(--text-primary);
    border-color: var(--border-control);
    font-weight: 600;
  }

  .tab-icon {
    display: inline-flex;
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .tab-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dirty-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent-color, var(--accent-fallback));
    flex-shrink: 0;
  }

  .tab-close-btn {
    width: 22px;
    height: 22px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    padding: 0;
  }

  .tab-close-btn:hover {
    background: var(--bg-control-hover);
    color: var(--text-primary);
  }

  .new-tab-btn {
    min-width: var(--control-min-height);
    min-height: var(--control-min-height);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-control);
    border: 1px solid var(--border-subtle);
    background: var(--bg-subtle);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background-color var(--transition-fast);
  }

  .new-tab-btn:hover {
    background: var(--bg-control-hover);
    color: var(--text-primary);
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

  .inspector-card {
    height: 184px;
    flex-shrink: 0;
    background: var(--bg-card);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-card);
    padding: var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    overflow: hidden;
  }

  .solid-backdrop .inspector-card {
    background: var(--bg-card-solid);
  }

  .inspector-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: var(--text-xs);
  }

  .inspector-title {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: 600;
    color: var(--status-ok);
  }

  .inspector-meta {
    color: var(--text-secondary);
  }

  .inspector-grid {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
    min-height: 0;
  }

  .format-box {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-height: 0;
    background: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-control);
    padding: var(--space-2);
  }

  .format-label {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-secondary);
  }

  .format-label code {
    font-family: var(--font-mono);
    background: var(--bg-code);
    padding: 1px 4px;
    border-radius: var(--radius-control);
  }

  .format-code {
    margin: 0;
    flex: 1;
    overflow: auto;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--text-primary);
  }

  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-1) var(--space-4);
    min-height: 28px;
    border-top: 1px solid var(--border-subtle);
    font-size: var(--text-xs);
    color: var(--text-secondary);
    user-select: none;
    flex-shrink: 0;
  }

  .status-left,
  .status-right {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .status-indicator {
    color: var(--text-primary);
    font-weight: 500;
  }

  .status-time {
    color: var(--text-muted);
  }

  .status-item {
    white-space: nowrap;
  }
</style>
