/**
 * Routes a note into one of the two editor panes.
 *
 * Both panes read their text from the same open tabs, so the second pane needs the tab loaded
 * without taking the first pane's place. Asking for a note the other pane already shows swaps
 * the two panes instead of showing one note twice.
 */

import { settingsStore } from './stores/settings.svelte';
import { tabsStore } from './stores/tabs.svelte';
import type { PaneId } from './types';
import { sameNotePath as samePath } from './notePaths';


/** The pane a note is open in, or `null` when it is in neither. */
export function paneShowing(relPath: string): PaneId | null {
  if (samePath(relPath, tabsStore.activeTabPath)) return 'primary';
  if (settingsStore.splitVisible && samePath(relPath, settingsStore.splitNotePath)) {
    return 'secondary';
  }
  return null;
}

/** Opens a note in the pane the user last worked in, so a tab click lands where they look. */
export async function openInActivePane(
  relPath: string,
  options?: { heading?: string; lineNumber?: number }
): Promise<void> {
  await tabsStore.openNoteInTab(relPath, options);
}

/** Opens a note beside the one on screen, from the file tree or the command palette. */
export async function openInSplit(relPath: string): Promise<void> {
  const loaded = await tabsStore.loadTab(relPath);
  if (!loaded) return;
  if (samePath(loaded, tabsStore.activeTabPath)) {
    // The note is already the one on the left, so the split would show it twice. Give the
    // left pane the note that was there before and keep this one on the right.
    const other = settingsStore.splitNotePath && !samePath(settingsStore.splitNotePath, loaded)
      ? settingsStore.splitNotePath : lastOtherTab(loaded);
    settingsStore.splitNotePath = null;
    tabsStore.activeTabPath = other;
    settingsStore.openSplitNote(loaded);
    return;
  }
  settingsStore.openSplitNote(loaded);
}

/** Opens the split on the note the user worked in before this one. */
export function toggleSplitNote(): void {
  if (settingsStore.splitMode !== 'off') {
    settingsStore.closeSplit();
    return;
  }
  settingsStore.openSplitNote(lastOtherTab(tabsStore.activeTabPath));
}

/** The most recently opened tab that is not `relPath`, or `null` when there is none. */
function lastOtherTab(relPath: string | null): string | null {
  for (let i = tabsStore.openTabs.length - 1; i >= 0; i--) {
    const tab = tabsStore.openTabs[i];
    if (!samePath(tab.relPath, relPath)) return tab.relPath;
  }
  return null;
}
