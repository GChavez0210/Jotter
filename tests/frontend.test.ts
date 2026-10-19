import { test } from 'node:test';
import assert from 'node:assert/strict';
import { EditorState } from '@codemirror/state';
import { tabsStore } from '../src/lib/stores/tabs.svelte';
import { vaultStore, fillDailyTemplate } from '../src/lib/stores/vault.svelte';
import { control } from './ipc.mock';
import { captureEditorOperation } from '../src/lib/editorOperations';
import { findRichBlocks } from '../src/lib/livePreviewBlocks';
import { invalidateImagePreviews, cachedImage, rememberImage, imagePreviewEpoch } from '../src/lib/imagePreviewCache';

function dirtyTab() {
  tabsStore.resetTabs();
  tabsStore.openTabs = [{relPath:'A.md',title:'A',content:'unsaved',savedContent:'',dirty:true,externalChangePending:false}];
  tabsStore.activeTabPath = 'A.md';
}

test('failed save retains dirty tab and prevents vault switch', async () => {
  dirtyTab();
  control.save = async () => {throw Error('disk full');};
  assert.equal(await tabsStore.flushAllAutosaves(), false);
  await tabsStore.closeTab('A.md');
  assert.equal(tabsStore.openTabs.length, 1);
  assert.equal(tabsStore.openTabs[0].dirty, true);
  vaultStore.vaultSnapshot = {rootPath:'A',name:'A',tree:[],noteCount:1};
  await vaultStore.openVaultByPath('B');
  await vaultStore.switchVault();
  assert.equal(vaultStore.vaultSnapshot.rootPath, 'A');
  assert.equal(control.openCalls, 0);
  assert.equal(control.closeCalls, 0);
});

test('closing waits for in-flight save and saves edits made during it', async () => {
  dirtyTab();
  let release!: () => void;
  const wait = new Promise<void>((resolve) => { release = resolve; });
  const writes: string[] = [];
  control.save = async (path, content) => {
    writes.push(content);
    if (writes.length === 1) await wait;
    return {relPath:path,title:path,content};
  };
  const saving = tabsStore.flushAutosaveFor('A.md');
  const closing = tabsStore.closeTab('A.md');
  tabsStore.handleEditorDocChange('A.md', 'latest');
  assert.equal(tabsStore.openTabs.length, 1);
  release();
  await Promise.all([saving, closing]);
  assert.deepEqual(writes, ['unsaved', 'latest']);
  assert.equal(tabsStore.openTabs.length, 0);
  tabsStore.clearAllTimers();
});

test('clipboard mutations reject edits and document replacement, including A to B to A', () => {
  const state = EditorState.create({doc:'original',selection:{anchor:0,head:4}});
  const view = {state} as any;
  const operation = captureEditorOperation(view, 1);
  assert.equal(operation.isCurrent(view, 1), true);
  view.state = state.update({changes:{from:0,insert:'x'}}).state;
  assert.equal(operation.isCurrent(view, 1), false);
  view.state = state;
  assert.equal(operation.isCurrent(view, 3), false);
});

test('image cache is bounded, invalidates across vaults, and rejects stale responses', () => {
  invalidateImagePreviews('A');
  const epoch = imagePreviewEpoch;
  rememberImage('photo.png', 'A-image', epoch);
  assert.equal(cachedImage('photo.png'), 'A-image');
  invalidateImagePreviews('B');
  assert.equal(cachedImage('photo.png'), undefined);
  rememberImage('photo.png', 'stale-A-image', epoch);
  assert.equal(cachedImage('photo.png'), undefined);
  for(let i=0;i<101;i++) rememberImage(`image${i}`, 'data', imagePreviewEpoch);
  assert.equal(cachedImage('image0'), undefined);
  assert.equal(cachedImage('image100'), 'data');
});

test('rich blocks respect indented code and render quote/list Mermaid and math', () => {
  const parse = (doc: string) => findRichBlocks(EditorState.create({doc}));
  assert.deepEqual(parse('    $$ x $$'), []);
  assert.deepEqual(parse('```text\n$$ x $$\n```'), []);
  const quote = parse('> ```mermaid\n> graph TD\n> A-->B\n> ```');
  assert.equal(quote.length, 1);
  assert.equal(quote[0].source, 'graph TD\nA-->B');
  const list = parse('- ```mermaid\n  graph TD; A-->B\n  ```');
  assert.equal(list.length, 1);
  assert.equal(list[0].kind, 'diagram');
  assert.equal(parse('> $$\n> x^2\n> $$')[0].source, 'x^2');
  assert.deepEqual(parse('```mermaid\ngraph TD'), []);
});

test('daily template uses the local date and fills placeholders', () => {
  const result = fillDailyTemplate('{{date}} {{title}} {{time}}', new Date(2026, 9, 19, 23, 7));
  assert.equal(result, '2026-10-19 2026-10-19 23:07');
});
