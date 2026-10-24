<script lang="ts">
  import { shortcutLabel } from './platform';
  import Icon from './ui/Icon.svelte';
  import Dialog from './ui/Dialog.svelte';
  import type { MarkdownFormatCommandId } from './markdownCommands';
  import { aiStore } from './stores/ai.svelte';
  import { settingsStore } from './stores/settings.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import { m } from './i18n.svelte';

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
      category: m.cmd_cat_focus(),
      label:
        settingsStore.focusMode === 'minimal'
          ? m.cmd_focus_minimal_exit()
          : m.cmd_focus_minimal(),
      shortcut: shortcutLabel('Ctrl+Shift+M'),
      action: () => settingsStore.toggleMinimalFocus(),
    },
    {
      id: 'focus-zen',
      category: m.cmd_cat_focus(),
      label:
        settingsStore.focusMode === 'zen'
          ? m.cmd_focus_zen_exit()
          : m.cmd_focus_zen(),
      shortcut: 'F11',
      action: () => settingsStore.toggleZenFocus(),
    },
    ...(settingsStore.focusMode !== 'off'
      ? [
          {
            id: 'focus-exit',
            category: m.cmd_cat_focus(),
            label: m.cmd_focus_exit(),
            shortcut: 'Esc',
            action: () => settingsStore.exitFocusMode(),
          },
        ]
      : []),
    {
      id: 'mode-toggle',
      category: m.cmd_cat_focus(),
      label:
        settingsStore.editingMode === 'live'
          ? m.cmd_mode_source()
          : m.cmd_mode_live(),
      shortcut: shortcutLabel('Ctrl+/'),
      action: () => settingsStore.toggleEditingMode(),
    },
    {
      id: 'opt-typewriter',
      category: m.cmd_cat_focus(),
      label: settingsStore.typewriterScrolling ? m.cmd_typewriter_off() : m.cmd_typewriter_on(),
      action: () =>
        settingsStore.setTypewriterScrolling(!settingsStore.typewriterScrolling),
    },
    {
      id: 'opt-dim-paragraphs',
      category: m.cmd_cat_focus(),
      label: settingsStore.dimInactiveParagraphs ? m.cmd_dim_off() : m.cmd_dim_on(),
      action: () =>
        settingsStore.setDimInactiveParagraphs(
          !settingsStore.dimInactiveParagraphs
        ),
    },
    {
      id: 'opt-line-numbers',
      category: m.cmd_cat_focus(),
      label: settingsStore.showLineNumbers ? m.cmd_line_numbers_hide() : m.cmd_line_numbers_show(),
      action: () => settingsStore.setShowLineNumbers(!settingsStore.showLineNumbers),
    },
    {
      id: 'toggle-sidebar',
      category: m.cmd_cat_focus(),
      label: m.cmd_toggle_sidebar(),
      shortcut: shortcutLabel('Ctrl+\\'),
      action: () => settingsStore.toggleSidebar(),
    },
    ...(vaultStore.vaultSnapshot
      ? [
          {
            id: 'quick-switcher',
            category: m.cmd_cat_navigation(),
            label: m.cmd_quick_switcher(),
            shortcut: shortcutLabel('Ctrl+O'),
            action: () => (settingsStore.showQuickSwitcher = true),
          },
          {
            id: 'view-search',
            category: m.cmd_cat_navigation(),
            label: m.cmd_view_search(),
            shortcut: shortcutLabel('Ctrl+Shift+F'),
            action: () => settingsStore.setLeftSidebarView('search'),
          },
          {
            id: 'view-tags',
            category: m.cmd_cat_navigation(),
            label: m.cmd_view_tags(),
            action: () => settingsStore.setLeftSidebarView('tags'),
          },
          {
            id: 'view-calendar',
            category: m.cmd_cat_navigation(),
            label: m.cmd_view_calendar(),
            action: () => settingsStore.showRightPanelTab('calendar'),
          },
          {
            id: 'daily-note',
            category: m.cmd_cat_navigation(),
            label: m.cmd_daily_note(),
            shortcut: shortcutLabel('Ctrl+Shift+D'),
            action: () => void vaultStore.openDailyNote(),
          },
          {
            id: 'view-trash',
            category: m.cmd_cat_navigation(),
            label: m.cmd_view_trash(),
            action: () => settingsStore.showRightPanelTab('trash'),
          },
          {
            id: 'view-files',
            category: m.cmd_cat_navigation(),
            label: m.cmd_view_files(),
            action: () => settingsStore.setLeftSidebarView('files'),
          },
          {
            id: 'toggle-backlinks',
            category: m.cmd_cat_focus(),
            label: settingsStore.showRightPanel ? m.cmd_backlinks_hide() : m.cmd_backlinks_show(),
            action: () => settingsStore.toggleRightPanel(),
          },
        ]
      : []),
    // Markdown Formatting
    {
      id: 'fmt-bold',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_bold(),
      shortcut: shortcutLabel('Ctrl+B'),
      action: () => onFormat('bold'),
    },
    {
      id: 'fmt-italic',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_italic(),
      shortcut: shortcutLabel('Ctrl+I'),
      action: () => onFormat('italic'),
    },
    {
      id: 'fmt-strike',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_strike(),
      shortcut: shortcutLabel('Ctrl+Shift+S'),
      action: () => onFormat('strikethrough'),
    },
    {
      id: 'fmt-code',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_code(),
      shortcut: shortcutLabel('Ctrl+E'),
      action: () => onFormat('inline_code'),
    },
    {
      id: 'fmt-h1',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_heading({ level: 1 }),
      shortcut: shortcutLabel('Ctrl+1'),
      action: () => onFormat('h1'),
    },
    {
      id: 'fmt-h2',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_heading({ level: 2 }),
      shortcut: shortcutLabel('Ctrl+2'),
      action: () => onFormat('h2'),
    },
    {
      id: 'fmt-h3',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_heading({ level: 3 }),
      shortcut: shortcutLabel('Ctrl+3'),
      action: () => onFormat('h3'),
    },
    {
      id: 'fmt-h4',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_heading({ level: 4 }),
      shortcut: shortcutLabel('Ctrl+4'),
      action: () => onFormat('h4'),
    },
    {
      id: 'fmt-bullet',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_bullet(),
      shortcut: shortcutLabel('Ctrl+Shift+8'),
      action: () => onFormat('bullet_list'),
    },
    {
      id: 'fmt-ordered',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_ordered(),
      shortcut: shortcutLabel('Ctrl+Shift+7'),
      action: () => onFormat('ordered_list'),
    },
    {
      id: 'fmt-task',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_task(),
      shortcut: shortcutLabel('Ctrl+Shift+9'),
      action: () => onFormat('task_list'),
    },
    {
      id: 'fmt-quote',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_quote(),
      shortcut: shortcutLabel('Ctrl+Shift+.'),
      action: () => onFormat('blockquote'),
    },
    {
      id: 'fmt-codeblock',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_codeblock(),
      shortcut: shortcutLabel('Ctrl+Shift+K'),
      action: () => onFormat('code_block'),
    },
    {
      id: 'fmt-table',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_table(),
      action: () => onFormat('table'),
    },
    {
      id: 'fmt-link',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_link(),
      shortcut: shortcutLabel('Ctrl+K'),
      action: () => onFormat('link'),
    },
    {
      id: 'fmt-wikilink',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_wikilink(),
      action: () => onFormat('wikilink'),
    },
    {
      id: 'fmt-hr',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_hr(),
      action: () => onFormat('hr'),
    },
    {
      id: 'fmt-due-date',
      category: m.cmd_cat_format(),
      label: m.cmd_fmt_due_date(),
      action: () => onFormat('due_date'),
    },
    // Outliner
    {
      id: 'outline-zoom',
      category: m.cmd_cat_outline(),
      label: m.cmd_outline_zoom(),
      shortcut: shortcutLabel('Ctrl+.'),
      action: () => onFormat('zoom_toggle'),
    },
    {
      id: 'outline-fold-all',
      category: m.cmd_cat_outline(),
      label: m.cmd_outline_fold_all(),
      action: () => onFormat('fold_all'),
    },
    {
      id: 'outline-unfold-all',
      category: m.cmd_cat_outline(),
      label: m.cmd_outline_unfold_all(),
      action: () => onFormat('unfold_all'),
    },
    // Clipboard & Vault
    {
      id: 'clip-copy',
      category: m.cmd_cat_clipboard(),
      label: m.cmd_clip_copy(),
      shortcut: shortcutLabel('Ctrl+C'),
      action: () => onCopyTrigger(),
    },
    {
      id: 'clip-paste-plain',
      category: m.cmd_cat_clipboard(),
      label: m.cmd_clip_paste_plain(),
      shortcut: shortcutLabel('Ctrl+Shift+V'),
      action: () => onPastePlainTrigger(),
    },
    {
      id: 'clip-mode-md',
      category: m.cmd_cat_clipboard(),
      label: m.cmd_clip_mode_md(),
      action: () =>
        settingsStore.setCopyMode('markdown_source', onRefreshInspector),
    },
    {
      id: 'clip-mode-plain',
      category: m.cmd_cat_clipboard(),
      label: m.cmd_clip_mode_plain(),
      action: () => settingsStore.setCopyMode('plain', onRefreshInspector),
    },
    {
      id: 'clip-mode-rich',
      category: m.cmd_cat_clipboard(),
      label: m.cmd_clip_mode_rich(),
      action: () => settingsStore.setCopyMode('rich', onRefreshInspector),
    },
    {
      id: 'clip-inspector',
      category: m.cmd_cat_clipboard(),
      label: m.cmd_clip_inspector(),
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
            category: m.cmd_cat_vault(),
            label: m.cmd_note_new(),
            shortcut: shortcutLabel('Ctrl+N'),
            action: () => {
              const stamp = new Date()
                .toISOString()
                .slice(0, 19)
                .replace(/[:T]/g, '-');
              void vaultStore.createNoteInVault('', m.note_default_name({ stamp }));
            },
          },
          {
            id: 'note-save',
            category: m.cmd_cat_vault(),
            label: m.cmd_note_save(),
            shortcut: shortcutLabel('Ctrl+S'),
            action: () => {
              if (tabsStore.activeTabPath) {
                void tabsStore.flushAutosaveFor(tabsStore.activeTabPath);
              }
            },
          },
          {
            id: 'note-close',
            category: m.cmd_cat_vault(),
            label: m.cmd_note_close(),
            shortcut: shortcutLabel('Ctrl+W'),
            action: () => {
              if (tabsStore.activeTabPath) {
                void tabsStore.closeTab(tabsStore.activeTabPath);
              }
            },
          },
          {
            id: 'vault-rebuild-index',
            category: m.cmd_cat_vault(),
            label: m.cmd_vault_rebuild(),
            action: () => void vaultStore.rebuildIndex(),
          },
          {
            id: 'vault-switch',
            category: m.cmd_cat_vault(),
            label: m.cmd_vault_switch(),
            action: () => void vaultStore.switchVault(),
          },
        ]
      : []),
    // AI, when a note is open
    ...(tabsStore.activeTab || tabsStore.scratchpadMode
      ? [
          {
            id: 'ai-rewrite',
            category: m.cmd_cat_ai(),
            label: m.cmd_ai_rewrite(),
            action: () => aiStore.openPanel('rewrite'),
          },
          {
            id: 'ai-translate',
            category: m.cmd_cat_ai(),
            label: m.cmd_ai_translate(),
            action: () => aiStore.openPanel('translate'),
          },
          {
            id: 'ai-summarize',
            category: m.cmd_cat_ai(),
            label: m.cmd_ai_summarize(),
            action: () => aiStore.openPanel('summarize'),
          },
        ]
      : []),
    {
      id: 'ai-settings',
      category: m.cmd_cat_ai(),
      label: m.cmd_ai_settings(),
      action: () => settingsStore.openSettings('ai'),
    },
    // Export & Settings
    {
      id: 'export-html',
      category: m.cmd_cat_export(),
      label: m.cmd_export_html(),
      action: () => onExportHtml(),
    },
    {
      id: 'export-pdf',
      category: m.cmd_cat_export(),
      label: m.cmd_export_pdf(),
      shortcut: shortcutLabel('Ctrl+Shift+P'),
      action: () => onExportPdf(),
    },
    {
      id: 'open-settings',
      category: m.cmd_cat_settings(),
      label: m.cmd_open_settings(),
      shortcut: shortcutLabel('Ctrl+,'),
      action: () => (settingsStore.showSettings = true),
    },
    // Theme
    {
      id: 'theme-system',
      category: m.cmd_cat_appearance(),
      label: m.cmd_theme_system(),
      action: () => settingsStore.setThemePreference('system'),
    },
    {
      id: 'theme-light',
      category: m.cmd_cat_appearance(),
      label: m.cmd_theme_light(),
      action: () => settingsStore.setThemePreference('light'),
    },
    {
      id: 'theme-dark',
      category: m.cmd_cat_appearance(),
      label: m.cmd_theme_dark(),
      action: () => settingsStore.setThemePreference('dark'),
    },
    {
      id: 'theme-high-contrast',
      category: m.cmd_cat_appearance(),
      label: m.cmd_theme_high_contrast(),
      action: () => settingsStore.setThemePreference('high-contrast'),
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

<Dialog label={m.cmd_palette()} {onClose} onkeydown={handleKeyDown}>
  <div class="picker-search">
    <Icon name="magnifying-glass" />
    <input
      bind:this={searchInputEl}
      type="text"
      class="picker-input"
      placeholder={m.cmd_placeholder()}
      aria-label={m.cmd_search_label()}
      bind:value={query}
    />
    <kbd>Esc</kbd>
  </div>

  <div class="picker-list" bind:this={listEl} role="listbox" aria-label={m.cmd_list_label()}>
    {#if filteredCommands.length === 0}
      <div class="picker-empty">{m.cmd_no_match({ query: query.trim() })}</div>
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
    <span><kbd>↑</kbd> <kbd>↓</kbd> {m.picker_hint_move()}</span>
    <span><kbd>Enter</kbd> {m.picker_hint_run()}</span>
    <span><kbd>Esc</kbd> {m.picker_hint_close()}</span>
  </div>
</Dialog>
