import { invoke } from '@tauri-apps/api/core';
import type { BackdropState, ClipboardPayload, CopyRequest } from './types';

export async function copyToClipboard(req: CopyRequest): Promise<ClipboardPayload> {
  return invoke<ClipboardPayload>('copy_to_clipboard', { req });
}

export async function previewClipboardPayload(req: CopyRequest): Promise<ClipboardPayload> {
  return invoke<ClipboardPayload>('preview_clipboard_payload', { req });
}

export async function pastePlainFromClipboard(): Promise<string> {
  return invoke<string>('paste_plain_from_clipboard');
}

export async function syncWindowTheme(darkMode: boolean | null): Promise<BackdropState> {
  return invoke<BackdropState>('sync_window_theme', { darkMode });
}

export async function getBackdropState(): Promise<BackdropState> {
  return invoke<BackdropState>('get_backdrop_state');
}
