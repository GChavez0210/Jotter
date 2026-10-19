<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** Accessible name of the dialog. */
    label: string;
    onClose: () => void;
    /** `picker` sits high on the screen like a search box; `panel` is centred. */
    variant?: 'picker' | 'panel';
    onkeydown?: (event: KeyboardEvent) => void;
    children: Snippet;
  }

  let { label, onClose, variant = 'picker', onkeydown, children }: Props = $props();
</script>

<!-- Clicking the scrim closes the dialog; Escape is handled by the app-level key handler. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="scrim {variant}"
  onmousedown={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
  {onkeydown}
>
  <div class="dialog {variant}" role="dialog" aria-modal="true" aria-label={label}>
    {@render children()}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: flex;
    justify-content: center;
    padding: 12vh var(--space-4) var(--space-4);
    background: var(--scrim);
  }

  .scrim.panel {
    align-items: center;
    padding-top: var(--space-4);
  }

  .dialog {
    display: flex;
    flex-direction: column;
    max-width: 100%;
    overflow: hidden;
    border: 1px solid var(--line-control);
    border-radius: var(--radius-dialog);
    background: var(--surface-raised);
    box-shadow: var(--shadow-popover);
  }

  .dialog.picker {
    width: 560px;
    max-height: 64vh;
    align-self: flex-start;
  }

  .dialog.panel {
    width: 760px;
    height: min(600px, 84vh);
  }
</style>
