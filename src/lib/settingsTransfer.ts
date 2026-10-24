/** Settings that describe this computer or this session, so an export leaves them out. */
const MACHINE_KEYS = new Set([
  'jotter.lastVaultPath',
  'jotter.recentlyOpened',
  'jotter.preFocusLayout',
  'jotter.dismissedConflicts',
  'jotter.taskCalendars',
]);

const KEY_PREFIX = 'jotter.';
const MAX_KEY_LENGTH = 100;
const MAX_VALUE_LENGTH = 100_000;
export const EXPORT_FORMAT = 'jotter-settings';
export const EXPORT_VERSION = 1;

export function isTransferableKey(key: string): boolean {
  return key.startsWith(KEY_PREFIX) && key.length <= MAX_KEY_LENGTH && !MACHINE_KEYS.has(key);
}

/** Builds the export file text from the stored settings. */
export function buildSettingsExport(settings: Record<string, string>, now = new Date()): string {
  const entries = Object.entries(settings)
    .filter(([key]) => isTransferableKey(key))
    .sort(([a], [b]) => a.localeCompare(b));
  return JSON.stringify(
    {
      format: EXPORT_FORMAT,
      version: EXPORT_VERSION,
      exportedAt: now.toISOString(),
      settings: Object.fromEntries(entries),
    },
    null,
    2
  );
}

export type ParsedImport =
  | { ok: true; settings: Record<string, string>; skipped: number }
  | { ok: false; reason: 'invalid' | 'newer-version' | 'empty' };

/** Reads an export file. Keeps only well-formed entries and counts the ones it drops. */
export function parseSettingsImport(text: string): ParsedImport {
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch {
    return { ok: false, reason: 'invalid' };
  }
  if (typeof data !== 'object' || data === null) return { ok: false, reason: 'invalid' };
  const file = data as { format?: unknown; version?: unknown; settings?: unknown };
  if (file.format !== EXPORT_FORMAT || typeof file.version !== 'number') {
    return { ok: false, reason: 'invalid' };
  }
  if (file.version > EXPORT_VERSION) return { ok: false, reason: 'newer-version' };
  if (typeof file.settings !== 'object' || file.settings === null || Array.isArray(file.settings)) {
    return { ok: false, reason: 'invalid' };
  }

  const settings: Record<string, string> = {};
  let skipped = 0;
  for (const [key, value] of Object.entries(file.settings)) {
    if (isTransferableKey(key) && typeof value === 'string' && value.length <= MAX_VALUE_LENGTH) {
      settings[key] = value;
    } else {
      skipped++;
    }
  }
  if (Object.keys(settings).length === 0) return { ok: false, reason: 'empty' };
  return { ok: true, settings, skipped };
}
