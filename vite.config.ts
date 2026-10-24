import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { paraglideVitePlugin } from '@inlang/paraglide-js';

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [
    svelte(),
    paraglideVitePlugin({
      project: './project.inlang',
      outdir: './src/lib/paraglide',
      strategy: ['globalVariable', 'baseLocale'],
      emitTsDeclarations: true,
    }),
  ],
  clearScreen: false,
  build: {
    target: 'esnext',
    minify: 'esbuild',
    cssMinify: true,
    rollupOptions: {
      output: {
        // Keep the editor engine and the message catalog out of the app chunk. Each changes
        // rarely, so a rebuilt app chunk leaves these cached, and no single file passes 500 kB.
        manualChunks(id) {
          if (/node_modules[\/]@codemirror[\/](view|state|language|commands|search|autocomplete)[\/]|node_modules[\/]@lezer[\/](common|highlight|lr)[\/]/.test(id)) return 'editor-engine';
          if (/node_modules[\/]@(codemirror|lezer)[\/]/.test(id)) return 'editor-languages';
          if (/src[\/]lib[\/]paraglide[\/]/.test(id)) return 'messages';
        },
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
});
