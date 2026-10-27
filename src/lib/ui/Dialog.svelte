<script lang="ts">
  import { onMount, type Snippet } from 'svelte';

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

  let dialogEl: HTMLDivElement | undefined = $state();

  const FOCUSABLE =
    'a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])';

  function focusableItems(): HTMLElement[] {
    if (!dialogEl) return [];
    return Array.from(dialogEl.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
      // A radio group or a tab strip moves one roving tab stop between its buttons, so the
      // ones it has taken out of the tab order are not where Tab can land.
      (el) => el.tabIndex >= 0 && (el.offsetParent !== null || el === document.activeElement)
    );
  }

  /**
   * Keeps Tab inside the dialog. `aria-modal` already hides the rest of the page from a screen
   * reader, so a Tab that walked out would move focus somewhere its user cannot see.
   */
  function handleTab(event: KeyboardEvent) {
    if (event.key !== 'Tab') return;
    const items = focusableItems();
    if (items.length === 0) {
      event.preventDefault();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement as HTMLElement | null;
    if (!active || !dialogEl?.contains(active)) {
      event.preventDefault();
      (event.shiftKey ? last : first).focus();
      return;
    }
    if (event.shiftKey && active === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  }

  onMount(() => {
    // Dialogs that own their first control, such as the pickers, focus it themselves.
    if (dialogEl && !dialogEl.contains(document.activeElement)) {
      focusableItems()[0]?.focus();
    }
  });
</script>

<!-- Clicking the scrim closes the dialog; Escape is handled by the app-level key handler. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="scrim {variant}"
  onmousedown={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
  onkeydown={(event) => {
    handleTab(event);
    onkeydown?.(event);
  }}
>
  <div
    class="dialog {variant}"
    bind:this={dialogEl}
    role="dialog"
    aria-modal="true"
    aria-label={label}
  >
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
