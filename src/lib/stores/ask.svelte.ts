import { aiStore } from './ai.svelte';
import { settingsStore } from './settings.svelte';

/** Whether the Ask your vault panel is open. The panel keeps its own request state. */
class AskStore {
  show = $state(false);

  /** Opens the panel in place of any other dialog. */
  open() {
    settingsStore.showSettings = false;
    settingsStore.showCommandPalette = false;
    settingsStore.showQuickSwitcher = false;
    settingsStore.showTemplatePicker = false;
    settingsStore.openVersionHistory(null);
    aiStore.closePanel();
    this.show = true;
  }

  close() {
    this.show = false;
  }
}

export const askStore = new AskStore();
