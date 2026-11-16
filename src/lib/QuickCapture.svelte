<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { appStorage } from './appStorage';
  import { CAPTURE_TIMEOUT_ERROR, hideCaptureWindow, listenQuickCaptureOpen, requestCapture, selectCaptureProfile } from './ipc';
  import { settingsStore } from './stores/settings.svelte';
  import { INBOX_PROFILE_ID, readProfiles, type CaptureProfile } from './captureProfiles';
  import { i18n, m } from './i18n.svelte';

  let text = $state('');
  let status = $state('');
  let failed = $state(false);
  let saving = $state(false);
  let box: HTMLTextAreaElement | undefined = $state();
  let profiles = $state<CaptureProfile[]>([]);
  let profile = $state<string>(INBOX_PROFILE_ID);

  function readStoredProfiles(): CaptureProfile[] {
    try {
      const raw = appStorage.getItem('jotter.captureProfiles');
      return raw ? readProfiles(JSON.parse(raw)) : [];
    } catch {
      return [];
    }
  }

  function chooseProfile(id: string) {
    profile = id;
    void selectCaptureProfile(id).catch((err) => {
      failed = true;
      status = captureErrorMessage(String(err));
    });
    box?.focus();
  }

  /** Turns a capture error from the main window into a message in the interface language. */
  function captureErrorMessage(message: string): string {
    if (message === 'No vault is currently open') return m.capture_no_vault();
    if (message === CAPTURE_TIMEOUT_ERROR) return m.capture_timeout();
    const planned = /^capture-(empty|path|collision)(?:: (.*))?$/s.exec(message);
    if (planned?.[1] === 'empty') return m.capture_error_empty();
    if (planned?.[1] === 'path') return m.capture_error_path({ path: planned[2] ?? '' });
    if (planned?.[1] === 'collision') return m.capture_error_collision({ path: planned[2] ?? '' });
    return m.capture_failed({ error: message });
  }

  /** The window outlives the main window's settings changes, so it rereads them each time it opens. */
  async function reloadSettings() {
    await appStorage.load({ readOnly: true });
    profiles = readStoredProfiles();
    const saved = appStorage.getItem('jotter.quickCaptureProfile');
    profile = saved && profiles.some((p) => p.id === saved) ? saved : INBOX_PROFILE_ID;
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
      await appStorage.load({ readOnly: true });
      const note = appStorage.getItem('jotter.quickCaptureNote') || 'Inbox.md';
      // The main window saves it, so a note open there keeps any unsaved typing.
      await requestCapture(note, entry, profile);
      text = '';
      failed = false;
      await hideCaptureWindow();
    } catch (err) {
      failed = true;
      status = captureErrorMessage(err instanceof Error ? err.message : String(err));
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
    {#if profiles.length > 0}
      <label class="profile">
        <span class="profile-label">{m.capture_profile_label()}</span>
        <select value={profile} onchange={(e) => chooseProfile(e.currentTarget.value)}>
          <option value={INBOX_PROFILE_ID}>{m.capture_profile_inbox()}</option>
          {#each profiles as p (p.id)}
            <option value={p.id}>{p.name || m.capture_profile_unnamed()}</option>
          {/each}
        </select>
      </label>
    {/if}
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
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
  }

  .profile {
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-muted);
    font-size: var(--fs-12);
  }

  .profile select {
    max-width: 160px;
    min-height: 24px;
    border: 1px solid var(--line-control);
    border-radius: var(--radius-control);
    background: var(--surface-field);
    color: var(--text-primary);
    font: var(--fs-12) var(--font-ui);
  }

  .profile select:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .status {
    color: var(--text-muted);
    font-size: var(--fs-12);
  }

  .status.failed {
    color: var(--status-danger);
  }
</style>
