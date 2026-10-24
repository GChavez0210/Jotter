<script lang="ts" generics="T extends string | number">
  import Icon from './Icon.svelte';
  import type { IconName } from './icons';

  interface Option {
    value: T;
    label: string;
    icon?: IconName;
    /** Tooltip with more detail than the label. */
    title?: string;
  }

  interface Props {
    options: readonly Option[];
    value: T;
    /** Accessible name of the group. */
    label: string;
    /** Fill the available width with equally sized options. */
    fill?: boolean;
    /** Show icons alone, for narrow windows. Options without an icon keep their label. */
    compact?: boolean;
    onchange: (value: T) => void;
  }

  let { options, value, label, fill = false, compact = false, onchange }: Props = $props();

  function handleKeydown(event: KeyboardEvent, index: number) {
    const step = event.key === 'ArrowRight' || event.key === 'ArrowDown' ? 1 : event.key === 'ArrowLeft' || event.key === 'ArrowUp' ? -1 : 0;
    if (!step) return;
    event.preventDefault();
    const next = options[(index + step + options.length) % options.length];
    onchange(next.value);
    const group = (event.currentTarget as HTMLElement).parentElement;
    (group?.querySelectorAll('button')[options.indexOf(next)] as HTMLButtonElement | undefined)?.focus();
  }
</script>

<div class="segmented" class:fill role="radiogroup" aria-label={label}>
  {#each options as option, index (option.value)}
    {@const iconOnly = compact && Boolean(option.icon)}
    <button
      type="button"
      class:icon-only={iconOnly}
      role="radio"
      aria-checked={option.value === value}
      aria-label={iconOnly ? option.label : undefined}
      tabindex={option.value === value ? 0 : -1}
      title={option.title ?? (iconOnly ? option.label : undefined)}
      onclick={() => onchange(option.value)}
      onkeydown={(event) => handleKeydown(event, index)}
    >
      {#if option.icon}<Icon name={option.icon} size={14} />{/if}
      {#if !iconOnly}{option.label}{/if}
    </button>
  {/each}
</div>

<style>
  .segmented {
    display: inline-flex;
    flex: none;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-card);
    background: var(--surface-app);
  }

  button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-muted);
    font: 500 var(--fs-13) / 1 var(--font-ui);
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      color var(--transition-fast);
  }

  /* Without a label the button would be a wide blank; square it around the icon. */
  button.icon-only {
    padding: 0;
    width: 28px;
    justify-content: center;
  }

  .segmented.fill {
    box-sizing: border-box;
    width: 100%;
  }

  .fill button {
    flex: 1 1 0;
    min-width: 0;
    justify-content: center;
  }

  button:hover {
    color: var(--text-primary);
  }

  button[aria-checked='true'] {
    background: var(--surface-raised);
    color: var(--text-primary);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.08);
  }
</style>
