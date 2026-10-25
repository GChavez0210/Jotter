<script lang="ts">
  import Dialog from './ui/Dialog.svelte';
  import Button from './ui/Button.svelte';
  import { stripMdExt, vaultStore } from './stores/vault.svelte';
  import { m } from './i18n.svelte';

  let prompt = $derived(vaultStore.linkUpdatePrompt);
  let name = $derived(prompt ? stripMdExt(prompt.oldRel.split('/').pop() ?? prompt.oldRel) : '');
</script>

{#if prompt}
  <Dialog
    label={m.link_update_title()}
    variant="panel"
    onClose={() => vaultStore.answerLinkUpdate('cancel')}
    onkeydown={(e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        vaultStore.answerLinkUpdate('cancel');
      }
    }}
  >
    <div class="link-update">
      <h2>{m.link_update_title()}</h2>
      <p>{m.link_update_body({ count: prompt.noteCount, name })}</p>
      <div class="actions">
        <Button variant="default" onclick={() => vaultStore.answerLinkUpdate('cancel')}>
          {m.common_cancel()}
        </Button>
        <Button variant="default" onclick={() => vaultStore.answerLinkUpdate('rename')}>
          {m.link_update_skip()}
        </Button>
        <Button variant="primary" onclick={() => vaultStore.answerLinkUpdate('update')}>
          {m.link_update_apply()}
        </Button>
      </div>
    </div>
  </Dialog>
{/if}

<style>
  .link-update {
    width: 420px;
    max-width: 100%;
    padding: var(--space-5);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  h2 {
    margin: 0;
    font: 600 var(--fs-15) / 1.3 var(--font-ui);
    color: var(--text-primary);
  }

  p {
    margin: 0;
    color: var(--text-secondary);
    font-size: var(--fs-13);
    line-height: 1.5;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
</style>
