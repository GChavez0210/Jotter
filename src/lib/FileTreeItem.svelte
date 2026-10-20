<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import type { VaultNode } from './types';
  import { validateEntryName } from './validation';
  import FileTreeItem from './FileTreeItem.svelte';

  interface Props {
    node: VaultNode;
    depth?: number;
    activeRelPath: string | null;
    expandedPaths: Record<string, boolean>;
    renamingRelPath: string | null;
    onSelectFile: (relPath: string) => void;
    onToggleFolder: (relPath: string) => void;
    onStartRename: (node: VaultNode) => void;
    onCommitRename: (node: VaultNode, newName: string) => void;
    onCancelRename: () => void;
    onContextMenu: (e: MouseEvent, node: VaultNode) => void;
    onRequestDelete: (node: VaultNode) => void;
  }

  let {
    node,
    depth = 0,
    activeRelPath,
    expandedPaths,
    renamingRelPath,
    onSelectFile,
    onToggleFolder,
    onStartRename,
    onCommitRename,
    onCancelRename,
    onContextMenu,
    onRequestDelete,
  }: Props = $props();

  let renameDraft = $state('');

  let isFolder = $derived(node.kind === 'folder');
  let isExpanded = $derived(Boolean(expandedPaths[node.relPath]));
  let isActive = $derived(activeRelPath === node.relPath);
  let isRenaming = $derived(renamingRelPath === node.relPath);
  let renameError = $derived(
    isRenaming ? validateEntryName(renameDraft) : null
  );

  $effect(() => {
    if (isRenaming) {
      renameDraft =
        node.kind === 'file' && node.name.toLowerCase().endsWith('.md')
          ? node.name.slice(0, -3)
          : node.name;
    }
  });

  function handleRowClick() {
    if (isRenaming) return;
    if (isFolder) {
      onToggleFolder(node.relPath);
    } else {
      onSelectFile(node.relPath);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (isRenaming) return;
    if (e.key === 'F2') {
      e.preventDefault();
      onStartRename(node);
    } else if (e.key === 'Delete') {
      e.preventDefault();
      onRequestDelete(node);
    }
  }

  function tryCommitRename() {
    const err = validateEntryName(renameDraft);
    if (err) return;
    onCommitRename(node, renameDraft);
  }

  function handleRenameKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      tryCommitRename();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onCancelRename();
    }
  }
</script>

<li
  class="tree-node"
  role="treeitem"
  aria-expanded={isFolder ? isExpanded : undefined}
  aria-selected={isActive}
>
  {#if isRenaming}
    <div class="rename-container" style:padding-left="{depth * 16 + 8}px">
      <div class="tree-row rename-row">
        <span class="row-icon">
          <Icon name={isFolder ? 'folder-simple' : 'file-text'} />
        </span>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          type="text"
          class="rename-input"
          class:invalid={Boolean(renameError)}
          aria-invalid={Boolean(renameError)}
          bind:value={renameDraft}
          autofocus
          onkeydown={handleRenameKeyDown}
          onblur={() => {
            if (!renameError) {
              onCommitRename(node, renameDraft);
            } else {
              onCancelRename();
            }
          }}
        />
      </div>
      {#if renameError}
        <div class="inline-error" role="alert">{renameError}</div>
      {/if}
    </div>
  {:else}
    <button
      type="button"
      class="tree-row"
      class:active={isActive}
      style:padding-left="{depth * 16 + 8}px"
      onclick={handleRowClick}
      onkeydown={handleKeyDown}
      oncontextmenu={(e) => onContextMenu(e, node)}
      title={node.relPath}
    >
      {#if isFolder}
        <span class="chevron-icon">
          <Icon name={isExpanded ? 'caret-down' : 'caret-right'} size={12} />
        </span>
        <span class="row-icon folder-color"><Icon name="folder-simple" /></span>
      {:else}
        <span class="chevron-spacer"></span>
        <span class="row-icon"><Icon name="file-text" /></span>
      {/if}
      <span class="row-label">
        {node.kind === 'file' && node.name.toLowerCase().endsWith('.md')
          ? node.name.slice(0, -3)
          : node.name}
      </span>
    </button>
  {/if}

  {#if isFolder && isExpanded && node.children.length > 0}
    <ul class="tree-children" role="group">
      {#each node.children as child (child.relPath)}
        <FileTreeItem
          node={child}
          depth={depth + 1}
          {activeRelPath}
          {expandedPaths}
          {renamingRelPath}
          {onSelectFile}
          {onToggleFolder}
          {onStartRename}
          {onCommitRename}
          {onCancelRename}
          {onContextMenu}
          {onRequestDelete}
        />
      {/each}
    </ul>
  {/if}
</li>

<style>
  .tree-node {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .tree-children {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .tree-row {
    width: 100%;
    min-height: var(--control-min-height);
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding-right: var(--space-2);
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-primary);
    font-size: var(--fs-12);
    text-align: left;
    cursor: pointer;
    transition: background-color var(--transition-fast);
  }

  .tree-row:hover {
    background: var(--surface-hover);
  }

  .tree-row.active {
    background: var(--selection);
    font-weight: 600;
  }

  .chevron-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .chevron-spacer {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
  }

  .row-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .row-icon.folder-color {
    color: var(--accent);
  }

  .row-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .rename-container {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-right: var(--space-2);
  }

  .rename-row {
    padding-left: 0;
    gap: var(--space-2);
  }

  .rename-input {
    flex: 1;
    min-width: 0;
    height: 26px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-control);
    border: 1px solid var(--accent);
    background: var(--surface-raised);
    color: var(--text-primary);
    font-size: var(--fs-12);
  }

  .rename-input.invalid {
    border-color: var(--status-danger);
  }

  .inline-error {
    font-size: var(--fs-12);
    color: var(--text-danger);
    padding: 2px var(--space-1);
    line-height: 1.3;
  }
</style>
