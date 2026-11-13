<script lang="ts">
  import Button from './ui/Button.svelte';
  import { settingsStore } from './stores/settings.svelte';
  import {
    CAPTURE_PRESETS,
    MAX_PROFILES,
    blankProfile,
    presetProfile,
    type CaptureMode,
    type CapturePresetKind,
    type CaptureProfile,
  } from './captureProfiles';
  import { m } from './i18n.svelte';

  const presetNames: Record<CapturePresetKind, () => string> = {
    idea: () => m.capture_preset_idea(),
    task: () => m.capture_preset_task(),
    meeting: () => m.capture_preset_meeting(),
    journal: () => m.capture_preset_journal(),
  };

  const profiles = $derived(settingsStore.captureProfiles);
  const full = $derived(profiles.length >= MAX_PROFILES);

  function add(profile: CaptureProfile) {
    settingsStore.setCaptureProfiles([...profiles, profile]);
  }

  function update(id: string, patch: Partial<CaptureProfile>) {
    settingsStore.setCaptureProfiles(profiles.map((p) => (p.id === id ? { ...p, ...patch } : p)));
  }

  function remove(id: string) {
    settingsStore.setCaptureProfiles(profiles.filter((p) => p.id !== id));
  }

  const value = (e: Event) => (e.currentTarget as HTMLInputElement | HTMLTextAreaElement).value;
</script>

<div class="profiles">
  <h4 class="profiles-title" id="capture-profiles-title">{m.settings_capture_profiles()}</h4>
  <p class="desc">{m.settings_capture_profiles_desc()}</p>
  <p class="desc">
    {m.settings_capture_profiles_vars()}
    {#each ['{{date}}', '{{time}}', '{{title}}', '{{text}}', '{{clipboard}}'] as name (name)}<code>{name}</code>{' '}{/each}
    <code>{'{{#if clipboard}}…{{else}}…{{/if}}'}</code>
    {m.settings_capture_profiles_vars_note()}
  </p>

  {#each profiles as profile (profile.id)}
    <div class="card" role="group" aria-label={profile.name || m.capture_profile_unnamed()}>
      <label class="field">
        <span>{m.capture_profile_name()}</span>
        <input type="text" maxlength="60" value={profile.name} onchange={(e) => update(profile.id, { name: value(e).trim() })} />
      </label>
      <label class="field">
        <span>{m.capture_profile_mode()}</span>
        <select
          value={profile.mode}
          onchange={(e) => update(profile.id, { mode: value(e) as CaptureMode })}
        >
          <option value="append">{m.capture_profile_mode_append()}</option>
          <option value="create">{m.capture_profile_mode_create()}</option>
        </select>
      </label>
      <label class="field">
        <span>{profile.mode === 'append' ? m.capture_profile_target_note() : m.capture_profile_target_folder()}</span>
        <input type="text" value={profile.target} onchange={(e) => update(profile.id, { target: value(e) })} />
      </label>
      {#if profile.mode === 'create'}
        <label class="field">
          <span>{m.capture_profile_filename()}</span>
          <input type="text" value={profile.fileName} onchange={(e) => update(profile.id, { fileName: value(e) })} />
        </label>
      {/if}
      <label class="field wide">
        <span>{m.capture_profile_template()}</span>
        <textarea rows="5" maxlength="3000" value={profile.template} onchange={(e) => update(profile.id, { template: value(e) })}></textarea>
      </label>
      {#if profile.mode === 'append'}
        <label class="check">
          <input
            type="checkbox"
            checked={profile.compact}
            onchange={(e) => update(profile.id, { compact: (e.currentTarget as HTMLInputElement).checked })}
          />
          <span>{m.capture_profile_compact()}</span>
        </label>
      {/if}
      <div class="row-end">
        <Button variant="danger" onclick={() => remove(profile.id)}>
          {m.capture_profile_remove({ name: profile.name || m.capture_profile_unnamed() })}
        </Button>
      </div>
    </div>
  {/each}

  <div class="add-row" role="group" aria-label={m.capture_profile_add()}>
    <span class="add-label">{m.capture_profile_add()}</span>
    {#each CAPTURE_PRESETS as kind (kind)}
      <Button
        disabled={full}
        onclick={() => add(presetProfile(kind, presetNames[kind](), crypto.randomUUID()))}
      >
        {presetNames[kind]()}
      </Button>
    {/each}
    <Button disabled={full} onclick={() => add(blankProfile(m.capture_preset_blank(), crypto.randomUUID()))}>
      {m.capture_preset_blank()}
    </Button>
  </div>
</div>

<style>
  .profiles {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .profiles-title {
    margin: 0;
    color: var(--text-primary);
    font-size: var(--fs-13);
    font-weight: 600;
  }

  .desc {
    margin: 0;
    color: var(--text-muted);
    font-size: var(--fs-12);
    line-height: 1.5;
  }

  code {
    padding: 0 4px;
    border-radius: 3px;
    background: var(--surface-field);
    color: var(--text-secondary);
    font-size: var(--fs-12);
  }

  .card {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-control);
  }

  .field {
    display: flex;
    flex: 1 1 180px;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
    color: var(--text-secondary);
    font-size: var(--fs-12);
  }

  .field.wide {
    flex-basis: 100%;
  }

  input[type='text'],
  select,
  textarea {
    min-height: var(--control-min-height);
    padding: 0 var(--space-2);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-control);
    background: var(--surface-field);
    color: var(--text-primary);
    font: var(--fs-13) var(--font-ui);
  }

  textarea {
    padding: var(--space-2);
    font-family: var(--font-mono, monospace);
    resize: vertical;
  }

  input:focus-visible,
  select:focus-visible,
  textarea:focus-visible {
    outline: 2px solid var(--focus-ring, var(--accent));
    outline-offset: 0;
  }

  .check {
    display: flex;
    flex-basis: 100%;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-secondary);
    font-size: var(--fs-13);
  }

  .row-end {
    display: flex;
    flex-basis: 100%;
    justify-content: flex-end;
  }

  .add-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .add-label {
    color: var(--text-secondary);
    font-size: var(--fs-12);
  }
</style>
