import { mock, test } from 'node:test';
import assert from 'node:assert/strict';
import { EditorState } from '@codemirror/state';
import { tabsStore } from '../src/lib/stores/tabs.svelte';
import { vaultStore, fillDailyTemplate } from '../src/lib/stores/vault.svelte';
import { control } from './ipc.mock';
import { captureEditorOperation } from '../src/lib/editorOperations';
import { findRichBlocks } from '../src/lib/livePreviewBlocks';
import { CalcError, evaluate, evaluateCalcBlock, formatNumber, inlineCalculation } from '../src/lib/calc';
import { parseFrontmatter, removeProperty, renameProperty, serializeProperty, setProperty } from '../src/lib/frontmatter';
import {
  createOutlinerExtension,
  insertZoomChild,
  itemAt,
  itemOnLine,
  moveItem,
  outlinerKeymap,
  shiftItem,
  zoomedRange,
  zoomIntoItem,
} from '../src/lib/outliner';
import { createFrontmatterHideExtension, positionAfterFrontmatter } from '../src/lib/frontmatterPreview';
import { noteCardsStore } from '../src/lib/stores/noteCards.svelte';
import { detectSyncProvider, findConflictCopies } from '../src/lib/syncConflicts';
import { describeDueDate, findDueDate } from '../src/lib/dueDates';
import { buildHomeSections, buildTagTree, type HomeSection } from '../src/lib/noteGraph';
import type { NoteCard } from '../src/lib/types';
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

test('outliner moves items with their children and stops at the parent', () => {
  const run = (doc: string, cursor: number, dir: -1 | 1) => {
    const state = EditorState.create({doc, selection:{anchor:cursor}});
    const spec = moveItem(state, dir);
    return spec ? state.update(spec).state : state;
  };
  const doc = '- a\n  - a1\n- b\n  - b1\n- c';
  const up = run(doc, doc.indexOf('b'), -1);
  assert.equal(up.doc.toString(), '- b\n  - b1\n- a\n  - a1\n- c');
  assert.equal(up.doc.sliceString(up.selection.main.head, up.selection.main.head + 1), 'b');
  assert.equal(run(doc, doc.indexOf('b'), 1).doc.toString(), '- a\n  - a1\n- c\n- b\n  - b1');
  assert.equal(run(doc, doc.indexOf('a1'), -1).doc.toString(), doc);
  assert.equal(moveItem(EditorState.create({doc:'plain'}), -1), null);
});

test('outliner indents under the previous sibling and outdents to the parent', () => {
  const run = (doc: string, cursor: number, dir: -1 | 1) => {
    const state = EditorState.create({doc, selection:{anchor:cursor}});
    const spec = shiftItem(state, dir);
    return spec ? state.update(spec).state.doc.toString() : doc;
  };
  assert.equal(run('- a\n- b\n  - b1', 5, 1), '- a\n  - b\n    - b1');
  assert.equal(run('1. a\n2. b', 7, 1), '1. a\n   2. b');
  assert.equal(run('- a\n- b', 1, 1), '- a\n- b');
  assert.equal(run('- a\n  - b\n    - c', 9, -1), '- a\n- b\n  - c');
  assert.equal(run('- a\n\t- b', 6, -1), '- a\n- b');
  assert.equal(run('- a\n\t- b', 0, 1), '- a\n\t- b');
});

test('outliner finds item blocks across blank lines', () => {
  const doc = EditorState.create({doc:'- a\n\n  more\n- b'}).doc;
  assert.deepEqual([itemOnLine(doc, 1)?.line, itemOnLine(doc, 1)?.end], [1, 3]);
  assert.equal(itemAt(doc, 3)?.line, 1);
  assert.equal(itemOnLine(doc, 3), null);
});

test('sync provider comes from the vault path', () => {
  assert.equal(detectSyncProvider('C:/Users/a/Dropbox/Notes'), 'dropbox');
  assert.equal(detectSyncProvider('C:/Users/a/OneDrive - Contoso/Vault'), 'onedrive');
  assert.equal(detectSyncProvider('G:/My Drive/Vault'), 'googledrive');
  assert.equal(detectSyncProvider('C:/Users/a/Box/Vault'), 'box');
  assert.equal(detectSyncProvider('C:/Users/a/iCloudDrive/Vault'), 'icloud');
  assert.equal(detectSyncProvider('C:/Users/a/Documents/Vault'), null);
});

