const cache = new Map<string, string>();
const MAX_BYTES = 16 * 1024 * 1024;
let bytes = 0;
let vault = '';
export let imagePreviewEpoch = 0;
export const IMAGE_REFRESH_EVENT = 'jotter-image-refresh';

export function invalidateImagePreviews(root = vault) {
  vault = root;
  cache.clear();
  bytes = 0;
  imagePreviewEpoch++;
  if (typeof window !== 'undefined') window.dispatchEvent(new Event(IMAGE_REFRESH_EVENT));
}

export function cachedImage(target: string) {
  return cache.get(`${vault}:${target}`);
}

export function rememberImage(target: string, value: string, epoch: number) {
  if (epoch !== imagePreviewEpoch || value.length * 2 > MAX_BYTES) return;
  const key = `${vault}:${target}`;
  bytes -= (cache.get(key)?.length ?? 0) * 2;
  cache.delete(key);
  while (bytes + value.length * 2 > MAX_BYTES || cache.size >= 100) {
    const oldest = cache.keys().next().value;
    if (oldest === undefined) break;
    bytes -= cache.get(oldest)!.length * 2;
    cache.delete(oldest);
  }
  cache.set(key, value);
  bytes += value.length * 2;
}
