<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Button from './ui/Button.svelte';
  import { aiFolderEditInstructions, aiFolderSetUp, aiFolderStatus } from './ipc';
  import { tabsStore } from './stores/tabs.svelte';
  import { m } from './i18n.svelte';
  import type { AiFolderSetup, AiFolderStatus } from './types';

  let status = $state<AiFolderStatus | null>(null);
  let confirming = $state(false);
  let busy = $state(false);
  let report = $state<AiFolderSetup | null>(null);
  let error = $state('');
  let confirmEl = $state<HTMLDivElement>();

  const loading = load();

  async function load() {
    try {
      status = await aiFolderStatus();
    } catch (err) {
      error = String(err);
    }
  }

  onMount(() => void loading);

  /** Asks to set up the folder, if that has not happened yet. */
  export async function offer() {
    await loading;
    if (status && !status.setUp) await ask();
  }

  async function ask() {
    // Counts what is there now, which may differ from when the card opened.
    await load();
    confirming = true;
    await tick();
    confirmEl?.scrollIntoView({ block: 'nearest' });
    confirmEl?.querySelector('button')?.focus();
  }

  async function setUp() {
    busy = true;
    error = '';
    try {
      // A moved note must hold the latest edits.
      if (!(await tabsStore.flushAllAutosaves())) {
        error = m.ai_folder_unsaved();
        return;
      }
      report = await aiFolderSetUp();
      for (const { from, to } of report.moved) tabsStore.updatePathsAfterRename(from, to);
      confirming = false;
      await load();
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  async function allowInstructions(input: HTMLInputElement) {
    const allowed = input.checked;
    busy = true;
    error = '';
    try {
      status = await aiFolderEditInstructions(allowed);
    } catch (err) {
      error = String(err);
      input.checked = !allowed;
    } finally {
      busy = false;
    }
  }
</script>

<div class="setting-card column-card ai-folder">
  <div class="setting-info">
    <div class="setting-label">{m.ai_folder_title()}</div>
    <div class="setting-desc">{m.ai_folder_desc()}</div>
  </div>

  {#if status?.setUp}
    <div class="setting-desc" role="status">
      {m.ai_folder_ready()}
      {#if report}
        {m.ai_folder_report({ created: report.created.length, moved: report.moved.length })}
        {#if report.kept.length}{m.ai_folder_kept({ notes: report.kept.join(', ') })}{/if}
      {/if}
    </div>
    <div class="setting-row-top">
      <div class="setting-info">
        <label for="toggle-ai-instructions" class="setting-label">{m.ai_folder_instructions()}</label>
        <div class="setting-desc">{m.ai_folder_instructions_desc()}</div>
      </div>
      <input
        id="toggle-ai-instructions"
        type="checkbox"
        class="fluent-toggle"
        checked={status.editInstructions}
        disabled={busy}
        onchange={(e) => void allowInstructions(e.currentTarget)}
      />
    </div>
  {:else if confirming && status}
    <div class="confirm" bind:this={confirmEl}>
      <div class="setting-desc" role="alert">
        {m.ai_folder_confirm()}
        {#if status.existingNotes}{m.ai_folder_confirm_existing({ count: status.existingNotes })}{/if}
        {#if status.legacyNotes}{m.ai_folder_confirm_move({ count: status.legacyNotes })}{/if}
      </div>
      <div class="button-row">
        <Button variant="primary" disabled={busy} onclick={() => void setUp()}>{m.ai_folder_create()}</Button>
        <Button disabled={busy} onclick={() => (confirming = false)}>{m.common_cancel()}</Button>
      </div>
    </div>
  {:else if status}
    <div class="button-row">
      <Button onclick={() => void ask()}>{m.ai_folder_set_up()}</Button>
    </div>
  {/if}

  {#if error}<div class="field-error" role="alert">{error}</div>{/if}
</div>

<style>
  .confirm {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
</style>