test('conflict copies match each provider and skip ambiguous names elsewhere', () => {
  const paths = [
    'Plan.md',
    "Plan (Alex's conflicted copy 2026-10-19).md",
    'Ideas.sync-conflict-20261019-101500-ABCDEFG.md',
    'Notes/Todo.md',
    'Notes/Todo-DESKTOP-4F2K9.md',
    'Plan-V2.md',
    'Plan (1).md',
    'Plan 2.md',
  ];
  const local = findConflictCopies(paths, null);
  assert.deepEqual(local.map((c) => [c.copyPath, c.originalPath, c.provider]), [
    ['Ideas.sync-conflict-20261019-101500-ABCDEFG.md', null, 'syncthing'],
    ["Plan (Alex's conflicted copy 2026-10-19).md", 'Plan.md', 'dropbox'],
  ]);
  const onedrive = findConflictCopies(paths, 'onedrive').map((c) => c.copyPath);
  assert.ok(onedrive.includes('Notes/Todo-DESKTOP-4F2K9.md'));
  assert.ok(!onedrive.includes('Plan-V2.md'));
  assert.ok(findConflictCopies(paths, 'googledrive').some((c) => c.copyPath === 'Plan (1).md'));
  assert.ok(findConflictCopies(paths, 'icloud').some((c) => c.copyPath === 'Plan 2.md'));
  assert.ok(!findConflictCopies(['Chapter 2.md'], 'icloud').length);
});

test('due dates match the three task syntaxes and describe relative days', () => {
  assert.equal(findDueDate('- [ ] Pay rent 📅 2026-11-01')?.date, '2026-11-01');
  assert.equal(findDueDate('* [x] Call due:2026-10-20 09:30')?.time, '09:30');
  assert.equal(findDueDate('* [x] Call due:2026-10-20 09:30')?.done, true);
  assert.equal(findDueDate('1. [ ] Review [due:: 2026-10-31] notes')?.date, '2026-10-31');
  assert.equal(findDueDate('- Plain due:2026-10-20'), null);
  assert.equal(findDueDate('- [ ] overdue:2026-10-04'), null);
  assert.equal(findDueDate('- [ ] Bad 📅 2026-02-30'), null);
  const now = new Date(2026, 9, 19, 12);
  assert.deepEqual(describeDueDate('2026-10-19', now), {label:'Today', overdue:false});
  assert.deepEqual(describeDueDate('2026-10-20', now), {label:'Tomorrow', overdue:false});
  assert.equal(describeDueDate('2026-10-01', now).overdue, true);
});

function card(path: string, extra: Partial<NoteCard> = {}): NoteCard {
  return {path, title:path.replace(/\.md$/, '').split('/').pop()!, modifiedMs:0, createdMs:0, preview:'', tags:[], linksTo:[], linkedFrom:0, ...extra};
}

test('tag tree nests tags and counts each note once', () => {
  const cards = [
    card('A.md', {tags:['project', 'project/alpha']}),
    card('B.md', {tags:['project/beta']}),
    card('C.md'),
  ];
  const {roots, untagged} = buildTagTree(cards);
  assert.deepEqual(roots.map((r) => r.tag), ['project']);
  assert.equal(roots[0].total, 2);
  assert.deepEqual(roots[0].children.map((c) => c.name), ['alpha', 'beta']);
  assert.deepEqual(untagged.map((c) => c.path), ['C.md']);
});

test('home sections skip notes already shown and honor filters', () => {
  const day = 24 * 60 * 60 * 1000;
  const now = new Date(2026, 9, 19);
  const cards = [
    card('Work/A.md', {modifiedMs:now.getTime() - day, createdMs:1, linkedFrom:5, tags:['work']}),
    card('Work/B.md', {modifiedMs:now.getTime() - 2 * day, createdMs:3, linkedFrom:1}),
    card('Home/C.md', {modifiedMs:now.getTime() - 200 * day, createdMs:2}),
  ];
  const sections = [
    {id:'o', kind:'recent-opened', title:'', limit:1, sort:'default', folder:'', tag:''},
    {id:'m', kind:'recent-modified', title:'', limit:5, sort:'default', folder:'Work', tag:''},
    {id:'l', kind:'most-linked', title:'', limit:5, sort:'default', folder:'', tag:''},
    {id:'r', kind:'rediscover', title:'', limit:1, sort:'default', folder:'', tag:''},
  ] as HomeSection[];
  const result = buildHomeSections(sections, cards, {openedPaths:['Work/B.md', 'Work/A.md'], now});
  assert.deepEqual(result.map((r) => r.cards.map((c) => c.path)), [['Work/B.md'], ['Work/A.md'], [], ['Home/C.md']]);
  const tagged = buildHomeSections([{...sections[1], folder:'', tag:'#work'}], cards, {openedPaths:[], now});
  assert.deepEqual(tagged[0].cards.map((c) => c.path), ['Work/A.md']);
});

// Regression tests for the Phase 2 review findings.

test('fold keys leave Ctrl+Alt free, since Windows reports AltGr as Ctrl+Alt', () => {
  assert.ok(outlinerKeymap.some((b) => b.key === 'Ctrl-Shift-['));
  assert.ok(!outlinerKeymap.some((b) => b.key?.startsWith('Ctrl-Alt-')));
});

test('Enter on the zoomed item adds a child and keeps the children visible', () => {
  let state = EditorState.create({doc:'intro\n- a\n  - child\n- b\n', selection:{anchor:7}, extensions:createOutlinerExtension()});
  state = state.update(zoomIntoItem(state)!).state;
  assert.deepEqual(zoomedRange(state), {from:6, to:19});
  state = state.update({selection:{anchor:9}}).state;
  state = state.update(insertZoomChild(state)!).state;
  assert.equal(state.doc.toString(), 'intro\n- a\n  - \n  - child\n- b\n');
  assert.equal(state.selection.main.head, 14);
  assert.deepEqual(zoomedRange(state), {from:6, to:24});
  // A sibling typed by the Markdown keymap stays inside the zoom, with the child after it.
  state = state.update({changes:{from:9, insert:'\n- '}}).state;
  assert.equal(state.doc.lineAt(zoomedRange(state)!.to).text, '  - child');
});

