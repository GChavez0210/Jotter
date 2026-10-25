<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { appStorage } from './appStorage';
  import { CAPTURE_TIMEOUT_ERROR, hideCaptureWindow, listenQuickCaptureOpen, requestCapture } from './ipc';
  import { settingsStore } from './stores/settings.svelte';
  import { i18n, m } from './i18n.svelte';

  let text = $state('');
  let status = $state('');
  let failed = $state(false);
  let saving = $state(false);
  let box: HTMLTextAreaElement | undefined = $state();

  /** The window outlives the main window's settings changes, so it rereads them each time it opens. */
  async function reloadSettings() {
    await appStorage.load();
    settingsStore.initAppearance();
    i18n.reloadPreference();
    // The title shows in the task switcher, so it follows the language too.
    void getCurrentWindow().setTitle(m.capture_window_title()).catch(() => {});
  }

  function focusBox() {
    status = '';
    failed = false;
    box?.focus();
    void reloadSettings();
  }

  async function save() {
    const entry = text.trim();
    if (!entry || saving) return;
    saving = true;
    try {
      await appStorage.load();
      const note = appStorage.getItem('jotter.quickCaptureNote') || 'Inbox.md';
      // The main window saves it, so a note open there keeps any unsaved typing.
      await requestCapture(note, entry);
      text = '';
      failed = false;
      await hideCaptureWindow();
    } catch (err) {
      failed = true;
      const message = err instanceof Error ? err.message : String(err);
      status = message === 'No vault is currently open'
        ? m.capture_no_vault()
        : message === CAPTURE_TIMEOUT_ERROR
          ? m.capture_timeout()
          : m.capture_failed({ error: message });
    } finally {
      saving = false;
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      void hideCaptureWindow();
    } else if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void save();
    }
  }

  onMount(() => {
    focusBox();
    let unlisten: (() => void) | undefined;
    void listenQuickCaptureOpen(focusBox).then((fn) => (unlisten = fn));
    // The window is hidden, not closed, so refocus whenever it comes back to the front.
    const onFocus = () => box?.focus();
    window.addEventListener('focus', onFocus);
    return () => {
      unlisten?.();
      window.removeEventListener('focus', onFocus);
    };
  });
</script>

<main class="capture">
  <!-- The window has no title bar, so the strip above the text box moves it. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="drag" onmousedown={() => void getCurrentWindow().startDragging()}></div>
  <textarea
    bind:this={box}
    bind:value={text}
    class="box"
    rows="4"
    placeholder={m.capture_placeholder()}
    aria-label={m.settings_capture()}
    onkeydown={onKeyDown}
  ></textarea>
  <div class="footer">
    <span class="status" class:failed role={failed ? 'alert' : 'status'}>{status || m.capture_hint()}</span>
  </div>
</main>

<style>
  .capture {
    display: flex;
    flex-direction: column;
    height: 100vh;
    box-sizing: border-box;
    border: 1px solid var(--line-control);
    background: var(--surface-raised);
    color: var(--text-primary);
  }

  .drag {
    flex: none;
    height: 14px;
    cursor: grab;
  }

  .box {
    flex: 1;
    min-height: 0;
    margin: 0 var(--space-3);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-control);
    background: var(--surface-editor);
    color: inherit;
    font: var(--fs-15) / 1.5 var(--font-ui);
    resize: none;
  }

  .box:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .footer {
    flex: none;
    padding: var(--space-2) var(--space-4);
  }

  .status {
    color: var(--text-muted);
    font-size: var(--fs-12);
  }

  .status.failed {
    color: var(--status-danger);
  }
</style>
