/**
 * Reads and edits YAML frontmatter for the properties form.
 *
 * Handles the subset notes use in practice: scalars, inline `[a, b]` lists, and block `- a`
 * lists. Anything else (nested maps, `|` and `>` blocks) is kept byte for byte and shown
 * read-only. Edits rewrite only the changed property, so comments, order, and the formatting
 * of untouched properties survive.
 */

export type PropertyType = 'text' | 'number' | 'checkbox' | 'date' | 'datetime' | 'list';

export type PropertyValue = string | number | boolean | string[] | null;

export interface Property {
  key: string;
  type: PropertyType;
  value: PropertyValue;
  /** False for values the form cannot round-trip, such as nested maps. */
  editable: boolean;
  /** The property's source lines, without the trailing newline. */
  raw: string;
}

export interface Frontmatter {
  /** Offset just past the closing fence line, including its newline when present. */
  end: number;
  properties: Property[];
  /** Source lines in order: property indexes, or the literal text of comments and blanks. */
  layout: Array<number | string>;
}

export interface TextChange {
  from: number;
  to: number;
  insert: string;
}

const KEY_RE = /^([^\s#\-:][^:]*?|"[^"]*"|'[^']*'):(?:[ \t]+(.*?))?\s*$/;
const DATE_RE = /^\d{4}-\d{2}-\d{2}$/;
const DATETIME_RE = /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}(?::\d{2})?$/;
const NUMBER_RE = /^[-+]?(?:\d+\.?\d*|\.\d+)(?:e[-+]?\d+)?$/i;

function unquote(text: string): string {
  if (text.length >= 2 && text.startsWith('"') && text.endsWith('"')) {
    try {
      return JSON.parse(text) as string;
    } catch {
      return text.slice(1, -1);
    }
  }
  if (text.length >= 2 && text.startsWith("'") && text.endsWith("'")) {
    return text.slice(1, -1).replace(/''/g, "'");
  }
  return text;
}

function stripComment(text: string): string {
  // `key: # note` has no value, only a comment.
  if (text.startsWith('#')) return '';
  if (/^["']/.test(text)) {
    // Keep the quoted scalar and drop a comment after its closing quote.
    const quoted = /^(?:"(?:[^"\\]|\\.)*"|'(?:[^']|'')*')/.exec(text);
    if (quoted && /^(?:\s+#.*)?\s*$/.test(text.slice(quoted[0].length))) return quoted[0];
    return text;
  }
  return text.replace(/\s+#.*$/, '');
}

function splitInlineList(inner: string): string[] {
  const items: string[] = [];
  let current = '';
  let quote: string | null = null;
  for (const ch of inner) {
    if (quote) {
      current += ch;
      if (ch === quote) quote = null;
    } else if (ch === '"' || ch === "'") {
      quote = ch;
      current += ch;
    } else if (ch === ',') {
      items.push(current);
      current = '';
    } else {
      current += ch;
    }
  }
  items.push(current);
  return items.map((s) => unquote(s.trim())).filter((s) => s !== '');
}

function parseScalar(text: string): { type: PropertyType; value: PropertyValue } {
  const quoted = /^["']/.test(text);
  const value = unquote(text);
  if (quoted) {
    return { type: DATE_RE.test(value) ? 'date' : DATETIME_RE.test(value) ? 'datetime' : 'text', value };
  }
  if (/^(true|false)$/i.test(value)) return { type: 'checkbox', value: value.toLowerCase() === 'true' };
  if (/^(null|~)?$/.test(value)) return { type: 'text', value: null };
  // A leading zero, as in a ZIP code `02134` or an ID `007`, would be lost as a number.
  if (NUMBER_RE.test(value) && !/^[-+]?0\d/.test(value)) return { type: 'number', value: Number(value) };
  if (DATE_RE.test(value)) return { type: 'date', value };
  if (DATETIME_RE.test(value)) return { type: 'datetime', value };
  return { type: 'text', value };
}

function parseProperty(key: string, rest: string, continuation: string[], raw: string): Property {
  const readOnly = (): Property => ({ key, type: 'text', value: raw, editable: false, raw });
  const value = stripComment(rest);
  if (value) {
    if (continuation.some((l) => l.trim())) return readOnly();
    if (value.startsWith('[') && value.endsWith(']')) {
      return { key, type: 'list', value: splitInlineList(value.slice(1, -1)), editable: true, raw };
    }
    if (/^[|>{&*!]/.test(value) || value === '[' ) return readOnly();
    return { key, ...parseScalar(value), editable: true, raw };
  }
  const items = continuation.filter((l) => l.trim());
  if (items.length === 0) return { key, type: 'text', value: null, editable: true, raw };
  if (!items.every((l) => /^\s*-(\s|$)/.test(l))) return readOnly();
  const list = items.map((l) => unquote(stripComment(l.replace(/^\s*-\s*/, '')).trim()));
  if (list.some((item) => /^[[{]|:\s/.test(item))) return readOnly();
  return { key, type: 'list', value: list.filter((s) => s !== ''), editable: true, raw };
}

/** Parses the frontmatter at the start of `doc`, or returns null when there is none. */
export function parseFrontmatter(doc: string): Frontmatter | null {
  if (!/^---[ \t]*\r?\n/.test(doc)) return null;
  const lines = doc.split('\n');
  let close = -1;
  for (let i = 1; i < lines.length; i++) {
    if (/^(---|\.\.\.)[ \t]*\r?$/.test(lines[i])) {
      close = i;
      break;
    }
  }
  if (close === -1) return null;

  const body = lines.slice(1, close).map((l) => l.replace(/\r$/, ''));
  const properties: Property[] = [];
  const layout: Array<number | string> = [];
  let i = 0;
  while (i < body.length) {
    const match = KEY_RE.exec(body[i]);
    if (!match) {
      layout.push(body[i]);
      i++;
      continue;
    }
    let j = i + 1;
    while (j < body.length && (/^\s/.test(body[j]) || /^-(\s|$)/.test(body[j]) || (!body[j].trim() && j + 1 < body.length && /^\s/.test(body[j + 1])))) {
      j++;
    }
    const raw = body.slice(i, j).join('\n');
    properties.push(parseProperty(unquote(match[1].trim()), match[2] ?? '', body.slice(i + 1, j), raw));
    layout.push(properties.length - 1);
    i = j;
  }
  // Every line must be a property, a comment, a stray list item, or blank. A note that opens with
  // a thematic break, such as `---` / `Intro text` / `---`, has no frontmatter, and hiding it
  // would hide text.
  const yamlLine = (text: string) => !text.trim() || /^\s*(#|-(\s|$))/.test(text);
  if (!layout.every((item) => typeof item === 'number' || yamlLine(item))) return null;

  let end = 0;
  for (let k = 0; k <= close; k++) end += lines[k].length + 1;
  return { end: Math.min(end, doc.length), properties, layout };
}

function needsQuotes(text: string): boolean {
  return (
    text === '' ||
    /^\s|\s$/.test(text) ||
    /^[-?:,[\]{}#&*!|>'"%@`]/.test(text) ||
    /:(\s|$)|\s#/.test(text) ||
    /^(true|false|null|~|yes|no|on|off)$/i.test(text) ||
    NUMBER_RE.test(text) ||
    DATE_RE.test(text) ||
    DATETIME_RE.test(text)
  );
}

function formatScalar(text: string, type: PropertyType = 'text'): string {
  if ((type === 'date' && DATE_RE.test(text)) || (type === 'datetime' && DATETIME_RE.test(text))) {
    return text;
  }
  return needsQuotes(text) ? JSON.stringify(text) : text;
}

function formatKey(key: string): string {
  return /^[^\s#\-:"'][^:]*$/.test(key) && !/\s$/.test(key) ? key : JSON.stringify(key);
}

/** Serializes one property the way Obsidian writes it. */
export function serializeProperty(key: string, type: PropertyType, value: PropertyValue): string {
  const k = formatKey(key);
  if (value === null || value === '') return `${k}:`;
  switch (type) {
    case 'list': {
      const items = Array.isArray(value) ? value : String(value).split(',');
      const clean = items.map((s) => s.trim()).filter(Boolean);
      if (clean.length === 0) return `${k}: []`;
      return [`${k}:`, ...clean.map((item) => `  - ${formatScalar(item)}`)].join('\n');
    }
    case 'checkbox':
      return `${k}: ${value === true || value === 'true'}`;
    case 'number': {
      const n = typeof value === 'number' ? value : Number(value);
      return Number.isFinite(n) ? `${k}: ${n}` : `${k}: ${formatScalar(String(value))}`;
    }
    default:
      return `${k}: ${formatScalar(String(value), type)}`;
  }
}

function rebuild(fm: Frontmatter, rawFor: (index: number) => string | null, extra: string[] = []): string {
  const out: string[] = [];
  for (const item of fm.layout) {
    if (typeof item === 'string') {
      out.push(item);
    } else {
      const raw = rawFor(item);
      if (raw !== null) out.push(raw);
    }
  }
  // New properties go after the last property, before trailing blank lines or comments.
  let insertAt = out.length;
  while (insertAt > 0 && !out[insertAt - 1].trim()) insertAt--;
  out.splice(insertAt, 0, ...extra);
  return `---\n${out.map((l) => `${l}\n`).join('')}---\n`;
}

function blockChange(doc: string, fm: Frontmatter | null, text: string): TextChange {
  if (!fm) return { from: 0, to: 0, insert: text };
  const closingNewline = doc.slice(0, fm.end).endsWith('\n');
  return { from: 0, to: fm.end, insert: closingNewline ? text : text.slice(0, -1) };
}

function findIndex(fm: Frontmatter | null, key: string): number {
  return fm ? fm.properties.findIndex((p) => p.key === key) : -1;
}

/** Sets a property's value, adding the property when it does not exist. */
export function setProperty(
  doc: string,
  key: string,
  type: PropertyType,
  value: PropertyValue
): TextChange {
  const fm = parseFrontmatter(doc);
  const line = serializeProperty(key, type, value);
  const index = findIndex(fm, key);
  if (!fm) return blockChange(doc, null, `---\n${line}\n---\n`);
  if (index === -1) return blockChange(doc, fm, rebuild(fm, (i) => fm.properties[i].raw, [line]));
  return blockChange(doc, fm, rebuild(fm, (i) => (i === index ? line : fm.properties[i].raw)));
}

/** Renames a property and keeps its value and position. Returns null when the name is taken. */
export function renameProperty(doc: string, from: string, to: string): TextChange | null {
  const fm = parseFrontmatter(doc);
  const index = findIndex(fm, from);
  if (!fm || index === -1 || from === to) return null;
  if (findIndex(fm, to) !== -1) return null;
  const prop = fm.properties[index];
  const line = prop.editable
    ? serializeProperty(to, prop.type, prop.value)
    : prop.raw.replace(/^[^\n]*?:/, `${formatKey(to)}:`);
  return blockChange(doc, fm, rebuild(fm, (i) => (i === index ? line : fm.properties[i].raw)));
}

/** Removes a property. Removes the whole block when it was the last line in it. */
export function removeProperty(doc: string, key: string): TextChange | null {
  const fm = parseFrontmatter(doc);
  const index = findIndex(fm, key);
  if (!fm || index === -1) return null;
  const remaining = fm.layout.filter((item) => item !== index);
  if (remaining.every((item) => typeof item === 'string' && !item.trim())) {
    return { from: 0, to: fm.end, insert: '' };
  }
  return blockChange(doc, fm, rebuild(fm, (i) => (i === index ? null : fm.properties[i].raw)));
}

/** True when `key` can be written as a property name. */
export function isValidPropertyKey(key: string): boolean {
  return key.trim() !== '' && !/[\n\r]/.test(key) && !/^[#-]/.test(key.trim());
}
