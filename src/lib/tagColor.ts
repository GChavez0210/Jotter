const TAG_COLORS = ['var(--tag-1)', 'var(--tag-2)', 'var(--tag-3)', 'var(--tag-4)', 'var(--tag-5)'];

/** Stable dot color for a tag, so a tag looks the same in the sidebar and the note header. */
export function tagColor(tag: string): string {
  let hash = 0;
  for (const ch of tag.toLowerCase()) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return TAG_COLORS[hash % TAG_COLORS.length];
}
