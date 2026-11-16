<script lang="ts">
  import { onMount } from 'svelte';
  import SearchPanel from '../../src/lib/SearchPanel.svelte';
  import { tabsStore } from '../../src/lib/stores/tabs.svelte';
  import { searchControl } from '../ipc.mock';
  import type { SearchHit } from '../../src/lib/types';
  const pending: Array<{ query: string; resolve: (hits: SearchHit[]) => void }> = [];
  const opened: string[] = [];
  searchControl.search = (query) => new Promise((resolve) => { pending.push({ query, resolve }); });
  Object.assign(window, { __review: { pending, opened } });
  onMount(() => {
    const open = tabsStore.openNoteInTab;
    tabsStore.openNoteInTab = async (path: string) => { opened.push(path); };
    return () => { tabsStore.openNoteInTab = open; };
  });
</script>
<SearchPanel />
