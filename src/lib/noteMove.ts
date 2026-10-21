import { m } from './i18n.svelte';
import type { VaultNode } from './types';

export const NOTE_DRAG_TYPE = 'application/x-jotter-note';

function findNode(nodes: readonly VaultNode[], path: string): VaultNode | undefined {
  for (const node of nodes) {
    if (node.relPath === path) return node;
    const child = findNode(node.children, path);
    if (child) return child;
  }
}

/** Plan an internal note move without renaming it or replacing another entry. */
export function noteMoveDestination(tree: readonly VaultNode[], source: string, folder: string): string | null {
  const note = findNode(tree, source);
  if (!note || note.kind !== 'file' || !note.name.toLowerCase().endsWith('.md')) {
    throw new Error(m.move_source_missing());
  }
  if (folder && findNode(tree, folder)?.kind !== 'folder') {
    throw new Error(m.move_folder_missing());
  }
  const name = source.split('/').pop()!;
  const destination = folder ? `${folder}/${name}` : name;
  if (destination === source) return null;
  const parent = folder ? findNode(tree, folder)!.children : tree;
  if (parent.some((node) => node.name.toLowerCase() === name.toLowerCase())) {
    throw new Error(m.move_name_taken({ name }));
  }
  return destination;
}
