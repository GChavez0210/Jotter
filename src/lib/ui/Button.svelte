<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import Icon from './Icon.svelte';
  import type { IconName } from './icons';

  interface Props extends HTMLButtonAttributes {
    /** `default` has a border, `primary` uses the accent, `subtle` is borderless until hover. */
    variant?: 'default' | 'primary' | 'subtle' | 'danger';
    icon?: IconName;
    /** Marks a toggle button as on; sets `aria-pressed`. */
    pressed?: boolean;
    /** The rendered button element, for focus management. */
    ref?: HTMLButtonElement;
    children?: Snippet;
  }

  let {
    variant = 'default',
    icon,
    pressed,
    ref = $bindable(),
    children,
    type = 'button',
    class: className = '',
    ...rest
  }: Props = $props();
</script>

<button
  bind:this={ref}
  {type}
  class="btn {variant} {className}"
  class:pressed
  aria-pressed={pressed}
  {...rest}
>
  {#if icon}<Icon name={icon} />{/if}
  {#if children}<span class="label">{@render children()}</span>{/if}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    min-height: var(--control-min-height);
    padding: 0 var(--space-3);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-card);
    background: var(--surface-field);
    color: var(--text-primary);
    font: 500 var(--fs-13) / 1 var(--font-ui);
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      border-color var(--transition-fast),
      color var(--transition-fast);
  }

  .btn:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  .btn:active:not(:disabled) {
    background: var(--surface-active);
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .subtle {
    border-color: transparent;
    background: transparent;
    color: var(--text-secondary);
  }

  .subtle:hover:not(:disabled) {
    color: var(--text-primary);
  }

  .primary {
    border-color: transparent;
    background: var(--accent);
    color: var(--on-accent);
  }

  .primary:hover:not(:disabled),
  .primary:active:not(:disabled) {
    background: var(--accent);
    filter: brightness(1.08);
  }

  .danger {
    border-color: transparent;
    background: var(--status-danger);
    color: var(--on-danger);
  }

  .danger:hover:not(:disabled),
  .danger:active:not(:disabled) {
    background: var(--status-danger);
    filter: brightness(1.08);
  }

  .pressed {
    border-color: transparent;
    background: var(--accent-soft);
    color: var(--accent);
  }

  .pressed:hover:not(:disabled) {
    background: var(--accent-soft);
  }
</style>
