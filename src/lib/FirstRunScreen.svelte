<script lang="ts">
  import appIcon from '../assets/app-icon.png';
  import Icon from './ui/Icon.svelte';
  import IconButton from './ui/IconButton.svelte';
  import Button from './ui/Button.svelte';
  import { pickVaultFolder } from './ipc';
  import type { RecentVault } from './types';
  import { validateEntryName } from './validation';
  import { m, SLOT, splitSlots } from './i18n.svelte';

  interface Props {
    recentVaults: RecentVault[];
    errorMessage?: string;
    onOpenVaultPath: (path: string) => void;
    onCreateVault: (parentPath: string, vaultName: string) => void;
    onRemoveRecentVault: (path: string) => void;
    onOpenScratchpad: () => void;
  }

  let {
    recentVaults,
    errorMessage = '',
    onOpenVaultPath,
    onCreateVault,
    onRemoveRecentVault,
    onOpenScratchpad,
  }: Props = $props();

  let mode = $state<'welcome' | 'create' | 'open_manual'>('welcome');
  let createParentPath = $state('');
  let createVaultName = $state(m.first_run_default_vault_name());
  let manualOpenPath = $state('');

  let vaultNameError = $derived(
    mode === 'create' ? validateEntryName(createVaultName) : null
  );

  async function handleOpenVaultClick() {
    try {
      const picked = await pickVaultFolder(m.first_run_pick_vault());
      if (picked) {
        onOpenVaultPath(picked);
        return;
      }
    } catch {
      mode = 'open_manual';
    }
  }

  async function handleBrowseParentClick() {
    try {
      const picked = await pickVaultFolder(m.first_run_pick_parent());
      if (picked) {
        createParentPath = picked;
      }
    } catch {
      // allow manual entry
    }
  }

  function submitCreateVault(e: SubmitEvent) {
    e.preventDefault();
    if (!createParentPath.trim() || vaultNameError) return;
    onCreateVault(createParentPath.trim(), createVaultName.trimStart());
  }

  function submitManualOpen(e: SubmitEvent) {
    e.preventDefault();
    if (!manualOpenPath.trim()) return;
    onOpenVaultPath(manualOpenPath.trim());
  }

  const openDescParts = $derived(splitSlots(m.first_run_open_desc({ ext: SLOT })));
</script>

