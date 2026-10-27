import { test } from 'node:test';
import assert from 'node:assert/strict';
import { build, transform } from 'esbuild';
import { compile, compileModule } from 'svelte/compiler';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { chromium, expect } from '@playwright/test';
import { createRequire } from 'node:module';

/** Edge is the WebView2 engine these checks ship on; other platforms use bundled Chromium. */
function launchBrowser() {
  return chromium.launch({ headless: true, channel: process.platform === 'win32' ? 'msedge' : undefined });
}

/** Bundles a harness component with the backend mocked, as the review tests run it. */
async function bundleHarness(fixture) {
  return build({
    stdin: {
      contents: `import { mount } from 'svelte'; import Harness from './tests/fixtures/${fixture}.svelte'; mount(Harness, {target: document.body});`,
      resolveDir: resolve('.'),
    },
    bundle: true, write: false, platform: 'browser', conditions: ['browser'], format: 'iife',
    // Styles ride along in the compiled components; library CSS and its fonts are not under test.
    loader: { '.css': 'empty', '.woff': 'empty', '.woff2': 'empty', '.ttf': 'empty', '.png': 'dataurl' },
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
}

/** Opens a page with the bundle on an origin, so the browser-preview storage fallback works. */
async function openHarness(browser, bundle) {
  const page = await browser.newPage();
  // The real page has a title and a language; the harness serves the same shell.
  await page.route('http://review.test/', route => route.fulfill({contentType:'text/html', body:'<!doctype html><html lang="en"><head><title>Jotter</title></head><body></body></html>'}));
  await page.goto('http://review.test/');
  // Component styles are compiled in, but the tokens and base rules come from real stylesheets.
  for (const sheet of ['src/styles/design-tokens.css', 'src/styles/picker.css', 'src/app.css']) {
    const css = await readFile(resolve(sheet), 'utf8');
    await page.addStyleTag({content: css.replace(/^@import[^;]+;$/gm, '')});
  }
  await page.addScriptTag({content: bundle.outputFiles[0].text});
  await page.addStyleTag({content: '*,*::before,*::after { transition:none !important; animation:none !important; }'});
  return page;
}


/** Runs axe over the page and returns its violations, worst first. */
async function auditPage(page, context) {
  const require = createRequire(import.meta.url);
  const axeSource = await readFile(require.resolve('axe-core'), 'utf8');
  await page.evaluate(axeSource);
  const result = await page.evaluate(async () => {
    // While a dialog is open the page behind it is inert and dimmed by the scrim, so the
    // scan follows the user and looks at the dialog only.
    const target = document.querySelector('[role="dialog"]') ?? document;
    // WCAG 2.2 AA, plus the best-practice rules for names and roles.
    const run = await window.axe.run(target, {
      runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'] },
      resultTypes: ['violations'],
    });
    return run.violations.map((violation) => ({
      id: violation.id,
      impact: violation.impact,
      help: violation.help,
      nodes: violation.nodes.slice(0, 4).map((node) => node.html.slice(0, 160)),
      // The checks' own summary says what failed, such as the contrast ratio it measured.
      why: violation.nodes.slice(0, 4).map((node) => node.failureSummary?.replace(/\s+/g, ' ').slice(0, 200)),
    }));
  });
  return result.map((violation) => ({ ...violation, context }));
}

test('find stays below dialogs and failed panels can close without discarding dirty notes', async () => {
  const bundle = await bundleHarness('ReviewHarness');
  const browser = await launchBrowser();
  try {
    const page = await openHarness(browser, bundle);
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

test('split view gives each pane its own text, cursor and scroll position', async () => {
  const bundle = await bundleHarness('SplitHarness');
  const browser = await launchBrowser();
  try {
    const page = await openHarness(browser, bundle);
    const panes = page.locator('.cm-content');
    await expect(panes).toHaveCount(2);

    // Each pane shows its own note, titled by its own header.
    await expect(page.getByRole('heading', {name: 'Left', exact: true})).toBeVisible();
    await expect(page.getByRole('heading', {name: 'Right', exact: true})).toBeVisible();

    // Typing in the second pane edits that note only, and makes it the active pane.
    await panes.nth(1).click();
    await page.keyboard.press('Control+Home');
    await page.keyboard.type('typed ');
    await expect(page.locator('#state')).toHaveText(/^secondary\|Left line 1\|/);
    await expect(page.locator('#state')).toHaveText(/\|typed Right line 1$/);

    // Clicking back into the first pane hands it the commands again.
    await panes.nth(0).click();
    await expect(page.locator('#state')).toHaveText(/^primary\|/);

    // The panes scroll on their own.
    const scrollers = page.locator('[data-editor-scroll]');
    const before = await scrollers.evaluateAll((els) => els.map((el) => el.scrollTop));
    await scrollers.nth(0).evaluate((el) => { el.scrollTop = 200; });
    const after = await scrollers.evaluateAll((els) => els.map((el) => el.scrollTop));
    assert.equal(after[0], 200, 'the first pane should have scrolled where it was told');
    assert.notEqual(before[0], after[0], 'the first pane should have moved');
    assert.equal(after[1], before[1], 'the second pane must not follow the first');
  } finally { await browser.close(); }
});


test('the app has no accessibility violations in its main states', async () => {
  const bundle = await bundleHarness('AppHarness');
  const browser = await launchBrowser();
  try {
    const page = await openHarness(browser, bundle);
    await expect(page.locator('.cm-content')).toHaveCount(1);
    const found = [];

    for (const theme of ['light', 'dark', 'high-contrast']) {
      await page.evaluate((value) => {
        window.__review.settingsStore.closeSplit();
        document.documentElement.setAttribute('data-theme', value);
      }, theme);
      found.push(...await auditPage(page, `${theme}: editor`));
    await page.keyboard.press('Control+p');
    await expect(page.getByPlaceholder('Type a command')).toBeVisible();
    found.push(...await auditPage(page, `${theme}: command palette`));
    await page.keyboard.press('Escape');

    await page.keyboard.press('Control+,');
    await expect(page.getByRole('dialog', {name: 'Settings'})).toBeVisible();
    for (const section of ['Editor', 'Appearance', 'Vault', 'System']) {
      await page.getByRole('dialog', {name: 'Settings'}).getByRole('button', {name: section, exact: true}).click();
      found.push(...await auditPage(page, `${theme}: settings: ${section}`));
    }
    await page.keyboard.press('Escape');

    await page.getByRole('button', {name: 'Split view', exact: true}).click();
    await expect(page.getByRole('region', {name: 'Second pane'})).toBeVisible();
    found.push(...await auditPage(page, `${theme}: split view`));

    await page.getByRole('button', {name: 'Version history', exact: true}).first().click();
    await expect(page.getByRole('dialog', {name: /Version history/})).toBeVisible();
    found.push(...await auditPage(page, `${theme}: version history`));
    await page.keyboard.press('Escape');

    }

    // The list is the audit's own output; keep it for reading when it is not empty.
    await writeFile(resolve('src-tauri/target/a11y-violations.json'), JSON.stringify(found, null, 2));
    assert.deepEqual(found, [], `accessibility violations:\n${JSON.stringify(found, null, 2)}`);
  } finally { await browser.close(); }
});

test('the keyboard reaches every control and stays inside an open dialog', async () => {
  const bundle = await bundleHarness('AppHarness');
  const browser = await launchBrowser();
  try {
    const page = await openHarness(browser, bundle);
    await expect(page.locator('.cm-content')).toHaveCount(1);

    // Tab stays inside a dialog, so focus never lands on the page the scrim covers.
    await page.keyboard.press('Control+p');
    await expect(page.getByPlaceholder('Type a command')).toBeFocused();
    const insideDialog = async () =>
      page.evaluate(() => Boolean(document.querySelector('[role="dialog"]')?.contains(document.activeElement)));
    for (let i = 0; i < 12; i++) {
      await page.keyboard.press('Tab');
      assert.equal(await insideDialog(), true, `Tab ${i + 1} left the dialog`);
    }
    for (let i = 0; i < 4; i++) {
      await page.keyboard.press('Shift+Tab');
      assert.equal(await insideDialog(), true, `Shift+Tab ${i + 1} left the dialog`);
    }
    await page.keyboard.press('Escape');
    await expect(page.getByRole('dialog')).toHaveCount(0);

    // Settings is a panel dialog with its own navigation, and it holds focus the same way.
    await page.keyboard.press('Control+,');
    await expect(page.getByRole('dialog', {name: 'Settings'})).toBeVisible();
    assert.equal(await insideDialog(), true, 'the dialog did not take focus when it opened');
    for (let i = 0; i < 25; i++) {
      await page.keyboard.press('Tab');
      assert.equal(await insideDialog(), true, `Tab ${i + 1} left Settings`);
    }
    await page.keyboard.press('Escape');

    // Both window splitters move from the keyboard, which WCAG 2.2 asks of a drag target.
    const sidebarWidth = () => page.locator('.sidebar, aside').first().evaluate(el => el.getBoundingClientRect().width);
    const sidebarHandle = page.getByRole('separator', {name: 'Resize sidebar'});
    await sidebarHandle.focus();
    const beforeSidebar = await sidebarWidth();
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('ArrowRight');
    await expect.poll(sidebarWidth).toBeGreaterThan(beforeSidebar);
    await page.keyboard.press('Home');
    await expect.poll(sidebarWidth).toBe(beforeSidebar);

    await page.getByRole('button', {name: 'Split view', exact: true}).click();
    const paneWidth = () => page.locator('.editor-column').first().evaluate(el => el.getBoundingClientRect().width);
    const sash = page.getByRole('separator', {name: 'Resize the panes'});
    await sash.focus();
    const beforePane = await paneWidth();
    await page.keyboard.press('ArrowLeft');
    await page.keyboard.press('ArrowLeft');
    await expect.poll(paneWidth).toBeLessThan(beforePane);
  } finally { await browser.close(); }
});


test('split commands, close, history, and keyboard targets follow the focused note', async () => {
  const browser = await launchBrowser();
  try {
    const page = await openHarness(browser, await bundleHarness('AppHarness'));
    await expect(page.locator('.cm-content')).toHaveCount(1);
    await page.keyboard.press('Control+Shift+Backslash');
    await expect(page.locator('.cm-content')).toHaveCount(2);
    const right = page.getByRole('region', {name:'Second pane'});
    await right.locator('.cm-content').click();
    await page.keyboard.press('Control+p');
    await page.getByPlaceholder('Type a command').fill('Version History');
    await page.keyboard.press('Enter');
    await expect(page.getByRole('dialog')).toHaveAccessibleName('Version history of Journal');
    await page.keyboard.press('Escape');
    await right.locator('.cm-content').click();
    await page.keyboard.press('Control+w');
    await expect(page.getByRole('tab', {name:'Audit', exact:true})).toBeVisible();
    await expect(page.getByRole('tab', {name:'Journal', exact:true})).toHaveCount(0);
    // Delete closes the named tab even when it is not the selected note.
    await page.getByRole('tab', {name:'Audit', exact:true}).focus();
    await page.keyboard.press('Delete');
    await expect(page.getByRole('tab')).toHaveCount(0);
  } finally { await browser.close(); }
});

test('a failed flush preserves edits and never invokes restore; successful restore diff points forward', async () => {
  const browser = await launchBrowser();
  try {
    const page = await openHarness(browser, await bundleHarness('AppHarness'));
    await expect(page.locator('.cm-content')).toHaveCount(1);
    await page.evaluate(() => {
      const {tabsStore, control} = window.__review;
      tabsStore.handleEditorDocChange('Work/Audit.md', '# Pending unsaved edit\n');
      control.save = async () => {throw Error('disk full');};
    });
    await page.getByRole('button', {name:'Version history',exact:true}).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.locator('.row.del')).toContainText('Pending unsaved edit');
    await expect(dialog.locator('.row.add')).toContainText('Earlier version');
    await dialog.getByRole('button',{name:'Restore this version'}).click();
    await expect(dialog.getByRole('alert')).toContainText('current edits could not be saved');
    assert.deepEqual(await page.evaluate(()=>window.__review.versionControl.restores), []);
    assert.equal(await page.evaluate(()=>window.__review.tabsStore.activeTab.content), '# Pending unsaved edit\n');
    assert.equal(await page.evaluate(()=>window.__review.tabsStore.activeTab.dirty), true);
    await page.evaluate(()=>{window.__review.control.save=async(path,content)=>({relPath:path,title:path,content});});
    await dialog.getByRole('button',{name:'Restore this version'}).click();
    await expect(page.locator('.cm-content')).toContainText('Earlier version');
    assert.deepEqual(await page.evaluate(()=>window.__review.versionControl.restores), ['Work/Audit.md']);
  } finally { await browser.close(); }
});

test('hidden and preview panes print the visible note, and stats follow focus without typing', async () => {
  const browser = await launchBrowser();
  try {
    const page = await openHarness(browser, await bundleHarness('AppHarness'));
    await expect(page.locator('.cm-content')).toHaveCount(1);
    await page.evaluate(()=>{window.__review.settingsStore.setWindowWidth(1600);window.__review.settingsStore.sidebarCollapsed=true;window.print=()=>{};});
    await page.getByRole('button',{name:'Split view',exact:true}).click();
    const editors=page.locator('.cm-content');
    await editors.nth(1).focus();
    await expect(page.getByRole('navigation',{name:'Location'})).toContainText('Journal');
    const rightWords=await page.evaluate(()=>window.__review.tabsStore.docStats.words);
    await editors.nth(0).focus();
    await expect.poll(()=>page.evaluate(()=>window.__review.tabsStore.docStats.words)).not.toBe(rightWords);
    await editors.nth(1).focus();
    await page.evaluate(()=>window.__review.settingsStore.setWindowWidth(650));
    await page.keyboard.press('Control+Shift+p');
    await expect(page.locator('.jotter-print-view')).toContainText('Audit');
    await page.evaluate(()=>{window.__review.settingsStore.setWindowWidth(1600);window.__review.settingsStore.openSplitPreview();});
    await page.getByRole('button',{name:'Close the second pane'}).focus();
    await page.keyboard.press('Control+Shift+p');
    await expect(page.locator('.jotter-print-view')).toContainText('Audit');
    await expect(page.locator('.jotter-print-view')).not.toContainText('Scratchpad');
  } finally { await browser.close(); }
});

test('the right editor offers reload for dirty external changes and receives heading navigation', async () => {
  const browser = await launchBrowser();
  try {
    const page = await openHarness(browser, await bundleHarness('AppHarness'));
    await expect(page.locator('.cm-content')).toHaveCount(1);
    await page.getByRole('button',{name:'Split view',exact:true}).click();
    await page.evaluate(async()=>{await window.__review.tabsStore.handleExternalPathsChanged(['Journal.md']);});
    const banner=page.locator('.external-reload-banner');
    await expect(banner).toContainText('Journal.md');
    await banner.getByRole('button',{name:'Reload from disk',exact:true}).click();
    await expect(page.getByRole('region',{name:'Second pane'}).locator('.cm-content')).toHaveText('disk');
    await page.evaluate(async()=>{
      const {tabsStore,control,settingsStore}=window.__review;
      settingsStore.setActivePane('secondary');
      control.read=async(path)=>({relPath:path,title:path,content:'# Top\n\nBody\n\n## Destination\nTarget'});
      await tabsStore.openNoteInTab('Target.md',{heading:'Destination'});
    });
    await expect.poll(()=>page.evaluate(()=>window.__review.tabsStore.docStats.cursorLine)).toBe(5);
  } finally { await browser.close(); }
});
