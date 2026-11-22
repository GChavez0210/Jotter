import { sameNotePath, notePathKey } from '../notePaths';
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
  planLinkUpdates,
  applyLinkEdits,
  quickCaptureAppend,
  pastePlainFromClipboard,
} from '../ipc';
import type { LinkUpdatePlan, RecentVault, TextEdit, VaultNode, VaultSnapshot } from '../types';
import { appStorage } from '../appStorage';
import { clipboardStore } from './clipboard.svelte';
import { settingsStore } from './settings.svelte';
import { tabsStore } from './tabs.svelte';
import { invalidateImagePreviews } from '../imagePreviewCache';
import { noteMoveDestination } from '../noteMove';
import { fillTemplate, formatIsoDate, templateUsesClipboard } from '../templates';
import { planCapture } from '../captureProfiles';
import {
  PENDING_EXTRACTION_KEY,
  buildExtractedNote,
  linkTargetFor,
  hasLinkUnsafeChars,
  parsePendingExtraction,
  splitPadding,
  type PendingExtraction,
} from '../extractNote';
import { applyToOpenNote, reverseEdits } from '../linkUpdates';
import { captureEdit, normalizeCaptureNote } from '../quickCapture';
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

export function fillDailyTemplate(template: string, now: Date): string {
  return fillTemplate(template, { now, title: formatIsoDate(now) });
}

export function stripMdExt(pathOrName: string): string {
  return pathOrName.replace(/\.md$/i, '');
}

const DISMISSED_CONFLICTS_KEY = 'jotter.dismissedConflicts';

function loadDismissedConflicts(): Record<string, string[]> {
  try {
    const saved = JSON.parse(appStorage.getItem(DISMISSED_CONFLICTS_KEY) ?? '{}');
    return saved && typeof saved === 'object' ? saved : {};
  } catch {
    return {};
  }
}

export type LinkUpdateChoice = 'update' | 'rename' | 'cancel';

interface ChangedNote {
  path: string;
  /** Edits that put the note's links back as they were. */
  reverse: TextEdit[];
}

class VaultStore {
  /** The question shown before a rename that would break links. */
  linkUpdatePrompt = $state<{
    oldRel: string;
    noteCount: number;
    resolve: (choice: LinkUpdateChoice) => void;
  } | null>(null);
  /** The last link update, kept so it can be undone. */
  lastLinkUpdate = $state<ChangedNote[] | null>(null);

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

