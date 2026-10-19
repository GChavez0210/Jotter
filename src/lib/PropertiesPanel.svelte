<script lang="ts">
  import { tick } from 'svelte';
  import {
    isValidPropertyKey,
    parseFrontmatter,
    removeProperty,
    renameProperty,
    setProperty,
    type Property,
    type PropertyType,
    type PropertyValue,
    type TextChange,
  } from './frontmatter';
  import Icon from './ui/Icon.svelte';
  import type { IconName } from './ui/icons';

  interface Props {
    content: string;
    onEdit: (change: TextChange) => void;
    /** Set by the header's "Add property" button to open a new row. */
    adding?: boolean;
  }

  let { content, onEdit, adding = $bindable(false) }: Props = $props();

  const TYPES: Array<{ value: PropertyType; label: string; icon: IconName }> = [
    { value: 'text', label: 'Text', icon: 'pencil-simple-line' },
    { value: 'list', label: 'List', icon: 'list-bullets' },
    { value: 'number', label: 'Number', icon: 'list-numbers' },
    { value: 'checkbox', label: 'Checkbox', icon: 'check' },
    { value: 'date', label: 'Date', icon: 'calendar-blank' },
    { value: 'datetime', label: 'Date and time', icon: 'calendar-blank' },
  ];

  // The form only needs the top of the note; frontmatter past 64 KB is left to Source mode.
  let fm = $derived(parseFrontmatter(content.slice(0, 64 * 1024)));
  // YAML cannot say that an empty value is a date, so remember types picked for empty values.
  let typeOverrides = $state<Record<string, PropertyType>>({});
  let properties = $derived(
    (fm?.properties ?? []).map((p) =>
      p.editable && p.value === null && typeOverrides[p.key] ? { ...p, type: typeOverrides[p.key] } : p
    )
  );
  let error = $state('');
  let newKeyInput: HTMLInputElement | undefined = $state();

  $effect(() => {
    if (adding) void tick().then(() => newKeyInput?.focus());
  });

  function edit(change: TextChange | null, failure = '') {
    error = change ? '' : failure;
    if (change) onEdit(change);
  }

  function convert(value: PropertyValue, to: PropertyType): PropertyValue {
    if (value === null) return to === 'checkbox' ? false : null;
    const text = Array.isArray(value) ? value.join(', ') : String(value);
    switch (to) {
      case 'list':
        return Array.isArray(value) ? value : text.split(',').map((s) => s.trim()).filter(Boolean);
      case 'checkbox':
        return value === true || /^(true|yes|1)$/i.test(text);
      case 'number': {
        const n = Number(text);
        return text.trim() !== '' && Number.isFinite(n) ? n : null;
      }
      case 'date':
        return /^\d{4}-\d{2}-\d{2}/.test(text) ? text.slice(0, 10) : null;
      case 'datetime':
        return /^\d{4}-\d{2}-\d{2}/.test(text) ? text.slice(0, 16).replace(' ', 'T') : null;
      default:
        return text;
    }
  }

  function setValue(prop: Property, value: PropertyValue, type: PropertyType = prop.type) {
    edit(setProperty(content, prop.key, type, value));
  }

  function rename(prop: Property, input: HTMLInputElement) {
    const next = input.value.trim();
    if (next === prop.key) return;
    if (!isValidPropertyKey(next)) {
      input.value = prop.key;
      error = 'Property names cannot be empty or start with # or -.';
      return;
    }
    const change = renameProperty(content, prop.key, next);
    if (!change) input.value = prop.key;
    edit(change, `A property named "${next}" already exists.`);
  }

  function addListItem(prop: Property, input: HTMLInputElement) {
    const items = input.value.split(',').map((s) => (prop.key === 'tags' ? s.trim().replace(/^#/, '') : s.trim())).filter(Boolean);
    if (items.length === 0) return;
    const current = Array.isArray(prop.value) ? prop.value : [];
    input.value = '';
    setValue(prop, [...current, ...items.filter((i) => !current.includes(i))]);
  }

  function removeListItem(prop: Property, item: string) {
    const current = Array.isArray(prop.value) ? prop.value : [];
    setValue(prop, current.filter((i) => i !== item));
  }

  function commitNewKey(input: HTMLInputElement) {
    const key = input.value.trim();
    adding = false;
    if (!key) return;
    if (!isValidPropertyKey(key)) {
      error = 'Property names cannot start with # or -.';
      return;
    }
    if (properties.some((p) => p.key === key)) {
      error = `A property named "${key}" already exists.`;
      return;
    }
    const type: PropertyType = key === 'tags' || key === 'aliases' ? 'list' : 'text';
    edit(setProperty(content, key, type, type === 'list' ? [] : null));
  }

  function typeIcon(type: PropertyType): IconName {
    return TYPES.find((t) => t.value === type)?.icon ?? 'pencil-simple-line';
  }
</script>

{#if properties.length > 0 || adding}
  <div class="properties" role="group" aria-label="Note properties">
    {#each properties as prop (prop.key)}
      <div class="row">
        <label class="key">
          <span class="type-icon" title={TYPES.find((t) => t.value === prop.type)?.label}>
            <Icon name={prop.editable ? typeIcon(prop.type) : 'code'} size={13} />
            {#if prop.editable}
              <select
                class="type-select"
                aria-label="Type of {prop.key}"
                value={prop.type}
                onchange={(e) => {
                  const to = (e.currentTarget as HTMLSelectElement).value as PropertyType;
                  typeOverrides[prop.key] = to;
                  setValue(prop, convert(prop.value, to), to);
                }}
              >
                {#each TYPES as t (t.value)}<option value={t.value}>{t.label}</option>{/each}
              </select>
            {/if}
          </span>
          <input
            class="key-input"
            value={prop.key}
            aria-label="Property name"
            spellcheck="false"
            onchange={(e) => rename(prop, e.currentTarget as HTMLInputElement)}
            onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()}
          />
        </label>

        <div class="value">
          {#if !prop.editable}
            <code class="raw" title="Switch to Source mode to edit this value">{prop.raw.split('\n').slice(1).join(' ').trim() || prop.raw}</code>
          {:else if prop.type === 'checkbox'}
            <input
              type="checkbox"
              aria-label={prop.key}
              checked={prop.value === true}
              onchange={(e) => setValue(prop, (e.currentTarget as HTMLInputElement).checked)}
            />
          {:else if prop.type === 'list'}
            <div class="list">
              {#each Array.isArray(prop.value) ? prop.value : [] as item (item)}
                <span class="item">
                  {item}
                  <button type="button" aria-label="Remove {item}" onclick={() => removeListItem(prop, item)}>
                    <Icon name="x" size={10} />
                  </button>
                </span>
              {/each}
              <input
                class="list-input"
                placeholder="Add"
                aria-label="Add to {prop.key}"
                onkeydown={(e) => {
                  if (e.key === 'Enter' || e.key === ',') {
                    e.preventDefault();
                    addListItem(prop, e.currentTarget as HTMLInputElement);
                  }
                }}
                onblur={(e) => addListItem(prop, e.currentTarget as HTMLInputElement)}
              />
            </div>
          {:else}
            <input
              class="value-input"
              type={prop.type === 'number' ? 'number' : prop.type === 'date' ? 'date' : prop.type === 'datetime' ? 'datetime-local' : 'text'}
              aria-label={prop.key}
              placeholder="Empty"
              value={prop.value === null ? '' : String(prop.value)}
              onchange={(e) => {
                const raw = (e.currentTarget as HTMLInputElement).value;
                setValue(prop, raw === '' ? null : prop.type === 'number' ? Number(raw) : raw);
              }}
              onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()}
            />
          {/if}
        </div>

        <button
          type="button"
          class="remove"
          aria-label="Remove property {prop.key}"
          title="Remove property"
          onclick={() => edit(removeProperty(content, prop.key))}
        >
          <Icon name="trash" size={13} />
        </button>
      </div>
    {/each}

    {#if adding}
      <div class="row">
        <label class="key">
          <span class="type-icon"><Icon name="plus" size={13} /></span>
          <input
            bind:this={newKeyInput}
            class="key-input"
            placeholder="Property name"
            aria-label="New property name"
            spellcheck="false"
            onblur={(e) => commitNewKey(e.currentTarget as HTMLInputElement)}
            onkeydown={(e) => {
              if (e.key === 'Enter') (e.currentTarget as HTMLInputElement).blur();
              if (e.key === 'Escape') {
                (e.currentTarget as HTMLInputElement).value = '';
                (e.currentTarget as HTMLInputElement).blur();
              }
            }}
          />
        </label>
      </div>
    {:else}
      <button type="button" class="add" onclick={() => (adding = true)}>
        <Icon name="plus" size={13} />Add property
      </button>
    {/if}

    {#if error}<div class="error" role="alert">{error}</div>{/if}
  </div>
{/if}

<style>
  .properties {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: var(--space-3);
    padding: var(--space-2) 0;
    border-top: 1px solid var(--line-subtle);
    border-bottom: 1px solid var(--line-subtle);
    font-size: var(--fs-13);
  }

  .row {
    display: grid;
    grid-template-columns: minmax(110px, 180px) 1fr 28px;
    align-items: center;
    gap: var(--space-2);
    min-height: 30px;
  }

  .row:hover .remove,
  .remove:focus-visible {
    opacity: 1;
  }

  .key {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
  }

  .type-icon {
    position: relative;
    display: inline-flex;
    flex: none;
    width: 16px;
    justify-content: center;
  }

  /* The select sits invisibly over the icon so the icon opens the type menu. */
  .type-select {
    position: absolute;
    inset: -4px;
    opacity: 0;
    cursor: pointer;
  }

  input:not([type='checkbox']) {
    width: 100%;
    min-width: 0;
    height: 26px;
    padding: 0 6px;
    border: 1px solid transparent;
    border-radius: 4px;
    background: transparent;
    color: var(--text-primary);
    font: inherit;
  }

  .key-input {
    color: var(--text-secondary);
  }

  input:not([type='checkbox']):hover {
    background: var(--surface-app);
  }

  input:not([type='checkbox']):focus {
    border-color: var(--line-control);
    background: var(--surface-app);
    outline: none;
  }

  input[type='checkbox'] {
    margin-left: 6px;
    accent-color: var(--accent);
  }

  .list {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
  }

  .item {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 22px;
    padding: 0 2px 0 8px;
    border-radius: 11px;
    background: var(--accent-soft);
    color: var(--text-primary);
  }

  .item button {
    display: inline-flex;
    padding: 3px;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }

  .item button:hover {
    color: var(--text-primary);
  }

  .list-input {
    flex: 1;
    width: auto !important;
    min-width: 60px !important;
  }

  .raw {
    padding: 0 6px;
    color: var(--text-muted);
    font: var(--fs-12) / 1.4 var(--font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .remove {
    display: inline-flex;
    justify-content: center;
    padding: 6px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    opacity: 0;
    cursor: pointer;
  }

  .remove:hover {
    background: var(--surface-app);
    color: var(--text-primary);
  }

  .add {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    align-self: flex-start;
    height: 28px;
    padding: 0 6px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    cursor: pointer;
  }

  .add:hover {
    background: var(--surface-app);
    color: var(--text-primary);
  }

  .error {
    padding: 2px 6px;
    color: var(--status-danger);
    font-size: var(--fs-12);
  }
</style>
