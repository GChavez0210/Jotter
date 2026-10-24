import { test } from 'node:test';
import assert from 'node:assert/strict';
import { build, transform } from 'esbuild';
import { compile, compileModule } from 'svelte/compiler';
import { readFile } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { chromium, expect } from '@playwright/test';

test('find stays below dialogs and failed panels can close without discarding dirty notes', async () => {
  const bundle = await build({
    stdin: { contents: "import { mount } from 'svelte'; import Harness from './tests/fixtures/ReviewHarness.svelte'; mount(Harness, {target: document.body});", resolveDir: resolve('.') },
    bundle: true, write: false, platform: 'browser', conditions: ['browser'], format: 'iife',
    plugins: [{ name: 'review-ui', setup(b) {
      b.onResolve({ filter: /\/ipc$/ }, () => ({ path: resolve('tests/ipc.mock.ts') }));
      b.onResolve({ filter: /clipboard\.svelte$/ }, () => ({ path: 'clipboard', namespace: 'review' }));
      b.onResolve({ filter: /^@tauri-apps\/api\/window$/ }, () => ({ path: resolve('tests/window.mock.ts') }));
      b.onLoad({ filter: /.*/, namespace: 'review' }, () => ({ contents: 'export const clipboardStore = {setStatus() {}};' }));
      b.onLoad({ filter: /\.svelte$/ }, async ({path}) => ({
        contents: compile(await readFile(path, 'utf8'), {filename: path, generate: 'client', css: 'injected'}).js.code,
        resolveDir: dirname(path), loader: 'js',
      }));
      b.onLoad({ filter: /\.svelte\.ts$/ }, async ({path}) => ({
        contents: compileModule((await transform(await readFile(path, 'utf8'), {loader:'ts'})).code, {filename:path, generate:'client'}).js.code,
        resolveDir: dirname(path), loader: 'js',
      }));
    }}],
  });
  const browser = await chromium.launch({ headless: true, channel: process.platform === 'win32' ? 'msedge' : undefined });
  try {
    const page = await browser.newPage();
    // Use an origin so the app's browser-preview storage fallback is available.
    await page.route('http://review.test/', route => route.fulfill({contentType:'text/html', body:'<!doctype html><html><body></body></html>'}));
    await page.goto('http://review.test/');
    await page.addScriptTag({content: bundle.outputFiles[0].text});
    await expect(page.locator('.cm-search')).toBeVisible();
    await page.getByRole('button', {name:'Open dialog', exact:true}).click();
    const above = await page.getByRole('button', {name:'Dialog action', exact:true}).evaluate(button => {
      const box = button.getBoundingClientRect();
      return button.contains(document.elementFromPoint(box.x + box.width/2, box.y + box.height/2));
    });
    assert.equal(above, true, 'editor search must not intercept dialog input');
    const layers = await page.evaluate(() => ({ search: +getComputedStyle(document.querySelector('.cm-panels')).zIndex, dialog: +getComputedStyle(document.querySelector('.scrim')).zIndex }));
    assert.ok(layers.search < layers.dialog);
    await page.getByRole('button', {name:'Dialog action', exact:true}).click();
    await page.getByRole('button', {name:'Fail optional panel', exact:true}).click();
    await expect(page.getByRole('alert')).toContainText('This panel could not load');
    let navigated = false;
    page.on('framenavigated', () => { navigated = true; });
    await page.getByRole('button', {name:'Reload Jotter', exact:true}).click();
    await expect(page.getByRole('alert')).toContainText('could not save your changes');
    assert.equal(navigated, false);
    await page.getByRole('dialog', {name:'Panel failed'}).getByRole('button', {name:'Close', exact:true}).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
  } finally { await browser.close(); }
});
