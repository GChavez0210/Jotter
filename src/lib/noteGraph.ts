import type { NoteCard } from './types';

/**
 * Pure helpers over the note cards from the index: the tag hierarchy for the tags-and-links
 * view and the sections of the Home page.
 */

export interface TagNode {
  /** Last segment of the tag, such as `alpha` in `project/alpha`. */
  name: string;
  /** Full tag path, such as `project/alpha`. */
  tag: string;
  /** Notes tagged with exactly this tag. */
  notes: NoteCard[];
  children: TagNode[];
  /** Notes tagged with this tag or any tag below it, each counted once. */
  total: number;
}

const byTitle = (a: NoteCard, b: NoteCard) =>
  a.title.localeCompare(b.title, undefined, { sensitivity: 'base' });

/** Builds the nested tag tree; `a/b` sits under `a`. Notes without tags are returned apart. */
export function buildTagTree(cards: NoteCard[]): { roots: TagNode[]; untagged: NoteCard[] } {
  const nodes = new Map<string, TagNode>();
  const getNode = (tag: string): TagNode => {
    let node = nodes.get(tag);
    if (!node) {
      node = { name: tag.split('/').pop()!, tag, notes: [], children: [], total: 0 };
      nodes.set(tag, node);
      const slash = tag.lastIndexOf('/');
      if (slash > 0) getNode(tag.slice(0, slash)).children.push(node);
    }
    return node;
  };

  const untagged: NoteCard[] = [];
  for (const card of cards) {
    if (card.tags.length === 0) untagged.push(card);
    for (const tag of card.tags) getNode(tag).notes.push(card);
  }

  const finish = (node: TagNode): Set<string> => {
    node.notes.sort(byTitle);
    node.children.sort((a, b) => a.name.localeCompare(b.name));
    const paths = new Set(node.notes.map((n) => n.path));
    for (const child of node.children) for (const p of finish(child)) paths.add(p);
    node.total = paths.size;
    return paths;
  };
  // getNode creates every parent, so only tags without a parent segment are roots.
  const roots = [...nodes.values()].filter((n) => n.tag.lastIndexOf('/') <= 0);
  roots.sort((a, b) => a.name.localeCompare(b.name));
  roots.forEach(finish);
  return { roots, untagged: untagged.sort(byTitle) };
}

// Home page ---------------------------------------------------------------------------------

export type HomeSectionKind =
  | 'recent-opened'
  | 'recent-created'
  | 'recent-modified'
  | 'tag'
  | 'most-linked'
  | 'least-linked'
  | 'rediscover';

export type HomeSort = 'default' | 'title' | 'modified' | 'created';

export interface HomeSection {
  id: string;
  kind: HomeSectionKind;
  title: string;
  /** Number of cards to show. */
  limit: number;
  sort: HomeSort;
  /** Only notes under this folder (vault-relative, no trailing slash). Empty for all. */
  folder: string;
  /** Only notes with this tag (or a tag below it). For `tag` sections, the tag to show. */
  tag: string;
  hidden?: boolean;
}

export const DEFAULT_HOME_SECTIONS: HomeSection[] = [
  { id: 'opened', kind: 'recent-opened', title: 'Recently opened', limit: 6, sort: 'default', folder: '', tag: '' },
  { id: 'modified', kind: 'recent-modified', title: 'Recently edited', limit: 6, sort: 'default', folder: '', tag: '' },
  { id: 'created', kind: 'recent-created', title: 'New notes', limit: 6, sort: 'default', folder: '', tag: '' },
  { id: 'linked', kind: 'most-linked', title: 'Most linked', limit: 6, sort: 'default', folder: '', tag: '' },
  { id: 'rediscover', kind: 'rediscover', title: 'Rediscover', limit: 1, sort: 'default', folder: '', tag: '' },
];

export const SECTION_KIND_LABELS: Record<HomeSectionKind, string> = {
  'recent-opened': 'Recently opened',
  'recent-created': 'Recently created',
  'recent-modified': 'Recently edited',
  tag: 'Notes with a tag',
  'most-linked': 'Most linked',
  'least-linked': 'Least linked',
  rediscover: 'Rediscover',
};

