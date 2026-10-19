/**
 * Detects the cloud folder a vault lives in and the conflict copies sync clients create when
 * two devices edit the same note. Jotter never merges or deletes copies on its own; the
 * conflicts panel lists them so the user can compare and remove them.
 */

export type SyncProvider = 'dropbox' | 'onedrive' | 'googledrive' | 'box' | 'icloud' | 'syncthing';

export const PROVIDER_NAMES: Record<SyncProvider, string> = {
  dropbox: 'Dropbox',
  onedrive: 'OneDrive',
  googledrive: 'Google Drive',
  box: 'Box',
  icloud: 'iCloud Drive',
  syncthing: 'Syncthing',
};

/** Guesses the sync client from the vault's absolute path. Returns null for local folders. */
export function detectSyncProvider(rootPath: string): SyncProvider | null {
  const segments = rootPath.replace(/\\/g, '/').split('/').map((s) => s.toLowerCase());
  for (const seg of segments) {
    if (seg === 'dropbox' || seg.startsWith('dropbox (')) return 'dropbox';
    if (seg === 'onedrive' || seg.startsWith('onedrive - ')) return 'onedrive';
    if (seg === 'google drive' || seg === 'my drive' || seg === 'googledrive' || seg === 'shared drives') {
      return 'googledrive';
    }
    if (seg === 'box' || seg === 'box sync') return 'box';
    if (seg === 'iclouddrive' || seg === 'icloud drive' || seg === 'mobile documents') return 'icloud';
  }
  return null;
}

export interface ConflictCopy {
  /** Vault-relative path of the conflict copy. */
  copyPath: string;
  /** Vault-relative path of the note it diverged from, when that note still exists. */
  originalPath: string | null;
  /** The client whose naming pattern matched. */
  provider: SyncProvider | 'unknown';
  /** True for names a person could choose too, such as `Note 2`; the user can dismiss these. */
  ambiguous: boolean;
}

interface Pattern {
  provider: SyncProvider | 'unknown';
  /** Returns the possible names of the original note, without the extension. */
  bases: (stem: string) => string[];
  /** Ambiguous patterns, such as `Note (1)`, count only next to an existing original. */
  needsOriginal: boolean;
  /** Restricts an ambiguous pattern to vaults inside these providers. */
  onlyIn?: SyncProvider[];
}

function suffix(re: RegExp): (stem: string) => string[] {
  return (stem) => {
    const match = re.exec(stem);
    return match ? [match[1]] : [];
  };
}

/**
 * A Windows computer name as OneDrive appends it, such as `DESKTOP-4F2K9` or `ALEX-PC`. It needs
 * two letters and a digit or an inner hyphen, so suffixes people write themselves, such as
 * `TODO`, `FINAL`, or `2026Q1`, are not mistaken for one.
 */
function isDeviceName(text: string): boolean {
  if (!/^[A-Z0-9][A-Z0-9-]{3,14}(?:-\d+)?$/.test(text)) return false;
  return (text.match(/[A-Z]/g) ?? []).length >= 2 && /\d|.-./.test(text);
}

const PATTERNS: Pattern[] = [
  // Dropbox: "Note (Alex's conflicted copy 2026-10-19).md", also with a trailing counter.
  // Nextcloud and ownCloud: "Note (conflicted copy 2026-10-19 101500).md".
  { provider: 'dropbox', bases: suffix(/^(.*) \((?:[^()]*'s )?conflicted copy(?: [\d -]+)?\)$/i), needsOriginal: false },
  // Dropbox: "Note (Case Conflict).md" and "Note (Selective Sync Conflict).md".
  { provider: 'dropbox', bases: suffix(/^(.*) \((?:case|selective sync) conflict(?: \d+)?\)$/i), needsOriginal: false },
  // Syncthing: "Note.sync-conflict-20261019-101500-ABCDEFG.md".
  { provider: 'syncthing', bases: suffix(/^(.*)\.sync-conflict-\d{8}-\d{6}(?:-[A-Z0-9]{7})?$/), needsOriginal: false },
  // OneDrive: "Note-DESKTOP-4F2K9.md", the device name appended after a hyphen. Device names
  // contain hyphens too, so every hyphen is a possible split point.
  {
    provider: 'onedrive',
    bases: (stem) => {
      const out: string[] = [];
      for (let i = stem.indexOf('-'); i > 0; i = stem.indexOf('-', i + 1)) {
        if (isDeviceName(stem.slice(i + 1))) out.push(stem.slice(0, i));
      }
      return out;
    },
    needsOriginal: true,
    onlyIn: ['onedrive'],
  },
  // Google Drive and Box: "Note (1).md" or "Note (Conflict).md".
  { provider: 'googledrive', bases: suffix(/^(.*) \((?:\d{1,3}|conflict(?: \d+)?)\)$/i), needsOriginal: true, onlyIn: ['googledrive', 'box'] },
  // iCloud Drive: "Note 2.md".
  { provider: 'icloud', bases: suffix(/^(.*) \d{1,3}$/), needsOriginal: true, onlyIn: ['icloud'] },
];

function splitExt(path: string): { stem: string; ext: string } {
  const slash = path.lastIndexOf('/');
  const dot = path.lastIndexOf('.');
  return dot > slash + 1 ? { stem: path.slice(0, dot), ext: path.slice(dot) } : { stem: path, ext: '' };
}

/**
 * Finds conflict copies among vault-relative paths. `provider` enables the ambiguous patterns
 * for the vault's sync client; the explicit Dropbox, Nextcloud, and Syncthing names always count.
 * `dismissed` holds copy paths the user marked as not a conflict; it only hides ambiguous names.
 */
export function findConflictCopies(
  paths: string[],
  provider: SyncProvider | null,
  dismissed: ReadonlySet<string> = new Set()
): ConflictCopy[] {
  const normalized = paths.map((p) => p.replace(/\\/g, '/'));
  const byLower = new Map(normalized.map((p) => [p.toLowerCase(), p]));
  const found: ConflictCopy[] = [];
  for (const path of normalized) {
    const { stem, ext } = splitExt(path);
    for (const pattern of PATTERNS) {
      if (pattern.onlyIn && (!provider || !pattern.onlyIn.includes(provider))) continue;
      const bases = pattern.bases(stem).filter((base) => base.trim());
      if (bases.length === 0) continue;
      const original =
        bases.map((base) => byLower.get(`${base}${ext}`.toLowerCase())).find(Boolean) ?? null;
      if (pattern.needsOriginal && !original) continue;
      if (pattern.needsOriginal && dismissed.has(path)) break;
      found.push({
        copyPath: path,
        originalPath: original,
        provider: pattern.onlyIn && provider ? provider : pattern.provider,
        ambiguous: pattern.needsOriginal,
      });
      break;
    }
  }
  return found.sort((a, b) => a.copyPath.localeCompare(b.copyPath));
}
