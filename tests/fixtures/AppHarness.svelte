<script lang="ts">
  /**
   * The whole app with a vault open and a note on screen, for the accessibility scan. The
   * backend is mocked, so the stores are filled here instead of through a real vault.
   */
  import App from '../../src/App.svelte';
  import { settingsStore } from '../../src/lib/stores/settings.svelte';
  import { tabsStore } from '../../src/lib/stores/tabs.svelte';
  import { vaultStore } from '../../src/lib/stores/vault.svelte';
  import type { VaultNode } from '../../src/lib/types';
  import { control, versionControl } from '../ipc.mock';
  Object.assign(window, { __review: { tabsStore, settingsStore, control, versionControl } });

  const NOTE = `# Audit\n\nA paragraph with a [[Wiki link]] and a #tag.\n\n- [ ] A task 📅 2026-12-01\n\n| Column | Value |\n| --- | --- |\n| One | 1 |\n`;

  const now = Date.now();
  const file = (name: string, relPath: string): VaultNode =>
    ({ kind: 'file', name, relPath, modifiedMs: now, createdMs: now, children: [] });
  const tree: VaultNode[] = [
    { kind: 'folder', name: 'Work', relPath: 'Work', modifiedMs: now, createdMs: now,
      children: [file('Audit.md', 'Work/Audit.md')] },
    file('Journal.md', 'Journal.md'),
  ];

  vaultStore.vaultSnapshot = { rootPath: '/vault', name: 'vault', tree, noteCount: 2 };
  tabsStore.openTabs = [
    { relPath: 'Work/Audit.md', title: 'Audit', content: NOTE, savedContent: NOTE, dirty: false, externalChangePending: false },
    { relPath: 'Journal.md', title: 'Journal', content: '# Journal\n', savedContent: '# Journal\n', dirty: true, externalChangePending: false },
  ];
  tabsStore.activeTabPath = 'Work/Audit.md';
  settingsStore.setWindowWidth(1400);
</script>

<App />
