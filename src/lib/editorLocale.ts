import { StateEffect, type Transaction } from '@codemirror/state';
import { WidgetType } from '@codemirror/view';
import { i18n } from './i18n.svelte';

/** Refresh translated decorations without replacing the document, history, folds, or zoom. */
export const refreshEditorLocale = StateEffect.define<void>();

export function editorLocaleChanged(transaction: Transaction): boolean {
  return transaction.effects.some((effect) => effect.is(refreshEditorLocale));
}

/** CodeMirror must replace a translated widget when its locale changes. */
export abstract class LocalizedWidget extends WidgetType {
  readonly locale = i18n.locale;
}
