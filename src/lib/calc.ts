/**
 * Arithmetic for inline `12*4 =` results and ```` ```calc ```` blocks.
 *
 * A small recursive-descent parser, so note text never reaches `eval`. Supports `+ - * / ^ %`,
 * parentheses, unary minus, `x% of y`, constants, a fixed set of functions, and variables.
 */

import { m } from './i18n.svelte';

const FUNCTIONS: Record<string, (...args: number[]) => number> = {
  sqrt: Math.sqrt,
  abs: Math.abs,
  round: (x, digits = 0) => {
    const f = 10 ** digits;
    return Math.round(x * f) / f;
  },
  floor: Math.floor,
  ceil: Math.ceil,
  min: Math.min,
  max: Math.max,
  sin: Math.sin,
  cos: Math.cos,
  tan: Math.tan,
  log: Math.log10,
  ln: Math.log,
  exp: Math.exp,
};

const CONSTANTS: Record<string, number> = { pi: Math.PI, e: Math.E };

export type Scope = Map<string, number>;

export class CalcError extends Error {}

type Token =
  | { kind: 'num'; value: number }
  | { kind: 'id'; name: string }
  | { kind: 'op'; op: string };

function tokenize(src: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  while (i < src.length) {
    const ch = src[i];
    if (/\s/.test(ch)) {
      i++;
      continue;
    }
    // `1,250.50` groups thousands with commas. A comma before fewer or more than three digits
    // separates function arguments, as in `max(1,2)`.
    const num = /^(?:\d{1,3}(?:,\d{3})+(?![\d,])(?:\.\d+)?|\d[\d_]*(?:\.\d+)?|\.\d+)(?:e[+-]?\d+)?/i.exec(
      src.slice(i)
    );
    if (num) {
      tokens.push({ kind: 'num', value: Number(num[0].replace(/[_,]/g, '')) });
      i += num[0].length;
      continue;
    }
    const id = /^[\p{L}_][\p{L}\p{N}_]*/u.exec(src.slice(i));
    if (id) {
      tokens.push({ kind: 'id', name: id[0] });
      i += id[0].length;
      continue;
    }
    const op = /^(\*\*|[-+*/^%(),×÷])/.exec(src.slice(i));
    if (!op) throw new CalcError(m.calc_unexpected({ token: ch }));
    const normalized = op[0] === '**' ? '^' : op[0] === '×' ? '*' : op[0] === '÷' ? '/' : op[0];
    tokens.push({ kind: 'op', op: normalized });
    i += op[0].length;
  }
  return tokens;
}

class Parser {
  private pos = 0;
  // Set by multiplicative() when the term it parsed was a lone `N%`.
  private termIsPercent = false;

  constructor(
    private readonly tokens: Token[],
    private readonly scope: Scope
  ) {}

  parse(): number {
    const value = this.additive();
    if (this.pos < this.tokens.length) throw new CalcError(m.calc_unexpected_input());
    return value;
  }

  private peek(): Token | undefined {
    return this.tokens[this.pos];
  }

  private isOp(op: string): boolean {
    const t = this.peek();
    return t?.kind === 'op' && t.op === op;
  }

  private isName(name: string): boolean {
    const t = this.peek();
    return t?.kind === 'id' && t.name.toLowerCase() === name;
  }

  private expect(op: string) {
    if (!this.isOp(op)) throw new CalcError(m.calc_expected({ token: op }));
    this.pos++;
  }

  private additive(): number {
    let left = this.multiplicative();
    while (this.isOp('+') || this.isOp('-')) {
      const op = (this.tokens[this.pos++] as { op: string }).op;
      let right = this.multiplicative();
      // `100 + 10%` adds ten percent of the left side, as calculators do.
      if (this.termIsPercent) right *= left;
      left = op === '+' ? left + right : left - right;
    }
    this.termIsPercent = false;
    return left;
  }

  private multiplicative(): number {
    let factors = 1;
    let percent = false;
    let left = this.factor((p) => (percent = p));
    for (;;) {
      const t = this.peek();
      let op: string;
      if (this.isOp('*') || this.isOp('/')) {
        op = (t as { op: string }).op;
        this.pos++;
      } else if ((t?.kind === 'id' && !this.isName('of')) || this.isOp('(')) {
        // Implicit multiplication: `2pi`, `3(4+1)`, `2 x` with a variable. Never `3 4`.
        op = '*';
      } else {
        break;
      }
      const right = this.factor(() => {});
      if (op === '/' && right === 0) throw new CalcError(m.calc_division_by_zero());
      left = op === '*' ? left * right : left / right;
      factors++;
    }
    this.termIsPercent = factors === 1 && percent;
    return left;
  }

  /** Unary signs, `^`, and a trailing `%` or `% of x`. */
  private factor(onPercent: (isPercent: boolean) => void): number {
    if (this.isOp('-')) {
      this.pos++;
      return -this.factor(onPercent);
    }
    if (this.isOp('+')) {
      this.pos++;
      return this.factor(onPercent);
    }
    let value = this.primary();
    if (this.isOp('^')) {
      this.pos++;
      value = value ** this.factor(() => {});
    }
    if (this.isOp('%')) {
      this.pos++;
      if (this.isName('of')) {
        this.pos++;
        return (value / 100) * this.factor(() => {});
      }
      onPercent(true);
      return value / 100;
    }
    return value;
  }

