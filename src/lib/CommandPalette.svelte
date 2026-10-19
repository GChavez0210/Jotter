<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import Dialog from './ui/Dialog.svelte';
  import type { MarkdownFormatCommandId } from './markdownCommands';
  import { settingsStore } from './stores/settings.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';

  interface Props {
    onFormat: (cmd: MarkdownFormatCommandId) => void;
    onCopyTrigger: () => void;
    onPastePlainTrigger: () => void;
    onRefreshInspector: () => void;
    onExportHtml: () => void;
    onExportPdf: () => void;
    onClose: () => void;
  }

  let {
    onFormat,
    onCopyTrigger,
    onPastePlainTrigger,
    onRefreshInspector,
    onExportHtml,
    onExportPdf,
    onClose,
  }: Props = $props();

  interface PaletteItem {
    id: string;
    category: string;
    label: string;
    shortcut?: string;
    action: () => void;
  }

  let query = $state('');
  let selectedIndex = $state(0);
  let searchInputEl: HTMLInputElement | undefined = $state();

  let allCommands = $derived<PaletteItem[]>([
    // Focus & View Modes
    {
      id: 'focus-minimal',
      category: 'Focus & View',
      label:
        settingsStore.focusMode === 'minimal'
          ? 'Exit Minimal Focus Mode'
          : 'Toggle Minimal Focus Mode (hide sidebar, tabs, toolbar, status bar)',
      shortcut: 'Ctrl+Shift+M',
      action: () => settingsStore.toggleMinimalFocus(),
    },
    {
      id: 'focus-zen',
      category: 'Focus & View',
      label:
        settingsStore.focusMode === 'zen'
          ? 'Exit Zen Full Screen Mode'
          : 'Toggle Zen Full Screen Mode (text canvas only)',
      shortcut: 'F11',
      action: () => settingsStore.toggleZenFocus(),
    },
    ...(settingsStore.focusMode !== 'off'
      ? [
          {
            id: 'focus-exit',
            category: 'Focus & View',
            label: 'Exit Focus Mode to Previous Layout',
            shortcut: 'Esc',
            action: () => settingsStore.exitFocusMode(),
          },
        ]
      : []),
    {
      id: 'mode-toggle',
      category: 'Focus & View',
      label:
        settingsStore.editingMode === 'live'
          ? 'Switch to Source Mode (show all raw Markdown syntax)'
          : 'Switch to Live Mode (inline rendered formatting)',
      shortcut: 'Ctrl+/',
      action: () => settingsStore.toggleEditingMode(),
    },
    {
      id: 'opt-typewriter',
      category: 'Focus & View',
      label: `${
        settingsStore.typewriterScrolling ? 'Disable' : 'Enable'
      } Typewriter Scrolling (keep active line centered)`,
      action: () =>
        settingsStore.setTypewriterScrolling(!settingsStore.typewriterScrolling),
    },
    {
      id: 'opt-dim-paragraphs',
      category: 'Focus & View',
      label: `${
        settingsStore.dimInactiveParagraphs ? 'Disable' : 'Enable'
      } Dim Inactive Paragraphs (focus active paragraph)`,
      action: () =>
        settingsStore.setDimInactiveParagraphs(
          !settingsStore.dimInactiveParagraphs
        ),
    },
    {
      id: 'toggle-sidebar',
      category: 'Focus & View',
      label: 'Toggle Left Sidebar',
      shortcut: 'Ctrl+\\',
      action: () => settingsStore.toggleSidebar(),
    },
    ...(vaultStore.vaultSnapshot
      ? [
          {
            id: 'quick-switcher',
            category: 'Navigation',
            label: 'Quick Switcher: Fuzzy Open Note by Name',
            shortcut: 'Ctrl+O',
            action: () => (settingsStore.showQuickSwitcher = true),
          },
          {
            id: 'view-search',
            category: 'Navigation',
            label: 'Open Full-Text Vault Search Panel',
            shortcut: 'Ctrl+Shift+F',
            action: () => settingsStore.setLeftSidebarView('search'),
          },
          {
            id: 'view-tags',
            category: 'Navigation',
            label: 'Open Tag Browser Panel',
            action: () => settingsStore.setLeftSidebarView('tags'),
          },
          {
            id: 'view-calendar',
            category: 'Navigation',
            label: 'Open Calendar View',
            action: () => settingsStore.setLeftSidebarView('calendar'),
          },
          {
            id: 'daily-note',
            category: 'Navigation',
            label: "Open Today's Daily Note",
            shortcut: 'Ctrl+Shift+D',
            action: () => void vaultStore.openDailyNote(),
          },
          {
            id: 'view-trash',
            category: 'Navigation',
            label: 'Open Recently Deleted Notes',
            action: () => settingsStore.setLeftSidebarView('trash'),
          },
          {
            id: 'view-files',
            category: 'Navigation',
            label: 'Open File Tree View',
            action: () => settingsStore.setLeftSidebarView('files'),
          },
          {
            id: 'toggle-backlinks',
            category: 'Focus & View',
            label: `${
              settingsStore.showRightPanel ? 'Hide' : 'Show'
            } Right Backlinks Panel`,
            action: () => settingsStore.toggleRightPanel(),
          },
        ]
      : []),
    // Markdown Formatting
    {
      id: 'fmt-bold',
      category: 'Format',
      label: 'Toggle Bold',
      shortcut: 'Ctrl+B',
      action: () => onFormat('bold'),
    },
    {
      id: 'fmt-italic',
      category: 'Format',
      label: 'Toggle Italic',
      shortcut: 'Ctrl+I',
      action: () => onFormat('italic'),
    },
    {
      id: 'fmt-strike',
      category: 'Format',
      label: 'Toggle Strikethrough',
      shortcut: 'Ctrl+Shift+S',
      action: () => onFormat('strikethrough'),
    },
    {
      id: 'fmt-code',
      category: 'Format',
      label: 'Toggle Inline Code',
      shortcut: 'Ctrl+E',
      action: () => onFormat('inline_code'),
    },
    {
      id: 'fmt-h1',
      category: 'Format',
      label: 'Heading 1',
      shortcut: 'Ctrl+1',
      action: () => onFormat('h1'),
    },
    {
      id: 'fmt-h2',
      category: 'Format',
      label: 'Heading 2',
      shortcut: 'Ctrl+2',
      action: () => onFormat('h2'),
    },
    {
      id: 'fmt-h3',
      category: 'Format',
      label: 'Heading 3',
      shortcut: 'Ctrl+3',
      action: () => onFormat('h3'),
    },
    {
      id: 'fmt-h4',
      category: 'Format',
      label: 'Heading 4',
      shortcut: 'Ctrl+4',
      action: () => onFormat('h4'),
    },
    {
      id: 'fmt-bullet',
      category: 'Format',
      label: 'Toggle Bullet List',
      shortcut: 'Ctrl+Shift+8',
      action: () => onFormat('bullet_list'),
    },
    {
      id: 'fmt-ordered',
      category: 'Format',
      label: 'Toggle Numbered List',
      shortcut: 'Ctrl+Shift+7',
      action: () => onFormat('ordered_list'),
    },
    {
      id: 'fmt-task',
      category: 'Format',
      label: 'Toggle Task List',
      shortcut: 'Ctrl+Shift+9',
      action: () => onFormat('task_list'),
    },
    {
      id: 'fmt-quote',
      category: 'Format',
      label: 'Toggle Blockquote',
      shortcut: 'Ctrl+Shift+.',
      action: () => onFormat('blockquote'),
    },
    {
      id: 'fmt-codeblock',
      category: 'Format',
      label: 'Insert Fenced Code Block',
      shortcut: 'Ctrl+Shift+K',
      action: () => onFormat('code_block'),
    },
    {
      id: 'fmt-table',
      category: 'Format',
      label: 'Insert Markdown Table',
      action: () => onFormat('table'),
    },
    {
      id: 'fmt-link',
      category: 'Format',
      label: 'Insert Markdown Link',
      shortcut: 'Ctrl+K',
      action: () => onFormat('link'),
    },
    {
      id: 'fmt-wikilink',
      category: 'Format',
      label: 'Insert Obsidian [[Wiki Link]]',
      action: () => onFormat('wikilink'),
    },
    {
      id: 'fmt-hr',
      category: 'Format',
      label: 'Insert Horizontal Rule (---)',
      action: () => onFormat('hr'),
    },
    // Clipboard & Vault
    {
      id: 'clip-copy',
      category: 'Clipboard',
      label: 'Copy Selection or Note with Line Breaks Preserved',
      shortcut: 'Ctrl+C',
      action: () => onCopyTrigger(),
    },
    {
      id: 'clip-paste-plain',
      category: 'Clipboard',
      label: 'Paste as Plain Text',
      shortcut: 'Ctrl+Shift+V',
      action: () => onPastePlainTrigger(),
    },
    {
      id: 'clip-mode-md',
      category: 'Clipboard',
      label: 'Set Copy Mode: Markdown source (default)',
      action: () =>
        settingsStore.setCopyMode('markdown_source', onRefreshInspector),
    },
    {
      id: 'clip-mode-plain',
      category: 'Clipboard',
      label: 'Set Copy Mode: Plain (text/plain only, Markdown stripped)',
      action: () => settingsStore.setCopyMode('plain', onRefreshInspector),
    },
    {
      id: 'clip-mode-rich',
      category: 'Clipboard',
      label: 'Set Copy Mode: Rich (HTML + plain)',
      action: () => settingsStore.setCopyMode('rich', onRefreshInspector),
    },
    {
      id: 'clip-inspector',
      category: 'Clipboard',
      label: 'Toggle Clipboard Payload Inspector',
      action: () => {
        settingsStore.showInspector = !settingsStore.showInspector;
        if (settingsStore.showInspector) onRefreshInspector();
      },
    },
    // Vault & Notes
    ...(vaultStore.vaultSnapshot
      ? [
          {
            id: 'note-new',
            category: 'Vault',
            label: 'Create New Note',
            shortcut: 'Ctrl+N',
            action: () => {
              const stamp = new Date()
                .toISOString()
                .slice(0, 19)
                .replace(/[:T]/g, '-');
              void vaultStore.createNoteInVault('', `Note ${stamp}`);
            },
          },
          {
            id: 'note-save',
            category: 'Vault',
            label: 'Save Active Note Now',
            shortcut: 'Ctrl+S',
            action: () => {
              if (tabsStore.activeTabPath) {
                void tabsStore.flushAutosaveFor(tabsStore.activeTabPath);
              }
            },
          },
          {
            id: 'note-close',
            category: 'Vault',
            label: 'Close Active Tab',
            shortcut: 'Ctrl+W',
            action: () => {
              if (tabsStore.activeTabPath) {
                void tabsStore.closeTab(tabsStore.activeTabPath);
              }
            },
          },
          {
            id: 'vault-rebuild-index',
            category: 'Vault',
            label: 'Rebuild SQLite & Tantivy Vault Index',
            action: () => void vaultStore.rebuildIndex(),
          },
          {
            id: 'vault-switch',
            category: 'Vault',
            label: 'Switch or Close Vault',
            action: () => void vaultStore.switchVault(),
          },
        ]
      : []),
    // Export & Settings
    {
      id: 'export-html',
      category: 'Export',
      label: 'Export Active Note to Self-Contained HTML',
      action: () => onExportHtml(),
    },
    {
      id: 'export-pdf',
      category: 'Export',
      label: 'Export Active Note to PDF (Print to PDF)',
      shortcut: 'Ctrl+Shift+P',
      action: () => onExportPdf(),
    },
    {
      id: 'open-settings',
      category: 'Settings',
      label: 'Open Settings (Editor, Copy, Appearance, Vault)',
      shortcut: 'Ctrl+,',
      action: () => (settingsStore.showSettings = true),
    },
    // Theme
    {
      id: 'theme-system',
      category: 'Appearance',
      label: 'Theme: Follow Windows System Theme',
      action: () => settingsStore.setThemePreference('system'),
    },
    {
      id: 'theme-light',
      category: 'Appearance',
      label: 'Theme: Light',
      action: () => settingsStore.setThemePreference('light'),
    },
    {
      id: 'theme-dark',
      category: 'Appearance',
      label: 'Theme: Dark',
      action: () => settingsStore.setThemePreference('dark'),
    },
  ]);

  let filteredCommands = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return allCommands;
    return allCommands.filter(
      (item) =>
        item.label.toLowerCase().includes(q) ||
        item.category.toLowerCase().includes(q) ||
        (item.shortcut?.toLowerCase().includes(q) ?? false)
    );
  });

  $effect(() => {
    // Reset selection when search query changes
    void query;
    selectedIndex = 0;
  });

  $effect(() => {
    searchInputEl?.focus();
  });

  function executeItem(item: PaletteItem) {
    onClose();
    item.action();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (filteredCommands.length > 0) {
        selectedIndex = (selectedIndex + 1) % filteredCommands.length;
      }
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (filteredCommands.length > 0) {
        selectedIndex =
          (selectedIndex - 1 + filteredCommands.length) %
          filteredCommands.length;
      }
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      const chosen = filteredCommands[selectedIndex];
      if (chosen) {
        executeItem(chosen);
      }
    }
  }

  let listEl: HTMLDivElement | undefined = $state();

  // Keep the keyboard selection visible while arrowing through a long list.
  $effect(() => {
    void selectedIndex;
    listEl?.querySelector('.picker-row.selected')?.scrollIntoView({ block: 'nearest' });
  });
