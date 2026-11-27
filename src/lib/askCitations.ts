import type { AskSource } from './types';

export interface CitedSource extends AskSource {
  /** The label the chat model cites, `path:line`. */
  location: string;
  number: number;
}

export type AnswerPart = string | CitedSource;

/** The prompt XML-escapes passage paths, so a model may cite the escaped spelling. */
const escaped = (text: string) =>
  text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&apos;');

/**
 * Splits an answer at its `[path:line]` citations. Sources are numbered in the order the answer
 * cites them; passages it never cites follow, because they were sent too. Brackets that hold
 * anything but known sources stay as text.
 */
export function citeAnswer(answer: string, sources: AskSource[]): { parts: AnswerPart[]; sources: CitedSource[] } {
  const found = new Map<string, AskSource>();
  for (const source of sources) {
    const location = `${source.chunk.path}:${source.chunk.line}`;
    if (!found.has(location)) found.set(location, source);
  }
  // Longest first, so `a.md:12` wins over `a.md:1`.
  const spellings = [...found.keys()]
    .flatMap((location) => [...new Set([location, escaped(location)])].map((text) => ({ text, location })))
    .sort((a, b) => b.text.length - a.text.length);

  const numbered = new Map<string, CitedSource>();
  const number = (location: string) => {
    let source = numbered.get(location);
    if (!source) {
      source = { ...found.get(location)!, location, number: numbered.size + 1 };
      numbered.set(location, source);
    }
    return source;
  };

  /** The citations of the bracket that opens at `start`, if it holds only known sources. */
  const bracket = (start: number) => {
    const locations: string[] = [];
    let at = start + 1;
    for (;;) {
      const match = spellings.find((spelling) => answer.startsWith(spelling.text, at));
      if (!match) return null;
      locations.push(match.location);
      at += match.text.length;
      if (answer[at] === ']') return { locations, end: at + 1 };
      const separator = /^\s*[,;]\s*/.exec(answer.slice(at));
      if (!separator) return null;
      at += separator[0].length;
    }
  };

  const parts: AnswerPart[] = [];
  let copied = 0;
  for (let at = answer.indexOf('['); at >= 0; at = answer.indexOf('[', at + 1)) {
    const cited = bracket(at);
    if (!cited) continue;
    if (at > copied) parts.push(answer.slice(copied, at));
    parts.push(...cited.locations.map(number));
    copied = cited.end;
    at = cited.end - 1;
  }
  if (copied < answer.length) parts.push(answer.slice(copied));

  for (const location of found.keys()) number(location);
  return { parts, sources: [...numbered.values()] };
}
