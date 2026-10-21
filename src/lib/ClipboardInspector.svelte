<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import { clipboardStore } from './stores/clipboard.svelte';
  import { COPY_MODE_LABELS, settingsStore } from './stores/settings.svelte';
  import { m, SLOT, splitSlots } from './i18n.svelte';

  const plainLabelParts = $derived(splitSlots(m.inspector_plain_label({ code: SLOT })));
  const htmlLabelParts = $derived(splitSlots(m.inspector_html_label({ code: SLOT })));
</script>

{#if settingsStore.showInspector && clipboardStore.lastPayload}
  <aside
    class="inspector-card"
    aria-label={m.inspector_label()}
  >
    <div class="inspector-header">
      <div class="inspector-title">
        <Icon name="check" />
        <span>
          {clipboardStore.lastPayload.mode === 'plain'
            ? m.inspector_plain_title()
            : m.inspector_dual_title({ mode: COPY_MODE_LABELS[clipboardStore.lastPayload.mode].label })}
        </span>
      </div>
      <span class="inspector-meta">
        {m.inspector_meta({ lines: clipboardStore.lastPayload.lineCount, chars: clipboardStore.lastPayload.charCount })}
      </span>
    </div>

    <div class="inspector-grid">
      <div class="format-box">
        <div class="format-label">
          <span>{plainLabelParts[0]}<code>\n</code>{plainLabelParts[1]}</span>
        </div>
        <pre class="format-code">{clipboardStore.lastPayload.plain}</pre>
      </div>

      <div class="format-box">
        <div class="format-label">
          <span>{htmlLabelParts[0]}<code>&lt;br&gt;</code>{htmlLabelParts[1]}</span>
        </div>
        <pre class="format-code">{clipboardStore.lastPayload.mode === 'plain'
            ? m.inspector_no_html()
            : clipboardStore.lastPayload.html}</pre>
      </div>
    </div>
  </aside>
{/if}

<style>
  .inspector-card {
    height: 184px;
    flex-shrink: 0;
    background: var(--surface-raised);
    border-top: 1px solid var(--line-subtle);
    padding: var(--space-3) var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    overflow: hidden;
  }

  .inspector-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: var(--fs-12);
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
    background: var(--surface-subtle);
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-control);
    padding: var(--space-2);
  }

  .format-label {
    font-size: var(--fs-12);
    font-weight: 600;
    color: var(--text-secondary);
  }

  .format-label code {
    font-family: var(--font-mono);
    background: var(--surface-code);
    padding: 1px 4px;
    border-radius: var(--radius-control);
  }

  .format-code {
    flex: 1;
    margin: 0;
    overflow: auto;
    font-family: var(--font-mono);
    font-size: var(--fs-11);
    line-height: 1.45;
    color: var(--text-primary);
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
