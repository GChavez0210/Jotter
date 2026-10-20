<script lang="ts">
  import { defaultSummonShortcut, shortcutLabel } from './platform';
  import { onMount } from 'svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import aboutLogo from '../assets/about-jack.png';
  import Icon from './ui/Icon.svelte';
  import Button from './ui/Button.svelte';
  import IconButton from './ui/IconButton.svelte';
  import Segmented from './ui/Segmented.svelte';
  import Dialog from './ui/Dialog.svelte';
  import ChoiceCards from './ui/ChoiceCards.svelte';
  import { COPY_MODE_LABELS, settingsStore } from './stores/settings.svelte';
  import { ACCENT_PRESETS } from './designTokens';
  import { systemStore } from './stores/system.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import { PROVIDER_NAMES } from './syncConflicts';
  import { taskCalendarStore } from './stores/taskCalendar.svelte';
  import { spellingStore } from './stores/spelling.svelte';
  import { openDictionariesFolder } from './ipc';
  import type { CopyMode } from './types';

  interface Props {
    onClose: () => void;
    onRefreshInspector: () => void;
  }

  let { onClose, onRefreshInspector }: Props = $props();

  type SettingsSection = 'editor' | 'copy' | 'appearance' | 'vault' | 'system' | 'about';
  let appVersion = $state('');
  let activeSection = $state<SettingsSection>('editor');
  let rebuildStatus = $state('');
  let isRebuilding = $state(false);
  let closeBtnRef: HTMLButtonElement | undefined = $state();


  const COPY_MODE_DETAILS: Record<
    CopyMode,
    { badge: string; formats: string; details: string }
  > = {
    markdown_source: {
      badge: 'Default',
      formats: 'Writes text/plain + text/html (<br> & &nbsp;)',
      details:
        'Preserves raw Markdown syntax and exact Notepad line breaks and indentation across Google Chat, Slack, Gmail, Word, and Notepad.',
    },
    plain: {
      badge: 'Opt-in Clean Text',
      formats: 'Writes text/plain ONLY (no CF_HTML)',
      details:
        'Strips Markdown markers (#, **, *, ~~, `, [label](url), [[Target|Alias]] outside inline code) and drops ``` / ~~~ fence lines so line counts match.',
    },
    rich: {
      badge: 'Formatted HTML',
      formats: 'Writes text/html (<br> & &nbsp;) + text/plain',
      details:
        'Renders inline Markdown (bold, italic, code, links) via comrak, drops code fences, preserves leading/repeated spaces with &nbsp;, and joins lines strictly with <br> instead of <p> margins.',
    },
  };

  function handleSelectCopyMode(mode: CopyMode) {
    settingsStore.setCopyMode(mode, () => {
      if (settingsStore.showInspector) onRefreshInspector();
    });
  }

  async function handleRebuildIndex() {
    if (!vaultStore.vaultSnapshot || isRebuilding) return;
    isRebuilding = true;
    rebuildStatus = 'Rebuilding SQLite and Tantivy indexes...';
    const count = await vaultStore.rebuildIndex();
    isRebuilding = false;
    if (count !== null) {
      rebuildStatus = `Rebuilt index for ${count} notes (in-app edit_log preserved).`;
    } else {
      rebuildStatus = 'Failed to rebuild vault index.';
    }
  }

  onMount(() => {
    closeBtnRef?.focus();
    void spellingStore.ensureLoaded();
    getVersion()
      .then((version) => (appVersion = version))
      .catch(() => {
        // Not running inside Tauri (browser preview).
      });
  });
</script>

