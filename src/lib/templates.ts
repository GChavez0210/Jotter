import type { VaultNode } from './types';

/**
 * Note templates. A template is any Markdown note in the templates folder. These variables are
 * filled in when a note is created from it:
 *
 * - `{{date}}` is today's date as `YYYY-MM-DD`.
 * - `{{time}}` is the current time as `HH:MM`.
 * - `{{title}}` is the new note's name.
 * - `{{prompt:Label}}` asks for a value, using `Label` as the field name.
 */

const PROMPT_RE = /\{\{\s*prompt\s*:\s*([^}]*?)\s*\}\}/gi;

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

export function fillTemplate(template: string, values: TemplateValues): string {
  const { now, title, prompts } = values;
  const time = `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}`;
  const date = formatIsoDate(now);
  // One pass, so a typed answer that contains `{{date}}` stays as typed.
  return template.replace(
    /\{\{\s*(?:(date|time|title)|prompt\s*:\s*([^}]*?))\s*\}\}/gi,
    (all, name: string | undefined, label: string | undefined) => {
      if (label !== undefined) return prompts ? (prompts[label] ?? '') : all;
      switch (name?.toLowerCase()) {
        case 'date': return date;
        case 'time': return time;
        default: return title;
      }
    }
  );
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
