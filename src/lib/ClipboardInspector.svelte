<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import { clipboardStore } from './stores/clipboard.svelte';
  import { COPY_MODE_LABELS, settingsStore } from './stores/settings.svelte';
</script>

{#if settingsStore.showInspector && clipboardStore.lastPayload}
  <aside
    class="inspector-card"
    class:solid-backdrop={settingsStore.effectiveSolidBackdrop}
    aria-label="Clipboard format inspector"
  >
    <div class="inspector-header">
      <div class="inspector-title">
        <Icon name="check" />
        <span>
          {clipboardStore.lastPayload.mode === 'plain'
            ? 'arboard::Clipboard::set_text Plain-Only Output (Plain)'
            : `arboard::Clipboard::set_html Dual-Format Output (${COPY_MODE_LABELS[clipboardStore.lastPayload.mode].label})`}
        </span>
      </div>
      <span class="inspector-meta">
        {clipboardStore.lastPayload.lineCount} lines · {clipboardStore.lastPayload.charCount} chars
      </span>
    </div>

    <div class="inspector-grid">
      <div class="format-box">
        <div class="format-label">
          <span>text/plain (real <code>\n</code> line breaks)</span>
        </div>
        <pre class="format-code">{clipboardStore.lastPayload.plain}</pre>
      </div>

      <div class="format-box">
        <div class="format-label">
          <span>text/html (every newline becomes <code>&lt;br&gt;</code>)</span>
        </div>
        <pre class="format-code">{clipboardStore.lastPayload.mode === 'plain'
            ? '(none — Plain mode writes text/plain only via set_text)'
            : clipboardStore.lastPayload.html}</pre>
      </div>
    </div>
  </aside>
{/if}

<style>
  .inspector-card {
    height: 184px;
    flex-shrink: 0;
    background: var(--surface-editor);
    border-top: 1px solid var(--line-subtle);
    padding: var(--space-3) var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    overflow: hidden;
  }

  .inspector-card.solid-backdrop {
    background: var(--surface-raised);
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
