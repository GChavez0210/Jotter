import { isLinux } from './platform';

export function notePathKey(path: string, caseSensitive = isLinux): string {
  const normalized = path.replace(/\\/g, '/');
  return caseSensitive ? normalized : normalized.toLowerCase();
}

export function sameNotePath(a: string | null, b: string | null, caseSensitive = isLinux): boolean {
  return a !== null && b !== null && notePathKey(a, caseSensitive) === notePathKey(b, caseSensitive);
}
