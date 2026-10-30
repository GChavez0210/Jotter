import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, writeFile, readFile, rename, chmod, stat, rmdir } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { setTimeout as pause } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This verification harness requires Windows WebView2. Test other platforms manually.');

// Run against the built app with isolated WebView storage, app data, and disposable notes.
const root = resolve(`src-tauri/target/desktop-verification/${Date.now()}`);
const vault = join(root, 'vault');
await mkdir(vault, {recursive:true});
await mkdir(join(root, 'app-data/Jotter/snippets'), {recursive:true});
await writeFile(join(root, 'app-data/Jotter/snippets/verify.css'), ':root { --verify-snippet: loaded; }\n.cm-content { line-height: 1.8; }');
// The headings and the plain fence feed the outline check; the rest feeds rendering and spelling.
const note = '# Verification\n\nWe receive the budget.\n\nA mispeled word.\n\nInline $x^2$.\n\n$$ x^2 + y^2 $$\n\n```mermaid\ngraph TD; A-->B\n```\n\n> ```mermaid\n> graph TD; C-->D\n> ```\n\n    $$ indented code $$\n\n## Details\n\n```\n# not a heading\n```\n\n## Wrap up\n\nEnd.\n';
await writeFile(join(vault,'Verification.md'),note);
await mkdir(join(vault,'Templates'),{recursive:true});
await writeFile(join(vault,'Templates/Daily.md'),'# {{title}}\nDate: {{date}}\n');
// Tauri embeds the frontend at compile time, but cargo does not watch `dist`, so a frontend-only
// change plus `cargo build` leaves the previous bundle inside the executable and this harness
// would check code that is no longer in the tree.
const exePath = resolve(process.env.JOTTER_TEST_EXE ?? 'src-tauri/target/release/jotter-desktop.exe');
const [exeStat, bundleStat] = await Promise.all([stat(exePath), stat(resolve('dist/index.html'))]);
if (exeStat.mtimeMs < bundleStat.mtimeMs) {
  throw new Error(
    `${exePath} is older than dist/index.html, so it still embeds an earlier frontend. ` +
    'Rebuild it: npm run build && touch src-tauri/src/lib.rs && cargo build --release',
  );
}

