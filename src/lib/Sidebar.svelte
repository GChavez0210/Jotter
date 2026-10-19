<script lang="ts">
  import CalendarView from './CalendarView.svelte';
  import FileTreeItem from './FileTreeItem.svelte';
  import SearchPanel from './SearchPanel.svelte';
  import TagBrowserPanel from './TagBrowserPanel.svelte';
  import { fluentIcons } from './icons';
  import { settingsStore } from './stores/settings.svelte';
  import type { RecentVault, VaultNode, VaultSnapshot } from './types';
  import { validateWindowsEntryName } from './validation';

  interface Props {
    snapshot: VaultSnapshot;
    recentVaults: RecentVault[];
    onOpenVault: (path: string) => void;
    activeRelPath: string | null;
    width: number;
    collapsed: boolean;
    onResize: (newWidth: number) => void;
    onSelectFile: (relPath: string) => void;
    onCreateNote: (parentFolderRel: string, noteName: string) => void;
    onCreateFolder: (parentFolderRel: string, folderName: string) => void;
    onRenameEntry: (oldRelPath: string, newRelPath: string) => void;
    onDeleteEntry: (relPath: string) => void;
    onSwitchVault: () => void;
  }

  let {
    snapshot,
    recentVaults,
    onOpenVault,
    activeRelPath,
    width,
    collapsed,
    onResize,
    onSelectFile,
    onCreateNote,
    onCreateFolder,
    onRenameEntry,
    onDeleteEntry,
    onSwitchVault,
  }: Props = $props();

  let expandedPaths: Record<string, boolean> = $state({});
  let renamingRelPath: string | null = $state(null);

  let creatingKind: 'note' | 'folder' | null = $state(null);
  let creatingParentRel = $state('');
  let creatingDraftName = $state('');

  let creatingValidationError = $derived(
    creatingKind ? validateWindowsEntryName(creatingDraftName) : null
  );

  let contextMenu: {
    x: number;
    y: number;
    node: VaultNode | null;
  } | null = $state(null);

  let pendingDeleteNode: VaultNode | null = $state(null);
  let isResizing = $state(false);
  let vaultMenuOpen = $state(false);

  function samePath(a: string, b: string): boolean {
    const norm = (p: string) => p.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
    return norm(a) === norm(b);
  }

  function chooseVault(path: string) {
    vaultMenuOpen = false;
    if (!samePath(path, snapshot.rootPath)) {
      onOpenVault(path);
    }
  }

  function toggleFolder(relPath: string) {
    expandedPaths = {
      ...expandedPaths,
      [relPath]: !expandedPaths[relPath],
    };
  }

  function startNewEntry(kind: 'note' | 'folder', parentRel = '') {
    contextMenu = null;
    creatingKind = kind;
    creatingParentRel = parentRel;
    creatingDraftName = kind === 'note' ? 'Untitled' : 'New Folder';
    if (parentRel) {
      expandedPaths = { ...expandedPaths, [parentRel]: true };
    }
  }

  function submitNewEntry(e: SubmitEvent) {
    e.preventDefault();
    if (!creatingKind) return;
    const err = validateWindowsEntryName(creatingDraftName);
    if (err) return;

    const name = creatingDraftName.trimStart();
    const kind = creatingKind;
    const parent = creatingParentRel;
    creatingKind = null;
    if (kind === 'note') {
      onCreateNote(parent, name);
    } else {
      onCreateFolder(parent, name);
    }
  }

  function handleStartRename(node: VaultNode) {
    contextMenu = null;
    renamingRelPath = node.relPath;
  }

  function handleCommitRename(node: VaultNode, newName: string) {
    const err = validateWindowsEntryName(newName);
    if (err) return;
    renamingRelPath = null;
    const cleanName = newName.trimStart();
    const slashIdx = node.relPath.lastIndexOf('/');
    const parentPrefix = slashIdx >= 0 ? `${node.relPath.slice(0, slashIdx)}/` : '';
    const newRel = `${parentPrefix}${cleanName}`;
    if (newRel !== node.relPath) {
      onRenameEntry(node.relPath, newRel);
    }
  }

  function handleContextMenu(e: MouseEvent, node: VaultNode | null) {
    e.preventDefault();
    e.stopPropagation();
    contextMenu = {
      x: Math.min(e.clientX, window.innerWidth - 190),
      y: Math.min(e.clientY, window.innerHeight - 180),
      node,
    };
  }

  function handleResizePointerDown(e: PointerEvent) {
    e.preventDefault();
    isResizing = true;

    const handleMove = (moveEvent: PointerEvent) => {
      const clamped = Math.max(180, Math.min(480, moveEvent.clientX));
      onResize(clamped);
    };

    const handleUp = () => {
      isResizing = false;
      window.removeEventListener('pointermove', handleMove);
      window.removeEventListener('pointerup', handleUp);
    };

    window.addEventListener('pointermove', handleMove);
    window.addEventListener('pointerup', handleUp);
  }
