import { fillTemplate, noteStamp } from './templates';

/**
 * Capture profiles decide where a quick capture entry goes and how it is written. A profile
 * either appends to one note or creates a new note in a folder. Its template can use the
 * variables in `templates.ts`, with `{{text}}` standing for the typed entry.
 *
 * Planning is pure: the same text, clipboard, time, and note list always give the same plan.
 * The caller resolves the clipboard and the time once, then writes the plan.
 */

export type CaptureMode = 'append' | 'create';

export interface CaptureProfile {
  id: string;
  name: string;
  mode: CaptureMode;
  /** Append: the note to add to, as a vault path. Create: the folder for new notes. */
  target: string;
  /** Create only: the new note's name, without `.md`. */
  fileName: string;
  /** What gets written: the appended entry, or the whole new note. */
  template: string;
  /** Append only: keep entries on consecutive lines instead of a blank line apart. */
  compact: boolean;
}

/** The profile that always exists: it appends the entry as typed to the inbox note. */
export const INBOX_PROFILE_ID = 'inbox';

export const MAX_PROFILES = 20;
const MAX_NAME = 60;
const MAX_PATH = 240;
const MAX_SEGMENT = 120;
// A full list of profiles has to fit the settings export's value limit of 100,000 characters.
const MAX_TEMPLATE = 3000;
const MAX_TITLE = 80;

export type CapturePresetKind = 'idea' | 'task' | 'meeting' | 'journal';

export const CAPTURE_PRESETS: readonly CapturePresetKind[] = ['idea', 'task', 'meeting', 'journal'];

/** A starter profile. The caller names it, since names follow the interface language. */
export function presetProfile(kind: CapturePresetKind, name: string, id: string): CaptureProfile {
  switch (kind) {
    case 'idea':
      return {
        id,
        name,
        mode: 'create',
        target: 'Ideas',
        fileName: '{{date}} {{title}}',
        template: '# {{title}}\n\nCaptured {{date}} {{time}}\n\n{{text}}\n',
        compact: false,
      };
    case 'task':
      return { id, name, mode: 'append', target: 'Tasks.md', fileName: '', template: '- [ ] {{text}}', compact: true };
    case 'meeting':
      return {
        id,
        name,
        mode: 'create',
        target: 'Meetings',
        fileName: '{{date}} {{title}}',
        template:
          '# {{title}}\n\n- **Date:** {{date}} {{time}}\n\n## Notes\n\n{{text}}\n\n## Action items\n\n- [ ] \n',
        compact: false,
      };
    case 'journal':
      return {
        id,
        name,
        mode: 'append',
        target: 'Journal/{{date}}.md',
        fileName: '',
        template: '- {{time}} {{text}}',
        compact: true,
      };
  }
}

export function blankProfile(name: string, id: string): CaptureProfile {
  return { id, name, mode: 'append', target: 'Inbox.md', fileName: '', template: '{{text}}', compact: false };
}

// Planning -----------------------------------------------------------------------------

export type CapturePlanErrorCode = 'empty' | 'path' | 'collision';

export class CapturePlanError extends Error {
  constructor(readonly code: CapturePlanErrorCode, detail = '') {
    super(`capture-${code}${detail ? `: ${detail}` : ''}`);
  }
}

export interface CaptureInput {
  text: string;
  /** The clipboard's text, read once by the caller. */
  clipboard?: string;
  now: Date;
  /** Whether the vault already has a note at this path. Compared the way the vault compares. */
  noteExists: (path: string) => boolean;
}

export interface CapturePlan {
  mode: CaptureMode;
  /** Vault path, ending in `.md`. */
  path: string;
  /** The entry to append, or the new note's whole text. */
  content: string;
  compact: boolean;
}

const WINDOWS_RESERVED = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$/i;

