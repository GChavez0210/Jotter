import { shortcutLabel } from '../platform';
import { readNote, saveNote } from '../ipc';
import { clipboardStore } from './clipboard.svelte';
import { settingsStore } from './settings.svelte';
import { m } from '../i18n.svelte';
import { sameNotePath, notePathKey } from '../notePaths';
import type { PaneId } from '../types';

export interface OpenTab {
  relPath: string;
  title: string;
  content: string;
  savedContent: string;
  dirty: boolean;
  externalChangePending: boolean;
}

export const SAMPLE_NOTE = shortcutLabel(`# Jotter — Live Preview & Clipboard Verification

Copy any block below with Ctrl+C (or the Copy button) and paste it into Google Chat, Slack, Gmail, Microsoft Word, or a plain-text editor.

## 1. Consecutive Single Line Breaks & Inline Formatting
Line A: First line with **bold text** and *italic text*.
Line B: Immediately below Line A (single newline, never collapsed into a space).
Line C: Immediately below Line B with \`inline code\` and ~~strikethrough~~.

## 2. Preserved Blank Lines (Single and Double Gaps)
Paragraph 1: Separated from Paragraph 2 by one blank line.

Paragraph 2: Separated from Paragraph 3 by two blank lines.


Paragraph 3: Exact blank line count stays intact just like a plain-text editor.

## 3. Obsidian Vault Syntax, Task Lists & Blockquotes
See [[Product Spec|Jotter Product Spec]] and [[Architecture#Clipboard Engine]] for #notes details.
- [x] Write text/plain with real \\n line breaks
- [x] Write text/html where every newline becomes <br>
- [ ] Click this task checkbox in Live mode to toggle it directly
> Tip: Place your cursor on any formatted line in Live mode to reveal its raw Markdown syntax, or press Ctrl+/ to switch between Live and Source modes.

## 4. Tables & Code Blocks
| Shortcut | Action | Mode |
| --- | --- | --- |
| Ctrl+Shift+M | Minimal Focus Mode | Editor + thin title bar |
| F11 | Zen Full Screen Mode | Text canvas only |
| Ctrl+P | Command Palette | Reachable everywhere |

\`\`\`rust
fn copy_preserves_newlines(input: &str) -> String {
    input.replace("\\r\\n", "\\n")
}
\`\`\``);

export interface PendingScrollTarget {
  pane?: PaneId;
  relPath: string;
  heading?: string;
  lineNumber?: number;
  seq: number;
}

class TabsStore {
  openTabs = $state<OpenTab[]>([]);
  private activePath = $state<string | null>(null);
  get activeTabPath(): string | null { return this.activePath; }
  set activeTabPath(path: string | null) {
    if (sameNotePath(path, settingsStore.splitNotePath)) {
      settingsStore.splitNotePath = this.openTabs.some((t) => sameNotePath(t.relPath, this.activePath))
        && !sameNotePath(path, this.activePath) ? this.activePath : null;
    }
    if (path !== this.activePath) this.pendingScrollTarget = null;
    this.activePath = path;
  }
  scratchpadMode = $state(false);
  scratchpadContent = $state(SAMPLE_NOTE);
  pendingScrollTarget = $state<PendingScrollTarget | null>(null);
  docStats = $state({
    lines: 0,
    words: 0,
    chars: 0,
    selectedChars: 0,
    selectedLines: 0,
    cursorLine: 1,
  });

  onNoteSaved: (() => void) | null = null;
  private scrollSeq = 0;
  private autosaveTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private saves = new Map<string, Promise<boolean>>();
  private generation = 0;

  activeTab = $derived(
    this.openTabs.find((t) => t.relPath === this.activeTabPath) ?? null
  );

  focusedTab = $derived(
    settingsStore.effectivePane === 'secondary'
      ? this.openTabs.find((t) => sameNotePath(t.relPath, settingsStore.splitNotePath)) ?? null
      : this.activeTab
  );

  get focusedTabPath(): string | null { return this.focusedTab?.relPath ?? null; }

