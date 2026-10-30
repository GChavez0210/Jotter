/**
 * Turns a release's Markdown notes into plain text for the update dialog. The notes are the
 * changelog section for that version, so headings, bold, code and links are the only syntax
 * worth handling.
 */
export function plainReleaseNotes(markdown: string): string {
  return markdown
    .replace(/\r\n?/g, '\n')
    .split('\n')
    .map((line) =>
      line
        .replace(/^\s{0,3}#{1,6}\s+/, '')
        .replace(/^(\s*)[-*+]\s+/, '$1• ')
        .replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
        .replace(/(\*\*|__)(.+?)\1/g, '$2')
        .replace(/`+([^`]+)`+/g, '$1')
        .replace(/\\([\\`*_{}[\]()#+\-.!|])/g, '$1')
        .trimEnd(),
    )
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}
