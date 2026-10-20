import { shortcutLabel } from '../platform';
import { readNote, saveNote } from '../ipc';
import { clipboardStore } from './clipboard.svelte';

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
  relPath: string;
  heading?: string;
  lineNumber?: number;
  seq: number;
}

class TabsStore {
  openTabs = $state<OpenTab[]>([]);
  activeTabPath = $state<string | null>(null);
  scratchpadMode = $state(false);
  scratchpadContent = $state(SAMPLE_NOTE);
  pendingScrollTarget = $state<PendingScrollTarget | null>(null);
  docStats = $state({
    lines: 0,
    words: 0,
    chars: 0,
    selectedChars: 0,
    selectedLines: 0,
  });

  onNoteSaved: (() => void) | null = null;
  private scrollSeq = 0;
  private autosaveTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private saves = new Map<string, Promise<boolean>>();
  private generation = 0;

  activeTab = $derived(
    this.openTabs.find((t) => t.relPath === this.activeTabPath) ?? null
  );

  resetTabs() {
    this.generation++;
    this.clearAllTimers();
    this.openTabs = [];
    this.activeTabPath = null;
    this.pendingScrollTarget = null;
  }

  clearPendingScrollTarget() {
    this.pendingScrollTarget = null;
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
    const existing = this.openTabs.find(
      (t) => t.relPath.toLowerCase() === relPath.toLowerCase()
    );
    if (existing) {
      this.activeTabPath = existing.relPath;
      if (options?.heading || options?.lineNumber) {
        this.pendingScrollTarget = {
          relPath: existing.relPath,
          heading: options.heading,
          lineNumber: options.lineNumber,
          seq: ++this.scrollSeq,
        };
      }
      return;
    }

    try {
      const generation = this.generation;
      const doc = await readNote(relPath);
      if (generation !== this.generation) return;
      const tab: OpenTab = {
        relPath: doc.relPath,
        title: doc.title,
        content: doc.content,
        savedContent: doc.content,
        dirty: false,
        externalChangePending: false,
      };
      this.openTabs = [...this.openTabs, tab];
      this.activeTabPath = tab.relPath;
      if (options?.heading || options?.lineNumber) {
        this.pendingScrollTarget = {
          relPath: tab.relPath,
          heading: options.heading,
          lineNumber: options.lineNumber,
          seq: ++this.scrollSeq,
        };
      }
      clipboardStore.setStatus(`Opened ${doc.relPath}`);
    } catch (err) {
      clipboardStore.setStatus(
        `Error opening note: ${err instanceof Error ? err.message : String(err)}`
      );
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
      ...this.openTabs.filter((t) => t.relPath !== relPath),
      newTab,
    ];
    this.activeTabPath = relPath;
  }

  async closeTab(relPath: string, e?: MouseEvent) {
    e?.stopPropagation();
    if (!(await this.flushAutosaveFor(relPath))) return;
    const idx = this.openTabs.findIndex((t) => t.relPath === relPath);
    if (idx < 0) return;

    const nextTabs = this.openTabs.filter((t) => t.relPath !== relPath);
    this.openTabs = nextTabs;

    if (this.activeTabPath === relPath) {
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
      t.relPath === docId
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
      const tab = this.openTabs.find((t) => t.relPath === relPath);
      if (!tab || !tab.dirty) return true;

      const contentToSave = tab.content;
      try {
        const savedDoc = await saveNote(relPath, contentToSave);
        this.openTabs = this.openTabs.map((t) =>
          t.relPath === relPath
            ? {
                ...t,
                savedContent: contentToSave,
                dirty: t.content !== contentToSave,
                title: savedDoc.title,
              }
            : t
        );
        this.onNoteSaved?.();
        clipboardStore.setStatus(`Saved ${savedDoc.relPath}`);
      } catch (err) {
        clipboardStore.setStatus(
          `Autosave failed: ${err instanceof Error ? err.message : String(err)}`
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
      if (t.relPath === oldRelPath) {
        const base = newRelPath.split('/').pop() ?? newRelPath;
        const title = base.toLowerCase().endsWith('.md')
          ? base.slice(0, -3)
          : base;
        return { ...t, relPath: newRelPath, title };
      }
      if (t.relPath.startsWith(oldPrefix)) {
        const updatedRel = `${newRelPath}/${t.relPath.slice(oldPrefix.length)}`;
        return { ...t, relPath: updatedRel };
      }
      return t;
    });

    if (this.activeTabPath === oldRelPath) {
      this.activeTabPath = newRelPath;
    } else if (this.activeTabPath?.startsWith(oldPrefix)) {
      this.activeTabPath = `${newRelPath}/${this.activeTabPath.slice(
        oldPrefix.length
      )}`;
    }
  }

  removeTabsUnderPath(relPath: string) {
    const prefix = `${relPath}/`;
    for (const [k, timer] of this.autosaveTimers.entries()) {
      if (k === relPath || k.startsWith(prefix)) {
        clearTimeout(timer);
        this.autosaveTimers.delete(k);
      }
    }

    const remaining = this.openTabs.filter(
      (t) => t.relPath !== relPath && !t.relPath.startsWith(prefix)
    );
    this.openTabs = remaining;
    if (
      this.activeTabPath &&
      (this.activeTabPath === relPath || this.activeTabPath.startsWith(prefix))
    ) {
      this.activeTabPath =
        remaining.length > 0 ? remaining[remaining.length - 1].relPath : null;
    }
  }

  dismissExternalChangeBanner(relPath: string) {
    this.openTabs = this.openTabs.map((t) =>
      t.relPath === relPath ? { ...t, externalChangePending: false } : t
    );
  }

  async reloadTabFromDisk(
    relPath: string,
    onReplaceActiveEditor?: (newContent: string) => void
  ) {
    try {
      const doc = await readNote(relPath);
      this.openTabs = this.openTabs.map((t) =>
        t.relPath === relPath
          ? {
              ...t,
              content: doc.content,
              savedContent: doc.content,
              dirty: false,
              externalChangePending: false,
            }
          : t
      );
      if (this.activeTabPath === relPath) {
        onReplaceActiveEditor?.(doc.content);
      }
      clipboardStore.setStatus(`Reloaded external changes: ${relPath}`);
    } catch {
      this.openTabs = this.openTabs.filter((t) => t.relPath !== relPath);
      if (this.activeTabPath === relPath) {
        this.activeTabPath =
          this.openTabs.length > 0
            ? this.openTabs[this.openTabs.length - 1].relPath
            : null;
      }
    }
  }

  async handleExternalPathsChanged(
    changedPaths: string[],
    onReplaceActiveEditor?: (newContent: string) => void
  ) {
    for (const rel of changedPaths) {
      const tab = this.openTabs.find(
        (t) => t.relPath.toLowerCase() === rel.toLowerCase()
      );
      if (!tab) continue;

      if (!tab.dirty) {
        await this.reloadTabFromDisk(tab.relPath, onReplaceActiveEditor);
      } else {
        this.openTabs = this.openTabs.map((t) =>
          t.relPath === tab.relPath ? { ...t, externalChangePending: true } : t
        );
        clipboardStore.setStatus(`External edit detected on ${tab.relPath}`);
      }
    }
  }
}

export const tabsStore = new TabsStore();
