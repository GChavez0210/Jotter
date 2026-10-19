<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import { queryCalendarDateNotes, queryCalendarDays } from './ipc';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import type {
    CalendarDateField,
    CalendarDaySummary,
    CalendarNoteEntry,
  } from './types';

  const WEEKDAYS = ['Su', 'Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa'] as const;
  const MONTH_NAMES = [
    'January',
    'February',
    'March',
    'April',
    'May',
    'June',
    'July',
    'August',
    'September',
    'October',
    'November',
    'December',
  ] as const;

  function toIsoDate(d: Date): string {
    const yyyy = d.getFullYear();
    const mm = String(d.getMonth() + 1).padStart(2, '0');
    const dd = String(d.getDate()).padStart(2, '0');
    return `${yyyy}-${mm}-${dd}`;
  }

  function parseIsoDate(iso: string): Date {
    const [y, m, d] = iso.split('-').map(Number);
    return new Date(y, (m || 1) - 1, d || 1);
  }

  function addDaysToIso(iso: string, deltaDays: number): string {
    const dt = parseIsoDate(iso);
    dt.setDate(dt.getDate() + deltaDays);
    return toIsoDate(dt);
  }

  const todayStr = toIsoDate(new Date());
  const now = new Date();

  let viewYear = $state(now.getFullYear());
  let viewMonth = $state(now.getMonth() + 1); // 1..12
  let dateField = $state<CalendarDateField>('modified');
  let focusedDateIso = $state(todayStr);
  let selectedDateIso = $state(todayStr);

  let dayCounts = $state<Record<string, number>>({});
  let dayNotes = $state<CalendarNoteEntry[]>([]);
  let loadingNotes = $state(false);
  let gridContainerEl: HTMLDivElement | undefined = $state();

  interface CalendarCell {
    dateIso: string;
    dayNum: number;
    inCurrentMonth: boolean;
    count: number;
    heatLevel: 0 | 1 | 2 | 3;
  }

  let calendarCells = $derived.by<CalendarCell[]>(() => {
    const firstOfMonth = new Date(viewYear, viewMonth - 1, 1);
    const startWeekday = firstOfMonth.getDay(); // 0 (Sun) .. 6 (Sat)
    const daysInMonth = new Date(viewYear, viewMonth, 0).getDate();

    const cells: CalendarCell[] = [];
    // Leading days from previous month
    for (let i = startWeekday - 1; i >= 0; i--) {
      const dt = new Date(viewYear, viewMonth - 1, -i);
      const iso = toIsoDate(dt);
      const count = dayCounts[iso] ?? 0;
      cells.push({
        dateIso: iso,
        dayNum: dt.getDate(),
        inCurrentMonth: false,
        count,
        heatLevel: computeHeat(count),
      });
    }

    // Current month days
    for (let d = 1; d <= daysInMonth; d++) {
      const dt = new Date(viewYear, viewMonth - 1, d);
      const iso = toIsoDate(dt);
      const count = dayCounts[iso] ?? 0;
      cells.push({
        dateIso: iso,
        dayNum: d,
        inCurrentMonth: true,
        count,
        heatLevel: computeHeat(count),
      });
    }

    // Trailing days to complete 6 rows (42 cells) or 5 rows (35 cells)
    const targetCells = cells.length > 35 ? 42 : 35;
    let nextDay = 1;
    while (cells.length < targetCells) {
      const dt = new Date(viewYear, viewMonth, nextDay++);
      const iso = toIsoDate(dt);
      const count = dayCounts[iso] ?? 0;
      cells.push({
        dateIso: iso,
        dayNum: dt.getDate(),
        inCurrentMonth: false,
        count,
        heatLevel: computeHeat(count),
      });
    }

    return cells;
  });

  function computeHeat(count: number): 0 | 1 | 2 | 3 {
    if (count <= 0) return 0;
    if (count === 1) return 1;
    if (count <= 3) return 2;
    return 3;
  }

  $effect(() => {
    const y = viewYear;
    const m = viewMonth;
    const field = dateField;
    const version = vaultStore.indexVersion;
    void version;

    if (!vaultStore.vaultSnapshot) {
      dayCounts = {};
      return;
    }

    const ym = `${y}-${String(m).padStart(2, '0')}`;
    void queryCalendarDays(ym, field)
      .then((rows: CalendarDaySummary[]) => {
        if (viewYear === y && viewMonth === m && dateField === field) {
          const map: Record<string, number> = {};
          for (const r of rows) {
            map[r.date] = r.count;
          }
          dayCounts = map;
        }
      })
      .catch(() => {
        if (viewYear === y && viewMonth === m && dateField === field) {
          dayCounts = {};
        }
      });
  });

  $effect(() => {
    const selIso = selectedDateIso;
    const field = dateField;
    const version = vaultStore.indexVersion;
    void version;

    if (!selIso || !vaultStore.vaultSnapshot) {
      dayNotes = [];
      loadingNotes = false;
      return;
    }

    loadingNotes = true;
    void queryCalendarDateNotes(selIso, field)
      .then((notes) => {
        if (selectedDateIso === selIso && dateField === field) {
          dayNotes = notes;
        }
      })
      .catch(() => {
        if (selectedDateIso === selIso && dateField === field) {
          dayNotes = [];
        }
      })
      .finally(() => {
        if (selectedDateIso === selIso && dateField === field) {
          loadingNotes = false;
        }
      });
  });

  function syncMonthToIso(iso: string) {
    const dt = parseIsoDate(iso);
    const y = dt.getFullYear();
    const m = dt.getMonth() + 1;
    if (y !== viewYear || m !== viewMonth) {
      viewYear = y;
      viewMonth = m;
    }
  }

  function stepMonth(delta: number) {
    const dt = new Date(viewYear, viewMonth - 1 + delta, 1);
    viewYear = dt.getFullYear();
    viewMonth = dt.getMonth() + 1;
  }

  function jumpToToday() {
    const today = toIsoDate(new Date());
    syncMonthToIso(today);
    focusedDateIso = today;
    selectedDateIso = today;
  }

  function handleSelectDay(iso: string) {
    syncMonthToIso(iso);
    focusedDateIso = iso;
    selectedDateIso = iso;
  }

  function handleGridKeyDown(e: KeyboardEvent) {
    if (e.key === 'ArrowLeft') {
      e.preventDefault();
      const next = addDaysToIso(focusedDateIso, -1);
      focusedDateIso = next;
      syncMonthToIso(next);
      return;
    }
    if (e.key === 'ArrowRight') {
      e.preventDefault();
      const next = addDaysToIso(focusedDateIso, 1);
      focusedDateIso = next;
      syncMonthToIso(next);
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      const next = addDaysToIso(focusedDateIso, -7);
      focusedDateIso = next;
      syncMonthToIso(next);
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      const next = addDaysToIso(focusedDateIso, 7);
      focusedDateIso = next;
      syncMonthToIso(next);
      return;
    }
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      selectedDateIso = focusedDateIso;
      syncMonthToIso(focusedDateIso);
      return;
    }
    if (e.key === 't' || e.key === 'T') {
      e.preventDefault();
      jumpToToday();
    }
  }

  function formatTimeShort(ms: number): string {
    if (!ms) return '';
    const d = new Date(ms);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }
