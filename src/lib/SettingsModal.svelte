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
  import { AI_PROVIDER_LABELS, aiStore } from './stores/ai.svelte';
  import type { AiProvider, CopyMode } from './types';

  interface Props {
    onClose: () => void;
    onRefreshInspector: () => void;
  }

  let { onClose, onRefreshInspector }: Props = $props();

  type SettingsSection = 'editor' | 'copy' | 'appearance' | 'vault' | 'ai' | 'system' | 'about';
  const SECTIONS: SettingsSection[] = ['editor', 'copy', 'appearance', 'vault', 'ai', 'system', 'about'];
  let appVersion = $state('');
  const requested = settingsStore.settingsSectionRequest as SettingsSection | null;
  let activeSection = $state<SettingsSection>(
    requested && SECTIONS.includes(requested) ? requested : 'editor'
  );
  settingsStore.settingsSectionRequest = null;

  const AI_PROVIDER_OPTIONS = (Object.keys(AI_PROVIDER_LABELS) as AiProvider[]).map((value) => ({
    value,
    label: AI_PROVIDER_LABELS[value],
  }));
  const AI_MODEL_EXAMPLES: Record<AiProvider, string> = {
    'openai-compatible': 'gpt-4o-mini',
    anthropic: 'claude-haiku-5-5',
    ollama: 'llama3.2',
  };
  const AI_STYLE_LIMIT = 4000;
  let aiError = $state('');
  let aiNotice = $state('');
  let aiBusy = $state(false);
  let keyDraft = $state('');
  let styleDraft = $state<string | null>(null);
  const aiProvider = $derived<AiProvider>(aiStore.status?.config.provider ?? 'openai-compatible');
  const aiEndpoint = $derived(aiStore.endpoint(aiProvider));
  const aiKey = $derived(aiStore.keyStatus(aiProvider));
  const aiIsLocal = $derived.by(() => {
    try {
      const host = new URL(aiEndpoint?.baseUrl ?? '').hostname;
      return host === 'localhost' || host === '127.0.0.1' || host === '[::1]';
    } catch {
      return false;
    }
  });
  const styleValue = $derived(styleDraft ?? aiStore.status?.config.styleInstructions ?? '');

  /** Runs an AI settings change and shows Rust's validation message when it fails. */
  async function aiChange(task: () => Promise<void>, success = '') {
    aiError = '';
    aiNotice = '';
    aiBusy = true;
    try {
      await task();
      aiNotice = success;
    } catch (err) {
      aiError = String(err);
    } finally {
      aiBusy = false;
    }
  }

  function saveStyle(text: string, success = 'Saved your writing style.') {
    return aiChange(async () => {
      await aiStore.updateConfig((c) => (c.styleInstructions = text));
      styleDraft = null;
    }, success);
  }
  let rebuildStatus = $state('');
  let isRebuilding = $state(false);
  let closeBtnRef: HTMLButtonElement | undefined = $state();


  const COPY_MODE_DETAILS: Record<CopyMode, { badge?: string; details: string }> = {
    markdown_source: {
      badge: 'Default',
      details: 'Copies the Markdown as written.',
    },
    plain: {
      details: 'Copies text without Markdown symbols.',
    },
    rich: {
      details: 'Copies formatted text with bold, italics, and links.',
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
    rebuildStatus = 'Rebuilding index…';
    const count = await vaultStore.rebuildIndex();
    isRebuilding = false;
    if (count !== null) {
      rebuildStatus = `Rebuilt index for ${count} notes.`;
    } else {
      rebuildStatus = 'Failed to rebuild vault index.';
    }
  }

  onMount(() => {
    closeBtnRef?.focus();
    void spellingStore.ensureLoaded();
    void aiStore.reload();
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
          class:active={activeSection === 'ai'}
          aria-current={activeSection === 'ai' ? 'page' : undefined}
          onclick={() => (activeSection = 'ai')}
        >
          <Icon name="sparkle" />
          <span>AI</span>
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
              Editing mode, focus aids, spelling, and font size.
            </p>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">Default editing mode</div>
                <div class="setting-desc">
                  Live Preview formats text as you write. Toggle with {shortcutLabel('Ctrl+/')}.
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
                  Keep the current line centered.
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
                  Dim everything except the current paragraph.
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
                  Underline misspelled words.
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
                      Jotter lets each vault and note pick languages. System uses your OS languages.
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
                        Override per note with <code>lang: es</code>, or skip one with <code>spellcheck: false</code>.
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
                    Add a language by dropping a Hunspell <code>.aff</code> and <code>.dic</code> pair in the folder, then reload.
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
                  Size of note text.
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
                  Link style for pasted images.
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
              What Copy puts on the clipboard. Line breaks always survive pasting.
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
                      {#if COPY_MODE_DETAILS[mode].badge}
                        <span class="copy-mode-badge">
                          {COPY_MODE_DETAILS[mode].badge}
                        </span>
                      {/if}
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
                <div class="setting-desc">System matches your OS.</div>
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
                    Color for links, selection, and buttons.
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
                <div class="setting-desc">Font for note text.</div>
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
              Index, home page, tasks, and daily notes.
            </p>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">Rebuild vault index</div>
                <div class="setting-desc">
                  {#if vaultStore.vaultSnapshot}
                    Re-scan <strong>{vaultStore.vaultSnapshot.name}</strong> ({vaultStore.vaultSnapshot.noteCount} notes) if search or links look wrong.
                  {:else}
                    Open a vault to rebuild its index.
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

            {#if vaultStore.vaultSnapshot && (vaultStore.syncProvider || vaultStore.syncConflicts.length > 0)}
              <div class="setting-card">
                <div class="setting-info">
                  <div class="setting-label">Cloud sync</div>
                  <div class="setting-desc">
                    {#if vaultStore.syncProvider}
                      Synced by {PROVIDER_NAMES[vaultStore.syncProvider]}. Keep the folder available
                      offline so search sees every note.
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
                  Show recent and related notes when no note is open.
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
                    Export open tasks with due dates (<code>📅 2026-11-01</code>) to an <code>.ics</code> file your calendar can subscribe to.
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
                  Show dotfiles in the sidebar. <code>.obsidian</code> stays hidden.
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
                  <kbd>{shortcutLabel('Ctrl Shift D')}</kbd> opens today's note. Templates can use <code>{'{{date}}'}</code> and <code>{'{{time}}'}</code>.
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
        {:else if activeSection === 'ai'}
          <section class="settings-section" aria-labelledby="sec-ai">
            <h3 id="sec-ai" class="section-title">AI</h3>
            <p class="section-subtitle">
              Summarize, rewrite, and translate with your own provider. Requests go straight to it.
            </p>

            {#if !aiStore.status}
              <div class="setting-card">
                <div class="setting-desc">{aiStore.loadError || 'Loading AI settings…'}</div>
              </div>
            {:else}
              <div class="setting-card">
                <div class="setting-info">
                  <label for="toggle-ai" class="setting-label">Use AI</label>
                  <div class="setting-desc">
                    While off, AI features are disabled and the app remains 100% local.
                  </div>
                </div>
                <input
                  id="toggle-ai"
                  type="checkbox"
                  class="fluent-toggle"
                  checked={aiStore.status.config.enabled}
                  disabled={aiBusy}
                  onchange={(e) => {
                    const enabled = (e.currentTarget as HTMLInputElement).checked;
                    void aiChange(() => aiStore.updateConfig((c) => (c.enabled = enabled)));
                  }}
                />
              </div>

              <div class="setting-card column-card">
                <div class="setting-info">
                  <div class="setting-label">Provider</div>
                  <div class="setting-desc">
                    OpenAI-compatible also works with OpenRouter, Groq, Mistral, and LM Studio.
                  </div>
                </div>
                <Segmented
                  label="Provider"
                  options={AI_PROVIDER_OPTIONS}
                  value={aiProvider}
                  onchange={(next) => {
                    keyDraft = '';
                    void aiChange(() => aiStore.updateConfig((c) => (c.provider = next)));
                  }}
                />
                <div class="field-row">
                  <label class="text-field">
                    <span>Base URL</span>
                    <input
                      type="url"
                      spellcheck="false"
                      value={aiEndpoint?.baseUrl ?? ''}
                      onchange={(e) => {
                        const value = (e.currentTarget as HTMLInputElement).value;
                        void aiChange(() => aiStore.setEndpointField(aiProvider, 'baseUrl', value));
                      }}
                    />
                  </label>
                  <label class="text-field">
                    <span>Model</span>
                    <input
                      type="text"
                      spellcheck="false"
                      placeholder={AI_MODEL_EXAMPLES[aiProvider]}
                      value={aiEndpoint?.model ?? ''}
                      onchange={(e) => {
                        const value = (e.currentTarget as HTMLInputElement).value;
                        void aiChange(() => aiStore.setEndpointField(aiProvider, 'model', value));
                      }}
                    />
                  </label>
                </div>
                <div class="setting-desc">
                  Use <code>https://</code> for cloud providers.
                </div>
              </div>

              {#if aiProvider !== 'ollama'}
                <div class="setting-card column-card">
                  <div class="setting-info">
                    <div class="setting-label">API key</div>
                    <div class="setting-desc">
                      {#if aiKey?.state === 'saved'}
                        Saved in {aiStore.status.credentialStore}. Sent only to
                        <code>{aiKey.origin}</code>.
                      {:else if aiKey?.state === 'endpointChanged'}
                        <span class="field-warning">
                          Base URL changed. Enter the key again to use it there.
                        </span>
                      {:else}
                        No key saved.{aiIsLocal ? ' Local servers may not need one.' : ''}
                        Keys stay in {aiStore.status.credentialStore}.
                      {/if}
                    </div>
                  </div>
                  <form
                    class="key-row"
                    onsubmit={(e) => {
                      e.preventDefault();
                      const key = keyDraft;
                      void aiChange(async () => {
                        await aiStore.saveKey(aiProvider, key);
                        keyDraft = '';
                      }, 'Saved the API key.');
                    }}
                  >
                    <label class="text-field">
                      <span>{aiKey?.state === 'saved' ? 'Replace key' : 'Key'}</span>
                      <input type="password" autocomplete="off" spellcheck="false" bind:value={keyDraft} />
                    </label>
                    <div class="button-row">
                      <Button type="submit" disabled={aiBusy || !keyDraft.trim()}>Save key</Button>
                      {#if aiKey && aiKey.state !== 'none'}
                        <Button
                          variant="danger"
                          disabled={aiBusy}
                          onclick={() =>
                            void aiChange(() => aiStore.deleteKey(aiProvider), 'Removed the API key.')}
                        >
                          Remove
                        </Button>
                      {/if}
                    </div>
                  </form>
                </div>
              {/if}

              <div class="setting-card column-card">
                <div class="setting-row-top">
                  <div class="setting-info">
                    <label for="ai-style" class="setting-label">Writing style</label>
                    <div class="setting-desc">
                      How rewrites and summaries should read. Translation and grammar fixes ignore it.
                    </div>
                  </div>
                  <Button
                    disabled={aiBusy || styleValue === aiStore.status.defaultStyleInstructions}
                    onclick={() =>
                      void saveStyle(aiStore.status!.defaultStyleInstructions, 'Restored the default style.')}
                  >
                    Reset to default
                  </Button>
                </div>
                <textarea
                  id="ai-style"
                  class="style-field"
                  class:default-style={styleValue === aiStore.status.defaultStyleInstructions}
                  rows="5"
                  maxlength={AI_STYLE_LIMIT}
                  placeholder={aiStore.status.defaultStyleInstructions}
                  value={styleValue}
                  oninput={(e) => (styleDraft = (e.currentTarget as HTMLTextAreaElement).value)}
                  onblur={() => {
                    if (styleDraft !== null && styleDraft !== aiStore.status?.config.styleInstructions) {
                      void saveStyle(styleDraft);
                    }
                  }}
                ></textarea>
                <div class="setting-desc">
                  {styleValue.length} / {AI_STYLE_LIMIT}
                </div>
              </div>

              {#if aiError}
                <div class="field-error" role="alert">{aiError}</div>
              {:else if aiNotice}
                <div class="setting-desc" role="status">{aiNotice}</div>
              {/if}
            {/if}
          </section>
        {:else if activeSection === 'system'}
          <section class="settings-section" aria-labelledby="sec-system">
            <h3 id="sec-system" class="section-title">System</h3>
            <p class="section-subtitle">
              Global hotkey and custom CSS.
            </p>

            <div class="setting-card column-card">
              <div class="setting-row-top">
                <div class="setting-info">
                  <label for="toggle-summon" class="setting-label">Summon hotkey</label>
                  <div class="setting-desc">
                    Show or hide Jotter from anywhere. Closing the window sends it to the tray.
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
                    Restyle Jotter with your own <code>.css</code> files. Add one to the folder, reload, then turn it on.
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
              {#if systemStore.snippets.length === 0}
                <div class="setting-desc">No snippets yet.</div>
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
                A Markdown notebook with smart editing and optional AI.
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

  .field-warning {
    color: var(--text-danger);
  }

  .key-row {
    display: flex;
    align-items: flex-end;
    gap: var(--space-3);
  }

  .style-field {
    min-height: 112px;
    padding: var(--space-2);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-control);
    background: var(--surface-field);
    color: var(--text-primary);
    font: var(--fs-13) / 1.5 var(--font-ui);
    resize: vertical;
  }

  .style-field.default-style,
  .style-field::placeholder {
    color: var(--text-muted);
    opacity: 1;
  }

  .style-field:focus-visible {
    outline: 2px solid var(--focus-ring, var(--accent));
    outline-offset: 0;
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
