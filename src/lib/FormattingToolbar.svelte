<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import type { MarkdownFormatCommandId } from './markdownCommands';
  import { settingsStore } from './stores/settings.svelte';

  interface Props {
    onFormat: (cmd: MarkdownFormatCommandId) => void;
  }

  let { onFormat }: Props = $props();
</script>

<div class="formatting-toolbar" role="toolbar" aria-label="Markdown formatting toolbar">
  <div class="toolbar-left">
    <!-- Headings -->
    <div class="btn-group" aria-label="Headings">
      <button
        type="button"
        class="fmt-btn text-btn"
        title="Heading 1 (Ctrl+1)"
        aria-label="Heading 1"
        onclick={() => onFormat('h1')}
      >
        H1
      </button>
      <button
        type="button"
        class="fmt-btn text-btn"
        title="Heading 2 (Ctrl+2)"
        aria-label="Heading 2"
        onclick={() => onFormat('h2')}
      >
        H2
      </button>
      <button
        type="button"
        class="fmt-btn text-btn"
        title="Heading 3 (Ctrl+3)"
        aria-label="Heading 3"
        onclick={() => onFormat('h3')}
      >
        H3
      </button>
    </div>

    <div class="sep" role="separator"></div>

    <!-- Inline Formatting -->
    <div class="btn-group" aria-label="Inline formatting">
      <button
        type="button"
        class="fmt-btn"
        title="Bold (Ctrl+B)"
        aria-label="Bold"
        onclick={() => onFormat('bold')}
      >
        <Icon name="text-b" />
      </button>
      <button
        type="button"
        class="fmt-btn"
        title="Italic (Ctrl+I)"
        aria-label="Italic"
        onclick={() => onFormat('italic')}
      >
        <Icon name="text-italic" />
      </button>
      <button
        type="button"
        class="fmt-btn"
        title="Strikethrough (Ctrl+Shift+S)"
        aria-label="Strikethrough"
        onclick={() => onFormat('strikethrough')}
      >
        <Icon name="text-strikethrough" />
      </button>
      <button
        type="button"
        class="fmt-btn"
        title="Inline code (Ctrl+E)"
        aria-label="Inline code"
        onclick={() => onFormat('inline_code')}
      >
        <Icon name="code" />
      </button>
    </div>

    <div class="sep" role="separator"></div>

    <!-- Lists & Blocks -->
    <div class="btn-group" aria-label="Lists and blocks">
      <button
        type="button"
        class="fmt-btn"
        title="Bullet list (Ctrl+Shift+8)"
        aria-label="Bullet list"
        onclick={() => onFormat('bullet_list')}
      >
        <Icon name="list-bullets" />
      </button>
      <button
        type="button"
        class="fmt-btn text-btn"
        title="Numbered list (Ctrl+Shift+7)"
        aria-label="Numbered list"
        onclick={() => onFormat('ordered_list')}
      >
        1.
      </button>
      <button
        type="button"
        class="fmt-btn"
        title="Task list (Ctrl+Shift+9 / Ctrl+L)"
        aria-label="Task list"
        onclick={() => onFormat('task_list')}
      >
        <Icon name="list-checks" />
      </button>
      <button
        type="button"
        class="fmt-btn"
        title="Blockquote (Ctrl+Shift+.)"
        aria-label="Blockquote"
        onclick={() => onFormat('blockquote')}
      >
        <Icon name="quotes" />
      </button>
      <button
        type="button"
        class="fmt-btn text-btn"
        title="Fenced code block (Ctrl+Shift+K)"
        aria-label="Fenced code block"
        onclick={() => onFormat('code_block')}
      >
        ```
      </button>
      <button
        type="button"
        class="fmt-btn"
        title="Insert table"
        aria-label="Insert table"
        onclick={() => onFormat('table')}
      >
        <Icon name="table" />
      </button>
      <button
        type="button"
        class="fmt-btn"
        title="Insert link (Ctrl+K)"
        aria-label="Insert link"
        onclick={() => onFormat('link')}
      >
        <Icon name="link-simple" />
      </button>
      <button
        type="button"
        class="fmt-btn text-btn"
        title="Insert Obsidian [[wiki link]]"
        aria-label="Insert wiki link"
        onclick={() => onFormat('wikilink')}
      >
        [[]]
      </button>
    </div>
  </div>

  <div class="toolbar-right">
    <button
      type="button"
      class="fmt-btn pill-btn"
      class:active-pill={settingsStore.typewriterScrolling}
      aria-pressed={settingsStore.typewriterScrolling}
      title="Typewriter scrolling — keep active line vertically centered"
      onclick={() =>
        settingsStore.setTypewriterScrolling(!settingsStore.typewriterScrolling)}
    >
      <span>Typewriter</span>
    </button>

    <button
      type="button"
      class="fmt-btn pill-btn"
      class:active-pill={settingsStore.dimInactiveParagraphs}
      aria-pressed={settingsStore.dimInactiveParagraphs}
      title="Dim all lines except the current paragraph"
      onclick={() =>
        settingsStore.setDimInactiveParagraphs(
          !settingsStore.dimInactiveParagraphs
        )}
    >
      <span>Focus Paragraph</span>
    </button>

    <div class="sep" role="separator"></div>

    <button
      type="button"
      class="fmt-btn pill-btn"
      title="Minimal focus mode — hide sidebar, tabs, toolbar, and status bar (Ctrl+Shift+M)"
      onclick={() => settingsStore.toggleMinimalFocus()}
    >
      <span>Minimal</span>
    </button>

    <button
      type="button"
      class="fmt-btn pill-btn"
      title="Zen full-screen focus mode — only the text canvas visible (F11)"
      onclick={() => settingsStore.toggleZenFocus()}
    >
      <Icon name="arrows-out-simple" />
      <span>Zen</span>
    </button>
  </div>
</div>

<style>
  .formatting-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-3);
    border-bottom: 1px solid var(--line-subtle);
    background: var(--surface-subtle);
    flex-shrink: 0;
    overflow-x: auto;
    scrollbar-width: none;
    user-select: none;
  }

  .toolbar-left,
  .toolbar-right {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex-shrink: 0;
  }

  .btn-group {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }

  .sep {
    width: 1px;
    height: 18px;
    background: var(--line-subtle);
    margin: 0 var(--space-1);
  }

  .fmt-btn {
    min-width: var(--control-min-height);
    min-height: var(--control-min-height);
    padding: 0 var(--space-2);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-1);
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font-size: var(--fs-12);
    font-weight: 500;
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      color var(--transition-fast),
      border-color var(--transition-fast);
  }

  .fmt-btn.text-btn {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: var(--fs-11);
  }

  .fmt-btn.pill-btn {
    border-color: var(--line-subtle);
    background: var(--surface-field);
    padding: 0 var(--space-2);
  }

  .fmt-btn:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .fmt-btn.active-pill {
    border-color: var(--accent);
    background: var(--selection);
    color: var(--text-primary);
    font-weight: 600;
  }
</style>
