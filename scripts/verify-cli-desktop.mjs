import { chromium, expect } from '@playwright/test';
import { spawn, execFile, spawnSync } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { setTimeout as pause } from 'node:timers/promises';

const exec = promisify(execFile);
const root = resolve(`src-tauri/target/cli-desktop-verification/${Date.now()}`);
const vault = join(root, 'vault');
await mkdir(vault, { recursive: true });
await writeFile(join(vault, 'Integration.md'), '# Integration\n\nInitial text.\n');
const env = { ...process.env, LOCALAPPDATA: join(root, 'app-data'), WEBVIEW2_USER_DATA_FOLDER: join(root, 'webview'), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9433' };
const desktop = resolve(process.env.JOTTER_TEST_EXE ?? 'src-tauri/target/release/jotter-desktop.exe');
const cli = resolve('src-tauri/target/release/jotter.exe');
const run = async (...args) => JSON.parse((await exec(cli, ['--vault', vault, ...args], { env, windowsHide: true, timeout: 15000 })).stdout);
const app = spawn(desktop, [], { env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
let logs = ''; app.stdout.on('data', d => logs += d); app.stderr.on('data', d => logs += d);
let browser, page;
const checks = [];
try {
  for (let i = 0; i < 60; i++) {
    if (app.exitCode !== null) throw new Error(`App exited: ${app.exitCode}. ${logs}`);
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:9433', { timeout: 1000 }); break; } catch { await pause(500); }
  }
  if (!browser) throw new Error('WebView2 did not expose the test endpoint');
  page = browser.contexts()[0].pages()[0];
  await page.waitForLoadState('domcontentloaded');
  await page.getByRole('button', { name: 'Enter path…' }).click();
  await page.getByLabel('Vault folder path').fill(vault);
  await page.getByRole('button', { name: 'Open folder', exact: true }).click();
  await page.getByRole('button', { name: 'Integration', exact: true }).click();
  const editor = page.locator('.cm-content');
  await expect(editor).toContainText('Initial text.');
  await run('append', 'Integration', '--text', 'CLI-CLEAN');
  await expect(editor).toContainText('CLI-CLEAN');
  checks.push('CLI append reloads a clean note in the native app');
  const cliVersions = await run('versions', 'Integration');
  const appVersions = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('list_note_versions', { relPath: 'Integration.md' }));
  expect(appVersions.map(v => v.id)).toEqual(cliVersions.map(v => v.id));
  expect(appVersions.length).toBeGreaterThan(0);
  checks.push('CLI snapshots appear in the native app under the same vault and note');

  // Leave enough time to inspect the dirty state before the real autosave timer fires.
  await page.evaluate(() => {
    window.__originalTimeout = window.setTimeout;
    window.setTimeout = (fn, ms, ...args) => window.__originalTimeout(fn, ms === 380 ? 3000 : ms, ...args);
  });
  await editor.click(); await page.keyboard.press('Control+End'); await page.keyboard.type('\nLOCAL-DIRTY');
  await run('append', 'Integration', '--text', 'CLI-DIRTY');
  const reload = page.getByRole('button', { name: 'Reload from disk', exact: true });
  await expect(reload).toBeVisible();
  await expect(editor).toContainText('LOCAL-DIRTY');
  await pause(3500);
  const diskBeforeChoice = await readFile(join(vault, 'Integration.md'), 'utf8');
  await writeFile(join(root, 'dirty-before-choice.md'), diskBeforeChoice);
  expect(diskBeforeChoice).toContain('CLI-DIRTY');
  expect(diskBeforeChoice).not.toContain('LOCAL-DIRTY');
  await reload.click();
  await expect(editor).toContainText('CLI-DIRTY');
  await expect(editor).not.toContainText('LOCAL-DIRTY');
  checks.push('A dirty note preserves disk changes until the user chooses Reload');

  await editor.click(); await page.keyboard.press('Control+End'); await page.keyboard.type('\nLOCAL-KEEP');
  await run('append', 'Integration', '--text', 'CLI-BEFORE-KEEP');
  await expect(reload).toBeVisible();
  await page.getByRole('button', { name: 'Keep my edits', exact: true }).click();
  await expect.poll(() => readFile(join(vault, 'Integration.md'), 'utf8')).toContain('LOCAL-KEEP');
  checks.push('Keep my edits explicitly resumes saving local edits');
  await page.evaluate(() => { window.setTimeout = window.__originalTimeout; });
  const requests = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18' } }, { jsonrpc: '2.0', id: 2, method: 'tools/list' }, { jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'read_note', arguments: { path: 'Integration' } } }];
  const mcp = spawnSync(cli, ['--vault', vault, '--read-only', 'mcp'], { env, windowsHide: true, encoding: 'utf8', input: requests.map(JSON.stringify).join('\n') + '\n', timeout: 15000 });
  expect(mcp.status).toBe(0); expect(mcp.stderr).toBe('');
  const replies = mcp.stdout.trim().split('\n').map(JSON.parse);
  expect(replies.length).toBe(3); expect(replies[1].result.tools.length).toBe(9); expect(replies[2].result.isError).toBe(false);
  checks.push('Read-only MCP responds with three JSON messages while the native app is open');
  await page.screenshot({ path: join(root, 'success.png') });
  await writeFile(join(root, 'results.json'), JSON.stringify({ root, checks }, null, 2));
  console.log(JSON.stringify({ root, checks }, null, 2));
} catch (error) {
  if (page && !page.isClosed()) {
    await page.screenshot({ path: join(root, 'failure.png') }).catch(() => {});
    await writeFile(join(root, 'failure.html'), await page.content()).catch(() => {});
  }
  await writeFile(join(root, 'failure.txt'), `${error.stack}\n${logs}`);
  throw error;
} finally {
  if (app.exitCode === null) app.kill();
  await browser?.close().catch(() => {});
  await writeFile(join(root, 'app.log'), logs);
}
