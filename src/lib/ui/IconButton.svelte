<script lang="ts">
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import Icon from './Icon.svelte';
  import type { IconName } from './icons';

  interface Props extends HTMLButtonAttributes {
    icon: IconName;
    /** Accessible name and tooltip. Include the shortcut in the tooltip, not here. */
    label: string;
    /** Tooltip text; defaults to `label`. */
    tooltip?: string;
    /** Marks a toggle button as on; sets `aria-pressed`. */
    pressed?: boolean;
    /** The rendered button element, for focus management. */
    ref?: HTMLButtonElement;
    size?: 'sm' | 'md';
  }

  let {
    icon,
    label,
    tooltip,
    pressed,
    ref = $bindable(),
    size = 'md',
    type = 'button',
    class: className = '',
    ...rest
  }: Props = $props();
</script>

<button
  bind:this={ref}
  {type}
  class="icon-btn {size} {className}"
  class:pressed
  aria-label={label}
  title={tooltip ?? label}
  aria-pressed={pressed}
  {...rest}
>
  <Icon name={icon} size={size === 'sm' ? 14 : 16} />
</button>

<style>
  .icon-btn {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      color var(--transition-fast);
  }

  .icon-btn.sm {
    width: 22px;
    height: 22px;
  }

  .icon-btn:hover:not(:disabled) {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .icon-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .pressed {
    color: var(--accent);
  }
</style>
