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
  const lines = markdown.split('\n');
  const headings: OutlineHeading[] = [];
  let fence: string | null = null;
  let start = 0;

  // Frontmatter only counts when the note opens with it.
  if (lines[0]?.trim() === '---') {
    const end = lines.findIndex((line, index) => index > 0 && line.trim() === '---');
    if (end > 0) start = end + 1;
  }

  for (let index = start; index < lines.length; index++) {
    const line = lines[index];
    const fenceMatch = /^\s{0,3}(`{3,}|~{3,})/.exec(line);
    if (fenceMatch) {
      const marker = fenceMatch[1];
      if (fence === null) {
        fence = marker[0];
      } else if (marker[0] === fence) {
        fence = null;
      }
      continue;
    }
    if (fence !== null) continue;

    // Up to three leading spaces still make a heading; four would make it an indented code block.
    const match = /^ {0,3}(#{1,6})(\s+.*)?$/.exec(line);
    if (!match) continue;
    const text = (match[2] ?? '').trim().replace(/\s+#+\s*$/, '');
    headings.push({ level: match[1].length, text, line: index + 1 });
  }

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
  const shallowest = Math.min(...headings.map((heading) => heading.level));
  return headings.map((heading) => heading.level - shallowest);
}
