<script lang="ts">
  import { defaultCaptureShortcut, defaultSummonShortcut, shortcutLabel } from './platform';
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
  import { ACCENT_PRESETS, type AccentPresetId } from './designTokens';
  import { systemStore } from './stores/system.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import { PROVIDER_NAMES } from './syncConflicts';
  import { taskCalendarStore } from './stores/taskCalendar.svelte';
  import { spellingStore } from './stores/spelling.svelte';
  import { exportSettingsFile, importSettingsFile, openDictionariesFolder } from './ipc';
  import { appStorage } from './appStorage';
  import { saveBeforeLeaving } from './saveBeforeLeaving';
  import { tabsStore } from './stores/tabs.svelte';
  import { settingsErrorMessage } from './settingsErrors';
  import { buildSettingsExport, parseSettingsImport } from './settingsTransfer';
  import { AI_PROVIDER_LABELS, aiStore } from './stores/ai.svelte';
  import type { AiProvider, CopyMode } from './types';
  import { i18n, LANGUAGE_NAMES, locales, m, splitSlots, SLOT, type LanguagePreference } from './i18n.svelte';

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

  const AI_PROVIDER_OPTIONS = $derived(
    (Object.keys(AI_PROVIDER_LABELS) as AiProvider[]).map((value) => ({
      value,
      label: AI_PROVIDER_LABELS[value],
    }))
  );
  const LANGUAGE_OPTIONS = $derived<{ value: LanguagePreference; label: string }[]>([
    { value: 'system', label: m.common_system() },
    ...locales.map((value) => ({ value, label: LANGUAGE_NAMES[value] })),
  ]);
  const AI_MODEL_EXAMPLES: Record<AiProvider, string> = {
    'openai-compatible': 'gpt-4o-mini',
    anthropic: 'claude-haiku-5-5',
    ollama: 'llama3.2',
  };
  const AI_STYLE_LIMIT = 4000;
  const ACCENT_NAMES: Record<AccentPresetId, () => string> = {
    ink: m.accent_ink,
    moss: m.accent_moss,
    plum: m.accent_plum,
    teal: m.accent_teal,
    rust: m.accent_rust,
    graphite: m.accent_graphite,
    amber: m.accent_amber,
    rose: m.accent_rose,
  };
  let aiError = $state('');

  let transferMessage = $state('');
  let transferError = $state('');
  let pendingImport = $state<{ settings: Record<string, string>; skipped: number } | null>(null);

  async function exportSettings() {
    transferMessage = '';
    transferError = '';
    try {
      await appStorage.flush();
      const path = await exportSettingsFile(buildSettingsExport(appStorage.snapshot()), {
        title: m.dialog_settings_export_title(),
        filter: m.dialog_settings_filter(),
      });
      if (path) transferMessage = m.settings_export_done({ path });
    } catch (err) {
      transferError = settingsErrorMessage(err);
    }
  }

  async function chooseImport() {
    transferMessage = '';
    transferError = '';
    pendingImport = null;
    try {
      const text = await importSettingsFile({
        title: m.dialog_settings_import_title(),
        filter: m.dialog_settings_filter(),
      });
      if (text === null) return;
      const parsed = parseSettingsImport(text);
      if (parsed.ok) {
        pendingImport = { settings: parsed.settings, skipped: parsed.skipped };
      } else if (parsed.reason === 'newer-version') {
        transferError = m.settings_import_newer();
      } else if (parsed.reason === 'empty') {
        transferError = m.settings_import_empty();
      } else {
        transferError = m.settings_import_invalid();
      }
    } catch (err) {
      transferError = settingsErrorMessage(err);
    }
  }

  async function applyImport() {
    if (!pendingImport) return;
    try {
      const incoming = pendingImport.settings;
      if (!(await saveBeforeLeaving(
        () => tabsStore.flushAllAutosaves(),
        () => appStorage.replaceTransferable(incoming),
      ))) {
        transferError = m.settings_import_unsaved();
        return;
      }
      // Stores read their saved values at startup, so a reload applies the import everywhere.
      window.location.reload();
    } catch (err) {
      transferError = settingsErrorMessage(err);
      pendingImport = null;
    }
  }
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

  function saveStyle(text: string, success = m.settings_ai_style_saved()) {
    return aiChange(async () => {
      await aiStore.updateConfig((c) => (c.styleInstructions = text));
      styleDraft = null;
    }, success);
  }
  let rebuildStatus = $state('');
  let isRebuilding = $state(false);
  let closeBtnRef: HTMLButtonElement | undefined = $state();


  function copyModeDetails(mode: CopyMode): { badge?: string; details: string } {
    if (mode === 'markdown_source') {
      return { badge: m.settings_copy_badge_default(), details: m.settings_copy_markdown_details() };
    }
    return { details: mode === 'plain' ? m.settings_copy_plain_details() : m.settings_copy_rich_details() };
  }

  function handleSelectCopyMode(mode: CopyMode) {
    settingsStore.setCopyMode(mode, () => {
      if (settingsStore.showInspector) onRefreshInspector();
    });
  }

  async function handleRebuildIndex() {
    if (!vaultStore.vaultSnapshot || isRebuilding) return;
    isRebuilding = true;
    rebuildStatus = m.settings_rebuilding_index();
    const count = await vaultStore.rebuildIndex();
    isRebuilding = false;
    if (count !== null) {
      rebuildStatus = m.settings_rebuilt_index({ count });
    } else {
      rebuildStatus = m.settings_rebuild_failed();
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

  const settingsVaultLanguagesDescParts = $derived(splitSlots(m.settings_vault_languages_desc({ lang: SLOT, skip: SLOT })));
  const settingsAddDictionaryParts = $derived(splitSlots(m.settings_add_dictionary({ aff: SLOT, dic: SLOT })));
  const settingsRebuildDescParts = $derived(splitSlots(m.settings_rebuild_desc({ vault: SLOT, count: vaultStore.vaultSnapshot?.noteCount ?? 0 })));
  const settingsTaskCalendarDescParts = $derived(splitSlots(m.settings_task_calendar_desc({ example: SLOT, ext: SLOT })));
  const settingsHiddenFilesDescParts = $derived(splitSlots(m.settings_hidden_files_desc({ folder: SLOT })));
  const settingsDailyNotesDescParts = $derived(splitSlots(m.settings_daily_notes_desc({ shortcut: SLOT, date: SLOT, time: SLOT })));
  const settingsAiHttpsParts = $derived(splitSlots(m.settings_ai_https({ scheme: SLOT })));
  const settingsAiKeySavedParts = $derived(splitSlots(m.settings_ai_key_saved({ store: aiStore.status?.credentialStore ?? '', origin: SLOT })));
  const settingsSnippetsDescParts = $derived(splitSlots(m.settings_snippets_desc({ ext: SLOT })));
</script>

<Dialog label={m.settings_title()} variant="panel" {onClose}>
    <div class="settings-body">
      <nav class="settings-nav" aria-label={m.settings_categories()}>
        <h2 class="nav-title">{m.settings_title()}</h2>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'editor'}
          aria-current={activeSection === 'editor' ? 'page' : undefined}
          onclick={() => (activeSection = 'editor')}
        >
          <Icon name="pencil-simple" />
          <span>{m.settings_section_editor()}</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'copy'}
          aria-current={activeSection === 'copy' ? 'page' : undefined}
          onclick={() => (activeSection = 'copy')}
        >
          <Icon name="copy" />
          <span>{m.settings_section_copy()}</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'appearance'}
          aria-current={activeSection === 'appearance' ? 'page' : undefined}
          onclick={() => (activeSection = 'appearance')}
        >
          <Icon name="eye" />
          <span>{m.settings_section_appearance()}</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'vault'}
          aria-current={activeSection === 'vault' ? 'page' : undefined}
          onclick={() => (activeSection = 'vault')}
        >
          <Icon name="folder-simple" />
          <span>{m.settings_section_vault()}</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'ai'}
          aria-current={activeSection === 'ai' ? 'page' : undefined}
          onclick={() => (activeSection = 'ai')}
        >
          <Icon name="sparkle" />
          <span>{m.settings_section_ai()}</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'system'}
          aria-current={activeSection === 'system' ? 'page' : undefined}
          onclick={() => (activeSection = 'system')}
        >
          <Icon name="gear-six" />
          <span>{m.settings_section_system()}</span>
        </button>
        <button
          type="button"
          class="nav-item nav-bottom"
          class:active={activeSection === 'about'}
          aria-current={activeSection === 'about' ? 'page' : undefined}
          onclick={() => (activeSection = 'about')}
        >
          <Icon name="info" />
          <span>{m.settings_section_about()}</span>
        </button>
      </nav>

      <div class="settings-content">
        <div class="close-btn">
          <IconButton
            bind:ref={closeBtnRef}
            icon="x"
            label={m.settings_close()}
            tooltip={m.settings_close_tooltip()}
            onclick={onClose}
          />
        </div>
        {#if activeSection === 'editor'}
          <section class="settings-section" aria-labelledby="sec-editor">
            <h3 id="sec-editor" class="section-title">{m.settings_section_editor()}</h3>
            <p class="section-subtitle">{m.settings_editor_subtitle()}</p>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">{m.settings_editing_mode()}</div>
                <div class="setting-desc">
                  {m.settings_editing_mode_desc({ shortcut: shortcutLabel('Ctrl+/') })}
                </div>
              </div>
              <Segmented
                label={m.settings_editing_mode()}
                value={settingsStore.editingMode}
                options={[
                  { value: 'live', label: m.editing_mode_live() },
                  { value: 'source', label: m.editing_mode_source() },
                ]}
                onchange={(mode) => settingsStore.setEditingMode(mode)}
              />
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-typewriter" class="setting-label">{m.settings_typewriter()}</label>
                <div class="setting-desc">{m.settings_typewriter_desc()}</div>
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
                <label for="toggle-line-numbers" class="setting-label">{m.settings_line_numbers()}</label>
                <div class="setting-desc">{m.settings_line_numbers_desc()}</div>
              </div>
              <input
                id="toggle-line-numbers"
                type="checkbox"
                class="fluent-toggle"
                checked={settingsStore.showLineNumbers}
                onchange={(e) =>
                  settingsStore.setShowLineNumbers((e.currentTarget as HTMLInputElement).checked)}
              />
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-dimming" class="setting-label">{m.settings_dimming()}</label>
                <div class="setting-desc">{m.settings_dimming_desc()}</div>
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
                <label for="toggle-spellcheck" class="setting-label">{m.settings_spellcheck()}</label>
                <div class="setting-desc">{m.settings_spellcheck_desc()}</div>
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
                    <div class="setting-label">{m.settings_dictionaries()}</div>
                    <div class="setting-desc">
                      {m.settings_dictionaries_desc()}
                    </div>
                  </div>
                  <Segmented
                    label={m.settings_dictionaries()}
                    value={spellingStore.engine}
                    options={[
                      { value: 'jotter', label: 'Jotter' },
                      { value: 'system', label: m.common_system() },
                    ]}
                    onchange={(engine) => spellingStore.setEngine(engine)}
                  />
                </div>
                {#if spellingStore.engine === 'jotter'}
                  <div class="setting-row-top">
                    <div class="setting-info">
                      <div class="setting-label">{m.settings_vault_languages()}</div>
                      <div class="setting-desc">
                        {settingsVaultLanguagesDescParts[0]}<code>lang: es</code>{settingsVaultLanguagesDescParts[1]}<code>spellcheck: false</code>{settingsVaultLanguagesDescParts[2]}
                      </div>
                    </div>
                    <div class="button-row">
                      <Button icon="arrow-clockwise" onclick={() => void spellingStore.reload()}>
                        {m.common_reload()}
                      </Button>
                      <Button icon="folder-open" onclick={() => void openDictionariesFolder()}>
                        {m.common_open_folder()}
                      </Button>
                    </div>
                  </div>
                  {#if !vaultStore.vaultSnapshot}
                    <div class="setting-desc">{m.settings_vault_languages_no_vault()}</div>
                  {:else}
                    <ul class="snippet-list dictionary-list">
                      {#each spellingStore.dictionaries as dict (dict.id)}
                        <li>
                          <label for="dict-{dict.id}">
                            {dict.name}
                            {#if !dict.bundled}<span class="dict-source">{m.settings_dictionary_added()}</span>{/if}
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
                    {settingsAddDictionaryParts[0]}<code>.aff</code>{settingsAddDictionaryParts[1]}<code>.dic</code>{settingsAddDictionaryParts[2]}
                  </div>
                  {#if spellingStore.loadError}
                    <div class="field-error" role="alert">{spellingStore.loadError}</div>
                  {/if}
                {/if}
              </div>
            {/if}

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">{m.settings_font_size()}</div>
                <div class="setting-desc">{m.settings_font_size_desc()}</div>
              </div>
              <Segmented
                label={m.settings_font_size()}
                value={settingsStore.editorFontSize}
                options={[12, 14, 16, 20].map((px) => ({ value: px, label: `${px} px` }))}
                onchange={(px) => settingsStore.setEditorFontSize(px)}
              />
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">{m.settings_image_links()}</div>
                <div class="setting-desc">{m.settings_image_links_desc()}</div>
              </div>
              <Segmented
                label={m.settings_image_links()}
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
            <h3 id="sec-copy" class="section-title">{m.settings_section_copy()}</h3>
            <p class="section-subtitle">{m.settings_copy_subtitle()}</p>

            <div class="copy-modes-list" role="radiogroup" aria-label={m.settings_copy_mode()}>
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
                      {#if copyModeDetails(mode).badge}
                        <span class="copy-mode-badge">
                          {copyModeDetails(mode).badge}
                        </span>
                      {/if}
                    </div>
                    <div class="copy-mode-desc">
                      {copyModeDetails(mode).details}
                    </div>
                  </div>
                </label>
              {/each}
            </div>
          </section>
        {:else if activeSection === 'appearance'}
          <section class="settings-section" aria-labelledby="sec-appearance">
            <h3 id="sec-appearance" class="section-title">{m.settings_section_appearance()}</h3>
            <p class="section-subtitle">{m.settings_appearance_subtitle()}</p>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">{m.settings_language()}</div>
                <div class="setting-desc">{m.settings_language_desc()}</div>
              </div>
              <Segmented
                label={m.settings_language()}
                value={i18n.preference}
                options={LANGUAGE_OPTIONS}
                onchange={(pref) => i18n.setPreference(pref)}
              />
            </div>

            <div class="setting-card column-card">
              <div class="setting-info">
                <div class="setting-label">{m.settings_theme()}</div>
                <div class="setting-desc">{m.settings_theme_desc()}</div>
              </div>
              <ChoiceCards
                label={m.settings_theme()}
                value={settingsStore.themePref}
                options={[
                  { value: 'system', label: m.common_system() },
                  { value: 'light', label: m.theme_paper() },
                  { value: 'dark', label: m.theme_night() },
                  { value: 'high-contrast', label: m.theme_high_contrast() },
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
                  <div class="setting-label">{m.settings_accent()}</div>
                  <div class="setting-desc">{m.settings_accent_desc()}</div>
                </div>
                <Segmented
                  label={m.settings_accent_source()}
                  value={settingsStore.accentColorMode}
                  options={[
                    { value: 'preset', label: 'Jotter' },
                    { value: 'system', label: m.common_system() },
                    { value: 'custom', label: m.settings_accent_custom() },
                  ]}
                  onchange={(mode) => settingsStore.setAccentColor(mode)}
                />
              </div>

              {#if settingsStore.accentColorMode === 'preset'}
                <div class="accent-swatches" role="radiogroup" aria-label={m.settings_accent_preset()}>
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
                      {ACCENT_NAMES[swatch.id]()}
                    </button>
                  {/each}
                </div>
              {:else if settingsStore.accentColorMode === 'custom'}
                <div class="accent-swatches">
                  <label class="color-picker-label" title={m.settings_custom_hex()}>
                    <span>{m.settings_pick_color()}</span>
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
                <div class="setting-label">{m.settings_writing_font()}</div>
                <div class="setting-desc">{m.settings_writing_font_desc()}</div>
              </div>
              <ChoiceCards
                label={m.settings_writing_font()}
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
            <h3 id="sec-vault" class="section-title">{m.settings_section_vault()}</h3>
            <p class="section-subtitle">{m.settings_vault_subtitle()}</p>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">{m.settings_rebuild()}</div>
                <div class="setting-desc">
                  {#if vaultStore.vaultSnapshot}
                    {settingsRebuildDescParts[0]}<strong>{vaultStore.vaultSnapshot.name}</strong>{settingsRebuildDescParts[1]}
                  {:else}
                    {m.settings_rebuild_no_vault()}
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
                {isRebuilding ? m.settings_rebuilding() : m.settings_rebuild_button()}
              </Button>
            </div>

            {#if vaultStore.vaultSnapshot && (vaultStore.syncProvider || vaultStore.syncConflicts.length > 0)}
              <div class="setting-card">
                <div class="setting-info">
                  <div class="setting-label">{m.settings_cloud_sync()}</div>
                  <div class="setting-desc">
                    {#if vaultStore.syncProvider}
                      {m.settings_synced_by({ provider: PROVIDER_NAMES[vaultStore.syncProvider] })}
                    {/if}
                    {#if vaultStore.syncConflicts.length > 0}
                      <strong>{m.settings_conflicts_need_review({ count: vaultStore.syncConflicts.length })}</strong>
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
                    {m.common_review()}
                  </Button>
                {/if}
              </div>
            {/if}

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-home-page" class="setting-label">{m.settings_home_page()}</label>
                <div class="setting-desc">{m.settings_home_page_desc()}</div>
              </div>
              <input
                id="toggle-home-page"
                type="checkbox"
                class="fluent-toggle"
                checked={settingsStore.showHomePage}
                onchange={(e) => settingsStore.setShowHomePage((e.currentTarget as HTMLInputElement).checked)}
              />
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-task-reminders" class="setting-label">{m.settings_task_reminders()}</label>
                <div class="setting-desc">{m.settings_task_reminders_desc()}</div>
              </div>
              <input
                id="toggle-task-reminders"
                type="checkbox"
                class="fluent-toggle"
                checked={settingsStore.taskReminders}
                onchange={(e) => settingsStore.setTaskReminders((e.currentTarget as HTMLInputElement).checked)}
              />
            </div>

            {#if vaultStore.vaultSnapshot}
              <div class="setting-card">
                <div class="setting-info">
                  <div class="setting-label">{m.settings_task_calendar()}</div>
                  <div class="setting-desc">
                    {settingsTaskCalendarDescParts[0]}<code>📅 2026-11-01</code>{settingsTaskCalendarDescParts[1]}<code>.ics</code>{settingsTaskCalendarDescParts[2]}
                    {#if taskCalendarStore.path}
                      <br />{m.settings_task_calendar_writing()} <code>{taskCalendarStore.path}</code>
                    {/if}
                    {#if taskCalendarStore.lastError}
                      <div class="rebuild-status" role="alert">{taskCalendarStore.lastError}</div>
                    {/if}
                  </div>
                </div>
                <div class="button-row">
                  <Button icon="calendar-blank" onclick={() => void taskCalendarStore.choosePath()}>
                    {taskCalendarStore.path ? m.settings_change_file() : m.settings_choose_file()}
                  </Button>
                  {#if taskCalendarStore.path}
                    <Button onclick={() => taskCalendarStore.turnOff()}>{m.common_turn_off()}</Button>
                  {/if}
                </div>
              </div>
            {/if}

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-hidden-files" class="setting-label">{m.settings_hidden_files()}</label>
                <div class="setting-desc">
                  {settingsHiddenFilesDescParts[0]}<code>.obsidian</code>{settingsHiddenFilesDescParts[1]}
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
                aria-label={m.settings_hidden_files()}
              />
            </div>

            <div class="setting-card column-card">
              <div class="setting-info">
                <div class="setting-label">{m.settings_daily_notes()}</div>
                <div class="setting-desc">
                  {settingsDailyNotesDescParts[0]}<kbd>{shortcutLabel('Ctrl Shift D')}</kbd>{settingsDailyNotesDescParts[1]}<code>{'{{date}}'}</code>{settingsDailyNotesDescParts[2]}<code>{'{{time}}'}</code>{settingsDailyNotesDescParts[3]}
                </div>
              </div>
              <div class="field-row">
                <label class="text-field">
                  <span>{m.settings_daily_folder()}</span>
                  <input
                    type="text"
                    value={settingsStore.dailyFolder}
                    placeholder={m.settings_daily_folder_placeholder()}
                    onchange={(e) =>
                      settingsStore.setDailyFolder((e.currentTarget as HTMLInputElement).value)}
                  />
                </label>
                <label class="text-field">
                  <span>{m.settings_templates_folder()}</span>
                  <input
                    type="text"
                    value={settingsStore.templatesFolder}
                    placeholder={m.common_none()}
                    onchange={(e) =>
                      settingsStore.setTemplatesFolder((e.currentTarget as HTMLInputElement).value)}
                  />
                </label>
                <label class="text-field">
                  <span>{m.settings_daily_template()}</span>
                  <input
                    type="text"
                    value={settingsStore.dailyTemplatePath}
                    placeholder={m.common_none()}
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
            <h3 id="sec-ai" class="section-title">{m.settings_section_ai()}</h3>
            <p class="section-subtitle">{m.settings_ai_subtitle()}</p>

            {#if !aiStore.status}
              <div class="setting-card">
                <div class="setting-desc">{aiStore.loadError || m.settings_ai_loading()}</div>
              </div>
            {:else}
              <div class="setting-card">
                <div class="setting-info">
                  <label for="toggle-ai" class="setting-label">{m.settings_ai_use()}</label>
                  <div class="setting-desc">{m.settings_ai_use_desc()}</div>
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
                  <div class="setting-label">{m.settings_ai_provider()}</div>
                  <div class="setting-desc">{m.settings_ai_provider_desc()}</div>
                </div>
                <Segmented
                  label={m.settings_ai_provider()}
                  options={AI_PROVIDER_OPTIONS}
                  value={aiProvider}
                  onchange={(next) => {
                    keyDraft = '';
                    void aiChange(() => aiStore.updateConfig((c) => (c.provider = next)));
                  }}
                />
                <div class="field-row">
                  <label class="text-field">
                    <span>{m.settings_ai_base_url()}</span>
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
                    <span>{m.settings_ai_model()}</span>
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
                  {settingsAiHttpsParts[0]}<code>https://</code>{settingsAiHttpsParts[1]}
                </div>
              </div>

              {#if aiProvider !== 'ollama'}
                <div class="setting-card column-card">
                  <div class="setting-info">
                    <div class="setting-label">{m.settings_ai_api_key()}</div>
                    <div class="setting-desc">
                      {#if aiKey?.state === 'saved'}
                        {settingsAiKeySavedParts[0]}<code>{aiKey.origin}</code>{settingsAiKeySavedParts[1]}
                      {:else if aiKey?.state === 'endpointChanged'}
                        <span class="field-warning">
                          {m.settings_ai_key_endpoint_changed()}
                        </span>
                      {:else}
                        {m.settings_ai_no_key()}{aiIsLocal ? ` ${m.settings_ai_local_no_key()}` : ''}
                        {m.settings_ai_keys_stay({ store: aiStore.status.credentialStore })}
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
                      }, m.settings_ai_key_saved_notice());
                    }}
                  >
                    <label class="text-field">
                      <span>{aiKey?.state === 'saved' ? m.settings_ai_replace_key() : m.settings_ai_key()}</span>
                      <input type="password" autocomplete="off" spellcheck="false" bind:value={keyDraft} />
                    </label>
                    <div class="button-row">
                      <Button type="submit" disabled={aiBusy || !keyDraft.trim()}>{m.settings_ai_save_key()}</Button>
                      {#if aiKey && aiKey.state !== 'none'}
                        <Button
                          variant="danger"
                          disabled={aiBusy}
                          onclick={() =>
                            void aiChange(() => aiStore.deleteKey(aiProvider), m.settings_ai_key_removed())}
                        >
                          {m.common_remove()}
                        </Button>
                      {/if}
                    </div>
                  </form>
                </div>
              {/if}

              <div class="setting-card column-card">
                <div class="setting-row-top">
                  <div class="setting-info">
                    <label for="ai-style" class="setting-label">{m.settings_ai_style()}</label>
                    <div class="setting-desc">{m.settings_ai_style_desc()}</div>
                  </div>
                  <Button
                    disabled={aiBusy || styleValue === aiStore.status.defaultStyleInstructions}
                    onclick={() =>
                      void saveStyle(aiStore.status!.defaultStyleInstructions, m.settings_ai_style_restored())}
                  >
                    {m.settings_ai_style_reset()}
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
            <h3 id="sec-system" class="section-title">{m.settings_section_system()}</h3>
            <p class="section-subtitle">{m.settings_system_subtitle()}</p>

            <div class="setting-card column-card">
              <div class="setting-row-top">
                <div class="setting-info">
                  <label for="toggle-summon" class="setting-label">{m.settings_summon()}</label>
                  <div class="setting-desc">{m.settings_summon_desc()}</div>
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
                <span>{m.settings_hotkey()}</span>
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
                  <label for="toggle-capture" class="setting-label">{m.settings_capture()}</label>
                  <div class="setting-desc">{m.settings_capture_desc()}</div>
                </div>
                <input
                  id="toggle-capture"
                  type="checkbox"
                  class="fluent-toggle"
                  checked={settingsStore.quickCaptureEnabled}
                  onchange={(e) =>
                    settingsStore.setQuickCapture((e.currentTarget as HTMLInputElement).checked)}
                />
              </div>
              <label class="text-field">
                <span>{m.settings_hotkey()}</span>
                <input
                  type="text"
                  value={settingsStore.quickCaptureShortcut}
                  placeholder={defaultCaptureShortcut}
                  onchange={(e) =>
                    settingsStore.setQuickCapture(
                      settingsStore.quickCaptureEnabled,
                      (e.currentTarget as HTMLInputElement).value
                    )}
                />
              </label>
              <label class="text-field">
                <span>{m.settings_capture_note()}</span>
                <input
                  type="text"
                  value={settingsStore.quickCaptureNote}
                  onchange={(e) =>
                    settingsStore.setQuickCaptureNote((e.currentTarget as HTMLInputElement).value)}
                />
              </label>
              {#if systemStore.quickCaptureError}
                <div class="field-error" role="alert">{systemStore.quickCaptureError}</div>
              {/if}
            </div>

            <div class="setting-card column-card">
              <div class="setting-row-top">
                <div class="setting-info">
                  <label for="toggle-tray" class="setting-label">{m.settings_tray()}</label>
                  <div class="setting-desc">{m.settings_tray_desc()}</div>
                </div>
                <input
                  id="toggle-tray"
                  type="checkbox"
                  class="fluent-toggle"
                  checked={settingsStore.trayEnabled}
                  onchange={(e) =>
                    settingsStore.setTrayEnabled((e.currentTarget as HTMLInputElement).checked)}
                />
              </div>
              <div class="setting-info">
                <div class="setting-label">{m.settings_close_action()}</div>
                <div class="setting-desc">{m.settings_close_action_desc()}</div>
              </div>
              <Segmented
                label={m.settings_close_action()}
                value={settingsStore.closeAction}
                options={[
                  { value: 'quit', label: m.settings_close_quit() },
                  { value: 'tray', label: m.settings_close_tray() },
                ]}
                onchange={(action) => settingsStore.setCloseAction(action)}
              />
              {#if systemStore.trayError}
                <div class="field-error" role="alert">{systemStore.trayError}</div>
              {/if}
            </div>

            <div class="setting-card column-card">
              <div class="setting-row-top">
                <div class="setting-info">
                  <div class="setting-label">{m.settings_snippets()}</div>
                  <div class="setting-desc">
                    {settingsSnippetsDescParts[0]}<code>.css</code>{settingsSnippetsDescParts[1]}
                  </div>
                </div>
                <div class="button-row">
                  <Button icon="arrow-clockwise" onclick={() => void systemStore.reloadSnippets()}>
                    {m.common_reload()}
                  </Button>
                  <Button icon="folder-open" onclick={() => void systemStore.openSnippetsFolder()}>
                    {m.common_open_folder()}
                  </Button>
                </div>
              </div>
              {#if systemStore.snippets.length === 0}
                <div class="setting-desc">{m.settings_no_snippets()}</div>
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

            <div class="setting-card column-card">
              <div class="setting-row-top">
                <div class="setting-info">
                  <div class="setting-label">{m.settings_transfer()}</div>
                  <div class="setting-desc">{m.settings_transfer_desc()}</div>
                </div>
                <div class="button-row">
                  <Button icon="export" onclick={() => void exportSettings()}>{m.settings_export()}</Button>
                  <Button icon="folder-open" onclick={() => void chooseImport()}>{m.settings_import()}</Button>
                </div>
              </div>
              {#if pendingImport}
                <div class="setting-desc" role="alert">
                  {m.settings_import_confirm({ count: Object.keys(pendingImport.settings).length })}
                  {#if pendingImport.skipped > 0}
                    {m.settings_import_skipped({ skipped: pendingImport.skipped })}
                  {/if}
                </div>
                <div class="button-row">
                  <Button variant="primary" onclick={() => void applyImport()}>{m.settings_import_apply()}</Button>
                  <Button onclick={() => (pendingImport = null)}>{m.common_cancel()}</Button>
                </div>
              {/if}
              {#if transferMessage}
                <div class="setting-desc" role="status">{transferMessage}</div>
              {/if}
              {#if transferError}
                <div class="field-error" role="alert">{transferError}</div>
              {/if}
            </div>
          </section>
        {:else if activeSection === 'about'}
          <section class="settings-section about" aria-labelledby="sec-about">
            <img class="about-mark" src={aboutLogo} alt={m.settings_about_logo_alt()} width="160" height="160" />
            <div class="about-copy">
              <h3 id="sec-about" class="section-title">Jotter</h3>
              {#if appVersion}
                <p class="about-version">{m.settings_about_version({ version: appVersion })}</p>
              {/if}
              <p class="about-desc">
                {m.settings_about_desc()}
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
