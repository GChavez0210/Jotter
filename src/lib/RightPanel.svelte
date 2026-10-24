<script lang="ts">
  import BacklinksPanel from './BacklinksPanel.svelte';
  import OutlinePanel from './OutlinePanel.svelte';
  import IconButton from './ui/IconButton.svelte';
  import { settingsStore } from './stores/settings.svelte';
  import { m } from './i18n.svelte';
</script>

{#if settingsStore.rightPanelVisible}
  <aside class="right-panel" aria-label={m.right_panel()}>
    <div class="panel-close">
      <IconButton
        icon="x"
        size="sm"
        label={m.right_panel_close()}
        onclick={() => settingsStore.toggleRightPanel()}
      />
    </div>
    <OutlinePanel />
    <BacklinksPanel />
  </aside>
{/if}

<style>
  .right-panel {
    width: 248px;
    height: 100%;
    flex-shrink: 0;
    background: var(--surface-editor);
    border-left: 1px solid var(--line-subtle);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    user-select: none;
    transition: background-color var(--transition-fast);
  }

  /* The close button sits above both sections, which carry their own titles. */
  .panel-close {
    flex: none;
    display: flex;
    justify-content: flex-end;
    padding: var(--space-1) var(--space-2) 0;
  }
</style>
