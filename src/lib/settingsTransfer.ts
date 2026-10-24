/** Settings that describe this computer or this session, so an export leaves them out. */
const MACHINE_KEYS = new Set([
  'jotter.lastVaultPath',
  'jotter.recentlyOpened',
  'jotter.preFocusLayout',
  'jotter.dismissedConflicts',
  'jotter.taskCalendars',
]);

const MAX_VALUE_LENGTH = 100_000;
export const EXPORT_FORMAT = 'jotter-settings';
export const EXPORT_VERSION = 1;

const oneOf = (...values: string[]) => (value: string) => values.includes(value);
const boolean = oneOf('true', 'false');
const text = (value: string) => !/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/.test(value);
const object = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);
const strings = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((entry) => typeof entry === 'string');
const json = (check: (value: unknown) => boolean) => (value: string) => {
  try { return check(JSON.parse(value)); } catch { return false; }
};

// Keep this module independent of stores and i18n: appStorage loads before either initializes.
const VALIDATORS: Record<string, (value: string) => boolean> = {
  'jotter.copyMode': oneOf('markdown_source', 'plain', 'rich'),
  'jotter.editingMode': oneOf('live', 'source'),
  'jotter.themePref': oneOf('system', 'light', 'dark', 'high-contrast'),
  'jotter.language': oneOf('system', 'en', 'es'),
  'jotter.accentMode': oneOf('preset', 'system', 'custom'),
  'jotter.accentColorMode': oneOf('system', 'custom'),
  'jotter.accentPreset': oneOf('ink', 'moss', 'plum', 'teal', 'rust', 'graphite', 'amber', 'rose'),
  'jotter.customAccentColor': (value) => /^#[0-9a-f]{6}$/i.test(value),
  'jotter.editorFontSize': oneOf('12', '14', '16', '20'),
  'jotter.sidebarWidth': (value) => value.trim() !== '' && Number.isFinite(Number(value)) && Number(value) >= 180 && Number(value) <= 480,
  'jotter.leftSidebarView': oneOf('files', 'search', 'tags', 'calendar', 'trash'),
  'jotter.rightPanelTab': oneOf('note', 'calendar', 'trash'),
  'jotter.focusMode': oneOf('off', 'minimal', 'zen'),
  'jotter.notesNavigation': oneOf('folders', 'tags'),
  'jotter.imageLinkFormat': oneOf('wikilink', 'markdown'),
  'jotter.writingFont': oneOf('serif', 'sans', 'mono'),
  'jotter.closeAction': oneOf('quit', 'tray'),
  'jotter.spellEngine': oneOf('jotter', 'system'),
  'jotter.aiRewriteStyle': oneOf('polish', 'shorter', 'formal', 'casual', 'grammar', 'clearer'),
  'jotter.aiTargetLanguage': (value) => text(value) && value.trim().length > 0 && value.length <= 100,
  'jotter.dailyFolder': text,
  'jotter.dailyTemplatePath': text,
  'jotter.summonShortcut': (value) => text(value) && value.length > 0 && value.length <= 100,
  'jotter.enabledSnippets': json(strings),
  'jotter.spellLanguages': json((value) => object(value) && Object.values(value).every(strings)),
  'jotter.homeSections': json((value) => object(value) && Object.values(value).every((sections) =>
    Array.isArray(sections) && sections.every((section) => object(section)
      && ['id', 'title', 'folder', 'tag'].every((key) => typeof section[key] === 'string')
      && typeof section.kind === 'string' && oneOf('recent-opened', 'recent-created', 'recent-modified', 'tag', 'most-linked', 'least-linked', 'rediscover')(section.kind)
      && typeof section.sort === 'string' && oneOf('default', 'title', 'modified', 'created')(section.sort)
      && typeof section.limit === 'number' && Number.isInteger(section.limit) && section.limit >= 1 && section.limit <= 100
      && (section.hidden === undefined || typeof section.hidden === 'boolean')))),
};
for (const name of ['typewriterScrolling', 'dimInactiveParagraphs', 'showHiddenFiles', 'showHomePage',
  'showLineNumbers', 'spellCheck', 'summonEnabled', 'taskReminders', 'trayEnabled', 'sidebarCollapsed', 'showRightPanel']) {
  VALIDATORS[`jotter.${name}`] = boolean;
}

export function isTransferableKey(key: string): boolean {
  return Object.hasOwn(VALIDATORS, key) && !MACHINE_KEYS.has(key);
}

export function isTransferableEntry(key: string, value: unknown): value is string {
  return isTransferableKey(key) && typeof value === 'string'
    && value.length <= MAX_VALUE_LENGTH && VALIDATORS[key](value);
}

/** Builds the export file text from the stored settings. */
export function buildSettingsExport(settings: Record<string, string>, now = new Date()): string {
  const entries = Object.entries(settings)
    .filter(([key, value]) => isTransferableEntry(key, value))
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
  if (file.format !== EXPORT_FORMAT || typeof file.version !== 'number' || !Number.isInteger(file.version) || file.version < 1) {
    return { ok: false, reason: 'invalid' };
  }
  if (file.version > EXPORT_VERSION) return { ok: false, reason: 'newer-version' };
  if (typeof file.settings !== 'object' || file.settings === null || Array.isArray(file.settings)) {
    return { ok: false, reason: 'invalid' };
  }

  const settings: Record<string, string> = {};
  let skipped = 0;
  for (const [key, value] of Object.entries(file.settings)) {
    if (isTransferableEntry(key, value)) {
      settings[key] = value;
    } else {
      skipped++;
    }
  }
  if (Object.keys(settings).length === 0) return { ok: false, reason: 'empty' };
  return { ok: true, settings, skipped };
}