</script>

<Dialog label="Command palette" {onClose} onkeydown={handleKeyDown}>
  <div class="picker-search">
    <Icon name="magnifying-glass" />
    <input
      bind:this={searchInputEl}
      type="text"
      class="picker-input"
      placeholder="Type a command"
      aria-label="Search commands"
      bind:value={query}
    />
    <kbd>Esc</kbd>
  </div>

  <div class="picker-list" bind:this={listEl} role="listbox" aria-label="Commands">
    {#if filteredCommands.length === 0}
      <div class="picker-empty">No command matches “{query.trim()}”.</div>
    {:else}
      {#each filteredCommands as item, idx (item.id)}
        {#if idx === 0 || filteredCommands[idx - 1].category !== item.category}
          <div class="picker-group">{item.category}</div>
        {/if}
        <button
          type="button"
          role="option"
          aria-selected={idx === selectedIndex}
          class="picker-row"
          class:selected={idx === selectedIndex}
          onmouseenter={() => (selectedIndex = idx)}
          onclick={() => executeItem(item)}
        >
          <span class="picker-label">{item.label}</span>
          {#if item.shortcut}
            <kbd>{item.shortcut}</kbd>
          {/if}
        </button>
      {/each}
    {/if}
  </div>

  <div class="picker-footer">
    <span><kbd>↑</kbd> <kbd>↓</kbd> to move</span>
    <span><kbd>Enter</kbd> to run</span>
    <span><kbd>Esc</kbd> to close</span>
  </div>
</Dialog>