  /**
   * Lookup tables for wiki-link targets, rebuilt only when the vault's file list changes. Live
   * Preview resolves every link on screen, so a scan of all notes per link would not scale.
   */
  private wikiTargetIndex = $derived.by(() => {
    const byPath = new Map<string, VaultNode>();
    const byStem = new Map<string, VaultNode>();
    for (const file of this.allFiles) {
      const pathKey = notePathKey(stripMdExt(file.relPath));
      if (!byPath.has(pathKey)) byPath.set(pathKey, file);
      const stemKey = notePathKey(stripMdExt(file.name));
      const best = byStem.get(stemKey);
      if (
        !best ||
        file.relPath.length < best.relPath.length ||
        (file.relPath.length === best.relPath.length && file.relPath < best.relPath)
      ) {
        byStem.set(stemKey, file);
      }
    }
    return { byPath, byStem };
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
      appStorage.setItem(DISMISSED_CONFLICTS_KEY, JSON.stringify(this.dismissedConflicts));
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
    const clean = notePathKey(stripMdExt(rawTarget.trim().replace(/\\/g, '/')));
    if (!clean) return null;
    const { byPath, byStem } = this.wikiTargetIndex;

    // 1. Exact relative path match (without .md)
    const exact = byPath.get(clean);
    if (exact) return exact;

    // 2. Same folder as the source note
    const effectiveSource = (sourceRelPath ?? tabsStore.activeTab?.relPath ?? '').replace(/\\/g, '/');
    const lastSlash = effectiveSource.lastIndexOf('/');
    if (lastSlash > 0) {
      const sourceDir = effectiveSource.slice(0, lastSlash);
      const sameFolder = byPath.get(notePathKey(stripMdExt(`${sourceDir}/${clean}`)));
      if (sameFolder) return sameFolder;
    }

    // 3. Shortest path among notes with that stem
    return clean.includes('/') ? null : (byStem.get(clean) ?? null);
  }

  async refreshRecentVaults() {
    try {
      this.recentVaults = await getRecentVaults();
    } catch {
      this.recentVaults = [];
    }
  }

  async openVaultByPath(path: string) {
    if (tabsStore.vaultTransition) return;
    tabsStore.vaultTransition = true;
    this.onboardingError = '';
    try {
      if (!(await tabsStore.flushAllAutosaves())) return;
      const snap = await openVault(path);
      invalidateImagePreviews(snap.rootPath);
      this.vaultSnapshot = snap;
      this.bumpIndexVersion();
      tabsStore.scratchpadMode = false;
      tabsStore.resetTabs(snap.generation);
      try {
        appStorage.setItem('jotter.lastVaultPath', snap.rootPath);
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
    } finally {
      tabsStore.vaultTransition = false;
    }
  }

  async createVault(parentPath: string, vaultName: string) {
    if (tabsStore.vaultTransition) return;
    tabsStore.vaultTransition = true;
    this.onboardingError = '';
    try {
      if (!(await tabsStore.flushAllAutosaves())) return;
      const snap = await createNewVault(parentPath, vaultName);
      invalidateImagePreviews(snap.rootPath);
      this.vaultSnapshot = snap;
      this.bumpIndexVersion();
      tabsStore.scratchpadMode = false;
      tabsStore.resetTabs(snap.generation);
      try {
        appStorage.setItem('jotter.lastVaultPath', snap.rootPath);
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
    } finally {
      tabsStore.vaultTransition = false;
    }
  }

  async switchVault() {
    if (tabsStore.vaultTransition) return;
    tabsStore.vaultTransition = true;
    try {
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
        appStorage.removeItem('jotter.lastVaultPath');
      } catch {
        // ignore
      }
      await this.refreshRecentVaults();
    } finally {
      tabsStore.vaultTransition = false;
    }
  }

  async removeRecent(path: string) {
    try {
      this.recentVaults = await removeRecentVaultEntry(path);
    } catch (err) {
      this.onboardingError = err instanceof Error ? err.message : String(err);
    }
  }

  /** Creates a note from a template note, filling in its variables. Returns the new note's path. */
  async createNoteFromTemplate(
    templateRelPath: string,
    parentFolderRel: string,
    noteName: string,
    prompts: Record<string, string>
  ): Promise<string | null> {
    try {
      const template = (await readNote(templateRelPath)).content;
      const title = noteName.replace(/\.md$/i, '');
      const clipboard = templateUsesClipboard(template)
        ? await pastePlainFromClipboard().catch(() => '')
        : undefined;
      const content = fillTemplate(template, { now: new Date(), title, prompts, clipboard });
      return await this.createNoteInVault(parentFolderRel, noteName, content);
    } catch (err) {
      clipboardStore.setStatus(
        m.status_create_note_failed({ error: err instanceof Error ? err.message : String(err) })
      );
      return null;
    }
  }

  async createNoteInVault(
    parentFolderRel: string,
    noteName: string,
    content?: string
  ): Promise<string | null> {
    const targetRel = parentFolderRel
      ? `${parentFolderRel}/${noteName}`
      : noteName;
    try {
      const titleBase = noteName.replace(/\.md$/i, '');
      const [snap, doc] = await createNote(targetRel, content ?? `# ${titleBase}\n\n`);
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
    let plan: LinkUpdatePlan | null = null;
    try {
      const found = await planLinkUpdates(oldRelPath, newRelPath);
      if (found.linkCount > 0) {
        const choice = await this.askLinkUpdate(oldRelPath, found.notes.length);
        if (choice === 'cancel') return false;
        if (choice === 'update') plan = found;
      }
    } catch {
      // Without a plan the rename still goes ahead, as it did before links were followed.
    }
    if (this.vaultSnapshot?.rootPath !== root) return false;
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
      if (plan) await this.updateLinks(plan);
      return true;
    } catch (err) {
      clipboardStore.setStatus(
        (action === 'move' ? m.status_move_failed : m.status_rename_failed)({ error: err instanceof Error ? err.message : String(err) })
      );
      return false;
    }
  }

  private askLinkUpdate(oldRel: string, noteCount: number): Promise<LinkUpdateChoice> {
    return new Promise((resolve) => {
      this.linkUpdatePrompt = { oldRel, noteCount, resolve };
    });
  }

  answerLinkUpdate(choice: LinkUpdateChoice) {
    const prompt = this.linkUpdatePrompt;
    this.linkUpdatePrompt = null;
    prompt?.resolve(choice);
  }

  /**
   * Applies edits to each note. The note on screen changes in one editor step, so Ctrl+Z undoes
   * it. Other notes are saved directly. `lastLinkUpdate` keeps what undoing all of them needs.
   */
  private async applyEdits(
    notes: { path: string; edits: TextEdit[] }[]
  ): Promise<{ changed: ChangedNote[]; skipped: number }> {
    const changed: ChangedNote[] = [];
    const savedOnDisk: string[] = [];
    let skipped = 0;
    for (const note of notes) {
      if (applyToOpenNote(note.path, note.edits)) {
        changed.push({ path: note.path, reverse: reverseEdits(note.edits) });
        continue;
      }
      // Saving under the open editor would leave it showing text the file no longer has.
      if (note.path === tabsStore.activeTabPath) {
        skipped += note.edits.length;
        continue;
      }
      try {
        const result = await applyLinkEdits(note.path, note.edits);
        skipped += result.skipped;
        if (result.applied > 0) {
          changed.push({ path: note.path, reverse: result.reverse });
          savedOnDisk.push(note.path);
        }
      } catch {
        skipped += note.edits.length;
      }
    }
    if (savedOnDisk.length > 0) await tabsStore.handleExternalPathsChanged(savedOnDisk);
    this.bumpIndexVersion();
    return { changed, skipped };
  }

  private async updateLinks(plan: LinkUpdatePlan) {
    const { changed, skipped } = await this.applyEdits(plan.notes);
    this.lastLinkUpdate = changed.length > 0 ? changed : null;
    const done = m.link_update_done({ count: changed.length });
    clipboardStore.setStatus(skipped > 0 ? `${done} ${m.link_update_skipped({ count: skipped })}` : done);
  }

  /**
   * Adds a quick capture entry to the inbox note and returns its path. The note on screen takes
   * the entry in the editor, keeping any unsaved typing; otherwise open tabs are saved first so
   * the write on disk cannot clash with them.
   */
  async captureEntry(note: string, text: string, compact = false): Promise<string> {
    const path = normalizeCaptureNote(note);
    const tab = tabsStore.openTabs.find((t) => sameNotePath(t.relPath, path));
    if (tab && tab.relPath === tabsStore.activeTabPath) {
      const edit = captureEdit(tab.content, text, compact);
      if (edit && applyToOpenNote(tab.relPath, [edit])) return tab.relPath;
    }
    if (!(await tabsStore.flushAllAutosaves())) throw new Error(m.capture_unsaved());
    const saved = await quickCaptureAppend(path, text, compact);
    this.bumpIndexVersion();
    return saved;
  }

  /**
   * Moves selected text into a new note and swaps it for a link. The order protects the text:
   * the new note is written and read back first, and `apply` replaces the selection only
   * after that. `apply` returns false when the source note changed since the selection was
   * captured, and then the original text stays. A record of the work in progress survives a
   * crash, so the next launch can say what was left behind.
   */
  async extractToNote(args: {
    sourceRel: string;
    text: string;
    name: string;
    folder: string;
    backLink: boolean;
    apply: (linkText: string) => boolean;
  }): Promise<{ path: string; replaced: boolean }> {
    const dest = `${args.folder ? `${args.folder}/` : ''}${args.name}.md`;
    if (hasLinkUnsafeChars(dest)) throw new Error(m.extract_path_link_chars({ path: dest }));
    const backTarget = args.backLink
      ? linkTargetFor(args.sourceRel, (t) => this.resolveWikiTarget(t, dest))
      : null;
    if (backTarget && hasLinkUnsafeChars(backTarget)) {
      throw new Error(m.extract_path_link_chars({ path: args.sourceRel }));
    }
    if (this.allFiles.some((f) => sameNotePath(f.relPath, dest))) {
      throw new Error(m.extract_exists({ path: dest }));
    }
    const backLinkLine = args.backLink
      ? m.extract_back_line({
          link: `[[${backTarget}]]`,
        })
      : null;
    const content = buildExtractedNote({
      title: args.name,
      body: splitPadding(args.text).core,
      backLinkLine,
    });

    const pending: PendingExtraction = { source: args.sourceRel, dest, startedMs: Date.now() };
    appStorage.setItem(PENDING_EXTRACTION_KEY, JSON.stringify(pending));
    try {
      // The record has to reach the disk before the note does.
      await appStorage.flush();
      const [snapshot, written] = await createNote(dest, content);
      this.vaultSnapshot = snapshot;
      this.bumpIndexVersion();
      if (written.content.replace(/\r\n/g, '\n') !== content) {
        throw new Error(m.extract_verify_failed({ path: dest }));
      }
      const link = linkTargetFor(dest, (t) => this.resolveWikiTarget(t, args.sourceRel));
      const replaced = args.apply(`[[${link}]]`);
      return { path: dest, replaced };
    } finally {
      appStorage.removeItem(PENDING_EXTRACTION_KEY);
    }
  }

  /** Reports an extraction that a crash or a forced quit cut short, then forgets it. */
  reportInterruptedExtraction() {
    const pending = parsePendingExtraction(appStorage.getItem(PENDING_EXTRACTION_KEY));
    if (!pending) return;
    appStorage.removeItem(PENDING_EXTRACTION_KEY);
    clipboardStore.setStatus(m.extract_interrupted({ dest: pending.dest, source: pending.source }));
  }

  /**
   * Captures an entry with one of the user's capture profiles and returns the note's path. The
   * clipboard is read once, and only when the profile uses it. A profile that no longer exists
   * falls back to the inbox note, so an entry is never lost to a stale setting.
   */
  async captureWithProfile(profileId: string, text: string): Promise<string> {
    const profile = settingsStore.captureProfiles.find((p) => p.id === profileId);
    if (!profile) return this.captureEntry(settingsStore.quickCaptureNote, text);

    const usesClipboard = [profile.template, profile.target, profile.fileName].some(templateUsesClipboard);
    const clipboard = usesClipboard ? await pastePlainFromClipboard().catch(() => '') : undefined;
    const now = new Date();
    const plan = () =>
      planCapture(profile, {
        text,
        clipboard,
        now,
        noteExists: (path) => this.allFiles.some((f) => sameNotePath(f.relPath, path)),
      });

    if (profile.mode === 'append') {
      const { path, content, compact } = plan();
      return this.captureEntry(path, content, compact);
    }
    // A note made elsewhere since the last scan can take the name, so refresh and plan again.
    for (let attempt = 0; ; attempt++) {
      const { path, content } = plan();
      try {
        const [snapshot] = await createNote(path, content);
        this.vaultSnapshot = snapshot;
        this.bumpIndexVersion();
        return path;
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        const exists = /already exists/i.test(message);
        if (!exists || attempt >= 2) throw err;
        const fresh = await getVaultSnapshot();
        if (fresh) this.vaultSnapshot = fresh;
      }
    }
  }

  /** Puts the links changed by the last rename back as they were. */
  async undoLinkUpdate() {
    const last = this.lastLinkUpdate;
    if (!last) return;
    this.lastLinkUpdate = null;
    const { changed } = await this.applyEdits(
      last.map((note) => ({ path: note.path, edits: note.reverse }))
    );
    clipboardStore.setStatus(m.link_update_undone({ count: changed.length }));
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
      (f) => sameNotePath(f.relPath, relPath)
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
    onReplaceEditor?: (relPath: string, newContent: string) => void
  ) {
    if (!this.vaultSnapshot) return;
    const generation = tabsStore.activeVaultGeneration;
    invalidateImagePreviews();
    try {
      const snap = await getVaultSnapshot();
      if (generation !== tabsStore.activeVaultGeneration) return;
      if (snap) this.vaultSnapshot = snap;
    } catch {
      // ignore
    }
    if (generation !== tabsStore.activeVaultGeneration) return;
    this.bumpIndexVersion();
    await tabsStore.handleExternalPathsChanged(changedPaths, onReplaceEditor);
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

