import { shortcutLabel } from '../platform';
import { previewClipboardPayload } from '../ipc';
import type { ClipboardPayload, CopyMode } from '../types';
import { COPY_MODE_LABELS } from './settings.svelte';
import { i18n, m } from '../i18n.svelte';

class ClipboardStore {
  lastPayload = $state<ClipboardPayload | null>(null);
  /** Empty until the first status, when the status bar shows a translated "Ready". */
  statusBanner = $state('');
  statusTimestamp = $state('');

  setStatus(msg: string) {
    this.statusBanner = msg;
    this.statusTimestamp = new Date().toLocaleTimeString(i18n.locale, {
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
    const details = {
      lines: m.line_count({ count: payload.lineCount }),
      chars: m.character_count({ count: payload.charCount }),
      mode: COPY_MODE_LABELS[payload.mode].label,
    };
    this.setStatus(source === 'cut' ? m.status_cut(details) : m.status_copied(details));
  }

  handlePastePlainComplete(charCount: number, lineCount: number) {
    this.setStatus(
      m.status_pasted_plain({
        lines: m.line_count({ count: lineCount }),
        chars: m.character_count({ count: charCount }),
        shortcut: shortcutLabel('Ctrl+Shift+V'),
      })
    );
  }
}

export const clipboardStore = new ClipboardStore();
