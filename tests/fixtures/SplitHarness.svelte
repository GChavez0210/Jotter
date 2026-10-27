<script lang="ts">
  /** Two note panes side by side, the layout App.svelte builds for split view. */
  import { onMount } from 'svelte';
  import NotePane from '../../src/lib/NotePane.svelte';
  import { settingsStore } from '../../src/lib/stores/settings.svelte';
  import { tabsStore } from '../../src/lib/stores/tabs.svelte';

  const LEFT = Array.from({ length: 60 }, (_, i) => `Left line ${i + 1}`).join('\n');
  const RIGHT = Array.from({ length: 60 }, (_, i) => `Right line ${i + 1}`).join('\n');

  onMount(() => {
    tabsStore.openTabs = [
      { relPath: 'Left.md', title: 'Left', content: LEFT, savedContent: LEFT, dirty: false, externalChangePending: false },
      { relPath: 'Right.md', title: 'Right', content: RIGHT, savedContent: RIGHT, dirty: false, externalChangePending: false },
    ];
    tabsStore.activeTabPath = 'Left.md';
    settingsStore.setWindowWidth(1600);
    settingsStore.sidebarCollapsed = true;
    settingsStore.splitMode = 'note';
    settingsStore.splitNotePath = 'Right.md';
  });

  let left = $derived(tabsStore.openTabs.find((t) => t.relPath === 'Left.md') ?? null);
  let right = $derived(tabsStore.openTabs.find((t) => t.relPath === 'Right.md') ?? null);
</script>

<div class="row" style="display:flex; height:300px;">
  <section
    style="flex:1; min-width:0; display:flex; flex-direction:column;"
    aria-label="First pane"
    onfocusin={() => settingsStore.setActivePane('primary')}
  >
    {#if left}
      <NotePane
        pane="primary"
        docId={left.relPath}
        initialDoc={left.content}
        tab={left}
        onEditor={() => {}}
        onOpenNote={() => {}}
      />
    {/if}
  </section>
  <section
    style="flex:1; min-width:0; display:flex; flex-direction:column;"
    aria-label="Second pane"
    onfocusin={() => settingsStore.setActivePane('secondary')}
  >
    {#if right}
      <NotePane
        pane="secondary"
        docId={right.relPath}
        initialDoc={right.content}
        tab={right}
        onEditor={() => {}}
        onOpenNote={() => {}}
      />
    {/if}
  </section>
</div>

<output id="state">{settingsStore.activePane}|{left?.content.split('\n')[0]}|{right?.content.split('\n')[0]}</output>
