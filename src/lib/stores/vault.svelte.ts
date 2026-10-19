import { detectSyncProvider, findConflictCopies, type ConflictCopy } from '../syncConflicts';
import {
  closeVault,
  createFolder,
  createNewVault,
  createNote,
  deleteEntry,
  getRecentVaults,
  getVaultSnapshot,
  openVault,
  readNote,
  rebuildVaultIndex,
  restoreDeletedEntry,
  removeRecentVaultEntry,
  renameEntry,
} from '../ipc';
import type { RecentVault, VaultNode, VaultSnapshot } from '../types';
import { clipboardStore } from './clipboard.svelte';
import { settingsStore } from './settings.svelte';
import { tabsStore } from './tabs.svelte';
import { invalidateImagePreviews } from '../imagePreviewCache';

function findFirstFileNode(nodes: VaultNode[]): VaultNode | null {
  for (const node of nodes) {
    if (node.kind === 'file') return node;
    if (node.children.length > 0) {
      const found = findFirstFileNode(node.children);
      if (found) return found;
    }
  }
  return null;
}

function collectAllFiles(nodes: VaultNode[], out: VaultNode[] = []): VaultNode[] {
  for (const node of nodes) {
    if (node.kind === 'file') {
      out.push(node);
    } else if (node.children.length > 0) {
      collectAllFiles(node.children, out);
    }
  }
  return out;
}

const DEFAULT_DAILY_TEMPLATE = '## Today\n\n- [ ] \n\n## Log\n\n';

function formatIsoDate(d: Date): string {
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `${d.getFullYear()}-${mm}-${dd}`;
}

export function fillDailyTemplate(template: string, now: Date): string {
  const time = `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}`;
  const date = formatIsoDate(now);
  return template
    .replace(/\{\{\s*date\s*\}\}/gi, date)
    .replace(/\{\{\s*title\s*\}\}/gi, date)
    .replace(/\{\{\s*time\s*\}\}/gi, time);
}

export function stripMdExt(pathOrName: string): string {
  return pathOrName.replace(/\.md$/i, '');
}

const DISMISSED_CONFLICTS_KEY = 'jotter.dismissedConflicts';

function loadDismissedConflicts(): Record<string, string[]> {
  try {
    const saved = JSON.parse(localStorage.getItem(DISMISSED_CONFLICTS_KEY) ?? '{}');
    return saved && typeof saved === 'object' ? saved : {};
  } catch {
    return {};
  }
}

class VaultStore {
  vaultSnapshot = $state<VaultSnapshot | null>(null);
  recentVaults = $state<RecentVault[]>([]);
  onboardingError = $state('');
  indexVersion = $state(0);
  deletedVersion = $state(0);

  constructor() {
    tabsStore.onNoteSaved = () => {
      this.bumpIndexVersion();
    };
  }

  allFiles = $derived<VaultNode[]>(
    this.vaultSnapshot ? collectAllFiles(this.vaultSnapshot.tree) : []
  );

  /** The cloud client syncing the vault folder, guessed from its path. */
  syncProvider = $derived(
    this.vaultSnapshot ? detectSyncProvider(this.vaultSnapshot.rootPath) : null
  );

  /** Copies the user marked as ordinary notes, per vault root. Local storage keeps them per computer. */
  private dismissedConflicts = $state<Record<string, string[]>>(loadDismissedConflicts());

  syncConflicts = $derived<ConflictCopy[]>(
    findConflictCopies(
      this.allFiles.map((f) => f.relPath),
      this.syncProvider,
      new Set(this.vaultSnapshot ? (this.dismissedConflicts[this.vaultSnapshot.rootPath] ?? []) : [])
    )
  );

  /** Stops listing an ambiguous name, such as `Lecture 2.md`, as a conflict copy. */
  dismissConflict(copyPath: string) {
    const root = this.vaultSnapshot?.rootPath;
    if (!root) return;
    const list = this.dismissedConflicts[root] ?? [];
    if (list.includes(copyPath)) return;
    this.dismissedConflicts = { ...this.dismissedConflicts, [root]: [...list, copyPath] };
    try {
      localStorage.setItem(DISMISSED_CONFLICTS_KEY, JSON.stringify(this.dismissedConflicts));
    } catch {
      // The choice lasts for this session only.
    }
  }

  bumpIndexVersion() {
    this.indexVersion += 1;
  }

