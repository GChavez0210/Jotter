/** Shared platform presentation for shortcuts and desktop capabilities. */
export function isMacPlatform(platform: string): boolean {
  return /mac/i.test(platform);
}

export function isLinuxPlatform(platform: string): boolean {
  // Android reports a Linux platform too, and it draws its own interface.
  return /linux/i.test(platform) && !/android/i.test(platform);
}

export const isMac = typeof navigator !== 'undefined' && isMacPlatform(navigator.platform);
export const isWindows = typeof navigator !== 'undefined' && /win/i.test(navigator.platform);
/** Linux desktops draw their own title bar, so Jotter leaves the window controls to them. */
export const isLinux = typeof navigator !== 'undefined' && isLinuxPlatform(navigator.platform);
export const defaultSummonShortcut = isMac ? 'Command+Alt+J' : 'Ctrl+Alt+J';
export const defaultCaptureShortcut = isMac ? 'Command+Alt+N' : 'Ctrl+Alt+N';

export function shortcutLabel(label: string, mac = isMac): string {
  return mac ? label.replace(/Ctrl/g, 'Cmd').replace(/Alt/g, 'Option') : label;
}
