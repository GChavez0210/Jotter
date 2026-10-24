<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { activeHeadingIndex, outlineDepths, parseOutline } from './outline';
  import { m } from './i18n.svelte';

  let headings = $derived(parseOutline(tabsStore.activeTab?.content ?? ''));
  let depths = $derived(outlineDepths(headings));
  let activeIndex = $derived(activeHeadingIndex(headings, tabsStore.docStats.cursorLine));
</script>

<section class="panel-section" aria-label={m.outline_title()}>
  <div class="panel-header">
    <span class="panel-icon"><Icon name="list-bullets" /></span>
    <span class="panel-title">{m.outline_title()}</span>
    <span class="count-badge">{headings.length}</span>
  </div>

  <div class="panel-body">
    {#if !tabsStore.activeTab}
      <div class="empty-state">{m.outline_no_note()}</div>
    {:else if headings.length === 0}
      <div class="empty-state">{m.outline_none()}</div>
    {:else}
      <ul class="outline-list" aria-label={m.outline_title()}>
        {#each headings as heading, index (`${heading.line}:${heading.text}`)}
          <li>
            <button
              type="button"
              class="outline-row"
              class:active={index === activeIndex}
              style:padding-left="calc(var(--space-4) + {depths[index] * 12}px)"
              aria-current={index === activeIndex ? 'location' : undefined}
              title={m.outline_jump({ line: heading.line })}
              onclick={() => tabsStore.requestScrollToLine(heading.line)}
            >
              <span class="level-mark" aria-hidden="true">H{heading.level}</span>
              <span class="outline-text">{heading.text || m.outline_untitled()}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>

<style>
  .panel-section {
    display: flex;
    flex-direction: column;
    min-height: 0;
    /* Share the panel with backlinks instead of pushing them off the bottom. */
    max-height: 50%;
    border-bottom: 1px solid var(--line-subtle);
  }

  .panel-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
    height: var(--bar-height);
    padding: 0 var(--space-3) 0 var(--space-4);
  }

  .panel-icon {
    display: grid;
    place-items: center;
    color: var(--text-muted);
  }

  .panel-title {
    flex: 1;
    color: var(--text-secondary);
    font: 600 var(--fs-11) / 1 var(--font-ui);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .count-badge {
    padding: 2px var(--space-2);
    border-radius: 999px;
    background: var(--surface-app);
    color: var(--text-muted);
    font: 600 var(--fs-11) / 1 var(--font-ui);
  }

  .panel-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-bottom: var(--space-2);
  }

  .empty-state {
    padding: var(--space-2) var(--space-4) var(--space-4);
    color: var(--text-muted);
    font: var(--fs-12) / 1.5 var(--font-ui);
  }

  .outline-list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .outline-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding-right: var(--space-3);
    min-height: var(--row-height);
    border: 0;
    background: transparent;
    color: var(--text-primary);
    font: var(--fs-13) / 1.3 var(--font-ui);
    text-align: left;
    cursor: default;
  }

  .outline-row:hover {
    background: var(--surface-hover);
  }

  .outline-row.active {
    color: var(--accent);
    font-weight: 600;
  }

  .level-mark {
    flex: none;
    color: var(--text-muted);
    font: 600 var(--fs-11) / 1 var(--font-mono);
  }

  .outline-row.active .level-mark {
    color: var(--accent);
  }

  .outline-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
