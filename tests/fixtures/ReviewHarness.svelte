<script lang="ts">
  import { onMount } from 'svelte';
  import { EditorView } from '@codemirror/view';
  import { openSearchPanel } from '@codemirror/search';
  import { searchExtension, searchTheme } from '../../src/lib/editorSearch';
  import Dialog from '../../src/lib/ui/Dialog.svelte';
  import LazyLoadError from '../../src/lib/LazyLoadError.svelte';
  import { tabsStore } from '../../src/lib/stores/tabs.svelte';
  import { control } from '../ipc.mock';

  let editor: HTMLDivElement;
  let showDialog = $state(false);
  let failed = $state(false);
  const close = () => { showDialog = false; failed = false; };
  onMount(() => {
    const view = new EditorView({ parent: editor, doc: 'alpha beta alpha', extensions: [searchExtension(), searchTheme] });
    openSearchPanel(view);
    // Keep one dirty note whose disk save fails. A recovery reload must retain it.
    tabsStore.openTabs = [{ relPath: 'Unsaved.md', title: 'Unsaved', content: 'keep me', savedContent: '', dirty: true, externalChangePending: false }];
    tabsStore.activeTabPath = 'Unsaved.md';
    control.save = async () => { throw Error('disk full'); };
    return () => view.destroy();
  });
</script>

<button onclick={() => { showDialog = true; }}>Open dialog</button>
<button onclick={() => { failed = true; }}>Fail optional panel</button>
<div bind:this={editor}></div>
{#if showDialog}
  <Dialog label="Review dialog" onClose={close}><button onclick={close}>Dialog action</button></Dialog>
{/if}
{#if failed}
  {#await Promise.reject(new Error('chunk unavailable'))}
  {:then}
    <span>Loaded</span>
  {:catch}
    <Dialog label="Panel failed" onClose={close}><LazyLoadError onClose={close} /></Dialog>
  {/await}
{/if}
