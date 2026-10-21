import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, writeFile, readFile, chmod } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { setTimeout as pause } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This verification harness requires Windows WebView2. Test other platforms manually.');

// Run against the built app with isolated WebView storage, app data, and disposable notes.
const root = resolve(`src-tauri/target/desktop-verification/${Date.now()}`);
const vault = join(root, 'vault');
await mkdir(vault, {recursive:true});
await mkdir(join(root, 'app-data/Jotter/snippets'), {recursive:true});
await writeFile(join(root, 'app-data/Jotter/snippets/verify.css'), ':root { --verify-snippet: loaded; }\n.cm-content { line-height: 1.8; }');
const note = '# Verification\n\nWe receive the budget.\n\nA mispeled word.\n\nInline $x^2$.\n\n$$ x^2 + y^2 $$\n\n```mermaid\ngraph TD; A-->B\n```\n\n> ```mermaid\n> graph TD; C-->D\n> ```\n\n    $$ indented code $$\n';
await writeFile(join(vault,'Verification.md'),note);
await mkdir(join(vault,'Templates'),{recursive:true});
await writeFile(join(vault,'Templates/Daily.md'),'# {{title}}\nDate: {{date}}\n');
const port = 9432;
const app = spawn(resolve(process.env.JOTTER_TEST_EXE ?? 'src-tauri/target/release/jotter.exe'),[],{
  windowsHide:true,
  env:{...process.env,LOCALAPPDATA:join(root,'app-data'),WEBVIEW2_USER_DATA_FOLDER:join(root,'webview'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`},
  stdio:['ignore','pipe','pipe'],
});
let logs = '';
app.stdout.on('data',(data)=>logs+=data);
app.stderr.on('data',(data)=>logs+=data);
const checks = [];
let browser;
let page;
try {
  for(let i=0;i<60;i++) {
    if(app.exitCode !== null) throw Error(`App exited with ${app.exitCode}: ${logs}`);
    try {browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`,{timeout:1000});break;} catch {await pause(500);}
  }
  if(!browser) throw Error(`WebView2 debugging endpoint unavailable: ${logs}`);
  page = browser.contexts()[0].pages()[0];
  const errors=[];
  page.on('pageerror',(error)=>errors.push(String(error)));
  page.on('console',(message)=>{if(message.type()==='error') errors.push(message.text());});
  await page.waitForLoadState('domcontentloaded');
  await expect(page.getByRole('button',{name:'Enter path…'})).toBeVisible();
  await page.evaluate(()=>{
    window.__verifyCsp=[];
    document.addEventListener('securitypolicyviolation',(event)=>window.__verifyCsp.push(`${event.violatedDirective}: ${event.blockedURI}`));
  });
  await page.getByRole('button',{name:'Enter path…'}).click();
  await page.getByLabel('Vault folder path').fill(vault);
  await page.getByRole('button',{name:'Open folder',exact:true}).click();
  await page.getByRole('button',{name:'Verification',exact:true}).click();
  if (process.env.JOTTER_CLOSE_ONLY) {
    await page.locator('.cm-content').click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('\nIMMEDIATE-NATIVE-CLOSE');
    await page.evaluate(()=>{void window.__TAURI_INTERNALS__.invoke('plugin:window|close',{label:'main'});});
    for(let i=0;i<40 && app.exitCode===null;i++) await pause(250);
    expect(app.exitCode).toBe(0);
    expect(await readFile(join(vault,'Verification.md'),'utf8')).toContain('IMMEDIATE-NATIVE-CLOSE');
    checks.push('Nonresident native close saves a last-moment edit before exiting');
    expect(errors).toEqual([]);
    await writeFile(join(root,'results.json'),JSON.stringify({checks,errors},null,2));
    console.log(JSON.stringify({root,checks},null,2));
  } else {
  await expect(page.locator('.cm-diagram-block svg')).toHaveCount(2);
  await expect(page.locator('.cm-math-block .katex')).toHaveCount(1);
  await expect(page.locator('.cm-math-inline .katex')).toHaveCount(1);
  checks.push('Real WebView2: math and top-level/quoted Mermaid render under CSP');
  await page.screenshot({path:join(root,'live-preview.png')});

  // Exercise the real OS Recycle Bin with a disposable fixture only.
  await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('create_note',{relPath:'RestoreProbe.md',initialContent:'restore fixture'}));
  await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('delete_entry',{relPath:'RestoreProbe.md'}));
  const deleted = await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('list_deleted_entries'));
  const item = deleted.find((item)=>item.originalRelPath==='RestoreProbe.md');
  expect(item).toBeTruthy();
  await page.evaluate((id)=>window.__TAURI_INTERNALS__.invoke('restore_deleted_entry',{id}),item.id);
  expect(await readFile(join(vault,'RestoreProbe.md'),'utf8')).toBe('restore fixture');
  checks.push('Real Windows Recycle Bin lists and restores a disposable note to its original path');

  await page.keyboard.press('Control+Shift+d');
  const date = new Date();
  const today = `${date.getFullYear()}-${String(date.getMonth()+1).padStart(2,'0')}-${String(date.getDate()).padStart(2,'0')}`;
  await expect(page.locator('h1.title')).toHaveText(today);
  expect(await readFile(join(vault,`Daily/${today}.md`),'utf8')).toContain(`Date: ${today}`);
  checks.push('Daily shortcut creates a note from the configured template');
  await page.getByRole('tab',{name:/Verification/}).click();

  await page.keyboard.press('Control+Shift+f');
  await page.getByLabel('Search vault notes').fill('recieve');
  await expect(page.locator('.hit-title')).toContainText('Verification');
  checks.push('Typo-tolerant search finds receive from recieve');

  await page.keyboard.press('Control+,');
  await page.getByRole('button',{name:'System',exact:true}).click();
  await page.getByLabel('verify',{exact:true}).check();
  await expect.poll(()=>page.evaluate(()=>getComputedStyle(document.documentElement).getPropertyValue('--verify-snippet').trim())).toBe('loaded');
  const editorLineSpacing = () => page.locator('.cm-content').evaluate((el) => {
    const style = getComputedStyle(el);
    return Number.parseFloat(style.lineHeight) / Number.parseFloat(style.fontSize);
  });
  await expect.poll(editorLineSpacing).toBeCloseTo(1.8, 2);
  await page.getByLabel('verify',{exact:true}).uncheck();
  await expect.poll(editorLineSpacing).not.toBeCloseTo(1.8, 2);
  await page.getByLabel('verify',{exact:true}).check();
  await expect.poll(editorLineSpacing).toBeCloseTo(1.8, 2);
  await expect(page.getByText('CSS snippets are small style files', {exact:false})).toBeVisible();
  await page.screenshot({path:join(root,'css-snippets-settings.png')});
  await page.getByLabel('Summon hotkey',{exact:true}).check();
  await expect(page.getByRole('alert')).toHaveCount(0);
  checks.push('CSS snippet example applies, toggles off cleanly, and native summon shortcut registers');
  await page.keyboard.press('Escape');
  // Jotter's dictionaries replace the WebView checker, so the native attribute is off.
  expect(await page.locator('.cm-content').getAttribute('spellcheck')).toBe('false');
  await expect(page.locator('.cm-misspelled',{hasText:'mispeled'})).toBeVisible();
  await expect(page.locator('.cm-misspelled',{hasText:'budget'})).toHaveCount(0);
  // Clicking into the word first puts the cursor inside it, as a user does before right-clicking.
  await page.locator('.cm-misspelled',{hasText:'mispeled'}).click();
  await expect(page.locator('.cm-misspelled',{hasText:'mispeled'})).toBeVisible();
  await page.locator('.cm-misspelled',{hasText:'mispeled'}).click({button:'right'});
  await page.getByRole('menuitem',{name:'misspelled',exact:true}).click();
  await expect(page.locator('.cm-content')).toContainText('A misspelled word.');
  await page.keyboard.press('Control+z');
  await expect(page.locator('.cm-misspelled',{hasText:'mispeled'})).toBeVisible();
  checks.push('Bundled en-US dictionary underlines a misspelling under the cursor, and its suggestion replaces it in one undo step');

  // Dark-mode print uses a light diagram palette, independent of screen colors.
  await page.evaluate(()=>{localStorage.setItem('jotter.themePref','dark');});
  await page.reload();
  // Reload reopens the last vault on Home with no tabs, so the note is reopened below.
  await expect(page.getByRole('region',{name:'Home'})).toBeVisible();
  await page.keyboard.press('Control+o');
  await page.getByPlaceholder('Find a note by name').fill('Verification');
  await page.keyboard.press('Enter');
  await expect(page.locator('h1.title')).toHaveText('Verification');
  await page.evaluate(()=>{window.print=()=>{};});
  await page.keyboard.press('Control+Shift+p');
  await expect(page.locator('.jotter-print-view svg')).toHaveCount(2);
  await expect(page.locator('.jotter-print-view style')).toHaveCount(0);
  await page.emulateMedia({media:'print'});
  expect(await page.locator('.jotter-print-view .pele').first().evaluate(el=>getComputedStyle(el).getPropertyValue('--pele-fg').trim())).toBe('#1b1b1b');
  await page.screenshot({path:join(root,'dark-mode-print.png'),fullPage:true});
  await page.emulateMedia({media:'screen'});
  checks.push('Dark-mode print renders SVG with dark labels on white');

  // A read-only destination forces the actual atomic write to fail.
  await chmod(join(vault,'Verification.md'),0o444);
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('\nSAVE-FAILURE-PROBE');
  await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('plugin:window|close',{label:'main'}));
  await expect(page.getByText(/Autosave failed:/)).toBeVisible();
  await expect(page.locator('.app-shell')).not.toHaveAttribute('inert');
  expect(app.exitCode).toBe(null);
  expect(await readFile(join(vault,'Verification.md'),'utf8')).not.toContain('SAVE-FAILURE-PROBE');
  await chmod(join(vault,'Verification.md'),0o666);
  await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('plugin:window|close',{label:'main'}));
  await expect.poll(()=>page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'main'}))).toBe(false);
  expect(await readFile(join(vault,'Verification.md'),'utf8')).toContain('SAVE-FAILURE-PROBE');
  await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('keep_app_open'));
  checks.push('Native close retains edits on real write failure, then saves and hides in tray after retry');

  // Delay IPC acknowledgements beyond five seconds. Native commands still execute normally.
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('\nSLOW-QUIT-PROBE');
  await page.evaluate(()=>{
    const callbacks = window.__TAURI_INTERNALS__.callbacks;
    const set = callbacks.set.bind(callbacks);
    callbacks.set = (id, callback) => set(id, data => setTimeout(()=>callback(data),6500));
    void window.__TAURI_INTERNALS__.invoke('plugin:event|emit',{event:'quit-requested',payload:true});
  });
  await pause(5500);
  expect(app.exitCode).toBe(null);
  for(let i=0;i<30 && app.exitCode===null;i++) await pause(500);
  expect(app.exitCode).toBe(0);
  expect(await readFile(join(vault,'Verification.md'),'utf8')).toContain('SLOW-QUIT-PROBE');
  checks.push('Quit waits beyond five seconds for a delayed save, then exits with edits on disk');
  expect(errors).toEqual([]);
  await writeFile(join(root,'results.json'),JSON.stringify({checks,errors},null,2));
  console.log(JSON.stringify({root,checks},null,2));
  }
} catch(error) {
  if(page && !page.isClosed()) await page.screenshot({path:join(root,'failure.png')}).catch(()=>{});
  if(page && !page.isClosed()) await writeFile(join(root,'failure.html'),await page.content()).catch(()=>{});
  await writeFile(join(root,'failure.txt'),`${error.stack}\n${logs}`);
  throw error;
} finally {
  await chmod(join(vault,'Verification.md'),0o666).catch(()=>{});
  if(app.exitCode===null) app.kill();
  await browser?.close().catch(()=>{});
  await writeFile(join(root,'app.log'),logs);
}