</script>

<div class="calendar-view" aria-label="Vault calendar view">
  <div class="calendar-top-controls">
    <div class="field-toggle" role="radiogroup" aria-label="Calendar date source">
      <button
        type="button"
        role="radio"
        aria-checked={dateField === 'modified'}
        class="field-btn"
        class:active={dateField === 'modified'}
        onclick={() => (dateField = 'modified')}
      >
        Modified
      </button>
      <button
        type="button"
        role="radio"
        aria-checked={dateField === 'created'}
        class="field-btn"
        class:active={dateField === 'created'}
        onclick={() => (dateField = 'created')}
      >
        Created
      </button>
    </div>

    <button
      type="button"
      class="today-btn"
      title="Jump to today (T)"
      onclick={jumpToToday}
    >
      Today
    </button>
  </div>

  <div class="month-nav">
    <button
      type="button"
      class="month-step-btn"
      aria-label="Previous month"
      onclick={() => stepMonth(-1)}
    >
      ‹
    </button>
    <div class="month-label">
      {MONTH_NAMES[viewMonth - 1]} {viewYear}
    </div>
    <button
      type="button"
      class="month-step-btn"
      aria-label="Next month"
      onclick={() => stepMonth(1)}
    >
      ›
    </button>
  </div>

  <!-- Keyboard-navigable month grid: Arrow keys move between days, Enter opens day, T jumps to today -->
  <div
    bind:this={gridContainerEl}
    class="month-grid-wrap"
    role="grid"
    tabindex="0"
    aria-label="{MONTH_NAMES[viewMonth - 1]} {viewYear} calendar grid (Arrow keys move, Enter selects day, T jumps to today)"
    onkeydown={handleGridKeyDown}
  >
    <div class="weekday-row" role="row">
      {#each WEEKDAYS as wd}
        <div class="weekday-cell" role="columnheader">{wd}</div>
      {/each}
    </div>

    <div class="days-grid">
      {#each calendarCells as cell (cell.dateIso)}
        <button
          type="button"
          tabindex="-1"
          class="day-cell heat-{cell.heatLevel}"
          class:outside-month={!cell.inCurrentMonth}
          class:is-today={cell.dateIso === todayStr}
          class:is-selected={cell.dateIso === selectedDateIso}
          class:is-focused={cell.dateIso === focusedDateIso}
          aria-pressed={cell.dateIso === selectedDateIso}
          title="{cell.dateIso}: {cell.count} {cell.count === 1 ? 'note' : 'notes'} {dateField}"
          onclick={() => {
            handleSelectDay(cell.dateIso);
            gridContainerEl?.focus();
          }}
        >
          <span class="day-num">{cell.dayNum}</span>
          {#if cell.count > 0}
            <span class="day-indicator">
              <span class="day-dot"></span>
              <span class="day-count">{cell.count}</span>
            </span>
          {/if}
        </button>
      {/each}
    </div>
  </div>

  <div class="day-notes-section">
    <div class="day-notes-header">
      <span>
        {dateField === 'modified' ? 'Modified' : 'Created'} on
        <strong>{selectedDateIso}</strong> ({dayNotes.length})
      </span>
      <span class="kbd-hint">←↑↓→ · Enter · T</span>
    </div>

    <div class="day-notes-scroll">
      {#if loadingNotes && dayNotes.length === 0}
        <div class="empty-day">Loading notes…</div>
      {:else if dayNotes.length === 0}
        <div class="empty-day">
          No notes {dateField} on {selectedDateIso}.
        </div>
      {:else}
        <ul class="day-note-list" aria-label="Notes touched on {selectedDateIso}">
          {#each dayNotes as note (note.path)}
            <li>
              <button
                type="button"
                class="day-note-card"
                class:active={tabsStore.activeTabPath === note.path}
                onclick={() => void tabsStore.openNoteInTab(note.path)}
              >
                <div class="day-note-top">
                  <span class="day-note-icon"><Icon name="file-text" /></span>
                  <span class="day-note-title">{note.title}</span>
                  <span class="day-note-time">
                    {formatTimeShort(
                      dateField === 'modified' ? note.modifiedMs : note.createdMs
                    )}
                  </span>
                </div>
                {#if note.path !== `${note.title}.md`}
                  <div class="day-note-path">{note.path}</div>
                {/if}
                {#if note.preview}
                  <div class="day-note-preview">{note.preview}</div>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
</div>

<style>
  .calendar-view {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .calendar-top-controls {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-2);
    border-bottom: 1px solid var(--line-subtle);
  }

  .field-toggle {
    display: inline-flex;
    background: var(--surface-subtle);
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-control);
    padding: 2px;
    gap: 2px;
  }

  .field-btn {
    min-height: 26px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-control);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-secondary);
    font-size: var(--fs-12);
    font-weight: 500;
    cursor: pointer;
    transition: background-color var(--transition-fast);
  }

  .field-btn.active {
    background: var(--surface-raised);
    color: var(--text-primary);
    border-color: var(--line-control);
    font-weight: 600;
  }

  .today-btn {
    min-height: 28px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-control);
    border: 1px solid var(--line-control);
    background: var(--surface-field);
    color: var(--text-primary);
    font-size: var(--fs-12);
    cursor: pointer;
  }

  .today-btn:hover {
    background: var(--surface-hover);
  }

  .month-nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-1) var(--space-2);
  }

  .month-label {
    font-size: var(--fs-12);
    font-weight: 600;
    color: var(--text-primary);
  }

  .month-step-btn {
    min-width: 28px;
    min-height: 28px;
    border-radius: var(--radius-control);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-secondary);
    font-size: var(--fs-15);
    cursor: pointer;
  }

  .month-step-btn:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .month-grid-wrap {
    padding: 0 var(--space-2) var(--space-2) var(--space-2);
    border-bottom: 1px solid var(--line-subtle);
    outline: none;
  }

  .month-grid-wrap:focus-visible {
    box-shadow: inset 0 0 0 2px var(--accent);
    border-radius: var(--radius-control);
  }

  .weekday-row {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    margin-bottom: 2px;
  }

  .weekday-cell {
    text-align: center;
    font-size: var(--fs-11);
    font-weight: 600;
    color: var(--text-muted);
    padding: 2px 0;
  }

  .days-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 2px;
  }

  .day-cell {
    min-height: 34px;
    padding: 2px;
    border-radius: var(--radius-control);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-primary);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1px;
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      border-color var(--transition-fast);
  }

  .day-cell:hover {
    background: var(--surface-hover);
  }

  .day-cell.outside-month {
    opacity: 0.42;
  }

  .day-cell.heat-1 {
    background: color-mix(
      in srgb,
      var(--accent) 10%,
      transparent
    );
  }

  .day-cell.heat-2 {
    background: color-mix(
      in srgb,
      var(--accent) 20%,
      transparent
    );
  }

  .day-cell.heat-3 {
    background: color-mix(
      in srgb,
      var(--accent) 32%,
      transparent
    );
  }

  .day-cell.is-today .day-num {
    color: var(--accent);
    font-weight: 700;
    text-decoration: underline;
  }

  .day-cell.is-focused {
    border-color: var(--line-control);
  }

  .day-cell.is-selected {
    background: var(--selection);
    border-color: var(--accent);
    font-weight: 600;
  }

  .day-num {
    font-size: var(--fs-11);
    line-height: 1.1;
  }

  .day-indicator {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }

  .day-dot {
    width: 4px;
    height: 4px;
    border-radius: 999px;
    background: var(--accent);
  }

  .day-count {
    font-family: var(--font-mono);
    font-size: var(--fs-11);
    line-height: 1;
    color: var(--text-secondary);
  }

  .day-notes-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: var(--space-2);
    gap: var(--space-2);
    overflow: hidden;
  }

  .day-notes-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: var(--fs-11);
    color: var(--text-secondary);
  }

  .kbd-hint {
    font-family: var(--font-mono);
    font-size: var(--fs-11);
    color: var(--text-muted);
  }

  .day-notes-scroll {
    flex: 1;
    overflow-y: auto;
  }

  .empty-day {
    padding: var(--space-3) var(--space-2);
    text-align: center;
    font-size: var(--fs-12);
    color: var(--text-secondary);
  }

  .day-note-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .day-note-card {
    width: 100%;
    text-align: left;
    padding: var(--space-2);
    border-radius: var(--radius-control);
    border: 1px solid var(--line-subtle);
    background: var(--surface-subtle);
    color: var(--text-primary);
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .day-note-card:hover {
    background: var(--surface-hover);
    border-color: var(--line-control);
  }

  .day-note-card.active {
    background: var(--selection);
    border-color: var(--accent);
  }

  .day-note-top {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
  }

  .day-note-icon {
    display: inline-flex;
    color: var(--accent);
    flex-shrink: 0;
  }

  .day-note-title {
    font-size: var(--fs-12);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .day-note-time {
    font-family: var(--font-mono);
    font-size: var(--fs-11);
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .day-note-path {
    font-size: var(--fs-11);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .day-note-preview {
    font-size: var(--fs-11);
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