  private primary(): number {
    const t = this.peek();
    if (!t) throw new CalcError(m.calc_unexpected_end());
    this.pos++;
    if (t.kind === 'num') return t.value;
    if (t.kind === 'op') {
      if (t.op !== '(') throw new CalcError(m.calc_unexpected({ token: t.op }));
      const value = this.additive();
      this.expect(')');
      return value;
    }
    if (this.scope.has(t.name)) return this.scope.get(t.name)!;
    const lower = t.name.toLowerCase();
    const fn = FUNCTIONS[lower];
    if (fn && this.isOp('(')) {
      this.pos++;
      const args: number[] = [];
      if (!this.isOp(')')) {
        args.push(this.additive());
        while (this.isOp(',')) {
          this.pos++;
          args.push(this.additive());
        }
      }
      this.expect(')');
      return fn(...args);
    }
    if (lower in CONSTANTS) return CONSTANTS[lower];
    throw new CalcError(m.calc_unknown_name({ name: t.name }));
  }
}

/** Evaluates an expression. Throws `CalcError` on bad input or a non-finite result. */
export function evaluate(src: string, scope: Scope = new Map()): number {
  const tokens = tokenize(src);
  if (tokens.length === 0) throw new CalcError(m.calc_empty());
  const parser = new Parser(tokens, scope);
  const value = parser.parse();
  if (!Number.isFinite(value)) throw new CalcError(m.calc_not_finite());
  return value;
}

/** Formats a result with up to 10 significant digits and no float noise. */
export function formatNumber(value: number): string {
  if (Object.is(value, -0)) value = 0;
  if (Number.isInteger(value) && Math.abs(value) < 1e15) return String(value);
  const precise = Number(value.toPrecision(10));
  if (Math.abs(precise) >= 1e15 || (Math.abs(precise) < 1e-6 && precise !== 0)) {
    return precise.toExponential().replace(/\.?0+e/, 'e');
  }
  return String(precise);
}

/**
 * Finds an inline `expression =` at the end of a line and returns its result and where the
 * expression starts. The expression needs an operator or function, so `x = 5`, `a == b`, and
 * `Version 2 =` stay text. Takes the longest suffix that parses, so `Total: 12*4 =` gives 48.
 */
export function inlineCalculation(line: string): { from: number; result: string } | null {
  const trimmed = line.trimEnd();
  if (!trimmed.endsWith('=') || /[=<>!:]=$/.test(trimmed)) return null;
  const body = trimmed.slice(0, -1);
  // Only the tail after the last character that cannot appear in an expression.
  const tail = /[\d\s.+\-*/^%(),×÷\p{L}_]*$/u.exec(body);
  if (!tail || !tail[0].trim()) return null;
  const base = tail.index;
  const expr = tail[0].slice(-200);
  const exprBase = base + tail[0].length - expr.length;
  for (let offset = 0; offset < expr.length; offset++) {
    if (/\s/.test(expr[offset])) continue;
    // Start only at a word boundary, so `price` never yields `e`.
    if (offset > 0 && /[\p{L}\p{N}_.]/u.test(expr[offset - 1])) continue;
    // Nor in the middle of a number such as `1,000`, which would give a wrong result.
    if (offset > 1 && expr[offset - 1] === ',' && /\d/.test(expr[offset - 2])) continue;
    // Nor inside a hyphenated run such as `2026-10-19`.
    if (offset > 1 && expr[offset - 1] === '-' && /\d/.test(expr[offset - 2])) continue;
    const candidate = expr.slice(offset);
    // A sign followed by a space is a list marker or prose dash, not a negative number.
    if (/^[-+]\s/.test(candidate)) continue;
    if (!/[-+*/^%×÷(]/.test(candidate.replace(/^-/, ''))) return null;
    try {
      const value = evaluate(candidate);
      return { from: exprBase + offset, result: formatNumber(value) };
    } catch {
      // Try a shorter suffix.
    }
  }
  return null;
}

export interface CalcLineResult {
  /** 0-based line index inside the block. */
  line: number;
  result?: string;
  error?: string;
}

/**
 * Evaluates the lines of a calc block in order. `name = expr` assigns a variable, any other
 * non-empty line is an expression, and `#` or `//` starts a comment. `sum` and `total` hold the
 * sum of the results since the last blank line; `prev` holds the previous result.
 */
export function evaluateCalcBlock(lines: string[]): CalcLineResult[] {
  const scope: Scope = new Map();
  const results: CalcLineResult[] = [];
  // A variable the user assigns keeps its value; the built-in of the same name steps aside.
  const assigned = new Set<string>();
  let runningSum = 0;
  for (let i = 0; i < lines.length; i++) {
    if (!lines[i].trim()) {
      runningSum = 0;
      continue;
    }
    const text = lines[i].replace(/(?:#|\/\/).*$/, '').trim();
    if (!text) continue;
    const assignment = /^([\p{L}_][\p{L}\p{N}_]*)\s*=\s*(.+)$/u.exec(text);
    const expr = assignment ? assignment[2] : text;
    if (!assigned.has('sum')) scope.set('sum', runningSum);
    if (!assigned.has('total')) scope.set('total', runningSum);
    try {
      const value = evaluate(expr, scope);
      if (assignment) {
        scope.set(assignment[1], value);
        assigned.add(assignment[1]);
      }
      if (!assigned.has('prev')) scope.set('prev', value);
      runningSum += value;
      results.push({ line: i, result: formatNumber(value) });
    } catch (err) {
      results.push({ line: i, error: err instanceof Error ? err.message : String(err) });
    }
  }
  return results;
}
