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
  listPinnedNotes,
  setNotePinned,
  removeRecentVaultEntry,
  renameEntry,
} from '../ipc';
import type { RecentVault, VaultNode, VaultSnapshot } from '../types';
import { clipboardStore } from './clipboard.svelte';
import { settingsStore } from './settings.svelte';
import { tabsStore } from './tabs.svelte';
import { invalidateImagePreviews } from '../imagePreviewCache';
import { noteMoveDestination } from '../noteMove';
import { m, trashName } from '../i18n.svelte';
import { isMac, isWindows } from '../platform';

function pinPathKey(path: string): string {
  return isMac || isWindows ? path.replace(/[A-Z]/g, (letter) => letter.toLowerCase()) : path;
}

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
  /** Relative paths pinned to the top of the sidebar, in the order they were pinned. */
  pinnedPaths = $state<string[]>([]);
  deletedVersion = $state(0);

  constructor() {
    tabsStore.onNoteSaved = () => {
      this.bumpIndexVersion();
    };
  }

  allFiles = $derived<VaultNode[]>(
    this.vaultSnapshot ? collectAllFiles(this.vaultSnapshot.tree) : []
  );

  /** Keep saved pins for restoration, but never offer missing notes as navigation targets. */
  visiblePinnedPaths = $derived.by(() => {
    const existing = new Map(this.allFiles.map((file) => [pinPathKey(file.relPath), file.relPath]));
    return this.pinnedPaths.flatMap((pin) => {
      const path = existing.get(pinPathKey(pin));
      return path === undefined ? [] : [path];
    });
  });

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
      await this.refreshPins();
      // The Home page is the start screen when it is on; otherwise open the first note.
      const firstFile = settingsStore.showHomePage ? null : findFirstFileNode(snap.tree);
      if (firstFile) {
        await tabsStore.openNoteInTab(firstFile.relPath);
      }
      clipboardStore.setStatus(m.status_opened_vault({ vault: snap.name, count: snap.noteCount }));
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      this.onboardingError = message;
      if (this.vaultSnapshot) {
        clipboardStore.setStatus(m.status_open_vault_failed({ error: message }));
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
      await this.refreshPins();
      // The Home page is the start screen when it is on; otherwise open the first note.
      const firstFile = settingsStore.showHomePage ? null : findFirstFileNode(snap.tree);
      if (firstFile) {
        await tabsStore.openNoteInTab(firstFile.relPath);
      }
      clipboardStore.setStatus(m.status_created_vault({ vault: snap.name }));
    } catch (err) {
      this.onboardingError = err instanceof Error ? err.message : String(err);
    }
  }

  async switchVault() {
    if (!(await tabsStore.flushAllAutosaves())) return;
    try {
      await closeVault();
    } catch (err) {
      clipboardStore.setStatus(m.status_close_vault_failed({ error: String(err) }));
      return;
    }
    invalidateImagePreviews('');
    this.vaultSnapshot = null;
    this.pinnedPaths = [];
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
      clipboardStore.setStatus(m.status_created({ path: doc.relPath }));
      return doc.relPath;
    } catch (err) {
      clipboardStore.setStatus(
        m.status_create_note_failed({ error: err instanceof Error ? err.message : String(err) })
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
      clipboardStore.setStatus(m.status_created_folder({ path: targetRel }));
    } catch (err) {
      clipboardStore.setStatus(
        m.status_create_folder_failed({ error: err instanceof Error ? err.message : String(err) })
      );
    }
  }

  async renameEntryInVault(oldRelPath: string, newRelPath: string, action: 'rename' | 'move' = 'rename'): Promise<boolean> {
    const root = this.vaultSnapshot?.rootPath;
    if (!(await tabsStore.flushAllAutosaves())) return false;
    if (!root || this.vaultSnapshot?.rootPath !== root) return false;
    try {
      const res = await renameEntry(oldRelPath, newRelPath);
      if (this.vaultSnapshot?.rootPath !== root) return false;
      this.vaultSnapshot = res.snapshot;
      this.bumpIndexVersion();
      tabsStore.updatePathsAfterRename(res.oldRelPath, res.newRelPath);
      // Rust rewrote any pin under the old path.
      await this.refreshPins();
      clipboardStore.setStatus(
        (action === 'move' ? m.status_moved : m.status_renamed)({ from: res.oldRelPath, to: res.newRelPath })
      );
      return true;
    } catch (err) {
      clipboardStore.setStatus(
        (action === 'move' ? m.status_move_failed : m.status_rename_failed)({ error: err instanceof Error ? err.message : String(err) })
      );
      return false;
    }
  }

  async moveNoteIntoFolder(source: string, folder: string, vaultRoot: string): Promise<boolean> {
    const snapshot = this.vaultSnapshot;
    if (!snapshot || snapshot.rootPath !== vaultRoot) return false;
    try {
      const destination = noteMoveDestination(snapshot.tree, source, folder);
      if (!destination) return false;
      return await this.renameEntryInVault(source, destination, 'move');
    } catch (err) {
      clipboardStore.setStatus(m.status_move_failed({ error: err instanceof Error ? err.message : String(err) }));
      return false;
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
      await this.refreshPins();
      clipboardStore.setStatus(m.status_moved_to_trash({ path: relPath, trash: trashName() }));
    } catch (err) {
      clipboardStore.setStatus(
        m.status_delete_failed({ error: err instanceof Error ? err.message : String(err) })
      );
    }
  }

  /** Re-reads the pins for the open vault. Pins live in app data, not in the vault. */
  async refreshPins() {
    if (!this.vaultSnapshot) {
      this.pinnedPaths = [];
      return;
    }
    const root = this.vaultSnapshot.rootPath;
    try {
      const pins = await listPinnedNotes();
      // A vault switch while the call was in flight must not show the old vault's pins.
      if (this.vaultSnapshot?.rootPath === root) this.pinnedPaths = pins;
    } catch {
      this.pinnedPaths = [];
    }
  }

  async togglePin(relPath: string) {
    const root = this.vaultSnapshot?.rootPath;
    if (!root) return;
    const pinned = !this.isPinned(relPath);
    try {
      const pins = await setNotePinned(relPath, pinned);
      if (this.vaultSnapshot?.rootPath === root) this.pinnedPaths = pins;
    } catch (err) {
      clipboardStore.setStatus(
        m.status_pin_failed({ error: err instanceof Error ? err.message : String(err) })
      );
    }
  }

  isPinned(relPath: string): boolean {
    return this.pinnedPaths.some((pin) => pinPathKey(pin) === pinPathKey(relPath));
  }

  /** Restores a note or folder from the OS trash and opens it if it is a note. */
  async restoreDeletedEntry(id: string): Promise<boolean> {
    try {
      const [relPath, snap] = await restoreDeletedEntry(id);
      this.deletedVersion++;
      invalidateImagePreviews();
      this.vaultSnapshot = snap;
      this.bumpIndexVersion();
      await this.refreshPins();
      if (relPath.toLowerCase().endsWith('.md')) {
        await tabsStore.openNoteInTab(relPath);
      }
      clipboardStore.setStatus(m.status_restored({ path: relPath }));
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
      clipboardStore.setStatus(m.status_daily_no_vault());
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
      clipboardStore.setStatus(m.status_created({ path: doc.relPath }));
    } catch (err) {
      clipboardStore.setStatus(
        m.status_daily_failed({ error: err instanceof Error ? err.message : String(err) })
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
      clipboardStore.setStatus(m.status_rebuilt({ count }));
      return count;
    } catch (err) {
      clipboardStore.setStatus(
        m.status_rebuild_failed({ error: err instanceof Error ? err.message : String(err) })
      );
      return null;
    }
  }
}

export const vaultStore = new VaultStore();

