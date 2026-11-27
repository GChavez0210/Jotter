<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './ui/Icon.svelte';
  import { aiAccessOverview, aiAccessSet } from './ipc';
  import { buildAccessTree, type AccessRow } from './aiAccessTree';
  import { m } from './i18n.svelte';
  import type { AiAccessOverview } from './types';

  interface Props {
    /** Called after the selection changes. */
    onChange?: () => void;
  }

  let { onChange }: Props = $props();

  let overview = $state<AiAccessOverview | null>(null);
  let expanded = $state<Record<string, boolean>>({});
  let busy = $state(false);
  let error = $state('');

  const tree = $derived(overview ? buildAccessTree(overview) : null);
  const rows = $derived.by(() => {
    const visible: { row: AccessRow; depth: number }[] = [];
    const walk = (list: AccessRow[], depth: number) => {
      for (const row of list) {
        visible.push({ row, depth });
        if (expanded[row.path]) walk(row.children, depth + 1);
      }
    };
    walk(tree?.children ?? [], 1);
    return visible;
  });

  onMount(async () => {
    try {
      overview = await aiAccessOverview();
    } catch (err) {
      error = String(err);
    }
  });

  async function set(path: string, input: HTMLInputElement) {
    const shared = input.checked;
    busy = true;
    error = '';
    try {
      overview = await aiAccessSet(path, shared);
      onChange?.();
    } catch (err) {
      error = String(err);
      input.checked = !shared;
    } finally {
      busy = false;
    }
  }
</script>

<div class="access">
  {#if tree && overview}
    <div class="muted" role="status">
      {m.ai_access_count({ shared: overview.notes.filter((note) => note.shared).length, total: overview.notes.length })}
    </div>
    <ul>
      <li>
        <span class="caret"></span>
        <label>
          <input
            type="checkbox"
            checked={tree.state === 'all'}
            indeterminate={tree.state === 'some'}
            disabled={busy}
            onchange={(e) => void set('', e.currentTarget)}
          />
          <span>{m.ai_access_vault()}</span>
        </label>
      </li>
      {#each rows as { row, depth } (row.path)}
        <li style:padding-left="{depth * 16}px">
          {#if row.folder}
            <button
              type="button"
              class="caret"
              aria-expanded={!!expanded[row.path]}
              aria-label={m.ai_access_toggle({ name: row.name })}
              onclick={() => (expanded[row.path] = !expanded[row.path])}
            >
              <Icon name={expanded[row.path] ? 'caret-down' : 'caret-right'} />
            </button>
          {:else}
            <span class="caret"></span>
          {/if}
          <label>
            <input
              type="checkbox"
              checked={row.state === 'all'}
              indeterminate={row.state === 'some'}
              disabled={busy || row.blocked}
              onchange={(e) => void set(row.path, e.currentTarget)}
            />
            <span>{row.name}{row.folder ? '/' : ''}</span>
            {#if row.blocked}<span class="muted">ai: false</span>{/if}
          </label>
        </li>
      {/each}
    </ul>
    {#if !overview.notes.length}<div class="muted">{m.ai_access_empty()}</div>{/if}
  {/if}
  {#if error}<div class="error" role="alert">{error}</div>{/if}
</div>

<style>
  .access {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  ul {
    max-height: 240px;
    overflow: auto;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li,
  label {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  li {
    min-height: 28px;
  }

  label {
    min-height: 24px;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  input {
    width: 16px;
    height: 16px;
    accent-color: var(--accent);
  }

  .caret {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text-muted);
  }

  button.caret {
    cursor: pointer;
  }

  .muted {
    color: var(--text-muted);
  }

  .error {
    color: var(--text-danger);
  }
</style>
