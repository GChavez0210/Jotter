import {
  baseLocale,
  isLocale,
  locales,
  overwriteGetLocale,
  overwriteSetLocale,
  type Locale,
} from './paraglide/runtime';

import { appStorage } from './appStorage';
import { m } from './paraglide/messages';
import { isWindows } from './platform';

export { m };

export type LanguagePreference = 'system' | Locale;

const STORAGE_KEY = 'jotter.language';

/** Display names in each language's own script, so a user can find theirs in any UI language. */
export const LANGUAGE_NAMES: Record<Locale, string> = {
  en: 'English',
  es: 'Español',
};

/** Picks the first supported UI language from the OS language list, matching on the primary subtag. */
export function matchSystemLocale(preferred: readonly string[]): Locale {
  for (const tag of preferred) {
    const primary = tag.toLowerCase().split('-')[0];
    if (isLocale(primary)) return primary;
  }
  return baseLocale;
}

function systemLanguages(): readonly string[] {
  return typeof navigator === 'undefined' ? [] : navigator.languages ?? [navigator.language];
}

function loadPreference(): LanguagePreference {
  try {
    const saved = appStorage.getItem(STORAGE_KEY);
    if (saved === 'system' || (saved && isLocale(saved))) return saved;
  } catch {
    // Storage can be unavailable; fall back to the OS language.
  }
  return 'system';
}

class I18nStore {
  preference = $state<LanguagePreference>(loadPreference());
  locale = $derived<Locale>(
    this.preference === 'system' ? matchSystemLocale(systemLanguages()) : this.preference
  );

  /** Rereads the saved language, for a window that outlives changes made in another one. */
  reloadPreference() {
    this.preference = loadPreference();
  }

  setPreference(next: LanguagePreference) {
    this.preference = next;
    try {
      appStorage.setItem(STORAGE_KEY, next);
    } catch {
      // ignore storage errors
    }
  }
}

export const i18n = new I18nStore();

/** Placeholder value for a message parameter that the template renders as markup. */
export const SLOT = '\u0001';

/**
 * Splits a message rendered with SLOT parameters into the text around them,
 * so a template can wrap each value in markup such as <strong> or <code>.
 * The template places the markup in order, so translations of these messages
 * must keep their placeholders in the English order.
 */
export function splitSlots(message: string): string[] {
  return message.split(SLOT);
}

export { locales };

/** The OS name for where deleted files go. */
export function trashName(): string {
  return isWindows ? m.trash_name_windows() : m.trash_name_other();
}

// Messages read the locale through this $state-backed getter, so every
// m.key() call in a template re-renders when the language changes, with no reload.
overwriteGetLocale(() => i18n.locale);
overwriteSetLocale((next) => i18n.setPreference(next));
