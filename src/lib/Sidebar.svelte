<script lang="ts">
  import LazyLoadError from './LazyLoadError.svelte';
  import { shortcutLabel } from './platform';
  import FileTreeItem from './FileTreeItem.svelte';
  import TagsLinksView from './TagsLinksView.svelte';
  import Icon from './ui/Icon.svelte';
  import Button from './ui/Button.svelte';
  import IconButton from './ui/IconButton.svelte';
  import type { IconName } from './ui/icons';
  import { settingsStore } from './stores/settings.svelte';
  import { designTokens } from './designTokens';
  import { maxSidebarWidth } from './layout';
  import { vaultStore } from './stores/vault.svelte';
  import type { LeftSidebarView, RecentVault, VaultNode, VaultSnapshot } from './types';
  import { validateEntryName } from './validation';
  import { NOTE_DRAG_TYPE, noteMoveDestination } from './noteMove';
  import { m, SLOT, splitSlots, trashName } from './i18n.svelte';

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
    creatingKind ? validateEntryName(creatingDraftName) : null
  );

  let contextMenu: {
    x: number;
    y: number;
    node: VaultNode | null;
  } | null = $state(null);

  let pendingDeleteNode: VaultNode | null = $state(null);
  let isResizing = $state(false);
  let vaultMenuOpen = $state(false);
  let draggedNote = $state<{ path: string; root: string } | null>(null);
  let dropTargetRel = $state<string | null>(null);
  let movingNote = $state(false);

  function endNoteDrag() {
    draggedNote = null;
    dropTargetRel = null;
  }

  function startNoteDrag(event: DragEvent, node: VaultNode) {
    if (movingNote || node.kind !== 'file' || !event.dataTransfer) {
      event.preventDefault();
      return;
    }
    contextMenu = null;
    draggedNote = { path: node.relPath, root: snapshot.rootPath };
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData(NOTE_DRAG_TYPE, node.relPath);
  }

  function canDropNote(event: DragEvent, folder: string) {
    if (!draggedNote || draggedNote.root !== snapshot.rootPath || movingNote ||
        !event.dataTransfer?.types.includes(NOTE_DRAG_TYPE)) return false;
    try {
      return noteMoveDestination(snapshot.tree, draggedNote.path, folder) !== null;
    } catch {
      // Accept a conflicting destination so dropping can explain why it failed.
      return true;
    }
  }

  function dragOverFolder(event: DragEvent, folder: string) {
    if (!canDropNote(event, folder)) return;
    event.preventDefault();
    event.stopPropagation();
    event.dataTransfer!.dropEffect = 'move';
    dropTargetRel = folder;
  }

  function leaveFolder(event: DragEvent) {
    if (event.relatedTarget instanceof Node && (event.currentTarget as HTMLElement).contains(event.relatedTarget)) return;
    dropTargetRel = null;
  }

  async function dropNote(event: DragEvent, folder: string) {
    if (!canDropNote(event, folder) || !draggedNote) return;
    event.preventDefault();
    event.stopPropagation();
    const source = draggedNote;
    endNoteDrag();
    movingNote = true;
    try {
      if (await vaultStore.moveNoteIntoFolder(source.path, folder, source.root)) {
        if (folder) expandedPaths = { ...expandedPaths, [folder]: true };
      }
    } finally {
      movingNote = false;
    }
  }

  // macOS draws its traffic lights over the top-left corner of the window.
  const isMac = navigator.userAgent.includes('Mac');

  const BASE_VIEWS = $derived<{ id: LeftSidebarView; label: string; icon: IconName; hint?: string }[]>([
    { id: 'files', label: m.sidebar_view_notes(), icon: 'notebook' },
    { id: 'search', label: m.sidebar_view_search(), icon: 'magnifying-glass', hint: shortcutLabel('Ctrl+Shift+F') },
    { id: 'tags', label: m.sidebar_view_tags(), icon: 'tag' },
  ]);

  // The conflicts view appears only while the vault has conflict copies to resolve.
  let VIEWS = $derived(
    vaultStore.syncConflicts.length > 0 || settingsStore.leftSidebarView === 'conflicts'
      ? [...BASE_VIEWS, { id: 'conflicts' as const, label: m.sidebar_view_conflicts(), icon: 'warning' as const }]
      : BASE_VIEWS
  );

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
    creatingDraftName = kind === 'note' ? m.note_untitled() : m.sidebar_new_folder_name();
    if (parentRel) {
      expandedPaths = { ...expandedPaths, [parentRel]: true };
    }
  }

  function submitNewEntry(e: SubmitEvent) {
    e.preventDefault();
    if (!creatingKind) return;
    const err = validateEntryName(creatingDraftName);
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
    const err = validateEntryName(newName);
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
      const widest = maxSidebarWidth(settingsStore.windowWidth);
      const clamped = Math.max(
        designTokens.layout.sidebarMinWidth,
        Math.min(widest, moveEvent.clientX)
      );
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

  const createEntryLabel = $derived.by(() => {
    if (creatingKind === 'folder') {
      return creatingParentRel ? m.sidebar_new_folder_in({ folder: creatingParentRel }) : m.sidebar_new_folder_in_root();
    }
    return creatingParentRel ? m.sidebar_new_note_in({ folder: creatingParentRel }) : m.sidebar_new_note_in_root();
  });
  const deleteConfirmParts = $derived(splitSlots(m.sidebar_confirm_move_to_trash({ name: SLOT, trash: trashName() })));

  /** Pinned notes keep their pin order; the label is the file name without its extension. */
  let pinnedNotes = $derived(
    vaultStore.visiblePinnedPaths.map((relPath) => ({
      relPath,
      title: relPath.slice(relPath.lastIndexOf('/') + 1).replace(/\.md$/i, ''),
    }))
  );
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
    aria-label={m.sidebar_file_tree()}
    oncontextmenu={(e) => handleContextMenu(e, null)}
  >
    <div class="sidebar-header" class:mac-inset={isMac} data-tauri-drag-region>
      <button
        type="button"
        class="vault-switcher-btn"
        title={m.sidebar_switch_vault_title({ path: snapshot.rootPath })}
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
        label={m.sidebar_hide()}
        tooltip={m.sidebar_hide_tooltip({ shortcut: shortcutLabel('Ctrl+\\') })}
        onclick={() => settingsStore.toggleSidebar()}
      />
    </div>

    {#if vaultMenuOpen}
      <div class="vault-menu" role="menu" aria-label={m.sidebar_switch_vault()}>
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
            <span class="vault-menu-name">{m.sidebar_other_vault()}</span>
          </span>
        </button>
      </div>
    {/if}

    <button
      type="button"
      class="search-btn"
      title={m.sidebar_find_note_title({ shortcut: shortcutLabel('Ctrl+O') })}
      onclick={() => (settingsStore.showQuickSwitcher = true)}
    >
      <Icon name="magnifying-glass" />
      <span>{m.sidebar_search_notes()}</span>
      <kbd>{shortcutLabel('Ctrl O')}</kbd>
    </button>

    <nav class="library" aria-label={m.sidebar_views()}>
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
        <span class="nav-switch" role="radiogroup" aria-label={m.sidebar_organize_by()}>
          <button
            type="button"
            role="radio"
            aria-checked={settingsStore.notesNavigation === 'folders'}
            class:on={settingsStore.notesNavigation === 'folders'}
            onclick={() => settingsStore.setNotesNavigation('folders')}
          >{m.sidebar_folders()}</button>
          <button
            type="button"
            role="radio"
            aria-checked={settingsStore.notesNavigation === 'tags'}
            class:on={settingsStore.notesNavigation === 'tags'}
            onclick={() => settingsStore.setNotesNavigation('tags')}
          >{m.sidebar_tags_links()}</button>
        </span>
        <IconButton
          icon="file-plus"
          size="sm"
          label={m.common_new_note()}
          onclick={() => startNewEntry('note', '')}
        />
        <IconButton
          icon="folder-simple-plus"
          size="sm"
          label={m.common_new_folder()}
          onclick={() => startNewEntry('folder', '')}
        />
      </div>

      {#if creatingKind}
        <form class="create-entry-bar" onsubmit={submitNewEntry}>
          <div class="create-entry-label">
            {createEntryLabel}
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
              {m.common_create()}
            </button>
            <button
              type="button"
              class="mini-btn"
              onclick={() => (creatingKind = null)}
            >
              {m.common_cancel()}
            </button>
          </div>
          {#if creatingValidationError}
            <div class="inline-error" role="alert">{creatingValidationError}</div>
          {/if}
        </form>
      {/if}

      {#if pendingDeleteNode}
        <div class="delete-confirm-bar" role="alertdialog" aria-label={m.sidebar_confirm_move_label({ trash: trashName() })}>
          <div class="delete-confirm-text">
            {deleteConfirmParts[0]}<strong>{pendingDeleteNode.name}</strong>{deleteConfirmParts[1]}
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
              {m.sidebar_move_to_trash({ trash: trashName() })}
            </button>
            <button
              type="button"
              class="mini-btn"
              onclick={() => (pendingDeleteNode = null)}
            >
              {m.common_cancel()}
            </button>
          </div>
        </div>
      {/if}

      {#if pinnedNotes.length > 0}
        <div class="pinned" aria-label={m.sidebar_pinned()}>
          <div class="pinned-label">{m.sidebar_pinned()}</div>
          <ul class="pinned-list">
            {#each pinnedNotes as pin (pin.relPath)}
              <li>
                <button
                  type="button"
                  class="pinned-item"
                  class:active={activeRelPath === pin.relPath}
                  title={pin.relPath}
                  onclick={() => onSelectFile(pin.relPath)}
                >
                  <Icon name="push-pin" size={14} />
                  <span class="pinned-title">{pin.title}</span>
                </button>
                <IconButton
                  icon="push-pin-slash"
                  size="sm"
                  label={m.sidebar_unpin_named({ title: pin.title })}
                  onclick={() => void vaultStore.togglePin(pin.relPath)}
                />
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      <div class="tree-scroll">
        {#if snapshot.tree.length === 0}
          <div class="empty-tree">
            <p>{m.sidebar_empty_vault()}</p>
            <Button icon="file-plus" onclick={() => startNewEntry('note', '')}>{m.sidebar_write_first_note()}</Button>
          </div>
        {:else if settingsStore.notesNavigation === 'tags'}
          <TagsLinksView {activeRelPath} {onSelectFile} />
        {:else}
          <button
            type="button"
            class="root-drop-target"
            class:drop-target={dropTargetRel === ''}
            aria-label={m.sidebar_vault_root()}
            title={m.sidebar_drop_to_root()}
            ondragover={(event) => dragOverFolder(event, '')}
            ondragleave={leaveFolder}
            ondrop={(event) => void dropNote(event, '')}
          ><Icon name="folder-simple" size={14} /> {snapshot.name}</button>
          <ul class="tree-root" role="tree" aria-label={m.sidebar_notes_and_folders()}>
            {#each snapshot.tree as node (node.relPath)}
              <FileTreeItem
                {node}
                depth={0}
                {activeRelPath}
                {expandedPaths}
                {renamingRelPath}
                {dropTargetRel}
                onNoteDragStart={startNoteDrag}
                onNoteDragEnd={endNoteDrag}
                onNoteDragOver={dragOverFolder}
                onNoteDragLeave={leaveFolder}
                onNoteDrop={(event, folder) => void dropNote(event, folder)}
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
      {#await import('./SearchPanel.svelte') then { default: SearchPanel }}
        <SearchPanel />
      {:catch}
        <LazyLoadError onClose={() => settingsStore.setLeftSidebarView('files')} />
      {/await}
    {:else if settingsStore.leftSidebarView === 'tags'}
      {#await import('./TagBrowserPanel.svelte') then { default: TagBrowserPanel }}
        <TagBrowserPanel />
      {:catch}
        <LazyLoadError onClose={() => settingsStore.setLeftSidebarView('files')} />
      {/await}
    {:else if settingsStore.leftSidebarView === 'conflicts'}
      {#await import('./SyncConflictsPanel.svelte') then { default: SyncConflictsPanel }}
        <SyncConflictsPanel />
      {:catch}
        <LazyLoadError onClose={() => settingsStore.setLeftSidebarView('files')} />
      {/await}
    {/if}

    <div class="sidebar-footer">
      <Button variant="primary" icon="note-pencil" class="new-note" onclick={() => startNewEntry('note', '')}>
        {m.common_new_note()} <kbd>{shortcutLabel('Ctrl N')}</kbd>
      </Button>
      <IconButton
        icon="gear-six"
        label={m.settings_title()}
        tooltip={m.sidebar_settings_tooltip({ shortcut: shortcutLabel('Ctrl+,') })}
        onclick={() => (settingsStore.showSettings = true)}
      />
    </div>

    <!-- Resize Sash -->
    <div
      class="resize-handle"
      class:dragging={isResizing}
      role="separator"
      aria-orientation="vertical"
      aria-label={m.sidebar_resize()}
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
      <span>{folderTarget ? m.sidebar_ctx_new_note_in({ folder: folderTarget }) : m.common_new_note()}</span>
    </button>
    <button
      type="button"
      class="ctx-item"
      role="menuitem"
      onclick={() => startNewEntry('folder', folderTarget)}
    >
      <Icon name="folder-simple-plus" />
      <span>{folderTarget ? m.sidebar_ctx_new_folder_in({ folder: folderTarget }) : m.common_new_folder()}</span>
    </button>
    {#if targetNode}
      <div class="ctx-sep" role="separator"></div>
      {#if targetNode.kind === 'file'}
        {@const pinned = vaultStore.isPinned(targetNode.relPath)}
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          onclick={() => {
            const target = targetNode.relPath;
            contextMenu = null;
            void vaultStore.togglePin(target);
          }}
        >
          <Icon name={pinned ? 'push-pin-slash' : 'push-pin'} />
          <span>{pinned ? m.sidebar_ctx_unpin() : m.sidebar_ctx_pin()}</span>
        </button>
      {/if}
      <button
        type="button"
        class="ctx-item"
        role="menuitem"
        onclick={() => handleStartRename(targetNode)}
      >
        <Icon name="pencil-simple-line" />
        <span>{m.sidebar_ctx_rename()}</span>
      </button>
      <button
        type="button"
        class="ctx-item danger-item"
        role="menuitem"
        onclick={() => {
          // Read before clearing: targetNode derives from contextMenu.
          pendingDeleteNode = targetNode;
          contextMenu = null;
        }}
      >
        <Icon name="trash" />
        <span>{m.sidebar_ctx_move_to_trash({ trash: trashName() })}</span>
      </button>
    {/if}
  </div>
{/if}

<style>
  .root-drop-target {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-2);
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-muted);
    text-align: left;
    font: inherit;
    font-size: var(--fs-12);
  }

  .root-drop-target.drop-target {
    border-color: var(--accent);
    background: var(--selection);
  }
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

  .section-label > span {
    flex: 1;
  }

  /* Pinned notes sit above the tree and never scroll with it, so they stay one click away. */
  .pinned {
    flex: none;
    max-height: 40%;
    overflow-y: auto;
    border-bottom: 1px solid var(--line-subtle);
    padding-bottom: var(--space-1);
  }

  .pinned-label {
    padding: var(--space-2) var(--space-2) var(--space-1) var(--space-4);
    color: var(--text-muted);
    font: 600 var(--fs-11) / 1 var(--font-ui);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .pinned-list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .pinned-list li {
    display: flex;
    align-items: center;
    padding-right: var(--space-2);
  }

  .pinned-list li:hover {
    background: var(--surface-hover);
  }

  .pinned-item {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-2) 0 var(--space-4);
    height: var(--row-height);
    border: 0;
    background: transparent;
    color: var(--text-primary);
    font: var(--fs-13) / 1 var(--font-ui);
    text-align: left;
    cursor: default;
  }

  .pinned-item.active {
    color: var(--accent);
    font-weight: 600;
  }

  .pinned-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .nav-switch {
    display: flex;
    gap: 2px;
  }

  .nav-switch button {
    padding: 3px 6px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    letter-spacing: inherit;
    text-transform: inherit;
    cursor: pointer;
  }

  .nav-switch button:hover {
    color: var(--text-primary);
  }

  .nav-switch button.on {
    background: var(--surface-hover);
    color: var(--text-primary);
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
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    color: var(--text-muted);
    font-size: var(--fs-13);
  }

  .empty-tree p {
    margin: 0;
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
