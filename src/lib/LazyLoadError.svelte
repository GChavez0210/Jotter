<script lang="ts">
  import Button from './ui/Button.svelte';
  import { m } from './i18n.svelte';
  import { appStorage } from './appStorage';
  import { tabsStore } from './stores/tabs.svelte';
  import { saveBeforeLeaving } from './saveBeforeLeaving';

  let { onClose }: { onClose: () => void } = $props();
  let error = $state(false);
  let busy = $state(false);
  async function reload() {
    busy = true;
    try {
      if (await saveBeforeLeaving(() => tabsStore.flushAllAutosaves(), () => appStorage.flush())) {
        window.location.reload();
      } else error = true;
    } catch {
      error = true;
    } finally {
      busy = false;
    }
  }
</script>

<div class="load-error" role="alert">
  <p>{error ? m.lazy_reload_save_failed() : m.lazy_load_failed()}</p>
  <div>
    <Button disabled={busy} onclick={() => void reload()}>{m.lazy_reload()}</Button>
    <Button onclick={onClose}>{m.common_close()}</Button>
  </div>
</div>

<style>
  .load-error { padding: var(--space-4); color: var(--text-primary); background: var(--surface-raised); }
  .load-error div { display: flex; flex-wrap: wrap; gap: var(--space-2); }
</style>
