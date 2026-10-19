import { test } from 'node:test';
import assert from 'node:assert/strict';
import { EditorState } from '@codemirror/state';
import { tabsStore } from '../src/lib/stores/tabs.svelte';
import { vaultStore, fillDailyTemplate } from '../src/lib/stores/vault.svelte';
import { control } from './ipc.mock';
import { captureEditorOperation } from '../src/lib/editorOperations';
import { findRichBlocks } from '../src/lib/livePreviewBlocks';
import { CalcError, evaluate, evaluateCalcBlock, formatNumber, inlineCalculation } from '../src/lib/calc';
import { parseFrontmatter, removeProperty, renameProperty, serializeProperty, setProperty } from '../src/lib/frontmatter';
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

test('calculator evaluates expressions, percentages, and functions', () => {
  assert.equal(evaluate('12*4'), 48);
  assert.equal(evaluate('2 + 3 * 4 ^ 2'), 50);
  assert.equal(evaluate('-(2 + 3) * 2'), -10);
  assert.equal(evaluate('100 + 10%'), 110);
  assert.equal(evaluate('200 - 25%'), 150);
  assert.equal(evaluate('15% of 80'), 12);
  assert.equal(evaluate('50%'), 0.5);
  assert.equal(evaluate('sqrt(16) + max(1, 7)'), 11);
  assert.equal(evaluate('2pi'), 2 * Math.PI);
  assert.equal(formatNumber(0.1 + 0.2), '0.3');
  assert.equal(formatNumber(1 / 3), '0.3333333333');
  assert.throws(() => evaluate('1/0'), CalcError);
  assert.throws(() => evaluate('3 4'), CalcError);
  assert.throws(() => evaluate('alert(1)'), CalcError);
});

test('inline calculation finds the expression before a trailing equals sign', () => {
  assert.deepEqual(inlineCalculation('12*4 ='), {from:0,result:'48'});
  assert.deepEqual(inlineCalculation('Total: 12*4 ='), {from:7,result:'48'});
  assert.equal(inlineCalculation('- 3*4 =')?.result, '12');
  assert.equal(inlineCalculation('I have 3 apples and 2*4 =')?.result, '8');
  assert.equal(inlineCalculation('the price 2*4=')?.result, '8');
  assert.equal(inlineCalculation('x = 5'), null);
  assert.equal(inlineCalculation('Version 2 ='), null);
  assert.equal(inlineCalculation('a == b'), null);
  assert.equal(inlineCalculation('if a >= 2*3 =='), null);
  assert.equal(inlineCalculation('no math here ='), null);
});

test('calc blocks assign variables and keep running sums', () => {
  const results = evaluateCalcBlock([
    'price = 12.5',
    'qty = 4 # boxes',
    'price * qty',
    'shipping = 7',
    '',
    'a = 1',
    'b = 2',
    'sum',
    'nope + 1',
  ]);
  assert.deepEqual(results.map((r) => r.result ?? 'err'), ['12.5', '4', '50', '7', '1', '2', '3', 'err']);
  assert.equal(results[2].line, 2);
});

test('frontmatter parses scalars, lists, and read-only values', () => {
  const doc = '---\ntitle: "Plan: v2"\ncount: 3\ndone: false\ndue: 2026-10-19\ntags: [a, "b c"]\naliases:\n  - One\n  - Two\n# note\nmeta:\n  nested: 1\n---\nBody';
  const fm = parseFrontmatter(doc)!;
  const byKey = Object.fromEntries(fm.properties.map((p) => [p.key, p]));
  assert.equal(byKey.title.value, 'Plan: v2');
  assert.equal(byKey.count.value, 3);
  assert.equal(byKey.done.type, 'checkbox');
  assert.equal(byKey.due.type, 'date');
  assert.deepEqual(byKey.tags.value, ['a', 'b c']);
  assert.deepEqual(byKey.aliases.value, ['One', 'Two']);
  assert.equal(byKey.meta.editable, false);
  assert.equal(doc.slice(fm.end), 'Body');
  assert.equal(parseFrontmatter('No frontmatter'), null);
  assert.equal(parseFrontmatter('---\nunclosed: 1\n'), null);
});

test('frontmatter edits rewrite only the changed property', () => {
  const apply = (doc: string, c: {from:number;to:number;insert:string} | null) =>
    c ? doc.slice(0, c.from) + c.insert + doc.slice(c.to) : doc;
  const doc = '---\n# keep me\ntitle: Old\nmeta:\n  nested: 1\n---\nBody';
  assert.equal(apply(doc, setProperty(doc, 'title', 'text', 'New: yes')),
    '---\n# keep me\ntitle: "New: yes"\nmeta:\n  nested: 1\n---\nBody');
  assert.equal(apply(doc, setProperty(doc, 'tags', 'list', ['x', 'y'])),
    '---\n# keep me\ntitle: Old\nmeta:\n  nested: 1\ntags:\n  - x\n  - y\n---\nBody');
  assert.equal(apply(doc, renameProperty(doc, 'title', 'name')),
    '---\n# keep me\nname: Old\nmeta:\n  nested: 1\n---\nBody');
  assert.equal(renameProperty(doc, 'title', 'meta'), null);
  assert.equal(apply('Body', setProperty('Body', 'done', 'checkbox', true)), '---\ndone: true\n---\nBody');
  assert.equal(apply('---\ndone: true\n---\nBody', removeProperty('---\ndone: true\n---\nBody', 'done')), 'Body');
  assert.equal(serializeProperty('n', 'text', '42'), 'n: "42"');
  assert.equal(serializeProperty('d', 'date', '2026-10-19'), 'd: 2026-10-19');
});