test('inline calculation reads thousands separators instead of starting mid-number', () => {
  assert.equal(inlineCalculation('1,000 + 1 =')?.result, '1001');
  assert.equal(inlineCalculation('Budget: 1,250.50 * 2 =')?.result, '2501');
  assert.equal(evaluate('max(1,2)'), 2);
  assert.equal(inlineCalculation('1,25 + 1 ='), null);
});

test('calc block variables named total, sum, or prev keep the assigned value', () => {
  assert.deepEqual(evaluateCalcBlock(['tax = 3', 'total = 100', 'total + tax']).map((r) => r.result), ['3', '100', '103']);
  assert.deepEqual(evaluateCalcBlock(['prev = 7', '1 + 1', 'prev']).map((r) => r.result), ['7', '2', '7']);
  assert.deepEqual(evaluateCalcBlock(['2', '3', 'sum']).map((r) => r.result), ['2', '3', '5']);
});

test('typing in a frontmatter-only note without a final newline starts a new line', () => {
  const doc = '---\ntitle: x\n---';
  let state = EditorState.create({doc, selection:{anchor:positionAfterFrontmatter(doc)}, extensions:createFrontmatterHideExtension()});
  state = state.update({changes:{from:doc.length, insert:'H'}, selection:{anchor:doc.length + 1}, userEvent:'input.type'}).state;
  assert.equal(state.doc.toString(), '---\ntitle: x\n---\nH');
  assert.equal(state.selection.main.head, state.doc.length);
});

test('a note that opens with a thematic break and prose has no frontmatter', () => {
  const doc = '---\nSome intro paragraph\n---\nBody';
  assert.equal(parseFrontmatter(doc), null);
  assert.equal(positionAfterFrontmatter(doc), 0);
  assert.equal(setProperty(doc, 'k', 'text', 'v').insert, '---\nk: v\n---\n');
  assert.ok(parseFrontmatter('---\n# comment\ntags:\n  - a\n\n- b\n---\n'));
});

test('home tag filters ignore the case the tag was written in', () => {
  const cards = [card('A.md', {tags:['Project/Alpha']}), card('B.md', {tags:['other']})];
  const section = {id:'t', kind:'tag', title:'', limit:5, sort:'default', folder:'', tag:'project'} as HomeSection;
  const [result] = buildHomeSections([section], cards, {openedPaths:[], now:new Date()});
  assert.deepEqual(result.cards.map((c) => c.path), ['A.md']);
});

test('OneDrive suffixes people write are not device names, and ambiguous copies can be dismissed', () => {
  const paths = ['Project.md', 'Project-TODO.md', 'Spec.md', 'Spec-FINAL.md', 'Budget.md', 'Budget-2026Q1.md',
    'Plan.md', 'Plan-DESKTOP-4F2K9.md', 'Draft.md', 'Draft-ALEX-PC.md'];
  assert.deepEqual(findConflictCopies(paths, 'onedrive').map((c) => c.copyPath), ['Draft-ALEX-PC.md', 'Plan-DESKTOP-4F2K9.md']);
  const icloud = ['Lecture.md', 'Lecture 2.md', 'Plan.md', "Plan (Alex's conflicted copy 2026-10-19).md"];
  assert.deepEqual(findConflictCopies(icloud, 'icloud').map((c) => [c.copyPath, c.ambiguous]),
    [['Lecture 2.md', true], ["Plan (Alex's conflicted copy 2026-10-19).md", false]]);
  const dismissed = new Set(['Lecture 2.md', "Plan (Alex's conflicted copy 2026-10-19).md"]);
  assert.deepEqual(findConflictCopies(icloud, 'icloud', dismissed).map((c) => c.copyPath), ["Plan (Alex's conflicted copy 2026-10-19).md"]);
});

test('note cards load a new vault at once and coalesce reloads while editing', async () => {
  mock.timers.enable({apis:['setTimeout']});
  try {
    vaultStore.vaultSnapshot = {rootPath:'Cards',name:'Cards',tree:[],noteCount:0};
    control.noteCardCalls = 0;
    await noteCardsStore.refresh(1);
    assert.equal(control.noteCardCalls, 1);
    for (let v = 2; v <= 5; v++) await noteCardsStore.refresh(v);
    assert.equal(control.noteCardCalls, 1);
    mock.timers.tick(1500);
    await Promise.resolve();
    assert.equal(control.noteCardCalls, 2);
    vaultStore.vaultSnapshot = {rootPath:'Other',name:'Other',tree:[],noteCount:0};
    await noteCardsStore.refresh(6);
    assert.equal(control.noteCardCalls, 3);
  } finally {
    mock.timers.reset();
  }
});
