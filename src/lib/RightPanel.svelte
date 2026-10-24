<script lang="ts">
  import BacklinksPanel from './BacklinksPanel.svelte';
  import OutlinePanel from './OutlinePanel.svelte';
  import CalendarView from './CalendarView.svelte';
  import RecentlyDeletedPanel from './RecentlyDeletedPanel.svelte';
  import IconButton from './ui/IconButton.svelte';
  import Segmented from './ui/Segmented.svelte';
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
    <div class="panel-tabs">
      <Segmented
        fill
        compact
        label={m.right_panel_tabs()}
        value={settingsStore.rightPanelTab}
        options={[
          { value: 'note', label: m.right_panel_tab_note(), icon: 'list-bullets' },
          { value: 'calendar', label: m.sidebar_view_calendar(), icon: 'calendar-blank' },
          { value: 'trash', label: m.sidebar_view_trash(), icon: 'trash' },
        ]}
        onchange={(tab) => settingsStore.showRightPanelTab(tab)}
      />
    </div>
    {#if settingsStore.rightPanelTab === 'calendar'}
      <CalendarView />
    {:else if settingsStore.rightPanelTab === 'trash'}
      <RecentlyDeletedPanel />
    {:else}
      <OutlinePanel />
      <BacklinksPanel />
    {/if}
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

  /* The close button and tabs sit above the sections, which carry their own titles. */
  .panel-close {
    flex: none;
    display: flex;
    justify-content: flex-end;
    padding: var(--space-1) var(--space-2) 0;
  }

  .panel-tabs {
    flex: none;
    padding: 0 var(--space-3) var(--space-2);
  }
</style>
