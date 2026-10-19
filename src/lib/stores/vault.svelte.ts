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

class VaultStore {
  vaultSnapshot = $state<VaultSnapshot | null>(null);
  recentVaults = $state<RecentVault[]>([]);
  onboardingError = $state('');

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

  async createNoteInVault(parentFolderRel: string, noteName: string) {
    const targetRel = parentFolderRel
      ? `${parentFolderRel}/${noteName}`
      : noteName;
    try {
      const titleBase = noteName.replace(/\.md$/i, '');
      const [snap, doc] = await createNote(targetRel, `# ${titleBase}\n\n`);
      this.vaultSnapshot = snap;
      tabsStore.addOrActivateCreatedDoc(doc.relPath, doc.title, doc.content);
      clipboardStore.setStatus(`Created ${doc.relPath}`);
    } catch (err) {
      clipboardStore.setStatus(
        `Failed to create note: ${err instanceof Error ? err.message : String(err)}`
      );
    }
  }

  async createFolderInVault(parentFolderRel: string, folderName: string) {
    const targetRel = parentFolderRel
      ? `${parentFolderRel}/${folderName}`
      : folderName;
    try {
      this.vaultSnapshot = await createFolder(targetRel);
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
    await tabsStore.handleExternalPathsChanged(
      changedPaths,
      onReplaceActiveEditor
    );
  }
}

export const vaultStore = new VaultStore();
