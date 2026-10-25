import { mount } from 'svelte';
import { getCurrentWindow } from '@tauri-apps/api/window';
import '@fontsource-variable/instrument-sans';
import '@fontsource-variable/literata/opsz.css';
import '@fontsource-variable/literata/opsz-italic.css';
import '@fontsource-variable/jetbrains-mono';
import './app.css';
import { appStorage } from './lib/appStorage';

// Stores read their saved values when they load, so the settings file must be read first.
await appStorage.load();

// The quick capture window loads this same page and shows only its own small screen.
let isCaptureWindow = false;
try {
  isCaptureWindow = getCurrentWindow().label === 'capture';
} catch {
  // A browser preview has no windows.
}
const target = document.getElementById('app')!;
let app;
if (isCaptureWindow) {
  const [{ default: QuickCapture }, { settingsStore }] = await Promise.all([
    import('./lib/QuickCapture.svelte'),
    import('./lib/stores/settings.svelte'),
  ]);
  settingsStore.initAppearance();
  app = mount(QuickCapture, { target });
} else {
  const { default: App } = await import('./App.svelte');
  app = mount(App, { target });
}

export default app;
