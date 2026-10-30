// Prints one version's section of CHANGELOG.md, for the GitHub release body. The updater
// shows the same text in the app, so it is the only place release notes get written.
//
//   node scripts/release-notes.mjs v0.6.0           print the section, or a pointer if it is missing
//   node scripts/release-notes.mjs v0.6.0 --strict  exit 1 if the section is missing or empty
import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

export function releaseNotes(changelog, version) {
  const wanted = version.replace(/^v/, '');
  const lines = changelog.replace(/\r\n?/g, '\n').split('\n');
  const start = lines.findIndex((line) => line.startsWith('## [') && line.slice(4).split(']')[0] === wanted);
  if (start === -1) return '';
  const end = lines.findIndex((line, index) => index > start && line.startsWith('## '));
  return lines.slice(start + 1, end === -1 ? undefined : end).join('\n').trim();
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [version, flag] = process.argv.slice(2);
  if (!version) {
    console.error('Usage: node scripts/release-notes.mjs <version> [--strict]');
    process.exit(2);
  }
  const notes = releaseNotes(await readFile(new URL('../CHANGELOG.md', import.meta.url), 'utf8'), version);
  if (!notes && flag === '--strict') {
    console.error(`CHANGELOG.md has no notes for ${version}. Move the Unreleased entries under a "## [${version.replace(/^v/, '')}]" heading first.`);
    process.exit(1);
  }
  console.log(notes || 'See CHANGELOG.md for what changed.');
}
