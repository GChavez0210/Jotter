const WINDOWS_RESERVED_NAMES = new Set([
  'CON',
  'PRN',
  'AUX',
  'NUL',
  'COM0',
  'COM1',
  'COM2',
  'COM3',
  'COM4',
  'COM5',
  'COM6',
  'COM7',
  'COM8',
  'COM9',
  'LPT0',
  'LPT1',
  'LPT2',
  'LPT3',
  'LPT4',
  'LPT5',
  'LPT6',
  'LPT7',
  'LPT8',
  'LPT9',
]);

const INVALID_CHAR_REGEX = /[<>:"/\\|?*\x00-\x1F]/;

/**
 * Validates a note or folder name against Windows filesystem naming rules.
 * Returns a clear human-readable error message if invalid, or `null` if valid.
 */
export function validateWindowsEntryName(rawName: string): string | null {
  if (!rawName || rawName.trim().length === 0) {
    return 'Name cannot be empty.';
  }

  if (rawName.endsWith(' ') || rawName.endsWith('.')) {
    return 'Windows names cannot end with a space or a period.';
  }

  if (/\.md$/i.test(rawName)) {
    const stem = rawName.slice(0, -3);
    if (!stem || stem.trim().length === 0) {
      return 'Note name cannot be empty.';
    }
    if (stem.endsWith(' ') || stem.endsWith('.')) {
      return 'Windows names cannot end with a space or a period.';
    }
  }

  const trimmed = rawName.trim();
  if (trimmed.startsWith('.')) {
    return `Names starting with a dot ("${trimmed}") are reserved.`;
  }

  const badCharMatch = rawName.match(INVALID_CHAR_REGEX);
  if (badCharMatch) {
    return `Invalid Windows character "${badCharMatch[0]}". Avoid < > : " / \\ | ? *`;
  }

  const stemBeforeDot = trimmed.split('.')[0]?.trim() ?? '';
  if (WINDOWS_RESERVED_NAMES.has(stemBeforeDot.toUpperCase())) {
    return `"${stemBeforeDot}" is a reserved Windows device name.`;
  }

  return null;
}
