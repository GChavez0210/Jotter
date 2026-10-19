<script lang="ts">
  import CalendarView from './CalendarView.svelte';
  import FileTreeItem from './FileTreeItem.svelte';
  import SearchPanel from './SearchPanel.svelte';
  import TagBrowserPanel from './TagBrowserPanel.svelte';
  import Icon from './ui/Icon.svelte';
  import Button from './ui/Button.svelte';
  import IconButton from './ui/IconButton.svelte';
  import type { IconName } from './ui/icons';
  import { settingsStore } from './stores/settings.svelte';
  import type { LeftSidebarView, RecentVault, VaultNode, VaultSnapshot } from './types';
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

  // macOS draws its traffic lights over the top-left corner of the window.
  const isMac = navigator.userAgent.includes('Mac');

  const VIEWS: { id: LeftSidebarView; label: string; icon: IconName; hint?: string }[] = [
    { id: 'files', label: 'Notes', icon: 'notebook' },
    { id: 'search', label: 'Full-text search', icon: 'magnifying-glass', hint: 'Ctrl+Shift+F' },
    { id: 'tags', label: 'Tags', icon: 'tag' },
    { id: 'calendar', label: 'Calendar', icon: 'calendar-blank' },
  ];

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
    <div class="sidebar-header" class:mac-inset={isMac} data-tauri-drag-region>
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
        <span class="vault-mark" aria-hidden="true">{snapshot.name.slice(0, 1).toUpperCase()}</span>
        <span class="vault-name">{snapshot.name}</span>
        <span class="vault-chevron"><Icon name="caret-down" size={12} /></span>
      </button>

      <IconButton
        icon="sidebar-simple"
        label="Hide sidebar"
        tooltip="Hide sidebar (Ctrl+\)"
        onclick={() => settingsStore.toggleSidebar()}
      />
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

    <button
      type="button"
      class="search-btn"
      title="Find a note by name (Ctrl+O)"
      onclick={() => (settingsStore.showQuickSwitcher = true)}
    >
      <Icon name="magnifying-glass" />
      <span>Search notes</span>
      <kbd>Ctrl O</kbd>
    </button>

    <nav class="library" aria-label="Sidebar views">
      {#each VIEWS as view (view.id)}
        <button
          type="button"
          class="nav-row"
          class:active={settingsStore.leftSidebarView === view.id}
          aria-current={settingsStore.leftSidebarView === view.id ? 'page' : undefined}
          title={view.hint ? `${view.label} (${view.hint})` : view.label}
          onclick={() => settingsStore.setLeftSidebarView(view.id)}
        >
          <Icon name={view.icon} />
          <span class="nav-label">{view.label}</span>
          {#if view.id === 'files'}
            <span class="nav-count">{snapshot.noteCount}</span>
          {/if}
        </button>
      {/each}
    </nav>

    {#if settingsStore.leftSidebarView === 'files'}
      <div class="section-label">
        <span>Folders</span>
        <IconButton
          icon="file-plus"
          size="sm"
          label="New note"
          onclick={() => startNewEntry('note', '')}
        />
        <IconButton
          icon="folder-simple-plus"
          size="sm"
          label="New folder"
          onclick={() => startNewEntry('folder', '')}
        />
      </div>

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
              <Icon name="file-plus" />
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

    <div class="sidebar-footer">
      <Button variant="primary" icon="note-pencil" class="new-note" onclick={() => startNewEntry('note', '')}>
        New note <kbd>Ctrl N</kbd>
      </Button>
      <IconButton
        icon="gear-six"
        label="Settings"
        tooltip="Settings (Ctrl+,)"
        onclick={() => (settingsStore.showSettings = true)}
      />
    </div>

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
      <Icon name="file-plus" />
      <span>New note{folderTarget ? ` in ${folderTarget}` : ''}</span>
    </button>
    <button
      type="button"
      class="ctx-item"
      role="menuitem"
      onclick={() => startNewEntry('folder', folderTarget)}
    >
      <Icon name="folder-simple-plus" />
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
        <Icon name="pencil-simple-line" />
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
        <Icon name="trash" />
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
    background: var(--surface-app);
    border-right: 1px solid var(--line-subtle);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    user-select: none;
    transition: background-color var(--transition-fast);
  }

  /* Mica, when enabled, shows through the sidebar only. */
  :global([data-backdrop='mica']) .sidebar-pane {
    background: var(--surface-mica);
  }

  .sidebar-header {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: var(--bar-height);
    flex: none;
    padding: 0 var(--space-2);
  }

  .sidebar-header.mac-inset {
    padding-left: 76px;
  }

  .search-btn {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 30px;
    margin: var(--space-1) var(--space-3) var(--space-2);
    padding: 0 var(--space-2);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-card);
    background: var(--surface-editor);
    color: var(--text-muted);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .search-btn:hover {
    border-color: var(--line-strong);
  }

  .search-btn span {
    flex: 1;
  }

  .library {
    display: flex;
    flex-direction: column;
    padding: 0 var(--space-2) var(--space-2);
  }

  .nav-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--row-height);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .nav-row:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .nav-row.active {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 500;
  }

  .nav-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .nav-count {
    color: var(--text-muted);
    font-size: var(--fs-12);
    font-variant-numeric: tabular-nums;
  }

  .section-label {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: var(--space-2) var(--space-2) var(--space-1) var(--space-4);
    color: var(--text-muted);
    font: 600 var(--fs-11) / 1 var(--font-ui);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .section-label span {
    flex: 1;
  }

  .sidebar-footer {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex: none;
    padding: var(--space-2);
    border-top: 1px solid var(--line-subtle);
  }

  .sidebar-footer :global(.new-note) {
    flex: 1;
    justify-content: flex-start;
    min-height: 32px;
  }

  .sidebar-footer :global(.new-note .label) {
    flex: 1;
    display: flex;
    align-items: center;
  }

  .sidebar-footer kbd {
    margin-left: auto;
    border-color: currentColor;
    background: transparent;
    color: inherit;
    opacity: 0.75;
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
    font-size: var(--fs-13);
    font-weight: 600;
    cursor: pointer;
    transition: background-color var(--transition-fast);
  }

  .vault-switcher-btn:hover {
    background: var(--surface-hover);
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

  .vault-mark {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    flex: none;
    border-radius: 5px;
    background: var(--accent);
    color: var(--on-accent);
    font: 600 var(--fs-11) / 1 var(--font-display);
  }

  .vault-menu {
    position: absolute;
    top: var(--bar-height);
    left: var(--space-2);
    right: var(--space-2);
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-1);
    max-height: 60%;
    overflow-y: auto;
    background: var(--surface-raised);
    border: 1px solid var(--line-control);
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
    font-size: var(--fs-12);
    text-align: left;
    cursor: pointer;
  }

  .vault-menu-item:hover {
    background: var(--surface-hover);
  }

  .vault-menu-item.current {
    font-weight: 600;
  }

  .vault-menu-check {
    width: 14px;
    flex-shrink: 0;
    color: var(--accent);
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
    font-size: var(--fs-11);
    font-weight: 400;
  }

  .delete-confirm-bar {
    padding: var(--space-2);
    background: var(--surface-subtle);
    border-bottom: 1px solid var(--line-subtle);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .create-entry-label,
  .delete-confirm-text {
    font-size: var(--fs-12);
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
    border: 1px solid var(--line-control);
    background: var(--surface-raised);
    color: var(--text-primary);
    font-size: var(--fs-12);
  }

  .create-input.invalid {
    border-color: var(--status-danger);
  }

  .inline-error {
    font-size: var(--fs-12);
    color: var(--text-danger);
    line-height: 1.3;
  }

  .mini-btn {
    min-height: var(--control-min-height);
    padding: 0 var(--space-2);
    border-radius: var(--radius-control);
    border: 1px solid var(--line-control);
    background: var(--surface-field);
    color: var(--text-primary);
    font-size: var(--fs-12);
    cursor: pointer;
  }

  .mini-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .mini-btn.primary {
    background: var(--accent);
    color: var(--on-accent);
    border-color: transparent;
  }

  .mini-btn.danger {
    background: var(--status-danger);
    color: var(--on-danger);
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
    font-size: var(--fs-12);
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
    border: 1px solid var(--line-control);
    background: var(--surface-field);
    color: var(--text-primary);
    font-size: var(--fs-12);
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
    background: var(--accent);
  }

  .fluent-context-menu {
    position: fixed;
    min-width: 196px;
    background: var(--surface-raised);
    border: 1px solid var(--line-control);
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
    font-size: var(--fs-12);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    text-align: left;
    cursor: pointer;
  }

  .ctx-item:hover {
    background: var(--surface-hover);
  }

  .ctx-item.danger-item {
    color: var(--text-danger);
  }

  .ctx-sep {
    height: 1px;
    background: var(--line-subtle);
    margin: var(--space-1) 0;
  }
</style>
