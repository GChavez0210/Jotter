import { m } from './i18n.svelte';

/**
 * Obsidian-style callouts: a blockquote whose first line is `> [!type]`, optionally followed by
 * `+` (foldable, open) or `-` (foldable, closed) and a custom title.
 */

export type CalloutKind =
  | 'note' | 'abstract' | 'info' | 'todo' | 'tip' | 'success' | 'question'
  | 'warning' | 'failure' | 'danger' | 'bug' | 'example' | 'quote';

export interface CalloutMarker {
  /** The type as written, lowercased. Unknown types keep their own title and use the note style. */
  type: string;
  kind: CalloutKind;
  fold: '+' | '-' | null;
  /** Custom title text, or an empty string when the default title applies. */
  title: string;
  /** Length of the marker through the `]` and fold sign, excluding the title. */
  markerEnd: number;
}

export const CALLOUT_COLORS: Record<CalloutKind, string> = {
  note: '#448aff',
  abstract: '#00b0ff',
  info: '#00b8d4',
  todo: '#448aff',
  tip: '#00bfa5',
  success: '#00c853',
  question: '#e5a100',
  warning: '#ff9100',
  failure: '#ff5252',
  danger: '#ff1744',
  bug: '#f50057',
  example: '#7c4dff',
  quote: '#9e9e9e',
};

const ALIASES: Record<string, CalloutKind> = {
  summary: 'abstract', tldr: 'abstract',
  hint: 'tip', important: 'tip',
  check: 'success', done: 'success',
  help: 'question', faq: 'question',
  caution: 'warning', attention: 'warning',
  fail: 'failure', missing: 'failure',
  error: 'danger',
  cite: 'quote',
};

const MARKER_RE = /^(\s{0,3}>[ \t]?)\[!([A-Za-z][\w-]*)\]([+-])?/;

export function calloutKind(type: string): CalloutKind {
  const key = type.toLowerCase();
  if (key in CALLOUT_COLORS) return key as CalloutKind;
  return ALIASES[key] ?? 'note';
}

/** Parses the first line of a blockquote. Returns null when it does not open a callout. */
export function parseCalloutMarker(line: string): CalloutMarker | null {
  const match = MARKER_RE.exec(line);
  if (!match) return null;
  const rest = line.slice(match[0].length);
  // `[!note]text` without a space is ordinary text, not a title.
  if (rest !== '' && !/^[ \t]/.test(rest)) return null;
  const type = match[2].toLowerCase();
  return {
    type,
    kind: calloutKind(type),
    fold: (match[3] as '+' | '-' | undefined) ?? null,
    title: rest.trim(),
    markerEnd: match[0].length,
  };
}

/** Length of the `> ` prefix that opens a callout line, for hiding it in Live Preview. */
export function calloutPrefixLength(line: string): number {
  return MARKER_RE.exec(line)?.[1].length ?? 0;
}

/** The title shown for a callout: the custom one, else the localized default for its type. */
export function calloutTitle(marker: CalloutMarker): string {
  if (marker.title) return marker.title;
  const known = marker.type in CALLOUT_COLORS || marker.type in ALIASES;
  if (!known) return marker.type.charAt(0).toUpperCase() + marker.type.slice(1);
  switch (marker.kind) {
    case 'note': return m.callout_note();
    case 'abstract': return m.callout_abstract();
    case 'info': return m.callout_info();
    case 'todo': return m.callout_todo();
    case 'tip': return m.callout_tip();
    case 'success': return m.callout_success();
    case 'question': return m.callout_question();
    case 'warning': return m.callout_warning();
    case 'failure': return m.callout_failure();
    case 'danger': return m.callout_danger();
    case 'bug': return m.callout_bug();
    case 'example': return m.callout_example();
    case 'quote': return m.callout_quote();
  }
}

const EXPORT_MARKER_RE = /^\[!([A-Za-z][\w-]*)\]([+-])?(?:[ \t]+([^\n]*))?(?:\n|$)/;

/**
 * Turns callout blockquotes in rendered note HTML into styled boxes. Foldable callouts become
 * `<details>`; print output keeps them open so no content is lost on paper.
 */
export function enhanceCallouts(doc: Document, target: 'file' | 'print'): boolean {
  let changed = false;
  for (const quote of Array.from(doc.querySelectorAll('blockquote'))) {
    // Only top-level quotes, as in the editor's Live Preview.
    if (quote.parentElement?.closest('blockquote, li, .jotter-callout')) continue;
    const first = quote.firstElementChild;
    if (!first || first.tagName !== 'P') continue;
    const html = first.innerHTML;
    const match = EXPORT_MARKER_RE.exec(html);
    if (!match) continue;
    const marker = {
      type: match[1].toLowerCase(),
      kind: calloutKind(match[1]),
      fold: (match[2] as '+' | '-' | undefined) ?? null,
      title: match[3]?.trim() ?? '',
      markerEnd: 0,
    } satisfies CalloutMarker;

    const foldable = marker.fold !== null;
    const box = doc.createElement(foldable ? 'details' : 'div');
    box.className = `jotter-callout jotter-callout-${marker.kind}`;
    box.setAttribute('style', `--callout:${CALLOUT_COLORS[marker.kind]}`);
    if (foldable && (marker.fold === '+' || target === 'print')) box.setAttribute('open', '');

    const title = doc.createElement(foldable ? 'summary' : 'div');
    title.className = 'jotter-callout-title';
    if (marker.title) title.innerHTML = marker.title;
    else title.textContent = calloutTitle(marker);
    box.appendChild(title);

    const body = doc.createElement('div');
    body.className = 'jotter-callout-body';
    const remainder = html.slice(match[0].length);
    if (remainder.trim()) {
      const p = doc.createElement('p');
      p.innerHTML = remainder;
      body.appendChild(p);
    }
    first.remove();
    while (quote.firstChild) body.appendChild(quote.firstChild);
    if (body.childNodes.length) box.appendChild(body);
    quote.replaceWith(box);
    changed = true;
  }
  return changed;
}

/**
 * Title text color. The bright accent alone is too light on white and some are too dark on black,
 * so it is mixed toward the surrounding text color, which keeps 4.5:1 contrast in both themes.
 */
export const CALLOUT_TITLE_COLOR = 'color-mix(in srgb, var(--callout) 55%, currentColor)';

/** Colors and layout for callouts in exported files, which have no Jotter theme variables. */
export const EXPORT_CALLOUT_CSS = `.jotter-callout{margin:1em 0;padding:0;border-left:3px solid var(--callout);border-radius:6px;background:color-mix(in srgb,var(--callout) 10%,transparent);print-color-adjust:exact;-webkit-print-color-adjust:exact;font-style:normal;color:inherit;}
.jotter-callout-title{padding:8px 14px;font-weight:600;color:${CALLOUT_TITLE_COLOR};}
.jotter-callout-body{padding:0 14px 6px;}
.jotter-callout-body>:last-child{margin-bottom:0.4em;}
summary.jotter-callout-title{cursor:pointer;}`;