/** Notes untouched for this long qualify for Rediscover. */
export const REDISCOVER_AGE_MS = 90 * 24 * 60 * 60 * 1000;

function hasTag(card: NoteCard, tag: string): boolean {
  // The index keeps tags as written, so `#Project` and `#project` both match `project`.
  const t = tag.replace(/^#/, '').toLowerCase();
  return card.tags.some((c) => {
    const lower = c.toLowerCase();
    return lower === t || lower.startsWith(`${t}/`);
  });
}

function inFolder(card: NoteCard, folder: string): boolean {
  const f = folder.replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
  return !f || card.path.toLowerCase().startsWith(`${f.toLowerCase()}/`);
}

/** A day-stable pseudo-random index, so Rediscover shows the same note all day. */
function dailyPick(count: number, now: Date, seed: string): number {
  let hash = 2166136261;
  for (const ch of `${now.getFullYear()}-${now.getMonth()}-${now.getDate()}-${seed}`) {
    hash = Math.imul(hash ^ ch.charCodeAt(0), 16777619);
  }
  return (hash >>> 0) % count;
}

export interface HomeContext {
  /** Vault paths in most-recently-opened order. */
  openedPaths: string[];
  now: Date;
}

/**
 * Fills each visible section with cards. A note appears only in the first section that
 * matches it, so later sections show different notes.
 */
export function buildHomeSections(
  sections: HomeSection[],
  cards: NoteCard[],
  ctx: HomeContext
): Array<{ section: HomeSection; cards: NoteCard[] }> {
  const used = new Set<string>();
  const byPath = new Map(cards.map((c) => [c.path, c]));
  const nowMs = ctx.now.getTime();
  const out: Array<{ section: HomeSection; cards: NoteCard[] }> = [];

  for (const section of sections) {
    if (section.hidden) continue;
    const pool = cards.filter(
      (c) => !used.has(c.path) && inFolder(c, section.folder) && (!section.tag || hasTag(c, section.tag))
    );
    let picked: NoteCard[];
    switch (section.kind) {
      case 'recent-opened': {
        const allowed = new Set(pool.map((c) => c.path));
        picked = ctx.openedPaths.filter((p) => allowed.has(p)).map((p) => byPath.get(p)!);
        break;
      }
      case 'recent-created':
        picked = [...pool].sort((a, b) => b.createdMs - a.createdMs);
        break;
      case 'recent-modified':
        picked = [...pool].sort((a, b) => b.modifiedMs - a.modifiedMs);
        break;
      case 'tag':
        picked = section.tag ? [...pool].sort((a, b) => b.modifiedMs - a.modifiedMs) : [];
        break;
      case 'most-linked':
        picked = pool.filter((c) => c.linkedFrom > 0).sort((a, b) => b.linkedFrom - a.linkedFrom || byTitle(a, b));
        break;
      case 'least-linked':
        picked = [...pool].sort((a, b) => a.linkedFrom - b.linkedFrom || a.linksTo.length - b.linksTo.length || byTitle(a, b));
        break;
      case 'rediscover': {
        const old = pool
          .filter((c) => nowMs - c.modifiedMs >= REDISCOVER_AGE_MS)
          .sort((a, b) => a.path.localeCompare(b.path));
        picked = [];
        for (let i = 0; i < Math.min(section.limit, old.length); i++) {
          const rest = old.filter((c) => !picked.includes(c));
          picked.push(rest[dailyPick(rest.length, ctx.now, `${section.id}-${i}`)]);
        }
        break;
      }
    }
    picked = picked.slice(0, Math.max(0, section.limit));
    if (section.sort === 'title') picked.sort(byTitle);
    else if (section.sort === 'modified') picked.sort((a, b) => b.modifiedMs - a.modifiedMs);
    else if (section.sort === 'created') picked.sort((a, b) => b.createdMs - a.createdMs);
    for (const c of picked) used.add(c.path);
    out.push({ section, cards: picked });
  }
  return out;
}