  /**
   * Resolves a wiki-link target (e.g. `Architecture` or `Specs/Architecture` or `Specs/Architecture.md`)
   * to a matching `VaultNode` in the open vault using:
   * 1. Exact relative path first
   * 2. Same folder as the source note
   * 3. Shortest path among notes with that stem
   */
  resolveWikiTarget(rawTarget: string, sourceRelPath?: string): VaultNode | null {
    const clean = stripMdExt(rawTarget.trim().replace(/\\/g, '/')).toLowerCase();
    if (!clean) return null;

    // 1. Exact relative path match (without .md)
    for (const file of this.allFiles) {
      if (stripMdExt(file.relPath).toLowerCase() === clean) {
        return file;
      }
    }

    // 2. Same folder as the source note
    const effectiveSource = (sourceRelPath ?? tabsStore.activeTab?.relPath ?? '').replace(/\\/g, '/');
    const lastSlash = effectiveSource.lastIndexOf('/');
    if (lastSlash > 0) {
      const sourceDir = effectiveSource.slice(0, lastSlash);
      const sameFolderNorm = stripMdExt(`${sourceDir}/${clean}`).toLowerCase();
      for (const file of this.allFiles) {
        if (stripMdExt(file.relPath).toLowerCase() === sameFolderNorm) {
          return file;
        }
      }
    }

    // 3. Shortest path among notes with that stem
    if (!clean.includes('/')) {
      let bestMatch: VaultNode | null = null;
      for (const file of this.allFiles) {
        if (stripMdExt(file.name).toLowerCase() === clean) {
          if (
            !bestMatch ||
            file.relPath.length < bestMatch.relPath.length ||
            (file.relPath.length === bestMatch.relPath.length &&
              file.relPath < bestMatch.relPath)
          ) {
            bestMatch = file;
          }
        }
      }
      return bestMatch;
    }

    return null;
  }

  async refreshRecentVaults() {
    try {
      this.recentVaults = await getRecentVaults();
    } catch {
      this.recentVaults = [];
    }
  }

