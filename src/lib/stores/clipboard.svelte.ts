import { previewClipboardPayload } from '../ipc';
import type { ClipboardPayload, CopyMode } from '../types';
import { COPY_MODE_LABELS } from './settings.svelte';

class ClipboardStore {
  lastPayload = $state<ClipboardPayload | null>(null);
  statusBanner = $state('Ready');
  statusTimestamp = $state('');

  setStatus(msg: string) {
    this.statusBanner = msg;
    this.statusTimestamp = new Date().toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  }

  async refreshPreview(sampleText: string, mode: CopyMode) {
    try {
      this.lastPayload = await previewClipboardPayload({
        text: sampleText,
        mode,
      });
    } catch {
      // ignore in browser-only preview
    }
  }

  handleCopyComplete(payload: ClipboardPayload, source: 'copy' | 'cut') {
    this.lastPayload = payload;
    const verb = source === 'cut' ? 'Cut' : 'Copied';
    const modeLabel = COPY_MODE_LABELS[payload.mode].label;
    this.setStatus(
      `${verb} ${payload.lineCount} ${
        payload.lineCount === 1 ? 'line' : 'lines'
      } (${payload.charCount} chars) in ${modeLabel} mode`
    );
  }

  handlePastePlainComplete(charCount: number, lineCount: number) {
    this.setStatus(
      `Pasted ${lineCount} ${
        lineCount === 1 ? 'line' : 'lines'
      } (${charCount} chars) as plain text (Ctrl+Shift+V)`
    );
  }
}

export const clipboardStore = new ClipboardStore();
