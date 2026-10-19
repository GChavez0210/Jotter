/**
 * Shared IPC types mirroring `src-tauri/src/types.rs`.
 */

export type CopyMode = 'plain' | 'rich' | 'markdown_source';

export interface CopyRequest {
  text: string;
  mode: CopyMode;
}

export interface ClipboardPayload {
  plain: string;
  html: string;
  mode: CopyMode;
  lineCount: number;
  charCount: number;
}

export interface BackdropState {
  micaApplied: boolean;
  backdropKind: string;
  darkMode: boolean | null;
}

export type ThemePreference = 'system' | 'light' | 'dark';
