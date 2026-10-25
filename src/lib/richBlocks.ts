/**
 * Math (KaTeX) and diagram (Pele, a Mermaid renderer) rendering shared by Live Preview and
 * export. Both libraries load on first use, so notes without math or diagrams pay nothing.
 */

import { EXPORT_CALLOUT_CSS, enhanceCallouts } from './callouts';
import { EXPORT_FOOTNOTE_CSS } from './footnotes';
import { evaluateCalcBlock } from './calc';
import { i18n, m } from './i18n.svelte';

type KatexModule = typeof import('katex');
type PeleModule = typeof import('pele/lazy');

let katexPromise: Promise<KatexModule> | null = null;
let pelePromise: Promise<PeleModule> | null = null;

function loadKatex(): Promise<KatexModule> {
  katexPromise ??= Promise.all([
    import('katex'),
    import('katex/dist/katex.min.css'),
  ]).then(([mod]) => mod);
  return katexPromise;
}

function loadPele(): Promise<PeleModule> {
  pelePromise ??= import('pele/lazy');
  return pelePromise;
}

export type MathOutput = 'html' | 'mathml';

const mathCache = new Map<string, string>();
const diagramCache = new Map<string, string>();
const CACHE_LIMIT = 200;

function remember(cache: Map<string, string>, key: string, value: string): string {
  if (cache.size >= CACHE_LIMIT) {
    const oldest = cache.keys().next().value;
    if (oldest !== undefined) cache.delete(oldest);
  }
  cache.set(key, value);
  return value;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/** Cached render result, so a widget redrawn during scrolling skips the async step. */
export function cachedMath(tex: string, displayMode: boolean, output: MathOutput = 'html') {
  return mathCache.get(`${output}:${displayMode ? 'D' : 'I'}:${tex}`);
}

export function cachedDiagram(source: string) {
  return diagramCache.get(`${i18n.locale}:${source}`);
}

/** Renders TeX to HTML. Invalid TeX renders as a red error message rather than throwing. */
export async function renderMath(
  tex: string,
  displayMode: boolean,
  output: MathOutput = 'html'
): Promise<string> {
  const key = `${output}:${displayMode ? 'D' : 'I'}:${tex}`;
  const hit = mathCache.get(key);
  if (hit !== undefined) return hit;
  const katex = (await loadKatex()).default;
  const html = katex.renderToString(tex, {
    displayMode,
    output,
    throwOnError: false,
    strict: 'ignore',
    trust: false,
  });
  return remember(mathCache, key, html);
}

/** Renders Mermaid source to SVG. Invalid source renders as an error message with the line. */
export async function renderDiagram(source: string): Promise<string> {
  const locale = i18n.locale;
  const key = `${locale}:${source}`;
  const hit = diagramCache.get(key);
  if (hit !== undefined) return hit;
  let html: string;
  try {
    const pele = await loadPele();
    html = (await pele.renderAsync(source)).svg;
  } catch (err) {
    const where =
      err && typeof err === 'object' && 'line' in err && Number(err.line) > 0
        ? Number(err.line)
        : 0;
    const message = escapeHtml(err instanceof Error ? err.message : String(err));
    const text = where ? m.diagram_error_at({ line: where, message }, { locale }) : m.diagram_error({ message }, { locale });
    html = `<div class="jotter-render-error">${text}</div>`;
  }
  return remember(diagramCache, key, html);
}

/** Pele colors for exported files, which do not have Jotter's theme variables. */
function exportDiagramCss(dark: boolean): string {
  const c = dark
    ? { fg: '#f5f5f5', muted: '#adadad', line: '#cccccc', surface: '#2c2c2c', alt: '#363636', border: '#5c5c5c', accent: '#7ea0ff' }
    : { fg: '#1b1b1b', muted: '#595959', line: '#4f4f4f', surface: '#f5f5f5', alt: '#ebebeb', border: '#a8a8a8', accent: '#2f4fa8' };
  return `.pele{--pele-bg:var(--bg-card,transparent);--pele-fg:${c.fg};--pele-muted:${c.muted};--pele-line:${c.line};--pele-surface:${c.surface};--pele-surface-alt:${c.alt};--pele-border:${c.border};--pele-accent:${c.accent};}
.jotter-diagram{margin:1em 0;overflow-x:auto;}
.jotter-math-display{margin:1em 0;overflow-x:auto;}
.jotter-render-error{color:#c42b1c;font-family:monospace;white-space:pre-wrap;}`;
}

/**
 * Replaces comrak's math spans and `mermaid` code blocks in rendered note HTML with KaTeX and
 * SVG. `file` output uses MathML so the exported file needs no fonts or stylesheet; `print`
 * output uses KaTeX HTML, styled by the stylesheet the app already loaded.
 */
export async function enhanceRenderedHtml(
  html: string,
  target: 'file' | 'print',
  dark: boolean
): Promise<string> {
  const hasMath = html.includes('data-math-style');
  const hasDiagram = html.includes('language-mermaid');
  const hasCalc = html.includes('language-calc');
  const hasCallout = html.includes('[!');
  const hasFootnotes = html.includes('data-footnotes');
  if (!hasMath && !hasDiagram && !hasCalc && !hasCallout && !hasFootnotes) return html;

  const doc = new DOMParser().parseFromString(html, 'text/html');
  const output: MathOutput = target === 'file' ? 'mathml' : 'html';

  for (const el of Array.from(doc.querySelectorAll<HTMLElement>('[data-math-style]'))) {
    const display = el.dataset.mathStyle === 'display';
    const wrapper = doc.createElement(display ? 'div' : 'span');
    wrapper.className = display ? 'jotter-math-display' : 'jotter-math-inline';
    wrapper.innerHTML = await renderMath((el.textContent ?? '').trim(), display, output);
    // A display block that is a paragraph's only child replaces the whole paragraph.
    const parent = el.parentElement;
    if (display && parent?.tagName === 'P' && parent.childNodes.length === 1) {
      parent.replaceWith(wrapper);
    } else {
      el.replaceWith(wrapper);
    }
  }

  for (const code of Array.from(doc.querySelectorAll('pre > code.language-mermaid'))) {
    const figure = doc.createElement('div');
    figure.className = 'jotter-diagram';
    figure.innerHTML = await renderDiagram(code.textContent ?? '');
    code.parentElement?.replaceWith(figure);
  }

  // Calc blocks export with each result written after its line, as the editor shows them.
  for (const code of Array.from(doc.querySelectorAll('pre > code.language-calc'))) {
    const lines = (code.textContent ?? '').replace(/\n$/, '').split('\n');
    const width = Math.max(...lines.map((l) => l.length));
    const results = new Map(evaluateCalcBlock(lines).map((r) => [r.line, r.result]));
    code.textContent = lines
      .map((l, i) => (results.get(i) ? `${l.padEnd(width)}  = ${results.get(i)}` : l))
      .join('\n');
  }

  if (hasCallout) enhanceCallouts(doc, target);

  const style = doc.createElement('style');
  style.textContent = `${exportDiagramCss(dark)}\n${EXPORT_CALLOUT_CSS}\n${EXPORT_FOOTNOTE_CSS}`;
  doc.head.appendChild(style);

  return html.trimStart().toLowerCase().startsWith('<!doctype')
    ? `<!DOCTYPE html>\n${doc.documentElement.outerHTML}`
    : doc.body.innerHTML;
}
