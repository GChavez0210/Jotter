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
    throw new Error('The dragged note is no longer in this vault.');
  }
  if (folder && findNode(tree, folder)?.kind !== 'folder') {
    throw new Error('The destination folder is no longer in this vault.');
  }
  const name = source.split('/').pop()!;
  const destination = folder ? `${folder}/${name}` : name;
  if (destination === source) return null;
  const parent = folder ? findNode(tree, folder)!.children : tree;
  if (parent.some((node) => node.name.toLowerCase() === name.toLowerCase())) {
    throw new Error(`A note or folder named "${name}" already exists there. Rename it or choose another folder.`);
  }
  return destination;
}