  async openVaultByPath(path: string) {
    this.onboardingError = '';
    try {
      if (!(await tabsStore.flushAllAutosaves())) return;
      const snap = await openVault(path);
      invalidateImagePreviews(snap.rootPath);
      this.vaultSnapshot = snap;
      this.bumpIndexVersion();
      tabsStore.scratchpadMode = false;
      tabsStore.resetTabs();
      try {
        localStorage.setItem('jotter.lastVaultPath', snap.rootPath);
      } catch {
        // ignore
      }
      await this.refreshRecentVaults();
      // The Home page is the start screen when it is on; otherwise open the first note.
      const firstFile = settingsStore.showHomePage ? null : findFirstFileNode(snap.tree);
      if (firstFile) {
        await tabsStore.openNoteInTab(firstFile.relPath);
      }
      clipboardStore.setStatus(
        `Opened vault "${snap.name}" (${snap.noteCount} notes)`
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      this.onboardingError = message;
      if (this.vaultSnapshot) {
        clipboardStore.setStatus(`Could not open vault: ${message}`);
      }
    }
  }

  async createVault(parentPath: string, vaultName: string) {
    this.onboardingError = '';
    try {
      if (!(await tabsStore.flushAllAutosaves())) return;
      const snap = await createNewVault(parentPath, vaultName);
      invalidateImagePreviews(snap.rootPath);
      this.vaultSnapshot = snap;
      this.bumpIndexVersion();
      tabsStore.scratchpadMode = false;
      tabsStore.resetTabs();
      try {
        localStorage.setItem('jotter.lastVaultPath', snap.rootPath);
      } catch {
        // ignore
      }
      await this.refreshRecentVaults();
      // The Home page is the start screen when it is on; otherwise open the first note.
      const firstFile = settingsStore.showHomePage ? null : findFirstFileNode(snap.tree);
      if (firstFile) {
        await tabsStore.openNoteInTab(firstFile.relPath);
      }
      clipboardStore.setStatus(`Created vault "${snap.name}"`);
    } catch (err) {
      this.onboardingError = err instanceof Error ? err.message : String(err);
    }
  }

  async switchVault() {
    if (!(await tabsStore.flushAllAutosaves())) return;
    try {
      await closeVault();
    } catch (err) {
      clipboardStore.setStatus(`Could not close vault: ${String(err)}`);
      return;
    }
    invalidateImagePreviews('');
    this.vaultSnapshot = null;
    this.bumpIndexVersion();
    tabsStore.scratchpadMode = false;
    tabsStore.resetTabs();
    try {
      localStorage.removeItem('jotter.lastVaultPath');
    } catch {
      // ignore
    }
    await this.refreshRecentVaults();
  }

  async removeRecent(path: string) {
    try {
      this.recentVaults = await removeRecentVaultEntry(path);
    } catch (err) {
      this.onboardingError = err instanceof Error ? err.message : String(err);
    }
  }

  async createNoteInVault(parentFolderRel: string, noteName: string): Promise<string | null> {
    const targetRel = parentFolderRel
      ? `${parentFolderRel}/${noteName}`
      : noteName;
    try {
      const titleBase = noteName.replace(/\.md$/i, '');
      const [snap, doc] = await createNote(targetRel, `# ${titleBase}\n\n`);
      this.vaultSnapshot = snap;
      this.bumpIndexVersion();
      tabsStore.addOrActivateCreatedDoc(doc.relPath, doc.title, doc.content);
      clipboardStore.setStatus(`Created ${doc.relPath}`);
      return doc.relPath;
    } catch (err) {
      clipboardStore.setStatus(
        `Failed to create note: ${err instanceof Error ? err.message : String(err)}`
      );
      return null;
    }
  }

  async createFolderInVault(parentFolderRel: string, folderName: string) {
    const targetRel = parentFolderRel
      ? `${parentFolderRel}/${folderName}`
      : folderName;
    try {
      this.vaultSnapshot = await createFolder(targetRel);
      this.bumpIndexVersion();
      clipboardStore.setStatus(`Created folder ${targetRel}`);
    } catch (err) {
      clipboardStore.setStatus(
        `Failed to create folder: ${err instanceof Error ? err.message : String(err)}`
      );
    }
  }

  async renameEntryInVault(oldRelPath: string, newRelPath: string) {
    if (!(await tabsStore.flushAllAutosaves())) return;
    try {
      const res = await renameEntry(oldRelPath, newRelPath);
      this.vaultSnapshot = res.snapshot;
      this.bumpIndexVersion();
      tabsStore.updatePathsAfterRename(res.oldRelPath, res.newRelPath);
      clipboardStore.setStatus(
        `Renamed ${res.oldRelPath} → ${res.newRelPath}`
      );
    } catch (err) {
      clipboardStore.setStatus(
        `Rename failed: ${err instanceof Error ? err.message : String(err)}`
      );
    }
  }

  async deleteEntryInVault(relPath: string) {
    if (!(await tabsStore.flushAllAutosaves())) return;
    try {
      this.vaultSnapshot = await deleteEntry(relPath);
      this.deletedVersion++;
      invalidateImagePreviews();
      this.bumpIndexVersion();
      tabsStore.removeTabsUnderPath(relPath);
      clipboardStore.setStatus(`Moved ${relPath} to Recycle Bin`);
    } catch (err) {
      clipboardStore.setStatus(
        `Delete failed: ${err instanceof Error ? err.message : String(err)}`
      );
    }
  }

  /** Restores a note or folder from the OS trash and opens it if it is a note. */
  async restoreDeletedEntry(id: string): Promise<boolean> {
    try {
      const [relPath, snap] = await restoreDeletedEntry(id);
      this.deletedVersion++;
      invalidateImagePreviews();
      this.vaultSnapshot = snap;
      this.bumpIndexVersion();
      if (relPath.toLowerCase().endsWith('.md')) {
        await tabsStore.openNoteInTab(relPath);
      }
      clipboardStore.setStatus(`Restored ${relPath}`);
      return true;
    } catch (err) {
      clipboardStore.setStatus(err instanceof Error ? err.message : String(err));
      return false;
    }
  }

  /**
   * Opens today's note in the daily folder, creating it from the template when missing.
   * The template is any vault note; `{{date}}`, `{{time}}` and `{{title}}` are filled in.
   */
  async openDailyNote(now = new Date()) {
    if (!this.vaultSnapshot) {
      clipboardStore.setStatus('Open a vault to use daily notes');
      return;
    }
    const title = formatIsoDate(now);
    const folder = settingsStore.dailyFolder;
    const relPath = folder ? `${folder}/${title}.md` : `${title}.md`;

    const existing = this.allFiles.find(
      (f) => f.relPath.toLowerCase() === relPath.toLowerCase()
    );
    if (existing) {
      await tabsStore.openNoteInTab(existing.relPath);
      return;
    }

    let template = DEFAULT_DAILY_TEMPLATE;
    const templatePath = settingsStore.dailyTemplatePath;
    if (templatePath) {
      try {
        template = (await readNote(templatePath)).content;
      } catch {
        // No template note: use the built-in one.
      }
    }
    try {
      const [snap, doc] = await createNote(relPath, fillDailyTemplate(template, now));
      this.vaultSnapshot = snap;
      this.bumpIndexVersion();
      tabsStore.addOrActivateCreatedDoc(doc.relPath, doc.title, doc.content);
      clipboardStore.setStatus(`Created ${doc.relPath}`);
    } catch (err) {
      clipboardStore.setStatus(
        `Failed to create daily note: ${err instanceof Error ? err.message : String(err)}`
      );
    }
  }

  async handleExternalFsEvent(
    changedPaths: string[],
    onReplaceActiveEditor?: (newContent: string) => void
  ) {
    if (!this.vaultSnapshot) return;
    invalidateImagePreviews();
    try {
      const snap = await getVaultSnapshot();
      if (snap) this.vaultSnapshot = snap;
    } catch {
      // ignore
    }
    this.bumpIndexVersion();
    await tabsStore.handleExternalPathsChanged(
      changedPaths,
      onReplaceActiveEditor
    );
  }
  async rebuildIndex(): Promise<number | null> {
    if (!this.vaultSnapshot) return null;
    try {
      const count = await rebuildVaultIndex();
      const snap = await getVaultSnapshot();
      if (snap) this.vaultSnapshot = snap;
      this.bumpIndexVersion();
      clipboardStore.setStatus(
        `Rebuilt SQLite & Tantivy indexes (${count} notes)`
      );
      return count;
    } catch (err) {
      clipboardStore.setStatus(
        `Index rebuild failed: ${err instanceof Error ? err.message : String(err)}`
      );
      return null;
    }
  }
}

export const vaultStore = new VaultStore();

