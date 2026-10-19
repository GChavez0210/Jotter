<script lang="ts">
  import { clipboardStore } from './stores/clipboard.svelte';
  import { settingsStore } from './stores/settings.svelte';
  import { tabsStore } from './stores/tabs.svelte';
</script>

{#if settingsStore.focusMode === 'off'}
  <footer class="status-bar" aria-label="Status bar">
    <div class="status-left">
      <span class="status-indicator">{clipboardStore.statusBanner}</span>
      {#if clipboardStore.statusTimestamp}
        <span class="status-time">({clipboardStore.statusTimestamp})</span>
      {/if}
    </div>

    <div class="status-right">
      <span class="status-item" title="Active editor rendering mode">
        Mode: {settingsStore.editingMode === 'live' ? 'Live' : 'Source'}
      </span>
      <span class="status-item" title="Active window backdrop">
        Backdrop: {settingsStore.effectiveSolidBackdrop
          ? 'Solid Fallback'
          : 'Windows 11 Mica'}
      </span>
      <span class="status-item" title="Active color theme">
        Theme: {settingsStore.effectiveDark ? 'Dark' : 'Light'} ({settingsStore.themePref})
      </span>
      {#if tabsStore.activeTab || tabsStore.scratchpadMode}
        <span class="status-item">
          {#if tabsStore.docStats.selectedChars > 0}
            Selected: {tabsStore.docStats.selectedLines} ln, {tabsStore.docStats.selectedChars} ch ·
          {/if}
          {tabsStore.docStats.lines} ln · {tabsStore.docStats.words} words
        </span>
      {/if}
    </div>
  </footer>
{/if}

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--space-4);
    height: 28px;
    border-top: 1px solid var(--line-subtle);
    font-size: var(--fs-12);
    color: var(--text-secondary);
    user-select: none;
    flex-shrink: 0;
  }

  .status-left {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    overflow: hidden;
  }

  .status-indicator {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .status-time {
    opacity: 0.75;
    flex-shrink: 0;
  }

  .status-right {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-shrink: 0;
  }

  .status-item {
    white-space: nowrap;
  }
</style>
