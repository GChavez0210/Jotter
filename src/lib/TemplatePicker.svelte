<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import Dialog from './ui/Dialog.svelte';
  import { readNote } from './ipc';
  import { settingsStore } from './stores/settings.svelte';
  import { stripMdExt, vaultStore } from './stores/vault.svelte';
  import { findTemplatePrompts, listTemplates, noteStamp } from './templates';
  import type { VaultNode } from './types';
  import { validateEntryName } from './validation';
  import { m } from './i18n.svelte';

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  let query = $state('');
  let selectedIndex = $state(0);
  let chosen = $state<VaultNode | null>(null);
  let promptLabels = $state<string[]>([]);
  let answers = $state<Record<string, string>>({});
  let noteName = $state('');
  let loadError = $state('');
  let inputEl: HTMLInputElement | undefined = $state();
  let nameEl: HTMLInputElement | undefined = $state();

  let templates = $derived(listTemplates(vaultStore.allFiles, settingsStore.templatesFolder));
  let matches = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? templates.filter((t) => t.relPath.toLowerCase().includes(q)) : templates;
  });

  let nameError = $derived.by(() => {
    const trimmed = noteName.trim().replace(/\\/g, '/');
    if (!trimmed) return m.template_name_required();
    const leaf = trimmed.split('/').pop() ?? trimmed;
    return validateEntryName(leaf);
  });

  $effect(() => {
    void query;
    selectedIndex = 0;
  });

  $effect(() => {
    if (!chosen) inputEl?.focus();
    else nameEl?.select();
  });

  async function choose(template: VaultNode) {
    loadError = '';
    try {
      const content = (await readNote(template.relPath)).content;
      promptLabels = findTemplatePrompts(content);
      answers = Object.fromEntries(promptLabels.map((label) => [label, '']));
      const name = m.note_default_name({ stamp: noteStamp() });
      const folder = settingsStore.templatePickerFolder;
      noteName = folder ? `${folder}/${name}` : name;
      chosen = template;
    } catch (err) {
      loadError = err instanceof Error ? err.message : String(err);
    }
  }

  async function create() {
    if (!chosen || nameError) return;
    const trimmed = noteName.trim().replace(/\\/g, '/');
    const slash = trimmed.lastIndexOf('/');
    const parent = slash >= 0 ? trimmed.slice(0, slash) : '';
    const name = slash >= 0 ? trimmed.slice(slash + 1) : trimmed;
    const template = chosen.relPath;
    onClose();
    await vaultStore.createNoteFromTemplate(template, parent, name, answers);
  }

  function handlePickKeyDown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' && matches.length) {
      e.preventDefault();
      selectedIndex = (selectedIndex + 1) % matches.length;
    } else if (e.key === 'ArrowUp' && matches.length) {
      e.preventDefault();
      selectedIndex = (selectedIndex - 1 + matches.length) % matches.length;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const target = matches[selectedIndex];
      if (target) void choose(target);
    }
  }

  function handleFormKeyDown(e: KeyboardEvent) {
    // A focused button, such as "Choose another template", acts on Enter itself.
    if (e.key === 'Enter' && !(e.target instanceof HTMLButtonElement)) {
      e.preventDefault();
      void create();
    }
  }

  let listEl: HTMLDivElement | undefined = $state();

  $effect(() => {
    void selectedIndex;
    listEl?.querySelector('.picker-row.selected')?.scrollIntoView({ block: 'nearest' });
  });
</script>

<Dialog
  label={m.template_picker()}
  {onClose}
  onkeydown={(e) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      if (chosen) chosen = null;
      else onClose();
      return;
    }
    if (chosen) handleFormKeyDown(e);
    else handlePickKeyDown(e);
  }}
>
  {#if !chosen}
    <div class="picker-search">
      <Icon name="magnifying-glass" />
      <input
        bind:this={inputEl}
        type="text"
        class="picker-input"
        placeholder={m.template_find()}
        aria-label={m.template_find()}
        bind:value={query}
      />
      <kbd>Esc</kbd>
    </div>

    <div class="picker-list" bind:this={listEl} role="listbox" aria-label={m.template_picker()}>
      {#if loadError}
        <div class="picker-empty">{loadError}</div>
      {:else if templates.length === 0}
        <div class="picker-empty">{m.template_none({ folder: settingsStore.templatesFolder || '—' })}</div>
      {:else if matches.length === 0}
        <div class="picker-empty">{m.quick_no_match({ query: query.trim() })}</div>
      {:else}
        {#each matches as item, idx (item.relPath)}
          <button
            type="button"
            role="option"
            aria-selected={idx === selectedIndex}
            class="picker-row"
            class:selected={idx === selectedIndex}
            onmouseenter={() => (selectedIndex = idx)}
            onclick={() => void choose(item)}
          >
            <Icon name="file-text" />
            <span class="picker-label">{stripMdExt(item.name)}</span>
          </button>
        {/each}
      {/if}
    </div>

    <div class="picker-footer">
      <span><kbd>↑</kbd> <kbd>↓</kbd> {m.picker_hint_move()}</span>
      <span><kbd>Enter</kbd> {m.template_hint_choose()}</span>
    </div>
  {:else}
    <div class="template-form">
      <div class="template-heading">{stripMdExt(chosen.name)}</div>
      <label class="template-field">
        <span>{m.template_note_name()}</span>
        <input type="text" bind:this={nameEl} bind:value={noteName} aria-invalid={nameError !== null} />
      </label>
      {#each promptLabels as label (label)}
        <label class="template-field">
          <span>{label}</span>
          <input type="text" bind:value={answers[label]} />
        </label>
      {/each}
      {#if nameError}
        <div class="template-error">{nameError}</div>
      {/if}
    </div>
    <div class="picker-footer">
      <button type="button" class="template-link" onclick={() => (chosen = null)}>
        {m.template_back()}
      </button>
      <span><kbd>Enter</kbd> {m.picker_hint_create()}</span>
    </div>
  {/if}
</Dialog>

<style>
  .template-form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
  }

  .template-heading {
    font-weight: 600;
    color: var(--text-primary);
  }

  .template-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--fs-13);
    color: var(--text-secondary);
  }

  .template-field input {
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-control);
    background: var(--surface-editor);
    color: var(--text-primary);
    font: inherit;
  }

  .template-field input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .template-error {
    font-size: var(--fs-13);
    color: var(--danger, #c42b1c);
  }

  .template-link {
    border: 0;
    background: transparent;
    padding: 0;
    color: var(--text-secondary);
    font: inherit;
    cursor: pointer;
  }

  .template-link:hover {
    color: var(--text-primary);
  }
</style>