/** Turns text into one safe path segment: no separators, reserved characters, or control codes. */
export function sanitizeSegment(text: string): string {
  return text
    .replace(/[\u0000-\u001f\u007f\\/:*?"<>|#^[\]]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/^[.\s]+|[.\s]+$/g, '');
}

/** The first line of the entry without list, quote, or heading markers, kept short. */
export function captureTitle(text: string): string {
  const firstLine = text.split(/\r?\n/).find((line) => line.trim() !== '') ?? '';
  const cleaned = firstLine
    .replace(/^\s*(?:#{1,6}\s+|>\s*|[-*+]\s+(?:\[[ xX]\]\s+)?|\d{1,9}[.)]\s+)+/, '')
    .replace(/\s+/g, ' ')
    .trim();
  return cleaned.length > MAX_TITLE ? `${cleaned.slice(0, MAX_TITLE).trimEnd()}…` : cleaned;
}

/** Checks a vault path. Returns it cleaned, or `null` when it could leave the vault or break a filesystem. */
export function cleanVaultPath(raw: string): string | null {
  const path = raw.trim().replace(/\\/g, '/').replace(/\/{2,}/g, '/').replace(/^\/+|\/+$/g, '');
  if (!path || path.length > MAX_PATH) return null;
  for (const segment of path.split('/')) {
    if (
      !segment ||
      segment.length > MAX_SEGMENT ||
      segment.startsWith('.') ||
      /[\u0000-\u001f\u007f:*?"<>|]/.test(segment) ||
      /[.\s]$/.test(segment) ||
      WINDOWS_RESERVED.test(segment)
    ) {
      return null;
    }
  }
  return path;
}

function withMd(path: string): string {
  return /\.md$/i.test(path) ? path : `${path}.md`;
}

/**
 * Plans one capture. Variables in paths and file names use single-line, separator-free values,
 * so a typed or copied value cannot add folders or step outside the vault.
 */
export function planCapture(profile: CaptureProfile, input: CaptureInput): CapturePlan {
  const entry = input.text.trim();
  const title = captureTitle(entry);
  const clipboard = input.clipboard;
  const common = { now: input.now, prompts: {} };

  const forPath = {
    ...common,
    title: sanitizeSegment(title) || 'Untitled',
    text: sanitizeSegment(title),
    clipboard: clipboard === undefined ? undefined : sanitizeSegment(captureTitle(clipboard)),
  };
  const forBody = { ...common, title, text: entry, clipboard };

  const content = fillTemplate(profile.template, forBody);
  if (!entry || content.trim() === '') throw new CapturePlanError('empty');

  if (profile.mode === 'append') {
    const path = cleanVaultPath(withMd(fillTemplate(profile.target, forPath)));
    if (!path) throw new CapturePlanError('path', profile.target);
    return { mode: 'append', path, content, compact: profile.compact };
  }

  const folder = profile.target.trim() === '' ? '' : cleanVaultPath(fillTemplate(profile.target, forPath));
  if (folder === null) throw new CapturePlanError('path', profile.target);
  const name = sanitizeSegment(fillTemplate(profile.fileName, forPath)) || `Capture ${noteStamp(input.now)}`;
  const join = (stem: string) => cleanVaultPath(withMd(folder ? `${folder}/${stem}` : stem));

  const first = join(name);
  if (!first) throw new CapturePlanError('path', profile.fileName);
  let path: string | null = first;
  for (let n = 2; path && input.noteExists(path); n++) {
    if (n > 999) throw new CapturePlanError('collision', first);
    path = join(`${name} ${n}`);
  }
  if (!path) throw new CapturePlanError('path', profile.fileName);
  return { mode: 'create', path, content: content.endsWith('\n') ? content : `${content}\n`, compact: false };
}

// Storage ------------------------------------------------------------------------------

/** Reads one stored profile, or `null` when it is malformed. Strings are bounded. */
function readProfile(value: unknown): CaptureProfile | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  const v = value as Record<string, unknown>;
  const str = (key: string, max: number) =>
    typeof v[key] === 'string' && (v[key] as string).length <= max ? (v[key] as string) : null;
  const id = str('id', 80);
  const name = str('name', MAX_NAME);
  const target = str('target', MAX_PATH);
  const fileName = str('fileName', MAX_PATH);
  const template = str('template', MAX_TEMPLATE);
  if (!id || id === INBOX_PROFILE_ID || name === null || target === null || fileName === null || template === null) {
    return null;
  }
  if (v.mode !== 'append' && v.mode !== 'create') return null;
  if (typeof v.compact !== 'boolean') return null;
  return { id, name, mode: v.mode, target, fileName, template, compact: v.compact };
}

/** Keeps the valid profiles from stored data, drops duplicates by id, and caps the count. */
export function readProfiles(value: unknown): CaptureProfile[] {
  if (!Array.isArray(value)) return [];
  const seen = new Set<string>();
  const profiles: CaptureProfile[] = [];
  for (const entry of value) {
    const profile = readProfile(entry);
    if (!profile || seen.has(profile.id)) continue;
    seen.add(profile.id);
    profiles.push(profile);
    if (profiles.length >= MAX_PROFILES) break;
  }
  return profiles;
}

/** Whether a stored settings value is a list of well-formed profiles, as the import check needs. */
export function isProfilesValue(raw: string): boolean {
  try {
    const parsed: unknown = JSON.parse(raw);
    return (
      Array.isArray(parsed) &&
      parsed.length <= MAX_PROFILES &&
      readProfiles(parsed).length === parsed.length
    );
  } catch {
    return false;
  }
}
