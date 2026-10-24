import { mount } from 'svelte';
import '@fontsource-variable/instrument-sans';
import '@fontsource-variable/literata/opsz.css';
import '@fontsource-variable/literata/opsz-italic.css';
import '@fontsource-variable/jetbrains-mono';
import './app.css';
import { appStorage } from './lib/appStorage';

// Stores read their saved values when they load, so the settings file must be read first.
await appStorage.load();
const { default: App } = await import('./App.svelte');

const app = mount(App, {
  target: document.getElementById('app')!,
});

export default app;
