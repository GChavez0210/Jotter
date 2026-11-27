import type { AiAccessOverview } from './types';

/** `all` also means that notes added to the folder later start out shared. */
export type AccessState = 'all' | 'some' | 'none';

export interface AccessRow {
  path: string;
  name: string;
  folder: boolean;
  blocked: boolean;
  state: AccessState;
  children: AccessRow[];
}

/** A folder's state from its own setting and its contents. Blocked notes never count. */
function folderState(shared: boolean, children: AccessRow[]): AccessState {
  const states = children.filter((child) => !child.blocked).map((child) => child.state);
  if (shared && states.every((state) => state === 'all')) return 'all';
  return !shared && states.every((state) => state === 'none') ? 'none' : 'some';
}

/** The vault as a tree for the note picker: folders first, each level sorted by name. */
export function buildAccessTree(overview: AiAccessOverview): AccessRow {
  const row = (path: string, folder: boolean): AccessRow => ({
    path,
    name: path.slice(path.lastIndexOf('/') + 1),
    folder,
    blocked: false,
    state: 'none',
    children: [],
  });
  const root = row('', true);
  const folders = new Map([['', root]]);
  const parentOf = (path: string) => folders.get(path.slice(0, Math.max(path.lastIndexOf('/'), 0)))!;
  // A parent path sorts before the paths inside it.
  for (const { path } of [...overview.folders].sort((a, b) => a.path.localeCompare(b.path))) {
    const folder = row(path, true);
    folders.set(path, folder);
    parentOf(path).children.push(folder);
  }
  for (const note of overview.notes) {
    parentOf(note.path).children.push({
      ...row(note.path, false),
      blocked: note.blocked,
      state: note.shared ? 'all' : 'none',
    });
  }
  const shared = new Map(overview.folders.map((folder) => [folder.path, folder.shared]));
  const finish = (node: AccessRow) => {
    node.children.forEach((child) => child.folder && finish(child));
    node.children.sort((a, b) => Number(b.folder) - Number(a.folder) || a.name.localeCompare(b.name));
    node.state = folderState(node.path ? shared.get(node.path)! : overview.vaultShared, node.children);
  };
  finish(root);
  return root;
}