  resetTabs() {
    this.generation++;
    this.clearAllTimers();
    this.openTabs = [];
    this.activeTabPath = null;
    this.pendingScrollTarget = null;
    // Both split modes show an open note, so the second pane closes with the tabs.
    settingsStore.closeSplit();
  }

  clearPendingScrollTarget() {
    this.pendingScrollTarget = null;
  }

  /** Recheck after the animation frame: a tab switch or newer request cancels this jump. */
  consumeScrollTarget(target: PendingScrollTarget): boolean {
    if (this.pendingScrollTarget?.seq !== target.seq) return false;
    this.pendingScrollTarget = null;
    return sameNotePath(target.pane === 'secondary' ? settingsStore.splitNotePath : this.activeTabPath, target.relPath);
  }

  /** Scrolls the open note to a line, for panels that have no handle on the editor. */
  requestScrollToLine(lineNumber: number) {
    const relPath = this.activeTabPath;
    if (!relPath) return;
    // The sequence makes a repeat jump to the same line a fresh request.
    this.pendingScrollTarget = { relPath, lineNumber, pane: 'primary', seq: ++this.scrollSeq };
  }

  clearAllTimers() {
    for (const timer of this.autosaveTimers.values()) {
      clearTimeout(timer);
    }
    this.autosaveTimers.clear();
  }

  async openNoteInTab(
    relPath: string,
    options?: { heading?: string; lineNumber?: number }
  ) {
    let pane = settingsStore.effectivePane;
    const loaded = await this.loadTab(relPath);
    if (!loaded) return;
    if (pane === 'secondary' && settingsStore.effectivePane === 'secondary') {
      if (sameNotePath(loaded, this.activeTabPath)) {
        const previous = settingsStore.splitNotePath;
        settingsStore.splitNotePath = null;
        this.activeTabPath = previous;
      }
      settingsStore.splitNotePath = loaded;
    } else {
      pane = 'primary';
      this.activeTabPath = loaded;
    }
    if (options?.heading || options?.lineNumber) {
      this.pendingScrollTarget = {
        relPath: loaded,
        pane,
        heading: options.heading,
        lineNumber: options.lineNumber,
        seq: ++this.scrollSeq,
      };
    }
  }

  /**
   * Reads a note into an open tab without making it the active one, and returns the path the
   * vault gave it. The second editor pane opens notes this way.
   */
  async loadTab(relPath: string): Promise<string | null> {
    const existing = this.openTabs.find(
      (t) => sameNotePath(t.relPath, relPath)
    );
    if (existing) return existing.relPath;

    try {
      const generation = this.generation;
      const doc = await readNote(relPath);
      if (generation !== this.generation) return null;
      const alreadyLoaded = this.openTabs.find((t) => sameNotePath(t.relPath, doc.relPath));
      if (alreadyLoaded) return alreadyLoaded.relPath;
      const tab: OpenTab = {
        relPath: doc.relPath,
        title: doc.title,
        content: doc.content,
        savedContent: doc.content,
        dirty: false,
        externalChangePending: false,
      };
      this.openTabs = [...this.openTabs, tab];
      clipboardStore.setStatus(m.status_opened({ path: doc.relPath }));
      return tab.relPath;
    } catch (err) {
      clipboardStore.setStatus(
        m.status_open_failed({ error: err instanceof Error ? err.message : String(err) })
      );
      return null;
    }
  }

  addOrActivateCreatedDoc(relPath: string, title: string, content: string) {
    const newTab: OpenTab = {
      relPath,
      title,
      content,
      savedContent: content,
      dirty: false,
      externalChangePending: false,
    };
    this.openTabs = [
      ...this.openTabs.filter((t) => !sameNotePath(t.relPath, relPath)),
      newTab,
    ];
    if (settingsStore.effectivePane === 'secondary' && !sameNotePath(relPath, this.activeTabPath)) {
      settingsStore.splitNotePath = relPath;
    } else this.activeTabPath = relPath;
  }