<div class="onboarding-container">
  <div class="onboarding-card" role="region" aria-label={m.first_run_welcome()}>
    <div class="hero-header">
      <img class="hero-logo" src={appIcon} alt="" width="48" height="48" />
      <div class="hero-titles">
        <h1 class="hero-title">Jotter</h1>
        <p class="hero-subtitle">
          {m.first_run_tagline()}
        </p>
      </div>
    </div>

    {#if errorMessage}
      <div class="error-banner" role="alert">{errorMessage}</div>
    {/if}

    <div class="primary-actions">
      <div class="action-row">
        <div class="action-info">
          <div class="action-title">{m.first_run_open_title()}</div>
          <div class="action-desc">
            {openDescParts[0]}<code>.md</code>{openDescParts[1]}
          </div>
        </div>
        <div class="action-buttons">
          <Button
            variant="primary"
            icon="folder-open"
            onclick={handleOpenVaultClick}
          >
            {m.first_run_open_vault()}
          </Button>
          <Button
            title={m.first_run_enter_path_title()}
            onclick={() =>
              (mode = mode === 'open_manual' ? 'welcome' : 'open_manual')}
          >
            {m.first_run_enter_path()}
          </Button>
        </div>
      </div>

      {#if mode === 'open_manual'}
        <form class="inline-form" onsubmit={submitManualOpen}>
          <label class="form-field">
            <span class="field-label">{m.first_run_vault_path()}</span>
            <input
              type="text"
              class="fluent-input"
              placeholder="C:\Users\you\Documents\MyVault"
              bind:value={manualOpenPath}
              required
            />
          </label>
          <div class="form-actions">
            <Button
              onclick={() => (mode = 'welcome')}
            >
              {m.common_cancel()}
            </Button>
            <Button
              variant="primary"
              type="submit"
            >
              {m.common_open_folder()}
            </Button>
          </div>
        </form>
      {/if}

      <div class="action-row">
        <div class="action-info">
          <div class="action-title">{m.first_run_create_title()}</div>
          <div class="action-desc">
            {m.first_run_create_desc()}
          </div>
        </div>
        <Button
          icon="plus"
          onclick={() => (mode = mode === 'create' ? 'welcome' : 'create')}
        >
          {m.first_run_create_vault()}
        </Button>
      </div>

      {#if mode === 'create'}
        <form class="inline-form" onsubmit={submitCreateVault}>
          <label class="form-field">
            <span class="field-label">{m.first_run_vault_name()}</span>
            <input
              type="text"
              class="fluent-input"
              aria-invalid={Boolean(vaultNameError)}
              placeholder={m.first_run_vault_name_placeholder()}
              bind:value={createVaultName}
              required
            />
            {#if vaultNameError}
              <span class="error-banner" role="alert">{vaultNameError}</span>
            {/if}
          </label>
          <div class="form-field">
            <span class="field-label">{m.first_run_parent_folder()}</span>
            <div class="input-with-btn">
              <input
                type="text"
                class="fluent-input"
                placeholder="C:\Users\you\Documents"
                bind:value={createParentPath}
                required
              />
              <Button
                onclick={handleBrowseParentClick}
              >
                {m.common_browse()}
              </Button>
            </div>
          </div>
          <div class="form-actions">
            <Button
              onclick={() => (mode = 'welcome')}
            >
              {m.common_cancel()}
            </Button>
            <Button
              variant="primary"
              type="submit"
              disabled={Boolean(vaultNameError)}
            >
              {m.app_create_and_open()}
            </Button>
          </div>
        </form>
      {/if}
    </div>

    {#if recentVaults.length > 0}
      <div class="recent-section">
        <h2 class="section-heading">{m.first_run_recent()}</h2>
        <ul class="recent-list">
          {#each recentVaults as vault (vault.path)}
            <li class="recent-item">
              <button
                type="button"
                class="recent-open-btn"
                onclick={() => onOpenVaultPath(vault.path)}
              >
                <span class="recent-icon"><Icon name="folder-simple" /></span>
                <span class="recent-text">
                  <span class="recent-name">{vault.name}</span>
                  <span class="recent-path">{vault.path}</span>
                </span>
              </button>
              <IconButton
                icon="x"
                label={m.first_run_remove_recent_label({ vault: vault.name })}
                tooltip={m.first_run_remove_recent()}
                onclick={() => onRemoveRecentVault(vault.path)}
              />
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    <div class="scratchpad-footer">
      <button type="button" class="link-btn" onclick={onOpenScratchpad}>
        {m.first_run_scratchpad()}
      </button>
    </div>
  </div>
</div>

<style>
  .onboarding-container {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    justify-content: center;
    padding: var(--space-12) var(--space-6);
  }

  .onboarding-card {
    width: 100%;
    max-width: 520px;
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
  }

  .hero-header {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .hero-logo {
    display: block;
    width: 48px;
    height: 48px;
    flex-shrink: 0;
    object-fit: contain;
  }

  .hero-titles {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .hero-title {
    margin: 0;
    font: 600 var(--fs-32) / 1.15 var(--font-display);
    letter-spacing: -0.01em;
  }

  .hero-subtitle {
    margin: 0;
    max-width: 44ch;
    color: var(--text-secondary);
    font: var(--fs-17) / var(--leading-normal) var(--font-display);
  }

  .error-banner {
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-card);
    background: color-mix(in srgb, var(--status-danger) 12%, transparent);
    color: var(--text-danger);
    font-size: var(--fs-13);
  }

  .primary-actions {
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--line-subtle);
  }

  .action-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-4) 0;
    border-bottom: 1px solid var(--line-subtle);
  }

  .action-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .action-title {
    font-weight: 600;
  }

  .action-desc {
    color: var(--text-muted);
    font-size: var(--fs-12);
  }

  .action-desc code {
    font-family: var(--font-mono);
    font-size: 0.95em;
  }

  .action-buttons {
    display: flex;
    gap: var(--space-2);
    flex: none;
  }

  .inline-form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4) 0;
    border-bottom: 1px solid var(--line-subtle);
  }

  .form-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .field-label {
    color: var(--text-secondary);
    font-size: var(--fs-12);
    font-weight: 500;
  }

  .input-with-btn {
    display: flex;
    gap: var(--space-2);
  }

  .fluent-input {
    flex: 1;
    min-width: 0;
    height: 32px;
    padding: 0 var(--space-3);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-card);
    background: var(--surface-field);
    color: var(--text-primary);
    font: inherit;
  }

  .fluent-input:focus {
    border-color: var(--accent);
    outline: none;
  }

  .fluent-input[aria-invalid='true'] {
    border-color: var(--status-danger);
  }

  .form-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }

  .recent-section {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .section-heading {
    margin: 0;
    color: var(--text-muted);
    font: 600 var(--fs-11) / 1 var(--font-ui);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .recent-list {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .recent-item {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .recent-open-btn {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2);
    border: 0;
    border-radius: var(--radius-card);
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .recent-open-btn:hover {
    background: var(--surface-hover);
  }

  .recent-icon {
    display: inline-flex;
    color: var(--text-muted);
  }

  .recent-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .recent-name {
    font-weight: 500;
  }

  .recent-path {
    overflow: hidden;
    color: var(--text-muted);
    font-size: var(--fs-12);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .scratchpad-footer {
    display: flex;
  }

  .link-btn {
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--accent);
    font: inherit;
    text-decoration: underline;
    text-decoration-color: color-mix(in srgb, var(--accent) 40%, transparent);
    text-underline-offset: 3px;
    cursor: pointer;
  }

  .link-btn:hover {
    text-decoration-color: currentColor;
  }
</style>
