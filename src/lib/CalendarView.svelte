<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import Button from './ui/Button.svelte';
  import IconButton from './ui/IconButton.svelte';
  import Segmented from './ui/Segmented.svelte';
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
  <div class="month-nav">
    <div class="month-label">{MONTH_NAMES[viewMonth - 1]} {viewYear}</div>
    <IconButton icon="caret-left" size="sm" label="Previous month" onclick={() => stepMonth(-1)} />
    <Button variant="subtle" title="Jump to today (T)" onclick={jumpToToday}>Today</Button>
    <IconButton icon="caret-right" size="sm" label="Next month" onclick={() => stepMonth(1)} />
  </div>

  <div class="date-source">
    <Segmented
      label="Calendar date source"
      value={dateField}
      options={[
        { value: 'modified', label: 'Modified' },
        { value: 'created', label: 'Created' },
      ]}
      onchange={(field) => (dateField = field)}
    />
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
        {dateField === 'modified' ? 'Modified' : 'Created'} {selectedDateIso}
      </span>
      <span class="day-count-label">{dayNotes.length}</span>
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

  .month-nav {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: var(--space-1) var(--space-2) var(--space-2) var(--space-4);
  }

  .month-label {
    flex: 1;
    font: 600 var(--fs-15) / 1.2 var(--font-display);
  }

  .date-source {
    padding: 0 var(--space-3) var(--space-2);
  }

  .month-grid-wrap {
    margin: 0 var(--space-3);
    border-radius: var(--radius-card);
    outline-offset: 2px;
  }

  .weekday-row,
  .days-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 2px;
  }

  .weekday-cell {
    padding: var(--space-1) 0;
    color: var(--text-muted);
    font: 600 var(--fs-11) / 1 var(--font-ui);
    letter-spacing: 0.04em;
    text-align: center;
    text-transform: uppercase;
  }

  .day-cell {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    aspect-ratio: 1;
    padding: 0;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font: var(--fs-12) var(--font-ui);
    font-variant-numeric: tabular-nums;
    cursor: pointer;
  }

  .day-cell:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .day-cell.outside-month {
    color: var(--text-muted);
    opacity: 0.5;
  }

  .day-cell.heat-1,
  .day-cell.heat-2,
  .day-cell.heat-3 {
    color: var(--text-primary);
  }

  .day-cell.is-today .day-num {
    color: var(--accent);
    font-weight: 700;
  }

  .day-cell.is-selected {
    background: var(--accent);
    color: var(--on-accent);
  }

  .day-cell.is-selected .day-num,
  .day-cell.is-selected .day-dot {
    color: var(--on-accent);
  }

  .day-cell.is-focused {
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .day-indicator {
    display: flex;
    gap: 2px;
  }

  .day-dot {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: currentColor;
    color: var(--accent);
  }

  .heat-2 .day-indicator::after,
  .heat-3 .day-indicator::after {
    content: '';
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0.7;
  }

  .heat-3 .day-indicator::before {
    content: '';
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0.45;
    order: 3;
  }

  .is-selected .day-indicator::before,
  .is-selected .day-indicator::after {
    background: var(--on-accent);
  }

  .day-count {
    display: none;
  }

  .day-notes-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    margin-top: var(--space-3);
    border-top: 1px solid var(--line-subtle);
  }

  .day-notes-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-3) var(--space-1) var(--space-4);
    color: var(--text-muted);
    font: 600 var(--fs-11) / 1 var(--font-ui);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .day-notes-header span:first-child {
    flex: 1;
  }

  .day-count-label {
    font-variant-numeric: tabular-nums;
    letter-spacing: 0;
  }

  .day-notes-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 0 var(--space-2) var(--space-2);
  }

  .empty-day {
    padding: var(--space-2);
    color: var(--text-muted);
    font-size: var(--fs-13);
  }

  .day-note-list {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .day-note-card {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: var(--space-2);
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .day-note-card:hover {
    background: var(--surface-hover);
  }

  .day-note-card.active {
    background: var(--accent-soft);
  }

  .day-note-top {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .day-note-icon {
    display: inline-flex;
    flex-shrink: 0;
    color: var(--text-muted);
  }

  .day-note-title {
    flex: 1;
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .day-note-card.active .day-note-title {
    color: var(--accent);
  }

  .day-note-time {
    flex-shrink: 0;
    color: var(--text-muted);
    font-size: var(--fs-12);
    font-variant-numeric: tabular-nums;
  }

  .day-note-path,
  .day-note-preview {
    overflow: hidden;
    padding-left: 24px;
    color: var(--text-muted);
    font-size: var(--fs-12);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .day-note-preview {
    color: var(--text-secondary);
  }
</style>