  async closeTab(relPath: string, e?: MouseEvent) {
    e?.stopPropagation();
    if (!(await this.flushAutosaveFor(relPath))) return;
    const idx = this.openTabs.findIndex((t) => sameNotePath(t.relPath, relPath));
    if (idx < 0) return;

    const nextTabs = this.openTabs.filter((t) => !sameNotePath(t.relPath, relPath));
    this.openTabs = nextTabs;
    // The second pane empties rather than closing, so the sash stays where the user put it.
    if (sameNotePath(settingsStore.splitNotePath, relPath)) settingsStore.splitNotePath = null;

    if (sameNotePath(this.activeTabPath, relPath)) {
      if (nextTabs.length === 0) {
        this.activeTabPath = null;
      } else {
        const nextIdx = Math.min(idx, nextTabs.length - 1);
        this.activeTabPath = nextTabs[nextIdx].relPath;
      }
    }
  }

  handleEditorDocChange(
    docId: string,
    newContent: string,
    onAfterScratchpadEdit?: () => void
  ) {
    if (docId === '__scratchpad__') {
      this.scratchpadContent = newContent;
      onAfterScratchpadEdit?.();
      return;
    }

    this.openTabs = this.openTabs.map((t) =>
      sameNotePath(t.relPath, docId)
        ? {
            ...t,
            content: newContent,
            dirty: newContent !== t.savedContent,
          }
        : t
    );

    this.scheduleAutosave(docId);
  }

  scheduleAutosave(relPath: string) {
    const prev = this.autosaveTimers.get(relPath);
    if (prev) clearTimeout(prev);
    const timer = setTimeout(() => {
      this.autosaveTimers.delete(relPath);
      void this.flushAutosaveFor(relPath);
    }, 380);
    this.autosaveTimers.set(relPath, timer);
  }

  async flushAutosaveFor(relPath: string): Promise<boolean> {
    const timer = this.autosaveTimers.get(relPath);
    if (timer) {
      clearTimeout(timer);
      this.autosaveTimers.delete(relPath);
    }

    const pending = this.saves.get(relPath);
    if (pending) return pending;
    const save = this.saveUntilClean(relPath);
    this.saves.set(relPath, save);
    try {
      return await save;
    } finally {
      this.saves.delete(relPath);
    }
  }

  private async saveUntilClean(relPath: string): Promise<boolean> {
    // Include edits made while an earlier save is waiting for disk.
    while (true) {
      const tab = this.openTabs.find((t) => sameNotePath(t.relPath, relPath));
      if (!tab || !tab.dirty) return true;

      const contentToSave = tab.content;
      try {
        const savedDoc = await saveNote(relPath, contentToSave);
        this.openTabs = this.openTabs.map((t) =>
          sameNotePath(t.relPath, relPath)
            ? {
                ...t,
                savedContent: contentToSave,
                dirty: t.content !== contentToSave,
                title: savedDoc.title,
              }
            : t
        );
        this.onNoteSaved?.();
        clipboardStore.setStatus(m.status_saved({ path: savedDoc.relPath }));
      } catch (err) {
        clipboardStore.setStatus(
          m.status_autosave_failed({ error: err instanceof Error ? err.message : String(err) })
        );
        return false;
      }
    }
  }

  async flushAllAutosaves() {
    const paths = this.openTabs.filter((t) => t.dirty).map((t) => t.relPath);
    for (const p of paths) {
      if (!(await this.flushAutosaveFor(p))) return false;
    }
    return !this.openTabs.some((t) => t.dirty);
  }

  updatePathsAfterRename(oldRelPath: string, newRelPath: string) {
    const oldPrefix = `${oldRelPath}/`;

    this.openTabs = this.openTabs.map((t) => {
      if (sameNotePath(t.relPath, oldRelPath)) {
        const base = newRelPath.split('/').pop() ?? newRelPath;
        const title = base.toLowerCase().endsWith('.md')
          ? base.slice(0, -3)
          : base;
        return { ...t, relPath: newRelPath, title };
      }
      if (notePathKey(t.relPath).startsWith(notePathKey(oldPrefix))) {
        const updatedRel = `${newRelPath}/${t.relPath.slice(oldPrefix.length)}`;
        return { ...t, relPath: updatedRel };
      }
      return t;
    });

    if (sameNotePath(this.activeTabPath, oldRelPath)) {
      this.activeTabPath = newRelPath;
    } else if (this.activeTabPath && notePathKey(this.activeTabPath).startsWith(notePathKey(oldPrefix))) {
      this.activeTabPath = `${newRelPath}/${this.activeTabPath.slice(
        oldPrefix.length
      )}`;
    }

    const split = settingsStore.splitNotePath;
    if (sameNotePath(split, oldRelPath)) {
      settingsStore.splitNotePath = newRelPath;
    } else if (split && notePathKey(split).startsWith(notePathKey(oldPrefix))) {
      settingsStore.splitNotePath = `${newRelPath}/${split.slice(oldPrefix.length)}`;
    }
  }

