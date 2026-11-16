import type { VaultNode } from './types';

/**
 * Note templates. A template is any Markdown note in the templates folder. These variables are
 * filled in when a note is created from it:
 *
 * - `{{date}}` is today's date as `YYYY-MM-DD`.
 * - `{{time}}` is the current time as `HH:MM`.
 * - `{{title}}` is the new note's name.
 * - `{{clipboard}}` is the clipboard's text.
 * - `{{text}}` is the text typed into quick capture.
 * - `{{prompt:Label}}` asks for a value, using `Label` as the field name.
 *
 * `{{#if name}}…{{else}}…{{/if}}` keeps one branch depending on whether a variable has text.
 * `name` is any variable above, and `!name` inverts the test. The `{{else}}` part is optional,
 * and blocks nest. Every variable and test reads from one set of values resolved up front, so
 * the same inputs always give the same note. A block with no closing `{{/if}}` stays as written.
 */

const PROMPT_RE = /\{\{\s*prompt\s*:\s*([^}]*?)\s*\}\}/gi;

const TOKEN_RE =
  /\{\{\s*(?:(date|time|title|clipboard|text)|prompt\s*:\s*([^}]*?)|#if\s+(!?)\s*(date|time|title|clipboard|text|prompt\s*:[^}]*?)|(else)|\/(if))\s*\}\}/gi;

export function formatIsoDate(d: Date): string {
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `${d.getFullYear()}-${mm}-${dd}`;
}

/** A timestamp for default note names, in local time: `YYYY-MM-DD-HH-MM-SS`. */
export function noteStamp(d = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${formatIsoDate(d)}-${pad(d.getHours())}-${pad(d.getMinutes())}-${pad(d.getSeconds())}`;
}

export interface TemplateValues {
  now: Date;
  title: string;
  /**
   * Answers to `{{prompt:Label}}` variables, by label. A missing answer fills in as empty.
   * Without this, prompts stay as written, as in the daily note, which has no form to ask.
   */
  prompts?: Record<string, string>;
  /** The clipboard's text. Without it, `{{clipboard}}` stays as written and tests read it as empty. */
  clipboard?: string;
  /** The quick capture entry. Without it, `{{text}}` stays as written and tests read it as empty. */
  text?: string;
}

/** The prompt labels a template asks for, in order of first use, without repeats. */
export function findTemplatePrompts(template: string): string[] {
  const labels: string[] = [];
  for (const match of template.matchAll(PROMPT_RE)) {
    const label = match[1];
    if (label && !labels.includes(label)) labels.push(label);
  }
  return labels;
}

/** Whether the template reads the clipboard, so callers can skip reading it when it does not. */
export function templateUsesClipboard(template: string): boolean {
  return /\{\{\s*(?:#if\s+!?\s*)?clipboard\s*\}\}/i.test(template);
}

type Subject = { kind: 'date' | 'time' | 'title' | 'clipboard' | 'text' } | { kind: 'prompt'; label: string };

type Node =
  | { type: 'text'; value: string }
  | { type: 'value'; subject: Subject; raw: string }
  | { type: 'if'; negate: boolean; subject: Subject; yes: Node[]; no: Node[] };

interface Frame {
  node: Extract<Node, { type: 'if' }>;
  inElse: boolean;
  open: string;
  elseRaw: string;
}

function subjectOf(name: string | undefined, label: string | undefined): Subject {
  if (label !== undefined) return { kind: 'prompt', label };
  return { kind: name!.toLowerCase() as 'date' };
}

function conditionSubject(raw: string): Subject {
  const prompt = /^prompt\s*:\s*([^]*?)\s*$/i.exec(raw);
  return prompt ? { kind: 'prompt', label: prompt[1] } : subjectOf(raw, undefined);
}

function parse(template: string): Node[] {
  const root: Node[] = [];
  const stack: Frame[] = [];
  const current = () => {
    const top = stack[stack.length - 1];
    return top ? (top.inElse ? top.node.no : top.node.yes) : root;
  };
  let last = 0;
  for (const match of template.matchAll(TOKEN_RE)) {
    const [all, name, label, negate, condition, otherwise, close] = match;
    if (match.index > last) current().push({ type: 'text', value: template.slice(last, match.index) });
    last = match.index + all.length;
    if (condition !== undefined) {
      const node = { type: 'if' as const, negate: negate === '!', subject: conditionSubject(condition), yes: [], no: [] };
      current().push(node);
      stack.push({ node, inElse: false, open: all, elseRaw: '' });
    } else if (otherwise !== undefined) {
      const top = stack[stack.length - 1];
      if (top && !top.inElse) {
        top.inElse = true;
        top.elseRaw = all;
      } else {
        current().push({ type: 'text', value: all });
      }
    } else if (close !== undefined) {
      if (stack.length > 0) stack.pop();
      else current().push({ type: 'text', value: all });
    } else {
      current().push({ type: 'value', subject: subjectOf(name, label), raw: all });
    }
  }
  if (last < template.length) current().push({ type: 'text', value: template.slice(last) });

  // A block that never closed stays as written, with its contents rendered in place.
  while (stack.length > 0) {
    const frame = stack.pop()!;
    const parent = current();
    const index = parent.indexOf(frame.node);
    const flat: Node[] = [{ type: 'text', value: frame.open }, ...frame.node.yes];
    if (frame.inElse) flat.push({ type: 'text', value: frame.elseRaw }, ...frame.node.no);
    parent.splice(index, 1, ...flat);
  }
  return root;
}

export function fillTemplate(template: string, values: TemplateValues): string {
  const { now, title, prompts, clipboard, text } = values;
  const time = `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}`;
  const date = formatIsoDate(now);

  /** The text a variable stands for, or `undefined` when this fill has no value for it. */
  const lookup = (subject: Subject): string | undefined => {
    switch (subject.kind) {
      case 'date': return date;
      case 'time': return time;
      case 'title': return title;
      case 'clipboard': return clipboard;
      case 'text': return text;
      case 'prompt': return prompts ? (prompts[subject.label] ?? '') : undefined;
    }
  };

  // One pass over the parsed template, so a typed answer that contains `{{date}}` or `{{#if}}`
  // stays as typed.
  const render = (nodes: Node[]): string =>
    nodes
      .map((node) => {
        if (node.type === 'text') return node.value;
        if (node.type === 'value') return lookup(node.subject) ?? node.raw;
        const has = (lookup(node.subject) ?? '').trim() !== '';
        return render(has !== node.negate ? node.yes : node.no);
      })
      .join('');
  return render(parse(template));
}

export function normalizeTemplatesFolder(folder: string): string {
  return folder.trim().replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
}

/** Notes inside the templates folder, including its subfolders, sorted by path. */
export function listTemplates(files: VaultNode[], folder: string): VaultNode[] {
  const prefix = normalizeTemplatesFolder(folder).toLowerCase();
  if (!prefix) return [];
  return files
    .filter((f) => f.relPath.toLowerCase().startsWith(`${prefix}/`) && /\.md$/i.test(f.relPath))
    .sort((a, b) => a.relPath.localeCompare(b.relPath));
}
