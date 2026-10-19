<script lang="ts" generics="T extends string">
  import type { Snippet } from 'svelte';

  interface Option {
    value: T;
    label: string;
  }

  interface Props {
    options: readonly Option[];
    value: T;
    /** Accessible name of the group. */
    label: string;
    onchange: (value: T) => void;
    /** Draws the preview above each option's label. */
    preview: Snippet<[T]>;
  }

  let { options, value, label, onchange, preview }: Props = $props();

  function handleKeydown(event: KeyboardEvent, index: number) {
    const step = event.key === 'ArrowRight' || event.key === 'ArrowDown' ? 1 : event.key === 'ArrowLeft' || event.key === 'ArrowUp' ? -1 : 0;
    if (!step) return;
    event.preventDefault();
    const nextIndex = (index + step + options.length) % options.length;
    onchange(options[nextIndex].value);
    const group = (event.currentTarget as HTMLElement).parentElement;
    (group?.children[nextIndex] as HTMLElement | undefined)?.focus();
  }
</script>

<div class="cards" role="radiogroup" aria-label={label}>
  {#each options as option, index (option.value)}
    <button
      type="button"
      role="radio"
      class="card"
      aria-checked={option.value === value}
      tabindex={option.value === value ? 0 : -1}
      onclick={() => onchange(option.value)}
      onkeydown={(event) => handleKeydown(event, index)}
    >
      <span class="preview">{@render preview(option.value)}</span>
      <span class="card-label">{option.label}</span>
    </button>
  {/each}
</div>

<style>
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(124px, 1fr));
    gap: var(--space-2);
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2);
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-card);
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .card:hover {
    border-color: var(--line-control);
  }

  .card[aria-checked='true'] {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }

  .preview {
    display: block;
    height: 56px;
    overflow: hidden;
    border-radius: var(--radius-control);
  }

  .card-label {
    font-size: var(--fs-13);
  }
</style>