const port = 9432;
const app = spawn(exePath,[],{
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
    // Jotter allows one instance, so a copy already running takes over this launch and it exits.
    if(app.exitCode !== null) throw Error(`App exited with ${app.exitCode}. Close any running Jotter and retry. ${logs}`);
    try {browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`,{timeout:1000});break;} catch {await pause(500);}
  }
  if(!browser) throw Error(`WebView2 debugging endpoint unavailable: ${logs}`);
  page = browser.contexts()[0].pages()[0];
  const errors=[];
  page.on('pageerror',(error)=>errors.push(String(error)));
  page.on('console',(message)=>{if(message.type()==='error') errors.push(message.text());});
  await page.waitForLoadState('domcontentloaded');
  await expect(page.getByRole('button',{name:'Enter path…'})).toBeVisible();
  function installProbes() {
    window.__verifyCsp=[];
    document.addEventListener('securitypolicyviolation',(event)=>window.__verifyCsp.push(`${event.violatedDirective}: ${event.blockedURI}`));
    window.__verifySettingsCalls=[];
    // Tauri's invoke property is read-only. Observe successful native responses through fetch.
    const fetch=window.fetch.bind(window);
    window.fetch=async(input,options)=>{
      const response=await fetch(input,options);
      const command=new URL(typeof input==='string'?input:input.url).pathname.split('/').at(-1);
      if((command==='configure_summon'||command==='configure_window_behavior')&&response.headers.get('Tauri-Response')==='ok') {
        window.__verifySettingsCalls.push({command,args:JSON.parse(options.body)});
      }
      return response;
    };
  }
  await page.addInitScript(installProbes);
  await page.evaluate(installProbes);
  await page.getByRole('button',{name:'Enter path…'}).click();
  await page.getByLabel('Vault folder path').fill(vault);
  await page.getByRole('button',{name:'Open folder',exact:true}).click();
  await page.getByRole('button',{name:'Verification',exact:true}).click();
  // A rejected path must not be read or copied into version history first.
  await writeFile(join(root,'outside.md'),'OUTSIDE-VAULT-FIXTURE');
  const rejectedHistoryPaths = await page.evaluate(async()=> {
    const invoke=window.__TAURI_INTERNALS__.invoke;
    const commands=[
      ['save_note',{relPath:'../outside.md',content:'changed'}],
      ['list_note_versions',{relPath:'../outside.md'}],
      ['read_note_version',{relPath:'../outside.md',id:'1000'}],
      ['restore_note_version',{relPath:'../outside.md',id:'1000'}],
    ];
    return Promise.all(commands.map(async([command,args])=>{
      try {await invoke(command,args);return false;} catch(e){return String(e).includes('Path traversal');}
    }));
  });
  expect(rejectedHistoryPaths).toEqual([true,true,true,true]);
  expect(await readFile(join(root,'outside.md'),'utf8')).toBe('OUTSIDE-VAULT-FIXTURE');
  checks.push('History commands reject outside-vault paths before reading or snapshotting');
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
  // Opening the sole active tab on the right must move it, never clone its editor.
  await page.locator('.tree-root').getByRole('button',{name:'Verification',exact:true}).click({button:'right'});
  await page.getByRole('menuitem',{name:'Open on the right'}).click();
  await expect(page.locator('.cm-content')).toHaveCount(1);
  await expect(page.getByRole('region',{name:'Second pane'}).locator('h1.title')).toHaveText('Verification');
  await expect(page.getByRole('region',{name:'Editor',exact:true}).locator('.cm-content')).toHaveCount(0);
  await page.getByRole('button',{name:'Close the second pane'}).click();
  await page.locator('.tree-root').getByRole('button',{name:'Verification',exact:true}).click();
  await expect(page.locator('.cm-content')).toHaveCount(1);
  checks.push('Open on the right moves the only open note without creating duplicate editors');
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

  // Pins live in app data, so they must survive a rename and outlive the window.
  // The search above left the sidebar on its own view; pinning needs the file tree.
  await page.getByRole('navigation',{name:'Sidebar views'}).getByRole('button',{name:'Notes'}).click();
  await page.locator('.tree-root').getByRole('button',{name:'Verification',exact:true}).click({button:'right'});
  await page.getByRole('menuitem',{name:'Pin to top'}).click();
  await expect(page.locator('.pinned-item')).toHaveCount(1);
  await expect(page.locator('.pinned-item')).toHaveText(/Verification/);
  const vaultRoot=(await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('get_vault_snapshot'))).rootPath;
  let vaultHash=0xcbf29ce484222325n;
  for(const byte of Buffer.from(vaultRoot.toLowerCase().replace(/\\/g,'/').replace(/\/+$/,''))) {
    vaultHash=BigInt.asUintN(64,(vaultHash^BigInt(byte))*0x100000001b3n);
  }
  const pinsFile=join(root,'app-data/Jotter/vaults',`${vaultHash.toString(16).padStart(16,'0')}.pins.json`);
  expect(JSON.parse(await readFile(pinsFile,'utf8')).pinned).toEqual(['Verification.md']);
  // Renaming through the sidebar carries the pin instead of dropping it.
  const renameTreeNote = async (from, to) => {
    await page.locator('.tree-root').getByRole('button',{name:from,exact:true}).click({button:'right'});
    await page.getByRole('menuitem',{name:'Rename'}).click();
    await page.locator('.rename-input').fill(to);
    await page.locator('.rename-input').press('Enter');
  };
  await renameTreeNote('Verification','Renamed');
  await expect(page.locator('.pinned-item')).toHaveText(/Renamed/);
  await renameTreeNote('Renamed','Verification');
  await expect(page.locator('.pinned-item')).toHaveText(/Verification/);
  checks.push('Pinning writes to app data, not the vault, and a rename carries the pin');

  // The outline lists real headings only, and clicking one moves the cursor to it.
  // The note's fenced "# not a heading" must not appear.
  await expect(page.locator('.outline-text')).toHaveText(['Verification','Details','Wrap up']);
  await page.locator('.outline-row',{hasText:'Wrap up'}).click();
  await expect(page.locator('.outline-row.active .outline-text')).toHaveText('Wrap up');
  await page.locator('.outline-row',{hasText:'Details'}).click();
  await expect(page.locator('.outline-row.active .outline-text')).toHaveText('Details');
  checks.push('Outline skips headings inside code fences, and clicking one moves the cursor there');

  // The preceding app rename suppresses its own watcher events for 900 ms.
  await pause(1000);
  await rename(join(vault,'Verification.md'),join(root,'Verification-backup.md'));
  await expect(page.locator('.tree-root').getByRole('button',{name:'Verification',exact:true})).toHaveCount(0);
  await expect(page.locator('.pinned-item')).toHaveCount(0);
  expect(JSON.parse(await readFile(pinsFile,'utf8')).pinned).toEqual(['Verification.md']);
  await rename(join(root,'Verification-backup.md'),join(vault,'Verification.md'));
  await expect(page.locator('.pinned-item')).toHaveText(/Verification/);
  await page.locator('.pinned-item').click();
  checks.push('External deletion hides a pin and external restoration makes it visible again');

  // A real window below the sidebar's width threshold, then the sidebar asked back.
  const setWindowWidth = (width) => page.evaluate(
    (w)=>window.__TAURI_INTERNALS__.invoke('plugin:window|set_size',{label:'main',value:{Logical:{width:w,height:760}}}),
    width,
  );
  const sidebar = page.getByRole('complementary',{name:'Vault file tree'});
  const notePanel = page.getByRole('complementary',{name:'Note panel'});
  await expect(notePanel).toBeVisible();
  await setWindowWidth(500);
  await expect(sidebar).toBeHidden();
  // The right panel yields its width before the sidebar does.
  await expect(notePanel).toBeHidden();
  await page.getByRole('button',{name:'Note panel',exact:true}).click();
  await expect(notePanel).toBeVisible();
  await expect(sidebar).toBeVisible();
  expect(await page.evaluate(()=>window.innerWidth)).toBe(856);
  await setWindowWidth(500);
  await page.getByRole('button',{name:'Show sidebar'}).click();
  await expect(sidebar).toBeVisible();
  // Just enough for the sidebar and the text column, not the width it started at.
  const widened = await page.evaluate(()=>window.innerWidth);
  expect(widened).toBeGreaterThan(500);
  expect(widened).toBeLessThan(1120);
  await setWindowWidth(500);
  await expect(sidebar).toBeHidden();
  await setWindowWidth(1120);
  await expect(sidebar).toBeVisible();
  checks.push('Narrow window hides the sidebar, showing it widens the window to fit, and widening restores it');

  // Settings live in the app data folder and the page saves its own copy on reload, so change
  // them through the UI. The resize handle sets the sidebar width to the pointer's position.
  const handle = await page.getByRole('separator',{name:'Resize sidebar'}).boundingBox();
  await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2);
  await page.mouse.down();
  await page.mouse.move(400, handle.y + handle.height / 2, {steps:5});
  await page.mouse.up();
  await page.reload();
  await expect(page.getByRole('region',{name:'Home'})).toBeVisible();
  await setWindowWidth(700);
  await expect(sidebar).toBeHidden();
  await expect(notePanel).toBeHidden();
  await setWindowWidth(760);
  await expect(sidebar).toBeVisible();
  await expect(notePanel).toBeHidden();
  await setWindowWidth(1008);
  await expect(notePanel).toBeVisible();
  await page.locator('.pinned-item').click();
  await setWindowWidth(1120);
  checks.push('A wide sidebar retains priority over the right panel at intermediate widths');

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
  await expect(page.getByText('Restyle Jotter with your own', {exact:false})).toBeVisible();
  await page.screenshot({path:join(root,'css-snippets-settings.png')});
  await page.getByLabel('Summon hotkey',{exact:true}).check();
  await expect.poll(()=>page.evaluate(()=>window.__verifySettingsCalls.some(call=>call.command==='configure_summon'&&call.args.enabled))).toBe(true);
  await expect(page.getByRole('alert')).toHaveCount(0);
  checks.push('CSS snippet example applies, toggles off cleanly, and native summon shortcut registers');
  // Hiding to the tray needs the tray icon, so choosing it turns the icon on.
  await page.getByRole('radio',{name:'Hides to the tray',exact:true}).click();
  await expect(page.getByLabel('Keep Jotter in the tray',{exact:true})).toBeChecked();
  await expect.poll(()=>page.evaluate(()=>window.__verifySettingsCalls.some(call=>call.command==='configure_window_behavior'&&call.args.trayResident&&call.args.hideOnClose))).toBe(true);
  await expect(page.getByRole('alert')).toHaveCount(0);
  checks.push('Close-to-tray turns on tray residency and the native tray icon accepts it');
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
  await page.keyboard.press('Control+p');
  await page.getByPlaceholder('Type a command').fill('Theme: Dark');
  await page.keyboard.press('Enter');
  await expect(page.locator('html')).toHaveAttribute('data-theme','dark');
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

  // Two notes side by side, each with its own text, and a preview of the open note.
  // The reload above closed every tab, so open a second note for the split to take. The window
  // has to hold two text columns with room to spare, or the sash has nowhere to travel.
  await setWindowWidth(1700);
  await page.keyboard.press('Control+o');
  await page.getByPlaceholder('Find a note by name').fill(today);
  await page.keyboard.press('Enter');
  await expect(page.locator('h1.title')).toHaveText(today);
  await page.getByRole('tab',{name:/Verification/}).click();
  await expect(page.locator('h1.title')).toHaveText('Verification');
  await page.getByRole('button',{name:'Split view',exact:true}).click();
  const secondPane = page.getByRole('region',{name:'Second pane'});
  await expect(secondPane).toBeVisible();
  await expect(page.locator('.cm-content')).toHaveCount(2);
  // The second pane took the note opened before this one, and the tab strip marks it.
  await expect(secondPane.locator('h1.title')).toHaveText(today);
  await expect(page.locator('.tab.in-split')).toHaveCount(1);
  // Typing lands in the pane the user clicked, and reaches that note's file.
  await secondPane.locator('.cm-content').click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('\nSPLIT-PANE-EDIT');
  await expect.poll(()=>readFile(join(vault,`Daily/${today}.md`),'utf8')).toContain('SPLIT-PANE-EDIT');
  expect(await readFile(join(vault,'Verification.md'),'utf8')).not.toContain('SPLIT-PANE-EDIT');
  await page.screenshot({path:join(root,'split-view.png')});
  // The sash moves the boundary between the panes.
  const paneWidth = () => page.locator('.editor-column').first().evaluate(el=>el.getBoundingClientRect().width);
  const startWidth = await paneWidth();
  const sash = await page.getByRole('separator',{name:'Resize the panes'}).boundingBox();
  await page.mouse.move(sash.x + sash.width / 2, sash.y + sash.height / 2);
  await page.mouse.down();
  await page.mouse.move(sash.x - 200, sash.y + sash.height / 2, {steps:5});
  await page.mouse.up();
  await expect.poll(paneWidth).toBeLessThan(startWidth - 100);
  // A window too narrow for two text columns hides the split and widening brings it back.
  await setWindowWidth(700);
  await expect(secondPane).toBeHidden();
  await expect(page.locator('.cm-content')).toHaveCount(1);
  await setWindowWidth(1700);
  await expect(secondPane).toBeVisible();
  // The preview pane renders the open note through the export renderer.
  await page.keyboard.press('Control+p');
  await page.getByPlaceholder('Type a command').fill('Rendered Preview');
  await page.keyboard.press('Enter');
  const preview = page.getByRole('region',{name:'Preview'});
  await expect(preview.locator('.preview-body h1')).toHaveText('Verification');
  await expect(preview.locator('.preview-body .katex')).not.toHaveCount(0);
  await page.screenshot({path:join(root,'split-preview.png')});
  await preview.getByRole('button',{name:'Close the second pane'}).click();
  await expect(page.locator('.cm-content')).toHaveCount(1);
  checks.push('Split view edits two notes independently, resizes, hides in a narrow window, and previews the open note');

  // Version history keeps the text a save replaced, and restores it.
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('\nVERSION-HISTORY-PROBE');
  await page.keyboard.press('Control+s');
  await expect.poll(()=>readFile(join(vault,'Verification.md'),'utf8')).toContain('VERSION-HISTORY-PROBE');
  await page.getByRole('button',{name:'Version history',exact:true}).click();
  const timeline = page.getByRole('dialog',{name:/Version history of Verification/});
  await expect(timeline.getByRole('navigation',{name:'Stored versions'}).locator('.entry')).not.toHaveCount(0);
  // The diff reads as what restoring would do: the probe line goes away.
  await expect(timeline.locator('.row.del').filter({hasText:'VERSION-HISTORY-PROBE'})).toHaveCount(1);
  await page.screenshot({path:join(root,'version-history.png')});
  await timeline.getByRole('button',{name:'Restore this version'}).click();
  await expect.poll(()=>readFile(join(vault,'Verification.md'),'utf8')).not.toContain('VERSION-HISTORY-PROBE');
  // The editor shows the restored text, not the edit that was undone.
  await expect(page.locator('.cm-content')).not.toContainText('VERSION-HISTORY-PROBE');
  // Restoring kept the replaced text, so the timeline can go back to it.
  await expect(timeline.locator('.row.add').filter({hasText:'VERSION-HISTORY-PROBE'})).toHaveCount(1);
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog',{name:/Version history/})).toHaveCount(0);
  checks.push('Version history stores the text a save replaced, shows it as a diff, and restores it to disk');

  // Failure to preserve the current text must abort a restore, while an ordinary save
  // still succeeds when its optional history marker cannot be written.
  await page.evaluate(async()=>{
    const invoke=window.__TAURI_INTERNALS__.invoke;
    await invoke('create_note',{relPath:'HistoryStorage.md',initialContent:'original history text'});
    await invoke('save_note',{relPath:'HistoryStorage.md',content:'saved current text'});
  });
  let historyHash=0xcbf29ce484222325n;
  for(const byte of Buffer.from('historystorage.md')) historyHash=BigInt.asUintN(64,(historyHash^BigInt(byte))*0x100000001b3n);
  const historyDir=join(root,'app-data/Jotter/vaults',`${vaultHash.toString(16).padStart(16,'0')}.versions`,historyHash.toString(16).padStart(16,'0'));
  const historyMarker=join(historyDir,'note.json');
  await rename(historyMarker,join(historyDir,'marker-backup.json'));
  await mkdir(historyMarker);
  try {
    await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('save_note',{relPath:'HistoryStorage.md',content:'latest protected text'}));
    const restoreError=await page.evaluate(async()=>{
      const invoke=window.__TAURI_INTERNALS__.invoke;
      const versions=await invoke('list_note_versions',{relPath:'HistoryStorage.md'});
      try {await invoke('restore_note_version',{relPath:'HistoryStorage.md',id:versions.at(-1).id});return '';}
      catch(e){return String(e);}
    });
    expect(restoreError).toContain('Cannot preserve the current note');
    expect(await readFile(join(vault,'HistoryStorage.md'),'utf8')).toBe('latest protected text');
  } finally {
    await rmdir(historyMarker);
    await rename(join(historyDir,'marker-backup.json'),historyMarker);
  }
  checks.push('Unavailable history storage leaves ordinary saves working and prevents unsafe restoration');

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
