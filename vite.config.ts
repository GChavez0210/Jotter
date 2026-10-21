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
