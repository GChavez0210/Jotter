import {
  autocompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from '@codemirror/autocomplete';
import type { Extension } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import { stripMdExt } from './stores/vault.svelte';
import type { VaultNode } from './types';

export interface WikiCompletionNote {
  title: string;
  relPathNoExt: string;
  relPath: string;
  hasDuplicateTitle: boolean;
}

export function buildWikiCompletionNotes(files: VaultNode[]): WikiCompletionNote[] {
  const titleCounts = new Map<string, number>();
  for (const f of files) {
    const stem = stripMdExt(f.name).toLowerCase();
    titleCounts.set(stem, (titleCounts.get(stem) ?? 0) + 1);
  }

  return files.map((f) => {
    const title = stripMdExt(f.name);
    const relPathNoExt = stripMdExt(f.relPath);
    const hasDuplicateTitle =
      (titleCounts.get(title.toLowerCase()) ?? 0) > 1;
    return {
      title,
      relPathNoExt,
      relPath: f.relPath,
      hasDuplicateTitle,
    };
  });
}

/**
 * Creates a CodeMirror 6 autocompletion extension that triggers when typing `[[`
 * and matches vault notes on both title and relative path.
 */
export function createWikiLinkAutocompleteExtension(
  getVaultFiles: () => VaultNode[]
): Extension {
  const wikiCompletionSource = (
    context: CompletionContext
  ): CompletionResult | null => {
    const line = context.state.doc.lineAt(context.pos);
    const beforeCursor = line.text.slice(0, context.pos - line.from);

    // Find the last unclosed `[[` on this line before the cursor
    const openIdx = beforeCursor.lastIndexOf('[[');
    if (openIdx < 0) return null;

    const afterOpen = beforeCursor.slice(openIdx + 2);
    // If there is already a closing `]]` between `[[` and the cursor, don't autocomplete
    if (afterOpen.includes(']]') || afterOpen.includes('[') || afterOpen.includes(']')) {
      return null;
    }

    // Do not trigger inside inline code spans on the line
    const prefixBeforeOpen = beforeCursor.slice(0, openIdx);
    const backtickCount = (prefixBeforeOpen.match(/`/g) ?? []).length;
    if (backtickCount % 2 === 1) {
      return null;
    }

    const fromPos = line.from + openIdx + 2;
    const rawQuery = afterOpen;
    // Allow matching on the note target portion before `#` or `|`
    const hashIdx = rawQuery.indexOf('#');
    const pipeIdx = rawQuery.indexOf('|');
    let splitIdx = -1;
    if (hashIdx >= 0 && pipeIdx >= 0) {
      splitIdx = Math.min(hashIdx, pipeIdx);
    } else if (hashIdx >= 0) {
      splitIdx = hashIdx;
    } else if (pipeIdx >= 0) {
      splitIdx = pipeIdx;
    }

    const noteFilter = (
      splitIdx >= 0 ? rawQuery.slice(0, splitIdx) : rawQuery
    )
      .trim()
      .toLowerCase();
    const suffix = splitIdx >= 0 ? rawQuery.slice(splitIdx) : '';

    const notes = buildWikiCompletionNotes(getVaultFiles());
    if (notes.length === 0) return null;

    const afterCursorText = line.text.slice(context.pos - line.from);
    const hasClosingBrackets = afterCursorText.startsWith(']]');
    const replaceTo = hasClosingBrackets ? context.pos + 2 : context.pos;

    const options: Completion[] = [];
    for (const note of notes) {
      const titleLower = note.title.toLowerCase();
      const pathLower = note.relPathNoExt.toLowerCase();
      if (
        noteFilter.length > 0 &&
        !titleLower.includes(noteFilter) &&
        !pathLower.includes(noteFilter)
      ) {
        continue;
      }

      const insertTarget = note.hasDuplicateTitle
        ? note.relPathNoExt
        : note.title;
      const inserted = `${insertTarget}${suffix}]]`;

      let boost = 0;
      if (noteFilter.length > 0) {
        if (titleLower === noteFilter || pathLower === noteFilter) {
          boost = 40;
        } else if (titleLower.startsWith(noteFilter)) {
          boost = 25;
        } else if (pathLower.startsWith(noteFilter)) {
          boost = 15;
        }
      }

      options.push({
        label: note.title,
        detail:
          note.relPathNoExt !== note.title ? note.relPathNoExt : undefined,
        boost,
        apply: (view: EditorView) => {
          view.dispatch({
            changes: {
              from: fromPos,
              to: replaceTo,
              insert: inserted,
            },
            selection: {
              anchor: fromPos + inserted.length,
            },
          });
        },
      });
    }

    if (options.length === 0) return null;

    return {
      from: fromPos,
      to: context.pos,
      filter: false,
      options,
    };
  };

  return autocompletion({
    override: [wikiCompletionSource],
    activateOnTyping: true,
    icons: false,
    maxRenderedOptions: 40,
  });
}
