<script lang="ts">
  /**
   * The open note rendered beside its text. The HTML comes from the same Rust renderer that
   * writes HTML and PDF exports, so the preview shows what an export will look like, with
   * math, diagrams, callouts and footnotes handled by the frontend as they are for print.
   */
  import { renderNoteHtmlPreview } from './ipc';
  import { enhanceRenderedHtml, exportDiagramCss } from './richBlocks';
  import { EXPORT_CALLOUT_CSS } from './callouts';
  import { EXPORT_FOOTNOTE_CSS } from './footnotes';
  import { settingsStore } from './stores/settings.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { m } from './i18n.svelte';

  /** Long enough that a burst of typing renders once, short enough to feel live. */
  const RENDER_DELAY_MS = 280;

  let body = $state('');
  let error = $state('');
  let scrollHost: HTMLDivElement | undefined = $state();
  let keptScrollTop = 0;

  let source = $derived(
    tabsStore.activeTab?.content ?? (tabsStore.scratchpadMode ? tabsStore.scratchpadContent : null)
  );
  let title = $derived(
    tabsStore.activeTab?.title.replace(/\.md$/i, '') || m.scratchpad_title()
  );
  let styles = $derived(
    `${exportDiagramCss(settingsStore.effectiveDark)}\n${EXPORT_CALLOUT_CSS}\n${EXPORT_FOOTNOTE_CSS}`
  );

  async function render(noteTitle: string, markdown: string, dark: boolean): Promise<string> {
    const rendered = await renderNoteHtmlPreview(noteTitle, markdown, dark);
    const full = await enhanceRenderedHtml(rendered, 'print', dark);
    const doc = new DOMParser().parseFromString(full, 'text/html');
    // Take the article only: the export document's own theme would restyle the app around it.
    return doc.querySelector('main, article')?.innerHTML ?? doc.body.innerHTML;
  }

  $effect(() => {
    const markdown = source;
    const noteTitle = title;
    const dark = settingsStore.effectiveDark;
    if (markdown === null) {
      body = '';
      error = '';
      return;
    }
    let dropped = false;
    const timer = setTimeout(() => {
      void render(noteTitle, markdown, dark)
        .then((html) => {
          if (dropped) return;
          keptScrollTop = scrollHost?.scrollTop ?? 0;
          body = html;
          error = '';
        })
        .catch((err) => {
          if (dropped) return;
          error = err instanceof Error ? err.message : String(err);
        });
    }, RENDER_DELAY_MS);
    return () => {
      dropped = true;
      clearTimeout(timer);
    };
  });

  // Rendering replaces the whole article, which would otherwise jump back to the top on
  // every keystroke.
  $effect(() => {
    void body;
    if (scrollHost) scrollHost.scrollTop = keptScrollTop;
  });

  /**
   * Keeps clicks inside the pane. Footnote markers and the back-references they pair with
   * scroll the preview; nothing else in a rendered note navigates.
   */
  function handleClick(event: MouseEvent) {
    const anchor = (event.target as HTMLElement | null)?.closest('a');
    if (!anchor) return;
    event.preventDefault();
    const href = anchor.getAttribute('href') ?? '';
    if (!href.startsWith('#') || !scrollHost) return;
    const target = scrollHost.querySelector(`[id="${CSS.escape(href.slice(1))}"]`);
    target?.scrollIntoView({ block: 'center' });
  }
</script>

<!-- Rendered note text, not a control; clicks only keep links from leaving the app. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="preview-scroll"
  bind:this={scrollHost}
  onscroll={() => (keptScrollTop = scrollHost?.scrollTop ?? 0)}
  onclick={handleClick}
>
  {#if error}
    <p class="preview-note">{m.split_preview_failed({ error })}</p>
  {:else if !body}
    <p class="preview-note">{m.split_preview_empty()}</p>
  {:else}
    <article class="preview-body">
      {@html `<style>${styles}</style>${body}`}
    </article>
  {/if}
</div>

<style>
  .preview-scroll {
    height: 100%;
    overflow-y: auto;
    background: var(--surface-editor);
  }

  .preview-note {
    margin: 0;
    padding: var(--space-12) var(--space-6);
    color: var(--text-muted);
    text-align: center;
  }

  .preview-body {
    max-width: var(--editor-max-width);
    margin: 0 auto;
    padding: var(--space-12) var(--space-6);
    color: var(--text-primary);
    font-family: var(--font-body);
    font-size: var(--editor-font-size, var(--fs-17));
    line-height: var(--leading-relaxed);
    overflow-wrap: break-word;
  }

  .preview-body :global(h1),
  .preview-body :global(h2),
  .preview-body :global(h3),
  .preview-body :global(h4),
  .preview-body :global(h5),
  .preview-body :global(h6) {
    margin: 1.5em 0 0.5em;
    font-family: var(--font-display);
    line-height: var(--leading-tight);
    color: var(--text-primary);
  }

  .preview-body :global(h1) {
    margin-top: 0;
    font-size: 1.8em;
  }

  .preview-body :global(h2) {
    font-size: 1.4em;
  }

  .preview-body :global(h3) {
    font-size: 1.2em;
  }

  .preview-body :global(p),
  .preview-body :global(ul),
  .preview-body :global(ol),
  .preview-body :global(blockquote),
  .preview-body :global(table) {
    margin: 0 0 1em;
  }

  .preview-body :global(a) {
    color: var(--accent);
  }

  .preview-body :global(code) {
    padding: 1px 4px;
    border-radius: var(--radius-control);
    background: var(--surface-app);
    font-family: var(--font-mono);
    font-size: 0.9em;
  }

  .preview-body :global(pre) {
    margin: 0 0 1em;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-card);
    background: var(--surface-app);
    overflow-x: auto;
  }

  .preview-body :global(pre code) {
    padding: 0;
    background: none;
  }

  .preview-body :global(blockquote) {
    padding: 2px var(--space-4);
    border-left: 3px solid var(--line-control);
    color: var(--text-secondary);
  }

  .preview-body :global(table) {
    border-collapse: collapse;
  }

  .preview-body :global(th),
  .preview-body :global(td) {
    padding: 4px var(--space-3);
    border: 1px solid var(--line-subtle);
    text-align: left;
  }

  .preview-body :global(img) {
    max-width: 100%;
    height: auto;
    border-radius: var(--radius-card);
  }

  .preview-body :global(hr) {
    margin: 1.5em 0;
    border: 0;
    border-top: 1px solid var(--line-subtle);
  }

  .preview-body :global(input[type='checkbox']) {
    margin-right: 6px;
  }

  .preview-body :global(li) {
    margin-bottom: 0.25em;
  }
</style>
