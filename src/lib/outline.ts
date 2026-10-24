import { markdownLanguage } from '@codemirror/lang-markdown';
import { parseFrontmatter } from './frontmatter';

/** One ATX heading in the open note. `line` is 1-based, matching the editor's line numbers. */
export interface OutlineHeading {
  level: number;
  text: string;
  line: number;
}

/**
 * Reads the ATX headings of a note, skipping YAML frontmatter and fenced code, so a `#` in a
 * shell snippet never reaches the outline. Setext headings are left out: Jotter writes ATX.
 */
export function parseOutline(markdown: string): OutlineHeading[] {
  const headings: OutlineHeading[] = [];
  const frontmatter = parseFrontmatter(markdown);
  // Comment-only YAML is ambiguous with headings between thematic breaks. Keep those visible.
  const start = frontmatter && frontmatter.properties.length > 0 ? frontmatter.end : 0;
  const body = markdown.slice(start);
  let offset = start;
  let line = 1;
  for (let index = 0; index < start; index++) if (markdown[index] === '\n') line++;
  markdownLanguage.parser.parse(body).iterate({
    enter(node) {
      if (!/^ATXHeading[1-6]$/.test(node.name)) return;
      const from = start + node.from;
      while (offset < from) if (markdown[offset++] === '\n') line++;
      const text = markdown.slice(from, start + node.to)
        .replace(/^#{1,6}(?:[ \t]+|$)/, '').trim().replace(/(?:^|\s+)#+\s*$/, '');
      headings.push({ level: Number(node.name.slice(-1)), text, line });
      return false;
    },
  });
  return headings;
}

/**
 * Index of the heading the cursor sits in: the last one at or above `cursorLine`. Returns -1
 * while the cursor is above the first heading, so nothing is highlighted there.
 */
export function activeHeadingIndex(headings: OutlineHeading[], cursorLine: number): number {
  let active = -1;
  for (let index = 0; index < headings.length; index++) {
    if (headings[index].line > cursorLine) break;
    active = index;
  }
  return active;
}

/**
 * Indent depth for display. Levels are relative to the note's own shallowest heading, so a note
 * written entirely in `##` does not render one step indented.
 */
export function outlineDepths(headings: OutlineHeading[]): number[] {
  if (headings.length === 0) return [];
  const shallowest = headings.reduce((level, heading) => Math.min(level, heading.level), 6);
  return headings.map((heading) => heading.level - shallowest);
}