</script>

<svelte:window
  onclick={() => {
    if (contextMenu) contextMenu = null;
    if (vaultMenuOpen) vaultMenuOpen = false;
  }}
  onkeydown={(e) => {
    if (e.key === 'Escape' && vaultMenuOpen) {
      vaultMenuOpen = false;
    }
  }}
/>

{#if !collapsed}
  <aside
    class="sidebar-pane"
    style:width="{width}px"
    aria-label="Vault file tree"
    oncontextmenu={(e) => handleContextMenu(e, null)}
  >
    <div class="sidebar-header">
      <button
        type="button"
        class="vault-switcher-btn"
        title="Switch vault ({snapshot.rootPath})"
        aria-haspopup="menu"
        aria-expanded={vaultMenuOpen}
        onclick={(e) => {
          e.stopPropagation();
          contextMenu = null;
          vaultMenuOpen = !vaultMenuOpen;
        }}
      >
        <span class="vault-folder-icon">{@html fluentIcons.folder16}</span>
        <span class="vault-name">{snapshot.name}</span>
        <span class="vault-chevron">{@html fluentIcons.chevronDown16}</span>
      </button>

      <div class="sidebar-actions">
        <button
          type="button"
          class="fluent-icon-btn"
          title="New note (Ctrl+N)"
          aria-label="New note"
          onclick={() => startNewEntry('note', '')}
        >
          {@html fluentIcons.documentAdd16}
        </button>
        <button
          type="button"
          class="fluent-icon-btn"
          title="New folder"
          aria-label="New folder"
          onclick={() => startNewEntry('folder', '')}
        >
          {@html fluentIcons.folderAdd16}
        </button>
      </div>
    </div>

    {#if vaultMenuOpen}
      <div class="vault-menu" role="menu" aria-label="Switch vault">
        {#each recentVaults as vault (vault.path)}
          {@const isCurrent = samePath(vault.path, snapshot.rootPath)}
          <button
            type="button"
            class="vault-menu-item"
            class:current={isCurrent}
            role="menuitemradio"
            aria-checked={isCurrent}
            title={vault.path}
            onclick={(e) => {
              e.stopPropagation();
              chooseVault(vault.path);
            }}
          >
            <span class="vault-menu-check">{isCurrent ? '✓' : ''}</span>
            <span class="vault-menu-text">
              <span class="vault-menu-name">{vault.name}</span>
              <span class="vault-menu-path">{vault.path}</span>
            </span>
          </button>
        {/each}
        <div class="ctx-sep" role="separator"></div>
        <button
          type="button"
          class="vault-menu-item"
          role="menuitem"
          onclick={(e) => {
            e.stopPropagation();
            vaultMenuOpen = false;
            onSwitchVault();
          }}
        >
          <span class="vault-menu-check"></span>
          <span class="vault-menu-text">
            <span class="vault-menu-name">Open or create another vault…</span>
          </span>
        </button>
      </div>
    {/if}

    <div class="sidebar-nav-tabs" role="tablist" aria-label="Sidebar views">
      <button
        type="button"
        role="tab"
        aria-selected={settingsStore.leftSidebarView === 'files'}
        class="nav-tab-btn"
        class:active={settingsStore.leftSidebarView === 'files'}
        title="Files"
        onclick={() => settingsStore.setLeftSidebarView('files')}
      >
        {@html fluentIcons.folder16}
        <span>Files</span>
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={settingsStore.leftSidebarView === 'search'}
        class="nav-tab-btn"
        class:active={settingsStore.leftSidebarView === 'search'}
        title="Full-text search (Ctrl+Shift+F)"
        onclick={() => settingsStore.setLeftSidebarView('search')}
      >
        {@html fluentIcons.search16}
        <span>Search</span>
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={settingsStore.leftSidebarView === 'tags'}
        class="nav-tab-btn"
        class:active={settingsStore.leftSidebarView === 'tags'}
        title="Tag browser"
        onclick={() => settingsStore.setLeftSidebarView('tags')}
      >
        {@html fluentIcons.tag16}
        <span>Tags</span>
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={settingsStore.leftSidebarView === 'calendar'}
        class="nav-tab-btn"
        class:active={settingsStore.leftSidebarView === 'calendar'}
        title="Calendar view"
        onclick={() => settingsStore.setLeftSidebarView('calendar')}
      >
        {@html fluentIcons.calendar16}
        <span>Calendar</span>
      </button>
    </div>

    {#if settingsStore.leftSidebarView === 'files'}
      {#if creatingKind}
        <form class="create-entry-bar" onsubmit={submitNewEntry}>
          <div class="create-entry-label">
            New {creatingKind}
            {creatingParentRel ? `in ${creatingParentRel}` : 'in root'}:
          </div>
          <div class="create-entry-controls">
            <!-- svelte-ignore a11y_autofocus -->
            <input
              type="text"
              class="create-input"
              class:invalid={Boolean(creatingValidationError)}
              aria-invalid={Boolean(creatingValidationError)}
              bind:value={creatingDraftName}
              autofocus
              onkeydown={(e) => {
                if (e.key === 'Escape') creatingKind = null;
              }}
            />
            <button
              type="submit"
              class="mini-btn primary"
              disabled={Boolean(creatingValidationError)}
            >
              Create
            </button>
            <button
              type="button"
              class="mini-btn"
              onclick={() => (creatingKind = null)}
            >
              Cancel
            </button>
          </div>
          {#if creatingValidationError}
            <div class="inline-error" role="alert">{creatingValidationError}</div>
          {/if}
        </form>
      {/if}

      {#if pendingDeleteNode}
        <div class="delete-confirm-bar" role="alertdialog" aria-label="Confirm move to Recycle Bin">
          <div class="delete-confirm-text">
            Move <strong>{pendingDeleteNode.name}</strong> to the Recycle Bin?
          </div>
          <div class="delete-confirm-actions">
            <button
              type="button"
              class="mini-btn danger"
              onclick={() => {
                const target = pendingDeleteNode!;
                pendingDeleteNode = null;
                onDeleteEntry(target.relPath);
              }}
            >
              Move to Recycle Bin
            </button>
            <button
              type="button"
              class="mini-btn"
              onclick={() => (pendingDeleteNode = null)}
            >
              Cancel
            </button>
          </div>
        </div>
      {/if}

      <div class="tree-scroll">
        {#if snapshot.tree.length === 0}
          <div class="empty-tree">
            <p>No Markdown notes in this vault yet.</p>
            <button
              type="button"
              class="new-first-note-btn"
              onclick={() => startNewEntry('note', '')}
            >
              {@html fluentIcons.documentAdd16}
              <span>Create first note</span>
            </button>
          </div>
        {:else}
          <ul class="tree-root" role="tree" aria-label="Notes and folders">
            {#each snapshot.tree as node (node.relPath)}
              <FileTreeItem
                {node}
                depth={0}
                {activeRelPath}
                {expandedPaths}
                {renamingRelPath}
                {onSelectFile}
                onToggleFolder={toggleFolder}
                onStartRename={handleStartRename}
                onCommitRename={handleCommitRename}
                onCancelRename={() => (renamingRelPath = null)}
                onContextMenu={(e, n) => handleContextMenu(e, n)}
                onRequestDelete={(n) => {
                  contextMenu = null;
                  pendingDeleteNode = n;
                }}
              />
            {/each}
          </ul>
        {/if}
      </div>
    {:else if settingsStore.leftSidebarView === 'search'}
      <SearchPanel />
    {:else if settingsStore.leftSidebarView === 'tags'}
      <TagBrowserPanel />
    {:else if settingsStore.leftSidebarView === 'calendar'}
      <CalendarView />
    {/if}

    <!-- Resize Sash -->
    <div
      class="resize-handle"
      class:dragging={isResizing}
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize sidebar"
      onpointerdown={handleResizePointerDown}
    ></div>
  </aside>
{/if}

{#if contextMenu}
  {@const targetNode = contextMenu.node}
  {@const folderTarget =
    targetNode?.kind === 'folder'
      ? targetNode.relPath
      : targetNode?.relPath.includes('/')
        ? targetNode.relPath.slice(0, targetNode.relPath.lastIndexOf('/'))
        : ''}
  <div
    class="fluent-context-menu"
    style:left="{contextMenu.x}px"
    style:top="{contextMenu.y}px"
    role="menu"
  >
    <button
      type="button"
      class="ctx-item"
      role="menuitem"
      onclick={() => startNewEntry('note', folderTarget)}
    >
      {@html fluentIcons.documentAdd16}
      <span>New note{folderTarget ? ` in ${folderTarget}` : ''}</span>
    </button>
    <button
      type="button"
      class="ctx-item"
      role="menuitem"
      onclick={() => startNewEntry('folder', folderTarget)}
    >
      {@html fluentIcons.folderAdd16}
      <span>New folder{folderTarget ? ` in ${folderTarget}` : ''}</span>
    </button>
    {#if targetNode}
      <div class="ctx-sep" role="separator"></div>
      <button
        type="button"
        class="ctx-item"
        role="menuitem"
        onclick={() => handleStartRename(targetNode)}
      >
        {@html fluentIcons.rename16}
        <span>Rename (F2)</span>
      </button>
      <button
        type="button"
        class="ctx-item danger-item"
        role="menuitem"
        onclick={() => {
          contextMenu = null;
          pendingDeleteNode = targetNode;
        }}
      >
        {@html fluentIcons.delete16}
        <span>Move to Recycle Bin (Del)</span>
      </button>
    {/if}
  </div>
{/if}

<style>
  .sidebar-pane {
    position: relative;
    height: 100%;
    flex-shrink: 0;
    background: var(--bg-card);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-card);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    user-select: none;
    transition: background-color var(--transition-fast);
  }

  .sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-1);
    padding: var(--space-2);
    border-bottom: 1px solid var(--border-subtle);
  }

  .sidebar-nav-tabs {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 2px;
    padding: 4px var(--space-2);
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-subtle);
  }

  .nav-tab-btn {
    min-height: var(--control-min-height);
    padding: 0 4px;
    border-radius: var(--radius-control);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 500;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      color var(--transition-fast),
      border-color var(--transition-fast);
  }

  .nav-tab-btn:hover {
    background: var(--bg-control-hover);
    color: var(--text-primary);
  }

  .nav-tab-btn.active {
    background: var(--bg-card-solid);
    color: var(--text-primary);
    border-color: var(--border-control);
    font-weight: 600;
  }

  .vault-switcher-btn {
    flex: 1;
    min-width: 0;
    min-height: var(--control-min-height);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-2);
    border-radius: var(--radius-control);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-primary);
    font-size: var(--text-xs);
    font-weight: 600;
    cursor: pointer;
    transition: background-color var(--transition-fast);
  }

  .vault-switcher-btn:hover {
    background: var(--bg-control-hover);
  }

  .vault-folder-icon {
    display: inline-flex;
    color: var(--accent-color, var(--accent-fallback));
    flex-shrink: 0;
  }

  .vault-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    text-align: left;
  }

  .vault-chevron {
    display: inline-flex;
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .vault-menu {
    position: absolute;
    top: 52px;
    left: var(--space-2);
    right: var(--space-2);
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-1);
    max-height: 60%;
    overflow-y: auto;
    background: var(--bg-card-solid);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-card);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.18);
  }

  .vault-menu-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-min-height);
    padding: var(--space-1) var(--space-2);
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-primary);
    font-size: var(--text-xs);
    text-align: left;
    cursor: pointer;
  }

  .vault-menu-item:hover {
    background: var(--bg-control-hover);
  }

  .vault-menu-item.current {
    font-weight: 600;
  }

  .vault-menu-check {
    width: 14px;
    flex-shrink: 0;
    color: var(--accent-color, var(--accent-fallback));
  }

  .vault-menu-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .vault-menu-name,
  .vault-menu-path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .vault-menu-path {
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 400;
  }

  .sidebar-actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .fluent-icon-btn {
    min-width: var(--control-min-height);
    min-height: var(--control-min-height);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-control);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      color var(--transition-fast);
  }

  .fluent-icon-btn:hover {
    background: var(--bg-control-hover);
    color: var(--text-primary);
  }

  .create-entry-bar,
  .delete-confirm-bar {
    padding: var(--space-2);
    background: var(--bg-subtle);
    border-bottom: 1px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .create-entry-label,
  .delete-confirm-text {
    font-size: var(--text-xs);
    color: var(--text-secondary);
  }

  .create-entry-controls,
  .delete-confirm-actions {
    display: flex;
    gap: var(--space-1);
  }

  .create-input {
    flex: 1;
    min-width: 0;
    min-height: var(--control-min-height);
    padding: 0 var(--space-2);
    border-radius: var(--radius-control);
    border: 1px solid var(--border-control);
    background: var(--bg-card-solid);
    color: var(--text-primary);
    font-size: var(--text-xs);
  }

  .create-input.invalid {
    border-color: var(--status-danger);
  }

  .inline-error {
    font-size: var(--text-xs);
    color: var(--text-danger);
    line-height: 1.3;
  }

  .mini-btn {
    min-height: var(--control-min-height);
    padding: 0 var(--space-2);
    border-radius: var(--radius-control);
    border: 1px solid var(--border-control);
    background: var(--bg-control);
    color: var(--text-primary);
    font-size: var(--text-xs);
    cursor: pointer;
  }

  .mini-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .mini-btn.primary {
    background: var(--accent-color, var(--accent-fallback));
    color: var(--accent-text, #ffffff);
    border-color: transparent;
  }

  .mini-btn.danger {
    background: var(--status-danger);
    color: #ffffff;
    border-color: transparent;
  }

  .tree-scroll {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: var(--space-1);
  }

  .tree-root {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .empty-tree {
    padding: var(--space-4) var(--space-3);
    text-align: center;
    color: var(--text-secondary);
    font-size: var(--text-xs);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
  }

  .new-first-note-btn {
    min-height: var(--control-min-height);
    padding: 0 var(--space-3);
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    border-radius: var(--radius-control);
    border: 1px solid var(--border-control);
    background: var(--bg-control);
    color: var(--text-primary);
    font-size: var(--text-xs);
    cursor: pointer;
  }

  .resize-handle {
    position: absolute;
    top: 0;
    right: 0;
    width: 5px;
    height: 100%;
    cursor: col-resize;
    background: transparent;
    transition: background-color var(--transition-fast);
  }

  .resize-handle:hover,
  .resize-handle.dragging {
    background: var(--accent-color, var(--accent-fallback));
  }

  .fluent-context-menu {
    position: fixed;
    min-width: 196px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-card);
    padding: var(--space-1);
    z-index: 100;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .ctx-item {
    min-height: var(--control-min-height);
    padding: 0 var(--space-3);
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-primary);
    font-size: var(--text-xs);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    text-align: left;
    cursor: pointer;
  }

  .ctx-item:hover {
    background: var(--bg-control-hover);
  }

  .ctx-item.danger-item {
    color: var(--text-danger);
  }

  .ctx-sep {
    height: 1px;
    background: var(--border-subtle);
    margin: var(--space-1) 0;
  }
</style>