  removeTabsUnderPath(relPath: string) {
    const prefix = `${relPath}/`;
    for (const [k, timer] of this.autosaveTimers.entries()) {
      if (sameNotePath(k, relPath) || notePathKey(k).startsWith(notePathKey(prefix))) {
        clearTimeout(timer);
        this.autosaveTimers.delete(k);
      }
    }

    const remaining = this.openTabs.filter(
      (t) => !sameNotePath(t.relPath, relPath) && !notePathKey(t.relPath).startsWith(notePathKey(prefix))
    );
    this.openTabs = remaining;
    const split = settingsStore.splitNotePath;
    if (split && (sameNotePath(split, relPath) || notePathKey(split).startsWith(notePathKey(prefix)))) {
      settingsStore.splitNotePath = null;
    }
    if (
      this.activeTabPath &&
      (sameNotePath(this.activeTabPath, relPath) || notePathKey(this.activeTabPath).startsWith(notePathKey(prefix)))
    ) {
      this.activeTabPath =
        remaining.length > 0 ? remaining[remaining.length - 1].relPath : null;
    }
  }

  /** Takes text written to a note outside the editor, such as a restored version. */
  applyRestoredContent(relPath: string, content: string) {
    this.openTabs = this.openTabs.map((t) =>
      sameNotePath(t.relPath, relPath)
        ? { ...t, content, savedContent: content, dirty: false, externalChangePending: false }
        : t
    );
  }

  dismissExternalChangeBanner(relPath: string) {
    this.openTabs = this.openTabs.map((t) =>
      sameNotePath(t.relPath, relPath) ? { ...t, externalChangePending: false } : t
    );
  }

  async reloadTabFromDisk(
    relPath: string,
    onReplaceEditor?: (relPath: string, newContent: string) => void
  ) {
    try {
      const doc = await readNote(relPath);
      this.openTabs = this.openTabs.map((t) =>
        sameNotePath(t.relPath, relPath)
          ? {
              ...t,
              content: doc.content,
              savedContent: doc.content,
              dirty: false,
              externalChangePending: false,
            }
          : t
      );
      // Either pane may hold the note, so the caller matches the path to its editor.
      onReplaceEditor?.(relPath, doc.content);
      clipboardStore.setStatus(m.status_reloaded({ path: relPath }));
    } catch {
      this.openTabs = this.openTabs.filter((t) => !sameNotePath(t.relPath, relPath));
      if (sameNotePath(this.activeTabPath, relPath)) {
        this.activeTabPath =
          this.openTabs.length > 0
            ? this.openTabs[this.openTabs.length - 1].relPath
            : null;
      }
    }
  }

  async handleExternalPathsChanged(
    changedPaths: string[],
    onReplaceEditor?: (relPath: string, newContent: string) => void
  ) {
    for (const rel of changedPaths) {
      const tab = this.openTabs.find(
        (t) => sameNotePath(t.relPath, rel)
      );
      if (!tab) continue;

      if (!tab.dirty) {
        await this.reloadTabFromDisk(tab.relPath, onReplaceEditor);
      } else {
        this.openTabs = this.openTabs.map((t) =>
          t.relPath === tab.relPath ? { ...t, externalChangePending: true } : t
        );
        clipboardStore.setStatus(m.status_external_edit({ path: tab.relPath }));
      }
    }
  }
}

export const tabsStore = new TabsStore();
