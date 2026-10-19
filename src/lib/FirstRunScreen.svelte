<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import IconButton from './ui/IconButton.svelte';
  import Button from './ui/Button.svelte';
  import { pickVaultFolder } from './ipc';
  import type { RecentVault } from './types';
  import { validateWindowsEntryName } from './validation';

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
  let createVaultName = $state('My Notes');
  let manualOpenPath = $state('');

  let vaultNameError = $derived(
    mode === 'create' ? validateWindowsEntryName(createVaultName) : null
  );

  async function handleOpenVaultClick() {
    try {
      const picked = await pickVaultFolder('Select a folder to open as a Jotter vault');
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
      const picked = await pickVaultFolder('Select parent folder for new vault');
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
</script>

<div class="onboarding-container">
  <div class="onboarding-card" role="region" aria-label="Welcome to Jotter">
    <div class="hero-header">
      <div class="hero-logo"><Icon name="notebook" size={20} /></div>
      <div class="hero-titles">
        <h1 class="hero-title">Jotter</h1>
        <p class="hero-subtitle">
          Windows-native Markdown notes compatible with Obsidian vaults.
        </p>
      </div>
    </div>

    {#if errorMessage}
      <div class="error-banner" role="alert">{errorMessage}</div>
    {/if}

    <div class="primary-actions">
      <div class="action-row">
        <div class="action-info">
          <div class="action-title">Open existing vault</div>
          <div class="action-desc">
            Choose an existing folder of Markdown (<code>.md</code>) files.
          </div>
        </div>
        <div class="action-buttons">
          <Button
            variant="primary"
            icon="folder-open"
            onclick={handleOpenVaultClick}
          >
            Open vault
          </Button>
          <Button
            title="Enter folder path manually"
            onclick={() =>
              (mode = mode === 'open_manual' ? 'welcome' : 'open_manual')}
          >
            Enter path…
          </Button>
        </div>
      </div>

      {#if mode === 'open_manual'}
        <form class="inline-form" onsubmit={submitManualOpen}>
          <label class="form-field">
            <span class="field-label">Vault folder path</span>
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
              Cancel
            </Button>
            <Button
              variant="primary"
              type="submit"
            >
              Open folder
            </Button>
          </div>
        </form>
      {/if}

      <div class="action-row">
        <div class="action-info">
          <div class="action-title">Create new vault</div>
          <div class="action-desc">
            Create a fresh folder for your plain-text Markdown notes.
          </div>
        </div>
        <Button
          icon="plus"
          onclick={() => (mode = mode === 'create' ? 'welcome' : 'create')}
        >
          Create vault
        </Button>
      </div>

      {#if mode === 'create'}
        <form class="inline-form" onsubmit={submitCreateVault}>
          <label class="form-field">
            <span class="field-label">Vault name</span>
            <input
              type="text"
              class="fluent-input"
              aria-invalid={Boolean(vaultNameError)}
              placeholder="Work Notes"
              bind:value={createVaultName}
              required
            />
            {#if vaultNameError}
              <span class="error-banner" role="alert">{vaultNameError}</span>
            {/if}
          </label>
          <div class="form-field">
            <span class="field-label">Parent folder</span>
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
                Browse…
              </Button>
            </div>
          </div>
          <div class="form-actions">
            <Button
              onclick={() => (mode = 'welcome')}
            >
              Cancel
            </Button>
            <Button
              variant="primary"
              type="submit"
              disabled={Boolean(vaultNameError)}
            >
              Create &amp; Open
            </Button>
          </div>
        </form>
      {/if}
    </div>

    {#if recentVaults.length > 0}
      <div class="recent-section">
        <h2 class="section-heading">Recent vaults</h2>
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
                label="Remove {vault.name} from recent vaults"
                tooltip="Remove from recent vaults"
                onclick={() => onRemoveRecentVault(vault.path)}
              />
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    <div class="scratchpad-footer">
      <button type="button" class="link-btn" onclick={onOpenScratchpad}>
        Open Clipboard Spike Scratchpad without a vault
      </button>
    </div>
  </div>
</div>

<style>
  .onboarding-container {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-6);
    overflow: auto;
  }

  .onboarding-card {
    width: 100%;
    max-width: 580px;
    background: var(--surface-editor);
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-card);
    padding: var(--space-6);
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }

  .hero-header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .hero-logo {
    width: 44px;
    height: 44px;
    border-radius: var(--radius-card);
    background: var(--selection);
    color: var(--accent);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .hero-titles {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .hero-title {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--fs-24);
    font-weight: 600;
    line-height: var(--leading-tight);
  }

  .hero-subtitle {
    margin: 0;
    font-size: var(--fs-13);
    color: var(--text-secondary);
  }

  .error-banner {
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    border: 1px solid var(--status-danger);
    color: var(--text-danger);
    background: var(--surface-subtle);
    font-size: var(--fs-12);
  }

  .primary-actions {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .action-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-4);
    background: var(--surface-subtle);
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-card);
  }

  .action-info {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .action-title {
    font-size: var(--fs-15);
    font-weight: 600;
  }

  .action-desc {
    font-size: var(--fs-12);
    color: var(--text-secondary);
  }

  .action-desc code {
    font-family: var(--font-mono);
    background: var(--surface-code);
    padding: 1px 4px;
    border-radius: var(--radius-control);
  }

  .action-buttons {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-shrink: 0;
  }

  .inline-form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    background: var(--surface-raised);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-card);
  }

  .form-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .field-label {
    font-size: var(--fs-12);
    font-weight: 600;
    color: var(--text-secondary);
  }

  .input-with-btn {
    display: flex;
    gap: var(--space-2);
  }

  .fluent-input {
    flex: 1;
    min-height: var(--control-min-height);
    padding: 0 var(--space-3);
    border-radius: var(--radius-control);
    border: 1px solid var(--line-control);
    background: var(--surface-field);
    color: var(--text-primary);
    font-size: var(--fs-13);
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
    font-size: var(--fs-12);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-secondary);
  }

  .recent-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .recent-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .recent-open-btn {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 40px;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    border: 1px solid var(--line-subtle);
    background: var(--surface-subtle);
    text-align: left;
    cursor: pointer;
    transition: background-color var(--transition-fast);
  }

  .recent-open-btn:hover {
    background: var(--surface-hover);
  }

  .recent-icon {
    color: var(--accent);
    display: inline-flex;
  }

  .recent-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .recent-name {
    font-size: var(--fs-13);
    font-weight: 600;
    color: var(--text-primary);
  }

  .recent-path {
    font-size: var(--fs-12);
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .scratchpad-footer {
    border-top: 1px solid var(--line-subtle);
    padding-top: var(--space-3);
    display: flex;
    justify-content: center;
  }

  .link-btn {
    min-height: var(--control-min-height);
    padding: 0 var(--space-3);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    color: var(--accent);
    font-size: var(--fs-12);
    font-weight: 500;
    cursor: pointer;
  }

  .link-btn:hover {
    background: var(--surface-hover);
    text-decoration: underline;
  }
</style>
