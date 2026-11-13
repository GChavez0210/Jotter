<script lang="ts">
  import { untrack } from 'svelte';
  import Dialog from './ui/Dialog.svelte';
  import Button from './ui/Button.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import { clipboardStore } from './stores/clipboard.svelte';
  import { hasLinkUnsafeChars, linkTargetFor, suggestExtractName } from './extractNote';
  import { sameNotePath } from './notePaths';
  import { validateEntryName } from './validation';
  import { m } from './i18n.svelte';

  interface Props {
    /** The selected text, as captured from the editor. */
    text: string;
    /** The note the text comes from. */
    sourceRel: string;
    /** Swaps the selection for the link. Returns false when the note changed meanwhile. */
    apply: (linkText: string) => boolean;
    onClose: () => void;
  }

  let { text, sourceRel, apply, onClose }: Props = $props();

  // The dialog is made for one selection, so the fields start from the props once.
  const initial = untrack(() => ({
    name: suggestExtractName(text),
    folder: sourceRel.includes('/') ? sourceRel.slice(0, sourceRel.lastIndexOf('/')) : '',
  }));
  let name = $state(initial.name);
  let folder = $state(initial.folder);
  let backLink = $state(true);
  let busy = $state(false);
  let failure = $state('');
  let nameEl: HTMLInputElement | undefined = $state();

  const cleanFolder = $derived(folder.trim().replace(/\\/g, '/').replace(/^\/+|\/+$/g, ''));
  const trimmedName = $derived(name.trim());
  const destination = $derived(`${cleanFolder ? `${cleanFolder}/` : ''}${trimmedName}.md`);

  /** The first problem with the name or folder, or `null` when the note can be made. */
  const problem = $derived.by(() => {
    if (!trimmedName) return m.extract_name_required();
    const nameError = validateEntryName(trimmedName);
    if (nameError) return nameError;
    if (hasLinkUnsafeChars(trimmedName)) return m.extract_name_link_chars();
    for (const part of cleanFolder ? cleanFolder.split('/') : []) {
      const folderError = validateEntryName(part);
      if (folderError) return folderError;
    }
    if (hasLinkUnsafeChars(destination)) return m.extract_path_link_chars({ path: destination });
    if (backLink) {
      const target = linkTargetFor(sourceRel, (t) => vaultStore.resolveWikiTarget(t, destination));
      if (hasLinkUnsafeChars(target)) return m.extract_path_link_chars({ path: sourceRel });
    }
    if (vaultStore.allFiles.some((f) => sameNotePath(f.relPath, destination))) {
      return m.extract_exists({ path: destination });
    }
    return null;
  });

  $effect(() => {
    nameEl?.select();
  });

  async function extract() {
    if (problem || busy) return;
    busy = true;
    failure = '';
    try {
      const result = await vaultStore.extractToNote({
        sourceRel,
        text,
        name: trimmedName,
        folder: cleanFolder,
        backLink,
        apply,
      });
      clipboardStore.setStatus(
        result.replaced
          ? m.extract_done({ path: result.path })
          : m.extract_not_replaced({ path: result.path })
      );
      onClose();
    } catch (err) {
      failure = m.extract_failed({ error: err instanceof Error ? err.message : String(err) });
      busy = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      if (!busy) onClose();
    } else if (e.key === 'Enter' && !e.isComposing && !(e.target instanceof HTMLButtonElement)) {
      e.preventDefault();
      void extract();
    }
  }
</script>

<Dialog label={m.extract_title()} {onClose} onkeydown={onKeydown}>
  <div class="form">
    <h2 class="heading">{m.extract_title()}</h2>
    <label class="field">
      <span>{m.extract_name()}</span>
      <input type="text" bind:this={nameEl} bind:value={name} aria-invalid={problem !== null} />
    </label>
    <label class="field">
      <span>{m.extract_folder()}</span>
      <input type="text" bind:value={folder} placeholder={m.common_none()} />
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={backLink} />
      <span>{m.extract_backlink()}</span>
    </label>
    <div class="preview">{m.extract_preview({ path: destination })}</div>
    {#if problem}
      <div class="error" role="alert">{problem}</div>
    {:else if failure}
      <div class="error" role="alert">{failure}</div>
    {/if}
    <div class="actions">
      <Button onclick={onClose} disabled={busy}>{m.common_cancel()}</Button>
      <Button variant="primary" onclick={() => void extract()} disabled={busy || problem !== null}>
        {m.extract_confirm()}
      </Button>
    </div>
  </div>
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
  }

  .heading {
    margin: 0;
    color: var(--text-primary);
    font-size: var(--fs-15);
    font-weight: 600;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    color: var(--text-secondary);
    font-size: var(--fs-13);
  }

  .field input[type='text'] {
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-control);
    background: var(--surface-editor);
    color: var(--text-primary);
    font: inherit;
  }

  .field input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-secondary);
    font-size: var(--fs-13);
  }

  .preview {
    color: var(--text-muted);
    font-size: var(--fs-12);
    overflow-wrap: anywhere;
  }

  .error {
    color: var(--danger, #c42b1c);
    font-size: var(--fs-13);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