<Dialog label="Settings" variant="panel" {onClose}>
    <div class="settings-body">
      <nav class="settings-nav" aria-label="Settings categories">
        <h2 class="nav-title">Settings</h2>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'editor'}
          aria-current={activeSection === 'editor' ? 'page' : undefined}
          onclick={() => (activeSection = 'editor')}
        >
          <Icon name="pencil-simple" />
          <span>Editor</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'copy'}
          aria-current={activeSection === 'copy' ? 'page' : undefined}
          onclick={() => (activeSection = 'copy')}
        >
          <Icon name="copy" />
          <span>Copy</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'appearance'}
          aria-current={activeSection === 'appearance' ? 'page' : undefined}
          onclick={() => (activeSection = 'appearance')}
        >
          <Icon name="eye" />
          <span>Appearance</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'vault'}
          aria-current={activeSection === 'vault' ? 'page' : undefined}
          onclick={() => (activeSection = 'vault')}
        >
          <Icon name="folder-simple" />
          <span>Vault</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'system'}
          aria-current={activeSection === 'system' ? 'page' : undefined}
          onclick={() => (activeSection = 'system')}
        >
          <Icon name="gear-six" />
          <span>System</span>
        </button>
        <button
          type="button"
          class="nav-item nav-bottom"
          class:active={activeSection === 'about'}
          aria-current={activeSection === 'about' ? 'page' : undefined}
          onclick={() => (activeSection = 'about')}
        >
          <Icon name="info" />
          <span>About</span>
        </button>
      </nav>

      <div class="settings-content">
        <div class="close-btn">
          <IconButton
            bind:ref={closeBtnRef}
            icon="x"
            label="Close settings"
            tooltip="Close settings (Esc)"
            onclick={onClose}
          />
        </div>
        {#if activeSection === 'editor'}
          <section class="settings-section" aria-labelledby="sec-editor">
            <h3 id="sec-editor" class="section-title">Editor</h3>
            <p class="section-subtitle">
              Configure default editing mode, focus aids, font scale, and image paste syntax.
            </p>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">Default editing mode</div>
                <div class="setting-desc">
                  Live mode renders headings, emphasis, links, task checkboxes, and images inline while keeping raw Markdown on the active cursor line ({shortcutLabel('Ctrl+/')}).
                </div>
              </div>
              <Segmented
                label="Default editing mode"
                value={settingsStore.editingMode}
                options={[
                  { value: 'live', label: 'Live Preview' },
                  { value: 'source', label: 'Source' },
                ]}
                onchange={(mode) => settingsStore.setEditingMode(mode)}
              />
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-typewriter" class="setting-label">
                  Typewriter scrolling
                </label>
                <div class="setting-desc">
                  Keep the active cursor line vertically centered in the editor viewport while typing.
                </div>
              </div>
              <input
                id="toggle-typewriter"
                type="checkbox"
                class="fluent-toggle"
                checked={settingsStore.typewriterScrolling}
                onchange={(e) =>
                  settingsStore.setTypewriterScrolling(
                    (e.currentTarget as HTMLInputElement).checked
                  )}
              />
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-dimming" class="setting-label">
                  Focus paragraph dimming
                </label>
                <div class="setting-desc">
                  Dim all lines outside the current paragraph to help focus on the active block.
                </div>
              </div>
              <input
                id="toggle-dimming"
                type="checkbox"
                class="fluent-toggle"
                checked={settingsStore.dimInactiveParagraphs}
                onchange={(e) =>
                  settingsStore.setDimInactiveParagraphs(
                    (e.currentTarget as HTMLInputElement).checked
                  )}
              />
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-spellcheck" class="setting-label">
                  Check spelling
                </label>
                <div class="setting-desc">
                  Underline misspelled words. Right-click a word for suggestions.
                </div>
              </div>
              <input
                id="toggle-spellcheck"
                type="checkbox"
                class="fluent-toggle"
                checked={settingsStore.spellCheck}
                onchange={(e) =>
                  settingsStore.setSpellCheck((e.currentTarget as HTMLInputElement).checked)}
              />
            </div>

            {#if settingsStore.spellCheck}
              <div class="setting-card column-card">
                <div class="setting-row-top">
                  <div class="setting-info">
                    <div class="setting-label">Spelling dictionaries</div>
                    <div class="setting-desc">
                      Jotter's dictionaries let each vault and note pick its languages. System uses the languages installed in your operating system for every note.
                    </div>
                  </div>
                  <Segmented
                    label="Spelling dictionaries"
                    value={spellingStore.engine}
                    options={[
                      { value: 'jotter', label: 'Jotter' },
                      { value: 'system', label: 'System' },
                    ]}
                    onchange={(engine) => spellingStore.setEngine(engine)}
                  />
                </div>
                {#if spellingStore.engine === 'jotter'}
                  <div class="setting-row-top">
                    <div class="setting-info">
                      <div class="setting-label">Languages for this vault</div>
                      <div class="setting-desc">
                        A note can override these with a <code>lang</code> property, such as <code>lang: es</code> or <code>lang: [en, fr]</code>. Set <code>spellcheck: false</code> to skip a note.
                      </div>
                    </div>
                    <div class="button-row">
                      <Button icon="arrow-clockwise" onclick={() => void spellingStore.reload()}>
                        Reload
                      </Button>
                      <Button icon="folder-open" onclick={() => void openDictionariesFolder()}>
                        Open folder
                      </Button>
                    </div>
                  </div>
                  {#if !vaultStore.vaultSnapshot}
                    <div class="setting-desc">Open a vault to choose its languages.</div>
                  {:else}
                    <ul class="snippet-list dictionary-list">
                      {#each spellingStore.dictionaries as dict (dict.id)}
                        <li>
                          <label for="dict-{dict.id}">
                            {dict.name}
                            {#if !dict.bundled}<span class="dict-source">added</span>{/if}
                          </label>
                          <input
                            id="dict-{dict.id}"
                            type="checkbox"
                            class="fluent-toggle"
                            checked={spellingStore.vaultLanguages.includes(dict.id)}
                            onchange={(e) =>
                              spellingStore.toggleVaultLanguage(
                                dict.id,
                                (e.currentTarget as HTMLInputElement).checked
                              )}
                          />
                        </li>
                      {/each}
                    </ul>
                  {/if}
                  <div class="setting-desc">
                    To add a language, put a Hunspell <code>.aff</code> and <code>.dic</code> pair with the same name in the folder, then reload. Words you add from the editor go to <code>personal.txt</code> there.
                  </div>
                  {#if spellingStore.loadError}
                    <div class="field-error" role="alert">{spellingStore.loadError}</div>
                  {/if}
                {/if}
              </div>
            {/if}

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">Editor font size</div>
                <div class="setting-desc">
                  Size of note text: 12, 14, 16 or 20 px.
                </div>
              </div>
              <Segmented
                label="Editor font size"
                value={settingsStore.editorFontSize}
                options={[12, 14, 16, 20].map((px) => ({ value: px, label: `${px} px` }))}
                onchange={(px) => settingsStore.setEditorFontSize(px)}
              />
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">Pasted image link format</div>
                <div class="setting-desc">
                  Choose how images pasted from the clipboard into the <code>attachments/</code> folder are inserted into the note.
                </div>
              </div>
              <Segmented
                label="Pasted image link format"
                value={settingsStore.imageLinkFormat}
                options={[
                  { value: 'wikilink', label: '![[name]]' },
                  { value: 'markdown', label: '![](path)' },
                ]}
                onchange={(format) => settingsStore.setImageLinkFormat(format)}
              />
            </div>
          </section>
        {:else if activeSection === 'copy'}
          <section class="settings-section" aria-labelledby="sec-copy">
            <h3 id="sec-copy" class="section-title">Copy</h3>
            <p class="section-subtitle">
              Choose the default clipboard copy mode. Every mode guarantees Notepad-identical line breaks when pasting into Google Chat, Slack, Gmail, Word, or Notepad.
            </p>

            <div class="copy-modes-list" role="radiogroup" aria-label="Default copy mode">
              {#each ['markdown_source', 'plain', 'rich'] as const as mode}
                <label
                  class="copy-mode-card"
                  class:selected={settingsStore.copyMode === mode}
                >
                  <input
                    type="radio"
                    name="default-copy-mode"
                    checked={settingsStore.copyMode === mode}
                    onchange={() => handleSelectCopyMode(mode)}
                  />
                  <div class="copy-mode-body">
                    <div class="copy-mode-header">
                      <span class="copy-mode-title">
                        {COPY_MODE_LABELS[mode].label}
                      </span>
                      <span class="copy-mode-badge">
                        {COPY_MODE_DETAILS[mode].badge}
                      </span>
                    </div>
                    <div class="copy-mode-formats">
                      {COPY_MODE_DETAILS[mode].formats}
                    </div>
                    <div class="copy-mode-desc">
                      {COPY_MODE_DETAILS[mode].details}
                    </div>
                  </div>
                </label>
              {/each}
            </div>
          </section>
        {:else if activeSection === 'appearance'}
          <section class="settings-section" aria-labelledby="sec-appearance">
            <h3 id="sec-appearance" class="section-title">Appearance</h3>
            <p class="section-subtitle">
              Theme, accent and writing font.
            </p>

            <div class="setting-card column-card">
              <div class="setting-info">
                <div class="setting-label">Theme</div>
                <div class="setting-desc">System follows your operating system's light or dark setting.</div>
              </div>
              <ChoiceCards
                label="Theme"
                value={settingsStore.themePref}
                options={[
                  { value: 'system', label: 'System' },
                  { value: 'light', label: 'Paper' },
                  { value: 'dark', label: 'Night' },
                  { value: 'high-contrast', label: 'High Contrast Dark' },
                ]}
                onchange={(pref) => settingsStore.setThemePreference(pref)}
              >
                {#snippet preview(pref)}
                  <span class="theme-preview {pref}" aria-hidden="true">
                    <span class="tp-side"></span>
                    <span class="tp-page"><span></span><span></span></span>
                  </span>
                {/snippet}
              </ChoiceCards>
            </div>

            <div class="setting-card column-card">
              <div class="setting-row-top">
                <div class="setting-info">
                  <div class="setting-label">Accent color</div>
                  <div class="setting-desc">
                    Used for links, the active note, selection and primary buttons.
                  </div>
                </div>
                <Segmented
                  label="Accent color source"
                  value={settingsStore.accentColorMode}
                  options={[
                    { value: 'preset', label: 'Jotter' },
                    { value: 'system', label: 'System' },
                    { value: 'custom', label: 'Custom' },
                  ]}
                  onchange={(mode) => settingsStore.setAccentColor(mode)}
                />
              </div>

              {#if settingsStore.accentColorMode === 'preset'}
                <div class="accent-swatches" role="radiogroup" aria-label="Accent preset">
                  {#each ACCENT_PRESETS as swatch (swatch.id)}
                    {@const selected = settingsStore.accentPreset === swatch.id}
                    <button
                      type="button"
                      role="radio"
                      class="swatch"
                      aria-checked={selected}
                      onclick={() => settingsStore.setAccentColor('preset', swatch.id)}
                    >
                      <span
                        class="swatch-dot"
                        class:active={selected}
                        style:background-color={settingsStore.effectiveDark ? swatch.dark : swatch.light}
                      ></span>
                      {swatch.label}
                    </button>
                  {/each}
                </div>
              {:else if settingsStore.accentColorMode === 'custom'}
                <div class="accent-swatches">
                  <label class="color-picker-label" title="Custom hex color">
                    <span>Pick a color</span>
                    <input
                      type="color"
                      value={settingsStore.customAccentColor}
                      oninput={(e) =>
                        settingsStore.setAccentColor(
                          'custom',
                          (e.currentTarget as HTMLInputElement).value
                        )}
                    />
                  </label>
                </div>
              {/if}
            </div>

            <div class="setting-card column-card">
              <div class="setting-info">
                <div class="setting-label">Writing font</div>
                <div class="setting-desc">Font for note text. Titles and headings stay in Literata.</div>
              </div>
              <ChoiceCards
                label="Writing font"
                value={settingsStore.writingFont}
                options={[
                  { value: 'serif', label: 'Serif · Literata' },
                  { value: 'sans', label: 'Sans · Instrument' },
                  { value: 'mono', label: 'Mono · JetBrains' },
                ]}
                onchange={(font) => settingsStore.setWritingFont(font)}
              >
                {#snippet preview(font)}
                  <span class="font-preview {font}" aria-hidden="true">Aa</span>
                {/snippet}
              </ChoiceCards>
            </div>

          </section>
        {:else if activeSection === 'vault'}
          <section class="settings-section" aria-labelledby="sec-vault">
            <h3 id="sec-vault" class="section-title">Vault</h3>
            <p class="section-subtitle">
              Manage the derived SQLite & Tantivy indexes and hidden file isolation.
            </p>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">Rebuild vault index</div>
                <div class="setting-desc">
                  {#if vaultStore.vaultSnapshot}
                    Re-scan all <code>.md</code> files in <strong>{vaultStore.vaultSnapshot.name}</strong> ({vaultStore.vaultSnapshot.noteCount} notes) and rebuild the SQLite metadata/links cache and Tantivy search index while preserving <code>edit_log</code>.
                  {:else}
                    Open a vault to rebuild its SQLite and Tantivy indexes.
                  {/if}
                </div>
                {#if rebuildStatus}
                  <div class="rebuild-status" role="status">{rebuildStatus}</div>
                {/if}
              </div>
              <Button
                variant="primary"
                icon="arrow-clockwise"
                disabled={!vaultStore.vaultSnapshot || isRebuilding}
                onclick={() => void handleRebuildIndex()}
              >
                {isRebuilding ? 'Rebuilding…' : 'Rebuild index'}
              </Button>
            </div>

            {#if vaultStore.vaultSnapshot}
              <div class="setting-card">
                <div class="setting-info">
                  <div class="setting-label">Cloud sync</div>
                  <div class="setting-desc">
                    {#if vaultStore.syncProvider}
                      This vault is in a {PROVIDER_NAMES[vaultStore.syncProvider]} folder. Keep the
                      vault folder downloaded on this device (in OneDrive and Google Drive, choose
                      "Always keep on this device" or "Available offline"), so search and backlinks
                      can read every note. Jotter lists conflict copies under Sync conflicts.
                    {:else}
                      No cloud folder detected. To sync, keep the vault inside your Dropbox,
                      OneDrive, Google Drive, or Box folder. Jotter still detects Dropbox, Nextcloud,
                      and Syncthing conflict copies here.
                    {/if}
                    {#if vaultStore.syncConflicts.length > 0}
                      <strong>{vaultStore.syncConflicts.length} conflict {vaultStore.syncConflicts.length === 1 ? 'copy needs' : 'copies need'} review.</strong>
                    {/if}
                  </div>
                </div>
                {#if vaultStore.syncConflicts.length > 0}
                  <Button
                    icon="warning"
                    onclick={() => {
                      settingsStore.setLeftSidebarView('conflicts');
                      onClose();
                    }}
                  >
                    Review
                  </Button>
                {/if}
              </div>
            {/if}

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-home-page" class="setting-label">Home page</label>
                <div class="setting-desc">
                  When no note is open, show cards for recent, new, linked, and forgotten notes. Click the gear on the Home page to choose its sections.
                </div>
              </div>
              <input
                id="toggle-home-page"
                type="checkbox"
                class="fluent-toggle"
                checked={settingsStore.showHomePage}
                onchange={(e) => settingsStore.setShowHomePage((e.currentTarget as HTMLInputElement).checked)}
              />
            </div>

            {#if vaultStore.vaultSnapshot}
              <div class="setting-card">
                <div class="setting-info">
                  <div class="setting-label">Task calendar</div>
                  <div class="setting-desc">
                    Writes checklist items with due dates, such as <code>- [ ] Pay rent 📅 2026-11-01</code>, to an <code>.ics</code> file and keeps it up to date. Subscribe to the file in your calendar app. Completed tasks are left out.
                    {#if taskCalendarStore.path}
                      <br />Writing to <code>{taskCalendarStore.path}</code>
                    {/if}
                    {#if taskCalendarStore.lastError}
                      <div class="rebuild-status" role="alert">{taskCalendarStore.lastError}</div>
                    {/if}
                  </div>
                </div>
                <div class="button-row">
                  <Button icon="calendar-blank" onclick={() => void taskCalendarStore.choosePath()}>
                    {taskCalendarStore.path ? 'Change file' : 'Choose file'}
                  </Button>
                  {#if taskCalendarStore.path}
                    <Button onclick={() => taskCalendarStore.turnOff()}>Turn off</Button>
                  {/if}
                </div>
              </div>
            {/if}

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-hidden-files" class="setting-label">
                  Show hidden files
                </label>
                <div class="setting-desc">
                  Off by default. `.obsidian` is always ignored for Obsidian vault safety.
                </div>
              </div>
              <input
                id="toggle-hidden-files"
                type="checkbox"
                class="fluent-toggle"
                checked={settingsStore.showHiddenFiles}
                onchange={(e) =>
                  settingsStore.setShowHiddenFiles(
                    (e.currentTarget as HTMLInputElement).checked
                  )}
                aria-label="Show hidden files"
              />
            </div>

            <div class="setting-card column-card">
              <div class="setting-info">
                <div class="setting-label">Daily notes</div>
                <div class="setting-desc">
                  <kbd>{shortcutLabel('Ctrl Shift D')}</kbd> opens today's note, named like <code>2026-10-19.md</code>, and creates it from the template note if it does not exist. In the template, <code>{'{{date}}'}</code> and <code>{'{{time}}'}</code> become the current date and time.
                </div>
              </div>
              <div class="field-row">
                <label class="text-field">
                  <span>Folder</span>
                  <input
                    type="text"
                    value={settingsStore.dailyFolder}
                    placeholder="Vault root"
                    onchange={(e) =>
                      settingsStore.setDailyFolder((e.currentTarget as HTMLInputElement).value)}
                  />
                </label>
                <label class="text-field">
                  <span>Template note</span>
                  <input
                    type="text"
                    value={settingsStore.dailyTemplatePath}
                    placeholder="None"
                    onchange={(e) =>
                      settingsStore.setDailyTemplatePath(
                        (e.currentTarget as HTMLInputElement).value
                      )}
                  />
                </label>
              </div>
            </div>
          </section>
        {:else if activeSection === 'system'}
          <section class="settings-section" aria-labelledby="sec-system">
            <h3 id="sec-system" class="section-title">System</h3>
            <p class="section-subtitle">
              Bring Jotter to the front from any app, and customize its look with your own CSS.
            </p>

            <div class="setting-card column-card">
              <div class="setting-row-top">
                <div class="setting-info">
                  <label for="toggle-summon" class="setting-label">Summon hotkey</label>
                  <div class="setting-desc">
                    Press the hotkey anywhere to show or hide Jotter. While this is on, Jotter keeps an icon in the tray and closing the window hides it instead of quitting. Quit from the tray icon.
                  </div>
                </div>
                <input
                  id="toggle-summon"
                  type="checkbox"
                  class="fluent-toggle"
                  checked={settingsStore.summonEnabled}
                  onchange={(e) =>
                    settingsStore.setSummon((e.currentTarget as HTMLInputElement).checked)}
                />
              </div>
              <label class="text-field">
                <span>Hotkey</span>
                <input
                  type="text"
                  value={settingsStore.summonShortcut}
                  placeholder={defaultSummonShortcut}
                  onchange={(e) =>
                    settingsStore.setSummon(
                      settingsStore.summonEnabled,
                      (e.currentTarget as HTMLInputElement).value
                    )}
                />
              </label>
              {#if systemStore.summonError}
                <div class="field-error" role="alert">{systemStore.summonError}</div>
              {/if}
            </div>

            <div class="setting-card column-card">
              <div class="setting-row-top">
                <div class="setting-info">
                  <div class="setting-label">CSS snippets</div>
                  <div class="setting-desc">
                    CSS snippets are small style files that customize Jotter’s appearance, such as colors, spacing, and text size. They change how the app looks without changing your notes.
                  </div>
                </div>
                <div class="button-row">
                  <Button icon="arrow-clockwise" onclick={() => void systemStore.reloadSnippets()}>
                    Reload
                  </Button>
                  <Button icon="folder-open" onclick={() => void systemStore.openSnippetsFolder()}>
                    Open folder
                  </Button>
                </div>
              </div>
              <div class="setting-desc">
                To add one, choose <strong>Open folder</strong>, create a plain-text file named
                <code>my-style.css</code> there, and save your CSS. Choose <strong>Reload</strong>,
                then turn on the file below. After editing a file, reload again to apply its changes.
              </div>
              <div class="setting-desc">
                For example, this snippet increases the editor’s line spacing:
              </div>
              <pre class="snippet-example"><code>{'.cm-content { line-height: 1.8; }'}</code></pre>
              <div class="setting-desc">
                Turn a snippet off to undo its appearance changes. Snippets load after the theme,
                so they can override its styles. They apply to every vault on this computer and
                stay in Jotter’s app data folder, outside your vaults.
              </div>
              {#if systemStore.snippets.length === 0}
                <div class="setting-desc">No snippets yet. Add a <code>.css</code> file to the folder, then reload.</div>
              {:else}
                <ul class="snippet-list">
                  {#each systemStore.snippets as snippet (snippet.name)}
                    <li>
                      <label for="snippet-{snippet.name}">{snippet.name}</label>
                      <input
                        id="snippet-{snippet.name}"
                        type="checkbox"
                        class="fluent-toggle"
                        checked={settingsStore.enabledSnippets.includes(snippet.name)}
                        onchange={(e) =>
                          settingsStore.setSnippetEnabled(
                            snippet.name,
                            (e.currentTarget as HTMLInputElement).checked
                          )}
                      />
                    </li>
                  {/each}
                </ul>
              {/if}
              {#if systemStore.snippetError}
                <div class="field-error" role="alert">{systemStore.snippetError}</div>
              {/if}
            </div>
          </section>
        {:else if activeSection === 'about'}
          <section class="settings-section about" aria-labelledby="sec-about">
            <img class="about-mark" src={aboutLogo} alt="Jack playing card with a J and full crown" width="160" height="160" />
            <div class="about-copy">
              <h3 id="sec-about" class="section-title">Jotter</h3>
              {#if appVersion}
                <p class="about-version">Version {appVersion}</p>
              {/if}
              <p class="about-desc">
                Jotter is a Markdown notes app for plain-text folders. Your notes stay as
                <code>.md</code> files on your own disk, it opens Obsidian vaults as they are,
                and it copies text that pastes cleanly into chat, email and documents.
              </p>
            </div>
          </section>
        {/if}
      </div>
    </div>
</Dialog>

<style>
  .settings-body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .settings-nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 188px;
    flex: none;
    padding: var(--space-4) var(--space-2);
    border-right: 1px solid var(--line-subtle);
    background: var(--surface-app);
  }

  .nav-title {
    margin: var(--space-1) var(--space-2) var(--space-3);
    font: 600 var(--fs-17) / 1.2 var(--font-display);
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--row-height);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .nav-bottom {
    margin-top: auto;
  }

  .nav-item:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .nav-item.active {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 500;
  }

  .settings-content {
    position: relative;
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: var(--space-5) var(--space-6) var(--space-8);
  }

  .close-btn {
    position: absolute;
    top: var(--space-3);
    right: var(--space-3);
  }

  .settings-section {
    display: flex;
    flex-direction: column;
  }

  .about {
    flex-direction: row;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: var(--space-6);
    padding-top: var(--space-4);
  }

  .about-copy {
    flex: 1 1 240px;
    min-width: 0;
  }

  .about-mark {
    display: block;
    width: 160px;
    max-width: 100%;
    height: auto;
    flex-shrink: 0;
    object-fit: contain;
  }

  .about-version {
    margin: 0;
    color: var(--text-muted);
    font-size: var(--fs-13);
    font-variant-numeric: tabular-nums;
  }

  .about-desc {
    max-width: 52ch;
    margin: var(--space-3) 0 0;
    color: var(--text-secondary);
    font: var(--fs-15) / var(--leading-relaxed) var(--font-display);
  }

  .about-desc code {
    font-family: var(--font-mono);
    font-size: 0.86em;
  }

  .section-title {
    margin: 0 0 var(--space-1);
    font: 600 var(--fs-24) / 1.25 var(--font-display);
  }

  .section-subtitle {
    margin: 0 0 var(--space-2);
    color: var(--text-muted);
    font-size: var(--fs-13);
  }

  .setting-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-6);
    padding: var(--space-4) 0;
    border-bottom: 1px solid var(--line-subtle);
  }

  .setting-card:last-child {
    border-bottom: 0;
  }

  .setting-card.column-card {
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-3);
  }

  .setting-row-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-6);
  }

  .setting-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .setting-label {
    color: var(--text-primary);
    font-weight: 600;
  }

  .setting-desc {
    color: var(--text-muted);
    font-size: var(--fs-12);
    line-height: var(--leading-normal);
  }

  .setting-desc code {
    font-family: var(--font-mono);
    font-size: 0.95em;
  }

  .field-row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
  }

  .text-field {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 180px;
    color: var(--text-secondary);
    font-size: var(--fs-12);
  }

  .text-field input {
    min-height: var(--control-min-height);
    padding: 0 var(--space-2);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-control);
    background: var(--surface-field);
    color: var(--text-primary);
    font: var(--fs-13) var(--font-ui);
  }

  .text-field input:focus-visible {
    outline: 2px solid var(--focus-ring, var(--accent));
    outline-offset: 0;
  }

  .field-error {
    color: var(--text-danger);
    font-size: var(--fs-12);
  }

  .button-row {
    display: flex;
    flex-shrink: 0;
    gap: var(--space-2);
  }

  .snippet-list {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .snippet-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-2) 0;
    font-family: var(--font-mono);
    font-size: var(--fs-13);
  }

  .snippet-example {
    margin: 0;
    padding: var(--space-3);
    border-radius: var(--radius-control);
    background: var(--surface-code);
    color: var(--text-primary);
    font-size: var(--fs-13);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .dictionary-list li {
    font-family: inherit;
  }

  .dict-source {
    margin-left: var(--space-2);
    color: var(--text-muted);
    font-size: var(--fs-12);
  }

  .rebuild-status {
    margin-top: var(--space-1);
    color: var(--status-ok);
    font-size: var(--fs-12);
  }

  /* Switch built on a native checkbox so keyboard and screen readers work unchanged. */
  .fluent-toggle {
    appearance: none;
    position: relative;
    width: 36px;
    height: 20px;
    flex: none;
    margin: 0;
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    background: var(--surface-field);
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      border-color var(--transition-fast);
  }

  .fluent-toggle::before {
    content: '';
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--text-secondary);
    transition: transform var(--transition-fast);
  }

  .fluent-toggle:checked {
    border-color: var(--accent);
    background: var(--accent);
  }

  .fluent-toggle:checked::before {
    transform: translateX(16px);
    background: var(--on-accent);
  }

  @media (forced-colors: active) {
    .fluent-toggle {
      appearance: auto;
    }

    .fluent-toggle::before {
      display: none;
    }
  }

  .copy-modes-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }

  .copy-mode-card {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-card);
    cursor: pointer;
  }

  .copy-mode-card:hover {
    border-color: var(--line-control);
  }

  .copy-mode-card.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }

  .copy-mode-card input {
    margin-top: 3px;
    accent-color: var(--accent);
  }

  .copy-mode-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .copy-mode-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .copy-mode-title {
    font-weight: 600;
  }

  .copy-mode-badge {
    padding: 1px 6px;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: var(--fs-11);
    font-weight: 500;
  }

  .copy-mode-formats {
    color: var(--text-secondary);
    font-family: var(--font-mono);
    font-size: var(--fs-12);
  }

  .copy-mode-desc {
    color: var(--text-muted);
    font-size: var(--fs-12);
    line-height: var(--leading-normal);
  }

  .accent-swatches {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
  }

  .accent-swatches[role='radiogroup'] {
    display: grid;
    grid-template-columns: repeat(8, minmax(0, 1fr));
    gap: var(--space-1);
  }

  .swatch {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    min-width: 0;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text-secondary);
    font: var(--fs-12) var(--font-ui);
    cursor: pointer;
  }

  .swatch-dot {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    box-shadow: 0 0 0 1px var(--line-subtle);
  }

  .swatch-dot.active {
    box-shadow:
      0 0 0 2px var(--surface-raised),
      0 0 0 4px var(--text-primary);
  }

  .swatch[aria-checked='true'] {
    color: var(--text-primary);
    font-weight: 500;
  }

  /* Fixed preview colors let each card show its own theme. */
  .theme-preview {
    display: flex;
    height: 100%;
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-control);
    overflow: hidden;
  }

  .tp-side {
    width: 32%;
  }

  .tp-page {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 10px 8px;
  }

  .tp-page span {
    height: 4px;
    border-radius: 2px;
    background: currentColor;
    opacity: 0.45;
  }

  .tp-page span:last-child {
    width: 60%;
  }

  .theme-preview.light {
    color: #1f1d1a;
  }

  .theme-preview.light .tp-side {
    background: #f3f1ea;
  }

  .theme-preview.light .tp-page {
    background: #fbfaf6;
  }

  .theme-preview.dark {
    color: #ece8df;
  }

  .theme-preview.dark .tp-side {
    background: #161512;
  }

  .theme-preview.dark .tp-page {
    background: #1c1b18;
  }

  .theme-preview.system {
    background: linear-gradient(135deg, #fbfaf6 50%, #1c1b18 50%);
  }

  .theme-preview.high-contrast {
    color: #ffffff;
    border-color: #8b8b8b;
  }

  .theme-preview.high-contrast .tp-side {
    background: #121212;
  }

  .theme-preview.high-contrast .tp-page {
    background: #000000;
  }

  .theme-preview.high-contrast .tp-page span {
    opacity: 1;
  }

  .theme-preview.system > span {
    visibility: hidden;
  }

  .font-preview {
    display: grid;
    place-items: center;
    height: 100%;
    border-radius: var(--radius-control);
    background: var(--surface-editor);
    color: var(--text-primary);
    font-size: 26px;
  }

  .font-preview.serif {
    font-family: var(--font-display);
  }

  .font-preview.sans {
    font-family: var(--font-ui);
  }

  .font-preview.mono {
    font-family: var(--font-mono);
  }

  .color-picker-label {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-secondary);
    font-size: var(--fs-13);
  }
</style>
