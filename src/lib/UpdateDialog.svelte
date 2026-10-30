<script lang="ts">
  /**
   * What an available update changes, and the button that installs it. Installing saves open
   * notes first, downloads the update, and restarts Jotter, so the dialog cannot be closed
   * while that runs.
   */
  import Button from './ui/Button.svelte';
  import Dialog from './ui/Dialog.svelte';
  import { plainReleaseNotes } from './releaseNotes';
  import { updateStore } from './stores/updates.svelte';
  import { m } from './i18n.svelte';

  interface Props {
    /** Writes open notes and settings to disk. The install does not start if this fails. */
    saveFirst: () => Promise<boolean>;
    onClose: () => void;
  }

  let { saveFirst, onClose }: Props = $props();

  let info = $derived(updateStore.info);
  let notes = $derived(info?.notes ? plainReleaseNotes(info.notes) : '');
  let percent = $derived(updateStore.progress === null ? null : Math.round(updateStore.progress * 100));
</script>

{#if info}
  <Dialog label={m.update_available_title({ version: info.version })} variant="panel" {onClose}>
    <header class="head">
      <div class="heading">
        <h2>{m.update_available_title({ version: info.version })}</h2>
        <p class="hint">
          {m.update_versions({ current: info.currentVersion })}
          {#if info.date}{m.update_released({ date: info.date })}{/if}
        </p>
      </div>
    </header>

    <section class="notes" aria-label={m.update_notes_heading()}>
      <h3>{m.update_notes_heading()}</h3>
      {#if notes}
        <pre class="text">{notes}</pre>
      {:else}
        <p class="empty">{m.update_no_notes()}</p>
      {/if}
    </section>

    <footer class="foot">
      <div class="status" role="status">
        {#if updateStore.installing}
          <label class="progress-label">
            {updateStore.stage === 'installing'
              ? m.update_installing()
              : percent === null
                ? m.update_downloading()
                : `${m.update_downloading()} ${percent}%`}
            <progress
              aria-label={m.update_progress_label()}
              max="100"
              value={updateStore.stage === 'installing' ? undefined : (percent ?? undefined)}
            ></progress>
          </label>
        {:else if !info.canInstall}
          <p class="note">{m.update_cannot_install()}</p>
        {/if}
        {#if updateStore.error}
          <p class="error" role="alert">{updateStore.error}</p>
        {/if}
      </div>
      <div class="actions">
        <Button disabled={updateStore.installing} onclick={onClose}>{m.update_later()}</Button>
        {#if info.canInstall}
          <Button
            variant="primary"
            icon="arrow-clockwise"
            disabled={updateStore.installing || updateStore.checking}
            onclick={() => void updateStore.install(saveFirst)}
          >
            {m.update_install()}
          </Button>
        {/if}
      </div>
    </footer>
  </Dialog>
{/if}

<style>
  .head {
    padding: var(--space-4) var(--space-4) var(--space-3);
    border-bottom: 1px solid var(--line-subtle);
  }

  h2 {
    margin: 0;
    font: 600 var(--fs-15) / var(--leading-tight) var(--font-ui);
  }

  .hint {
    margin: 2px 0 0;
    color: var(--text-muted);
    font-size: var(--fs-12);
  }

  .notes {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-3) var(--space-4);
  }

  h3 {
    margin: 0 0 var(--space-2);
    font: 600 var(--fs-12) / var(--leading-tight) var(--font-ui);
    color: var(--text-secondary);
  }

  .text {
    margin: 0;
    font: inherit;
    font-size: var(--fs-13);
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .empty,
  .note {
    margin: 0;
    color: var(--text-muted);
    font-size: var(--fs-13);
  }

  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--line-subtle);
  }

  .status {
    flex: 1;
    min-width: 0;
  }

  .progress-label {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    color: var(--text-secondary);
    font-size: var(--fs-12);
  }

  progress {
    width: 100%;
  }

  .error {
    margin: 0;
    color: var(--text-danger);
    font-size: var(--fs-12);
    overflow-wrap: anywhere;
  }

  .actions {
    display: flex;
    flex: none;
    gap: var(--space-2);
  }
</style>
