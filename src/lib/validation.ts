import { m } from './i18n.svelte';

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
 * Validates a note or folder name for vaults shared across desktop operating systems.
 * Returns a clear human-readable error message if invalid, or `null` if valid.
 */
export function validateEntryName(rawName: string): string | null {
  if (!rawName || rawName.trim().length === 0) {
    return m.validation_name_empty();
  }

  if (rawName.endsWith(' ') || rawName.endsWith('.')) {
    return m.validation_name_trailing();
  }

  if (/\.md$/i.test(rawName)) {
    const stem = rawName.slice(0, -3);
    if (!stem || stem.trim().length === 0) {
      return m.validation_note_name_empty();
    }
    if (stem.endsWith(' ') || stem.endsWith('.')) {
      return m.validation_name_trailing();
    }
  }

  const trimmed = rawName.trim();
  if (trimmed.startsWith('.')) {
    return m.validation_name_dot({ name: trimmed });
  }

  const badCharMatch = rawName.match(INVALID_CHAR_REGEX);
  if (badCharMatch) {
    return m.validation_bad_char({ char: badCharMatch[0] });
  }

  const stemBeforeDot = trimmed.split('.')[0]?.trim() ?? '';
  if (WINDOWS_RESERVED_NAMES.has(stemBeforeDot.toUpperCase())) {
    return m.validation_reserved({ name: stemBeforeDot });
  }

  return null;
}
