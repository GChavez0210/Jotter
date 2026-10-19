import {
  closeVault,
  createFolder,
  createNewVault,
  createNote,
  deleteEntry,
  getRecentVaults,
  getVaultSnapshot,
  openVault,
  removeRecentVaultEntry,
  renameEntry,
} from '../ipc';
import type { RecentVault, VaultNode, VaultSnapshot } from '../types';
import { clipboardStore } from './clipboard.svelte';
import { tabsStore } from './tabs.svelte';

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

export function stripMdExt(pathOrName: string): string {
  return pathOrName.replace(/\.md$/i, '');
}

class VaultStore {
  vaultSnapshot = $state<VaultSnapshot | null>(null);
  recentVaults = $state<RecentVault[]>([]);
  onboardingError = $state('');
  indexVersion = $state(0);

  constructor() {
    tabsStore.onNoteSaved = () => {
      this.bumpIndexVersion();
    };
  }

  allFiles = $derived<VaultNode[]>(
    this.vaultSnapshot ? collectAllFiles(this.vaultSnapshot.tree) : []
  );

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
      await tabsStore.flushAllAutosaves();
      const snap = await openVault(path);
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
      const firstFile = findFirstFileNode(snap.tree);
      if (firstFile) {
        await tabsStore.openNoteInTab(firstFile.relPath);
      }
      clipboardStore.setStatus(
        `Opened vault "${snap.name}" (${snap.noteCount} notes)`
      );
    } catch (err) {
      this.onboardingError = err instanceof Error ? err.message : String(err);
    }
  }

  async createVault(parentPath: string, vaultName: string) {
    this.onboardingError = '';
    try {
      await tabsStore.flushAllAutosaves();
      const snap = await createNewVault(parentPath, vaultName);
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
      const firstFile = findFirstFileNode(snap.tree);
      if (firstFile) {
        await tabsStore.openNoteInTab(firstFile.relPath);
      }
      clipboardStore.setStatus(`Created vault "${snap.name}"`);
    } catch (err) {
      this.onboardingError = err instanceof Error ? err.message : String(err);
    }
  }

  async switchVault() {
    await tabsStore.flushAllAutosaves();
    try {
      await closeVault();
    } catch {
      // ignore
    }
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
    await tabsStore.flushAllAutosaves();
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
    try {
      this.vaultSnapshot = await deleteEntry(relPath);
      this.bumpIndexVersion();
      tabsStore.removeTabsUnderPath(relPath);
      clipboardStore.setStatus(`Moved ${relPath} to Recycle Bin`);
    } catch (err) {
      clipboardStore.setStatus(
        `Delete failed: ${err instanceof Error ? err.message : String(err)}`
      );
    }
  }

  async handleExternalFsEvent(
    changedPaths: string[],
    onReplaceActiveEditor?: (newContent: string) => void
  ) {
    if (!this.vaultSnapshot) return;
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
}

export const vaultStore = new VaultStore();
