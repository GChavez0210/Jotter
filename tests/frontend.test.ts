import { systemStore } from '../src/lib/stores/system.svelte';
import { settingsStore } from '../src/lib/stores/settings.svelte';
import { systemControl } from './ipc.mock';
import { isMac, isWindows, isMacPlatform, shortcutLabel } from '../src/lib/platform';
import { designTokens } from '../src/lib/designTokens';
import { maxSidebarWidth, rightPanelFitsWindow, sidebarFitsWindow, titleBarIsCompact, widthForSidebar } from '../src/lib/layout';
import { validateEntryName } from '../src/lib/validation';
import { mock, test } from 'node:test';
import { i18n, m, matchSystemLocale, SLOT, splitSlots } from '../src/lib/i18n.svelte';
import enMessages from '../messages/en.json';
import esMessages from '../messages/es.json';
import assert from 'node:assert/strict';
import { EditorState } from '@codemirror/state';
import { tabsStore } from '../src/lib/stores/tabs.svelte';
import { vaultStore, fillDailyTemplate } from '../src/lib/stores/vault.svelte';
import { control } from './ipc.mock';
import { pinControl } from './ipc.mock';
import { activeHeadingIndex, outlineDepths, parseOutline } from '../src/lib/outline';
import { captureEditorOperation } from '../src/lib/editorOperations';
import { findRichBlocks } from '../src/lib/livePreviewBlocks';
import { CalcError, evaluate, evaluateCalcBlock, formatNumber, inlineCalculation } from '../src/lib/calc';
import { parseFrontmatter, removeProperty, renameProperty, serializeProperty, setProperty } from '../src/lib/frontmatter';
import {
  createOutlinerExtension,
  foldCaretAttributes,
  FoldCaretWidget,
  insertZoomChild,
  itemAt,
  itemOnLine,
  moveItem,
  outlinerKeymap,
  shiftItem,
  zoomedRange,
  zoomIntoItem,
  frontmatterEndLine,
} from '../src/lib/outliner';
import { createFrontmatterHideExtension, frontmatterNeedsReparse, positionAfterFrontmatter } from '../src/lib/frontmatterPreview';
import { ChangeSet } from '@codemirror/state';
import { foldable, foldEffect, unfoldEffect } from '@codemirror/language';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { readFileSync } from 'node:fs';
import { calendarControl } from './ipc.mock';
import { taskCalendarStore } from '../src/lib/stores/taskCalendar.svelte';
import { noteCardsStore } from '../src/lib/stores/noteCards.svelte';
import { detectSyncProvider, findConflictCopies } from '../src/lib/syncConflicts';
import { describeDueDate, findDueDate } from '../src/lib/dueDates';
import { buildHomeSections, buildTagTree, type HomeSection } from '../src/lib/noteGraph';
import type { NoteCard } from '../src/lib/types';
import { collectCheckableWords, findCheckableWords, matchDictionaries, resolveNoteLanguages } from '../src/lib/spellcheck';
import { invalidateImagePreviews, cachedImage, rememberImage, imagePreviewEpoch } from '../src/lib/imagePreviewCache';
import { aiResultChange, aiTargetRange } from '../src/lib/aiText';
import { aiPopoverPosition, clampAiPopoverPosition } from '../src/lib/aiPopover';
import { noteMoveDestination } from '../src/lib/noteMove';
import { aiStore, normalizeRewriteStyle } from '../src/lib/stores/ai.svelte';
import { aiControl } from './ipc.mock';
import { history, undo, undoDepth } from '@codemirror/commands';
import { EditorView, ViewPlugin } from '@codemirror/view';
import { refreshEditorLocale } from '../src/lib/editorLocale';
import { searchPhrases } from '../src/lib/editorSearch';
import { createLivePreviewExtension } from '../src/lib/livePreview';
import { createCalcExtension } from '../src/lib/calcPreview';
import { cachedDiagram, renderDiagram } from '../src/lib/richBlocks';
import { foldedRanges } from '@codemirror/language';
import type { AiConfig, AiResult, AiStatus, VaultNode, VaultSnapshot } from '../src/lib/types';

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
  assert.equal(run('1. a\n2. b', 7, 1), '1. a\n   1. b');
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

// Regression tests for the low-severity Phase 2 review findings.

test('inline calculation subtracts whenever the line ends with =', () => {
  assert.equal(inlineCalculation('4-7=')?.result, '-3');
  assert.equal(inlineCalculation('100-25 =')?.result, '75');
  assert.equal(inlineCalculation('see 10-5 =')?.result, '5');
  assert.equal(inlineCalculation('2*3 - 1 =')?.result, '5');
  assert.equal(inlineCalculation('555 - 1234 =')?.result, '-679');
  assert.equal(inlineCalculation('call 555-1234'), null);
  assert.equal(inlineCalculation('Meeting 2026-10-19'), null);
});

test('frontmatter reads comment-only values, comments after quotes, and leading zeros', () => {
  const doc = '---\ntags: # topics\n  - a\n  - b\ntitle: "Hello" # c\nnick: \'it\'\'s\' # d\nzip: 02134\nzero: 0\nhalf: -0.5\n---\n';
  const byKey = Object.fromEntries(parseFrontmatter(doc)!.properties.map((p) => [p.key, p]));
  assert.equal(byKey.tags.editable, true);
  assert.deepEqual(byKey.tags.value, ['a', 'b']);
  assert.equal(byKey.title.value, 'Hello');
  assert.equal(byKey.nick.value, "it's");
  assert.deepEqual([byKey.zip.type, byKey.zip.value], ['text', '02134']);
  assert.equal(byKey.zero.value, 0);
  assert.equal(byKey.half.value, -0.5);
});

test('outliner commands ignore list syntax in code blocks and frontmatter', () => {
  const at = (doc: string, text: string) =>
    EditorState.create({doc, selection:{anchor:doc.indexOf(text)}, extensions:[markdown({base:markdownLanguage}), createOutlinerExtension()]});
  const code = at('```\n- a\n  - b\n- c\n```\n', '- c');
  assert.equal(moveItem(code, -1), null);
  assert.equal(shiftItem(code, 1), null);
  assert.equal(foldable(code, code.doc.line(2).from, code.doc.line(2).to), null);
  const indented = at('Text\n\n    - a\n    - b\n', '- b');
  assert.equal(moveItem(indented, -1), null);
  const yaml = at('---\ntags:\n- a\n- b\n---\n- x\n- y\n', '- b');
  assert.equal(moveItem(yaml, -1), null);
  assert.equal(shiftItem(yaml, 1), null);
  const body = at('---\ntags:\n- a\n---\n- x\n- y\n', '- y');
  assert.equal(body.update(moveItem(body, -1)!).state.doc.toString(), '---\ntags:\n- a\n---\n- y\n- x\n');
});

test('outliner renumbers ordered lists after moves and indents', () => {
  const run = (doc: string, text: string, fn: (s: EditorState) => any) => {
    const state = EditorState.create({doc, selection:{anchor:doc.indexOf(text)}});
    const next = state.update(fn(state)).state;
    return {doc:next.doc.toString(), at:next.doc.sliceString(next.selection.main.head, next.selection.main.head + 1)};
  };
  assert.deepEqual(run('1. a\n2. b\n3. c', 'c', (s) => moveItem(s, -1)), {doc:'1. a\n2. c\n3. b', at:'c'});
  assert.equal(run('5) a\n6) b', 'b', (s) => moveItem(s, -1)).doc, '5) b\n6) a');
  assert.equal(run('1. a\n2. b\n3. c', 'b', (s) => shiftItem(s, 1)).doc, '1. a\n   1. b\n2. c');
  assert.equal(run('1. a\n   1. x\n2. b\n3. c', 'b', (s) => shiftItem(s, 1)).doc, '1. a\n   1. x\n   2. b\n2. c');
  assert.equal(run('1. a\n   1. x\n   2. y\n2. b', 'x', (s) => shiftItem(s, -1)).doc, '1. a\n2. x\n   1. y\n3. b');
  assert.equal(run('- a\n- b', 'b', (s) => moveItem(s, -1)).doc, '- b\n- a');
});

test('due date times use the same 24-hour range as the indexer', () => {
  assert.deepEqual([findDueDate('- [ ] Late due:2026-10-20 25:00')?.date, findDueDate('- [ ] Late due:2026-10-20 25:00')?.time], ['2026-10-20', null]);
  assert.equal(findDueDate('- [ ] Odd 📅 2026-10-20 12:60')?.time, null);
  assert.equal(findDueDate('- [ ] Bracket [due:: 2026-10-20 24:00]'), null);
  assert.equal(findDueDate('- [ ] Night 📅 2026-10-20 23:59')?.time, '23:59');
});

test('fold carets expose their action and state, and views drop tree roles', () => {
  assert.equal(foldCaretAttributes(false)['aria-label'], 'Fold list item');
  assert.equal(foldCaretAttributes(true)['aria-expanded'], 'false');
  assert.equal(foldCaretAttributes(false)['aria-hidden'], undefined);
  const source = (name: string) => readFileSync(`src/lib/${name}`, 'utf8');
  assert.ok(!/role="(tree|treeitem|group)"/.test(source('TagsLinksView.svelte')));
  for (const name of ['TagsLinksView.svelte', 'HomePage.svelte', 'SyncConflictsPanel.svelte']) {
    for (const tag of source(name).match(/<div\b[^>]*\baria-label=[^>]*>/g) ?? []) {
      assert.match(tag, /\brole="/, `${name}: ${tag}`);
    }
  }
});

test('the task calendar shows why the file picker did not open', async () => {
  vaultStore.vaultSnapshot = {rootPath:'Cal',name:'Cal',tree:[],noteCount:0};
  calendarControl.pick = async () => { throw new Error('Cannot open the file picker.'); };
  await taskCalendarStore.choosePath();
  assert.equal(taskCalendarStore.lastError, 'Cannot open the file picker.');
  assert.equal(taskCalendarStore.path, '');
  assert.deepEqual(calendarControl.writes, []);
});

test('hidden frontmatter reparses only for edits near it or on the first line', () => {
  const check = (doc: string, at: number, hiddenTo: number | null) =>
    frontmatterNeedsReparse(EditorState.create({doc}).doc, ChangeSet.of({from:at, insert:'x'}, doc.length), hiddenTo);
  const fm = '---\na: 1\n---\nbody\nmore\n';
  const closingEnd = fm.indexOf('\nbody');
  assert.equal(check(fm, fm.indexOf('a:'), closingEnd), true);
  assert.equal(check(fm, fm.indexOf('body') + 2, closingEnd), true);
  assert.equal(check(fm, fm.indexOf('more') + 2, closingEnd), false);
  const plain = 'Title\nbody\n';
  assert.equal(check(plain, 2, null), true);
  assert.equal(check(plain, plain.indexOf('body'), null), false);
  const open = '---\nintro\n\nlater\n';
  assert.equal(check(open, open.indexOf('later'), null), true);
});

// Regression tests for the review leftovers. The frontmatter and due-date cases repeat the Rust
// tests in src-tauri/src/links.rs and src-tauri/src/tasks.rs, so both parsers stay in step.

test('frontmatter detection matches the Rust indexer', () => {
  assert.equal(parseFrontmatter('---\nSome intro paragraph #idea\n---\nBody'), null);
  const yaml = parseFrontmatter('---\n# comment\ntags:\n  - a\n\n- b\n"a:b": 1\nempty:\n---\nBody');
  assert.deepEqual(yaml?.properties.map((p) => p.key), ['tags', 'a:b', 'empty']);
  assert.ok(parseFrontmatter('---\r\ntitle: x\r\n---\r\nBody'));
  for (const notYaml of ['---\nurl:http://x\n---\n', '---\n  indented\n---\n', '---\ntitle: x\n# c\n  stray\n---\n']) {
    assert.equal(parseFrontmatter(notYaml), null, notYaml);
  }
});

test('due dates inside code spans are ignored, as the Rust indexer ignores them', () => {
  for (const syntax of ['due:2026-01-01', '📅 2026-01-01', '[due:: 2026-01-01]']) {
    assert.equal(findDueDate(`- [ ] Example \`\`x \` ${syntax} \` y\`\``), null);
    assert.equal(findDueDate(`- [ ] Example \`\`\`${syntax}\`\`\``), null);
  }
  const multi = '- [ ] Example ``é ` due:2026-01-01`` then due:2026-11-06';
  const actual = findDueDate(multi)!;
  assert.equal(actual.date, '2026-11-06');
  assert.equal(multi.slice(actual.from, actual.to), 'due:2026-11-06');
  assert.equal(findDueDate('- [ ] Read `due:2026-01-01` docs'), null);
  assert.equal(findDueDate('- [ ] Run `📅 2026-01-02` then ship 📅 2026-11-03')?.date, '2026-11-03');
  assert.equal(findDueDate('- [ ] Odd `é` text due:2026-11-04')?.date, '2026-11-04');
  assert.equal(findDueDate('- [ ] Unclosed ` tick due:2026-11-05')?.date, '2026-11-05');
  const line = '- [ ] Run `x` then ship 📅 2026-11-03';
  const found = findDueDate(line)!;
  assert.equal(line.slice(found.from, found.to), '📅 2026-11-03');
});

test('inline integer subtraction works with or without spaces', () => {
  assert.equal(inlineCalculation('100-250 =')?.result, '-150');
  assert.equal(inlineCalculation('100 -250 =')?.result, '-150');
  assert.equal(inlineCalculation('100- 250 =')?.result, '-150');
  assert.equal(inlineCalculation('100 - 250 =')?.result, '-150');
  assert.equal(evaluate('100-250'), -150);
  assert.equal(evaluateCalcBlock(['100-250'])[0].result, '-150');
});

test('ordered list edits preserve the original first number even when later numbers are lower', () => {
  const edit = (doc: string, text: string, operation: (s: EditorState) => any) => {
    const state = EditorState.create({doc, selection:{anchor:doc.indexOf(text)}});
    return state.update(operation(state)).state.doc.toString();
  };
  assert.equal(edit('5) a\n1) b\n2) c', 'b', (s) => moveItem(s, -1)), '5) b\n6) a\n7) c');
  assert.equal(edit('5. a\n1. b\n2. c', 'a', (s) => moveItem(s, 1)), '5. b\n6. a\n7. c');
  assert.equal(edit('5. a\n1. b\n2. c', 'b', (s) => shiftItem(s, 1)), '5. a\n   1. b\n6. c');
  assert.equal(edit('5. a\n   8. x\n   1. y\n2. b', 'x', (s) => shiftItem(s, -1)), '5. a\n6. x\n   8. y\n7. b');
});

test('fold caret uses native button activation and leaves keyboard defaults available', () => {
  class Element {
    attributes: Record<string, string> = {};
    listeners: Record<string, (e: any) => void> = {};
    children: Element[] = [];
    constructor(readonly tagName: string) {}
    setAttribute(name: string, value: string) { this.attributes[name] = value; }
    addEventListener(name: string, listener: (e: any) => void) { this.listeners[name] = listener; }
    append(child: Element) { this.children.push(child); }
  }
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'document');
  Object.defineProperty(globalThis, 'document', {configurable:true, value:{createElement:(name: string) => new Element(name)}});
  try {
    for (const folded of [false, true]) {
      const effects: any[] = [];
      const anchor = new FoldCaretWidget(3, 12, folded).toDOM({dispatch:(spec: any) => effects.push(spec.effects)} as any) as unknown as Element;
      const button = anchor.children[0];
      assert.equal(button.tagName, 'button');
      assert.equal(button.attributes.type, 'button');
      assert.equal(button.attributes['aria-label'], folded ? 'Unfold list item' : 'Fold list item');
      for (const key of ['Enter', ' ', 'Tab']) {
        let stopped = false;
        button.listeners.keydown({key, stopPropagation() { stopped = true; }, preventDefault() { assert.fail('native keyboard defaults must remain enabled'); }});
        assert.equal(stopped, true);
      }
      button.listeners.click({stopPropagation() {}});
      assert.equal(effects.length, 1);
      assert.equal(effects[0].is(folded ? unfoldEffect : foldEffect), true);
      assert.deepEqual(effects[0].value, {from:3, to:12});
    }
  } finally {
    if (previous) Object.defineProperty(globalThis, 'document', previous);
    else Reflect.deleteProperty(globalThis, 'document');
  }
});

test('the outliner caches the frontmatter end and updates it when the top changes', () => {
  const doc = '---\nitems:\n  - a\n    - b\n---\n- x\n  - y\n';
  let state = EditorState.create({doc, extensions:[markdown(), createOutlinerExtension()]});
  const outdentAt = (s: EditorState, lineNo: number) =>
    shiftItem(s.update({selection:{anchor:s.doc.line(lineNo).from}}).state, -1);
  assert.equal(frontmatterEndLine(state), 5);
  assert.equal(outdentAt(state, 4), null);
  assert.ok(outdentAt(state, 7));
  // An edit below the frontmatter keeps the cached line; breaking the opening fence clears it.
  state = state.update({changes:{from:state.doc.length, insert:'- z\n'}}).state;
  assert.equal(frontmatterEndLine(state), 5);
  state = state.update({changes:{from:0, to:3, insert:'+++'}}).state;
  assert.equal(frontmatterEndLine(state), 0);
  assert.ok(outdentAt(state, 4));
  // Typing the closing fence in a note that opens with one finds the new end.
  let open = EditorState.create({doc:'---\ntitle: x\nbody\n', extensions:createOutlinerExtension()});
  assert.equal(frontmatterEndLine(open), 0);
  open = open.update({changes:{from:'---\ntitle: x\n'.length, to:'---\ntitle: x\nbody'.length, insert:'---'}}).state;
  assert.equal(frontmatterEndLine(open), 3);
});

test('spell check finds prose words and skips identifiers', () => {
  const words = (text: string) => findCheckableWords(text).map((w) => w.word);
  assert.deepEqual(words('The quick brwn fox'), ['The', 'quick', 'brwn', 'fox']);
  // Apostrophes stay inside words but not at their edges.
  assert.deepEqual(words("don't ’quoted’ John’s"), ["don't", 'quoted', 'John’s']);
  // Hyphenated words are checked part by part.
  assert.deepEqual(words('well-known'), ['well', 'known']);
  // Acronyms, camelCase, digits, file names, addresses, LaTeX commands, and entities are skipped.
  assert.deepEqual(words('NASA camelCase v2 2nd index.ts me@x.org e.g. \\frac &nbsp; a ok'), ['ok']);
  const [first] = findCheckableWords("  'hello", 10);
  assert.deepEqual(first, {from:13, to:18, word:'hello'});
});

test('spell check skips code, links, tags, math, and frontmatter', () => {
  const doc = [
    '---',
    'title: Notte',
    '---',
    'Prose wrods with `codde` and [labbel](https://exmple.com/pathh).',
    '[[Wikki Page]] and #tagg and $x + yy$ cost $5 and $10.',
    '```',
    'fencedd text',
    '```',
    '<span class="hidn">shown</span> https://barre.example.org done',
    '<!-- commnt -->',
  ].join('\n');
  const state = EditorState.create({doc, extensions:markdown({base:markdownLanguage})});
  const words = collectCheckableWords(state, [{from:0, to:doc.length}]).map((w) => w.word);
  assert.deepEqual(words, [
    'Prose', 'wrods', 'with', 'and', 'labbel',
    'and', 'and', 'cost', 'and',
    // Text between inline HTML tags is prose; the tags and their attributes are not.
    'shown', 'done',
  ]);
  // The word under the cursor is left alone while it is being typed.
  const at = doc.indexOf('wrods') + 5;
  assert.ok(!collectCheckableWords(state, [{from:0, to:doc.length}], at).some((w) => w.word === 'wrods'));
});

test('spell check languages come from the note, then the vault', () => {
  const config = {vaultLanguages:['en-US'], available:['en-US', 'en-GB', 'es', 'pt-BR'], check:async () => []};
  const langs = (doc: string) => resolveNoteLanguages(EditorState.create({doc}), config);
  assert.deepEqual(langs('Plain note'), ['en-US']);
  assert.deepEqual(langs('---\nlang: es-MX\n---\nHola'), ['es']);
  assert.deepEqual(langs('---\nlang: [en-GB, pt]\n---\n'), ['en-GB', 'pt-BR']);
  assert.deepEqual(langs('---\nlang: en_gb\n---\n'), ['en-GB']);
  // A language without a dictionary checks nothing rather than flagging every word.
  assert.deepEqual(langs('---\nlang: ja\n---\n'), []);
  assert.deepEqual(langs('---\nspellcheck: false\nlang: es\n---\n'), []);
  assert.deepEqual(langs('---\ntitle: x\n---\n'), ['en-US']);
  assert.deepEqual(matchDictionaries(['en', 'EN-us', 'fr'], config.available), ['en-US']);
});


test('shortcut hints follow the desktop platform', () => {
  assert.equal(isMacPlatform('MacIntel'), true);
  assert.equal(isMacPlatform('Win32'), false);
  assert.equal(shortcutLabel('Copy (Ctrl+Shift+C)', true), 'Copy (Cmd+Shift+C)');
  assert.equal(shortcutLabel('Ctrl+Alt+J', true), 'Cmd+Option+J');
  assert.equal(shortcutLabel('Ctrl+O', false), 'Ctrl+O');
});

test('shared vault names reject nonportable entries on every desktop', () => {
  for (const name of ['CON', 'note.', 'a/b', 'a:b', 'note ', '.hidden']) {
    assert.notEqual(validateEntryName(name), null, name);
  }
  assert.equal(validateEntryName('Reunión.md'), null);
});


test('failed summon changes restore the last working setting and keep the error', async () => {
  const calls: string[] = [];
  systemControl.configure = async (enabled, shortcut) => {
    calls.push(`${enabled}:${shortcut}`);
    if (shortcut === 'occupied') throw new Error('Shortcut is occupied');
  };
  settingsStore.setSummon(true, 'Ctrl+Alt+J');
  await systemStore.syncSummon();
  settingsStore.setSummon(true, 'occupied');
  await systemStore.syncSummon();
  assert.equal(settingsStore.summonEnabled, true);
  assert.equal(settingsStore.summonShortcut, 'Ctrl+Alt+J');
  await systemStore.syncSummon();
  assert.equal(systemStore.summonError, 'Shortcut is occupied');
  assert.equal(calls.length, 2);
  settingsStore.setSummon(false);
  await systemStore.syncSummon();
  assert.equal(systemStore.summonError, '');
});

test('the outline reads ATX headings and ignores frontmatter and fenced code', () => {
  const note = [
    '---',
    'title: Notes',
    'tags: [a]',
    '---',
    '# Top',
    'text',
    '## Second',
    '```bash',
    '# not a heading',
    '```',
    '    # indented code, not a heading',
    '### Third ###',
    '#no-space-is-a-tag',
    '   ## Three spaces still counts',
  ].join('\n');
  assert.deepEqual(parseOutline(note), [
    { level: 1, text: 'Top', line: 5 },
    { level: 2, text: 'Second', line: 7 },
    { level: 3, text: 'Third', line: 12 },
    { level: 2, text: 'Three spaces still counts', line: 14 },
  ]);
  // A tilde fence does not close a backtick fence.
  assert.deepEqual(parseOutline('```\n~~~\n# hidden\n```\n# shown'), [
    { level: 1, text: 'shown', line: 5 },
  ]);
});

test('the outline highlights the heading the cursor sits in', () => {
  const headings = parseOutline('intro\n# One\na\n## Two\nb');
  assert.equal(activeHeadingIndex(headings, 1), -1);
  assert.equal(activeHeadingIndex(headings, 2), 0);
  assert.equal(activeHeadingIndex(headings, 3), 0);
  assert.equal(activeHeadingIndex(headings, 4), 1);
  assert.equal(activeHeadingIndex(headings, 99), 1);
  assert.equal(activeHeadingIndex([], 5), -1);
});

test('outline fences follow Markdown marker length, trailing text, and container rules', () => {
  for (const note of [
    '~~~~\n~~~\n# hidden\n~~~~\n# shown',
    '~~~\n~~~ trailing text\n# hidden\n~~~\n# shown',
    '````\n```\n# hidden\n````\n# shown',
    '> ~~~\n> # hidden\n> ~~~\n# shown',
    '    ~~~\n    # hidden\n    ~~~\n# shown',
  ]) assert.deepEqual(parseOutline(note).map((heading) => heading.text), ['shown']);
});

test('outline distinguishes thematic breaks from YAML and preserves CRLF line numbers', () => {
  assert.deepEqual(parseOutline('---\n# Visible\n---\n# End').map((heading) => heading.text), ['Visible', 'End']);
  assert.deepEqual(parseOutline('---\r\ntitle: Note\r\n# YAML comment\r\n...\r\n## Shown ###\r\n'), [
    { level: 2, text: 'Shown', line: 5 },
  ]);
});

test('outline jumps cancel on a tab switch and do not consume a newer request', () => {
  tabsStore.resetTabs();
  tabsStore.activeTabPath = 'A.md';
  tabsStore.requestScrollToLine(10);
  const old = tabsStore.pendingScrollTarget!;
  tabsStore.activeTabPath = 'B.md';
  tabsStore.activeTabPath = 'A.md';
  assert.equal(tabsStore.consumeScrollTarget(old), false);
  assert.equal(tabsStore.pendingScrollTarget, null);
  tabsStore.activeTabPath = 'B.md';
  tabsStore.requestScrollToLine(10);
  const first = tabsStore.pendingScrollTarget!;
  tabsStore.requestScrollToLine(10);
  const second = tabsStore.pendingScrollTarget!;
  assert.notEqual(first.seq, second.seq);
  assert.equal(tabsStore.consumeScrollTarget(first), false);
  assert.equal(tabsStore.pendingScrollTarget?.seq, second.seq);
  assert.equal(tabsStore.consumeScrollTarget(second), true);
});

test('external tree updates hide missing pins and restore them without losing saved order', () => {
  const file = (relPath: string): VaultNode => ({ kind: 'file', name: relPath, relPath, modifiedMs: 0, createdMs: 0, children: [] });
  vaultStore.vaultSnapshot = { rootPath: 'B', name: 'B', tree: [file('Keep.md'), file('Gone.md')], noteCount: 2 };
  vaultStore.pinnedPaths = ['Gone.md', 'Keep.md'];
  assert.deepEqual(vaultStore.visiblePinnedPaths, ['Gone.md', 'Keep.md']);
  vaultStore.vaultSnapshot = { ...vaultStore.vaultSnapshot, tree: [file('Keep.md')], noteCount: 1 };
  assert.deepEqual(vaultStore.visiblePinnedPaths, ['Keep.md']);
  assert.deepEqual(vaultStore.pinnedPaths, ['Gone.md', 'Keep.md']);
  vaultStore.vaultSnapshot = { ...vaultStore.vaultSnapshot, tree: [file('Keep.md'), file('Gone.md')], noteCount: 2 };
  assert.deepEqual(vaultStore.visiblePinnedPaths, ['Gone.md', 'Keep.md']);
  vaultStore.vaultSnapshot = { ...vaultStore.vaultSnapshot, tree: [file('keep.md'), file('Gone.md')], noteCount: 2 };
  assert.deepEqual(vaultStore.visiblePinnedPaths, isMac || isWindows ? ['Gone.md', 'keep.md'] : ['Gone.md']);
  assert.equal(vaultStore.isPinned('keep.md'), isMac || isWindows);
});

test('outline indents are relative to the shallowest heading in the note', () => {
  assert.deepEqual(outlineDepths(parseOutline('## A\n### B\n## C')), [0, 1, 0]);
  assert.deepEqual(outlineDepths(parseOutline('# A\n## B')), [0, 1]);
  assert.deepEqual(outlineDepths([]), []);
});

test('pinning a note keeps the backend order and unpinning removes it', async () => {
  pinControl.pinned = [];
  await vaultStore.openVaultByPath('B');
  assert.deepEqual(vaultStore.pinnedPaths, []);

  await vaultStore.togglePin('a.md');
  await vaultStore.togglePin('Work/b.md');
  assert.deepEqual(vaultStore.pinnedPaths, ['a.md', 'Work/b.md']);
  assert.equal(vaultStore.isPinned('Work/b.md'), true);
  assert.equal(vaultStore.isPinned('missing.md'), false);

  await vaultStore.togglePin('a.md');
  assert.deepEqual(vaultStore.pinnedPaths, ['Work/b.md']);
});

test('closing a vault drops its pins so the next vault starts clean', async () => {
  pinControl.pinned = ['a.md'];
  await vaultStore.openVaultByPath('B');
  assert.deepEqual(vaultStore.pinnedPaths, ['a.md']);
  await vaultStore.switchVault();
  assert.deepEqual(vaultStore.pinnedPaths, []);
});

test('a narrow window drops the sidebar, and showing it asks for exactly the width it needs', () => {
  const sidebar = designTokens.layout.sidebarDefaultWidth;
  const needed = widthForSidebar(sidebar);
  assert.equal(needed, sidebar + designTokens.layout.editorMinWidth);
  assert.equal(sidebarFitsWindow(needed, sidebar), true);
  assert.equal(sidebarFitsWindow(needed - 1, sidebar), false);
  // The window minimum must leave room for the text column once the sidebar steps aside.
  assert.ok(designTokens.layout.windowMinWidth >= designTokens.layout.editorMinWidth);
  assert.ok(designTokens.layout.windowMinWidth < needed);
  // A wider sidebar needs a wider window, so dragging it out hides it sooner.
  assert.ok(widthForSidebar(designTokens.layout.sidebarMaxWidth) > needed);
});

test('the sidebar cannot be dragged wider than the window can hold', () => {
  const { sidebarMinWidth, sidebarMaxWidth, editorMinWidth, windowMinWidth } = designTokens.layout;
  // A roomy window allows the full range.
  assert.equal(maxSidebarWidth(2000), sidebarMaxWidth);
  // A narrow window stops the drag where the text column would start losing space.
  assert.equal(maxSidebarWidth(800), 800 - editorMinWidth);
  // Dragging to the ceiling never hides the sidebar it is dragging.
  for (const width of [windowMinWidth, 700, 900, 1400]) {
    assert.ok(sidebarFitsWindow(width, maxSidebarWidth(width)) || width < widthForSidebar(sidebarMinWidth));
  }
  // The floor wins in a window too narrow for any sidebar at all.
  assert.equal(maxSidebarWidth(windowMinWidth), sidebarMinWidth);
});

test('the right panel yields its width before the sidebar does', () => {
  const { sidebarDefaultWidth, editorMinWidth, rightPanelWidth } = designTokens.layout;
  const bothFit = sidebarDefaultWidth + rightPanelWidth + editorMinWidth;
  assert.equal(rightPanelFitsWindow(bothFit, sidebarDefaultWidth, true), true);
  assert.equal(rightPanelFitsWindow(bothFit - 1, sidebarDefaultWidth, true), false);

  // A user-collapsed sidebar lets the panel use its space.
  assert.equal(rightPanelFitsWindow(rightPanelWidth + editorMinWidth, sidebarDefaultWidth, false), true);

  // At every width, the panel goes no later than the sidebar does.
  for (const width of [480, 560, 608, 700, 856, 1120]) {
    const sidebarFits = sidebarFitsWindow(width, sidebarDefaultWidth);
    const panelFits = rightPanelFitsWindow(width, sidebarDefaultWidth, true);
    if (panelFits) assert.ok(sidebarFits, `panel outlived the sidebar at ${width}`);
  }
});

test('the title bar gives up its labels below the compact width', () => {
  const compactAt = designTokens.layout.titleBarCompactWidth;
  assert.equal(titleBarIsCompact(compactAt), false);
  assert.equal(titleBarIsCompact(compactAt - 1), true);
  // The narrowest window the user can drag to must still be a compact one.
  assert.equal(titleBarIsCompact(designTokens.layout.windowMinWidth), true);
});

test('tray and close-button settings reach the backend once per change', async () => {
  const calls: string[] = [];
  systemControl.configureWindow = async (tray, hide) => {
    calls.push(`${tray}:${hide}`);
    if (tray && hide) throw new Error('Cannot change the tray icon');
  };
  settingsStore.trayEnabled = true;
  settingsStore.closeAction = 'quit';
  await systemStore.syncWindowBehavior();
  await systemStore.syncWindowBehavior();
  assert.deepEqual(calls, ['true:false']);
  assert.equal(systemStore.trayError, '');

  settingsStore.closeAction = 'tray';
  await systemStore.syncWindowBehavior();
  assert.deepEqual(calls, ['true:false', 'true:true']);
  assert.equal(systemStore.trayError, 'Cannot change the tray icon');
  assert.equal(settingsStore.trayEnabled, true);
  assert.equal(settingsStore.closeAction, 'quit');

  // A failed change is not recorded as applied, so the next sync retries it.
  settingsStore.trayEnabled = false;
  settingsStore.closeAction = 'quit';
  await systemStore.syncWindowBehavior();
  assert.deepEqual(calls, ['true:false', 'true:true', 'false:false']);
  assert.equal(systemStore.trayError, '');
});

test('AI targets skip frontmatter and results apply as one undoable step', () => {
  const doc = '---\ntags: [a]\n---\nFirst line\nSecond line';
  let state = EditorState.create({doc, extensions:[history()], selection:{anchor:20, head:30}});
  const note = aiTargetRange(state, 'note')!;
  assert.equal(note.text, 'First line\nSecond line');
  assert.equal(aiTargetRange(state, 'selection')!.text, doc.slice(20, 30));
  assert.equal(aiTargetRange(EditorState.create({doc:'  \n'}), 'note'), null);

  state = state.update(aiResultChange(state, note, 'Primera\nSegunda', 'replace')).state;
  assert.equal(state.doc.toString(), '---\ntags: [a]\n---\nPrimera\nSegunda');
  let undone: EditorState | null = null;
  undo({state, dispatch: (tr) => {undone = tr.state;}});
  assert.equal(undone!.doc.toString(), doc);
});

test('AI results insert below as their own paragraph', () => {
  const below = (doc: string, from: number, to: number) => {
    const state = EditorState.create({doc});
    return state.update(aiResultChange(state, {from, to}, ' Summary ', 'below')).state.doc.toString();
  };
  assert.equal(below('Alpha', 0, 5), 'Alpha\n\nSummary');
  assert.equal(below('Alpha\nBeta', 0, 5), 'Alpha\n\nSummary\n\nBeta');
  assert.equal(below('Alpha\n\nBeta', 0, 6), 'Alpha\n\nSummary\n\nBeta');
  assert.equal(below('Alpha\n', 0, 6), 'Alpha\n\nSummary\n');
});

test('AI menu opens centered and stays within the viewport', () => {
  const menu = {width: 440, height: 380};
  const viewport = {width: 1200, height: 800};
  assert.deepEqual(aiPopoverPosition(menu, viewport), {left:380,top:210});
  assert.deepEqual(aiPopoverPosition({width:440,height:560}, viewport), {left:380,top:120});
  // A long preview must not push controls out of reach on a small window.
  const small = {width:320,height:480};
  const bounded = {width:296,height:456};
  assert.deepEqual(aiPopoverPosition(bounded, small), {left:12,top:12});
});

test('a moved AI menu stays within bounds after dragging or resizing', () => {
  const menu = {width:440,height:380};
  const viewport = {width:1200,height:800};
  assert.deepEqual(clampAiPopoverPosition({left:250,top:120},menu,viewport), {left:250,top:120});
  assert.deepEqual(clampAiPopoverPosition({left:-200,top:-300},menu,viewport), {left:12,top:12});
  assert.deepEqual(clampAiPopoverPosition({left:5000,top:5000},menu,viewport), {left:748,top:408});
  assert.deepEqual(clampAiPopoverPosition({left:748,top:408},menu,{width:640,height:460}), {left:188,top:68});
});

function moveTestSnapshot(): VaultSnapshot {
  const node = (relPath: string, kind: 'file' | 'folder', children: VaultNode[] = []): VaultNode => ({
    relPath, name:relPath.split('/').pop()!, kind, children, modifiedMs:0, createdMs:0,
  });
  return {rootPath:'move-test-vault',name:'Move test',noteCount:3,tree:[
    node('A.md','file'), node('Taken.md','file'),
    node('Folder','folder',[
      node('Folder/Taken.md','file'), node('Folder/Nested','folder'),
    ]),
  ]};
}

test('note moves retain filenames and reject collisions and missing destinations', () => {
  const {tree} = moveTestSnapshot();
  assert.equal(noteMoveDestination(tree,'A.md','Folder'), 'Folder/A.md');
  assert.equal(noteMoveDestination(tree,'A.md','Folder/Nested'), 'Folder/Nested/A.md');
  assert.equal(noteMoveDestination(tree,'Folder/Taken.md','Folder/Nested'), 'Folder/Nested/Taken.md');
  assert.equal(noteMoveDestination(tree,'A.md',''), null);
  assert.equal(noteMoveDestination(tree,'Folder/Taken.md','Folder'), null);
  assert.throws(()=>noteMoveDestination(tree,'Taken.md','Folder'), /already exists/);
  assert.throws(()=>noteMoveDestination(tree,'Folder/Taken.md',''), /already exists/);
  assert.throws(()=>noteMoveDestination(tree,'A.md','missing'), /destination folder/);
  assert.throws(()=>noteMoveDestination(tree,'missing.md','Folder'), /dragged note/);
  assert.throws(()=>noteMoveDestination(tree,'Folder',''), /dragged note/);
});

test('dropping an open note saves its edits before moving and follows its new path', async () => {
  const oldSnapshot = vaultStore.vaultSnapshot;
  const oldSave = control.save;
  const oldRename = control.rename;
  const snapshot = moveTestSnapshot();
  dirtyTab();
  vaultStore.vaultSnapshot = snapshot;
  const calls: string[] = [];
  control.save = async (path,content) => {calls.push(`save:${path}:${content}`);return {relPath:path,title:path,content};};
  control.rename = async (oldRelPath,newRelPath) => {
    calls.push(`move:${oldRelPath}:${newRelPath}`);
    return {oldRelPath,newRelPath,snapshot};
  };
  try {
    assert.equal(await vaultStore.moveNoteIntoFolder('A.md','Folder',snapshot.rootPath), true);
    assert.deepEqual(calls,['save:A.md:unsaved','move:A.md:Folder/A.md']);
    assert.equal(tabsStore.activeTabPath,'Folder/A.md');
    assert.equal(tabsStore.openTabs[0].content,'unsaved');
    assert.equal(tabsStore.openTabs[0].dirty,false);
    assert.equal(await vaultStore.moveNoteIntoFolder('A.md','',snapshot.rootPath),false);
    assert.equal(await vaultStore.moveNoteIntoFolder('A.md','Folder','another-vault'),false);
    assert.equal(calls.length,2);
  } finally {
    control.save = oldSave;
    control.rename = oldRename;
    vaultStore.vaultSnapshot = oldSnapshot;
    tabsStore.resetTabs();
  }
});

test('note drops preserve the original path when saving or moving fails or the vault switches', async () => {
  const oldSnapshot = vaultStore.vaultSnapshot;
  const oldSave = control.save;
  const oldRename = control.rename;
  const snapshot = moveTestSnapshot();
  let renames = 0;
  control.rename = async () => {renames++;throw Error('destination already exists');};
  try {
    dirtyTab();
    vaultStore.vaultSnapshot = snapshot;
    control.save = async () => {throw Error('disk full');};
    assert.equal(await vaultStore.moveNoteIntoFolder('A.md','Folder',snapshot.rootPath),false);
    assert.equal(renames,0);
    assert.equal(tabsStore.openTabs[0].dirty,true);
    control.save = async (path,content) => ({relPath:path,title:path,content});
    assert.equal(await vaultStore.moveNoteIntoFolder('A.md','Folder',snapshot.rootPath),false);
    assert.equal(renames,1);
    assert.equal(tabsStore.activeTabPath,'A.md');

    dirtyTab();
    let finish!: () => void;
    const waiting = new Promise<void>((resolve) => {finish=resolve;});
    control.save = async (path,content) => {await waiting;return {relPath:path,title:path,content};};
    const moving = vaultStore.moveNoteIntoFolder('A.md','Folder',snapshot.rootPath);
    vaultStore.vaultSnapshot = {...snapshot,rootPath:'different-vault'};
    finish();
    assert.equal(await moving,false);
    assert.equal(renames,1);
    assert.equal(tabsStore.activeTabPath,'A.md');
  } finally {
    control.save = oldSave;
    control.rename = oldRename;
    vaultStore.vaultSnapshot = oldSnapshot;
    tabsStore.resetTabs();
  }
});

test('AI selections retain full-document protection context', () => {
  const selection = (doc: string, text: string) => aiTargetRange(EditorState.create({
    doc, selection: {anchor: doc.indexOf(text), head: doc.indexOf(text) + text.length},
  }), 'selection');
  for (const doc of [
    '```sh\nsecret_command\n```',
    '> ```sh\n> secret_command\n> ```',
    'Intro\n\n    secret_command\n',
    'Run `secret_command` now.',
    'Run `first\nsecret_command` now.',
    'See [[secret_command]].',
    'See [guide](secret_command).',
    '[guide]: /secret_command.md',
    'Math $secret_command$ here.',
    '$$\nsecret_command\n$$',
    'Tagged #secret_command today.',
  ]) {
    assert.throws(() => selection(doc, 'secret_command'), /protected Markdown/, doc);
  }
  assert.throws(() => selection('---\ntitle: secret\n---\nBody', 'secret'), /note properties/);
  assert.equal(selection('Read `code` today.', 'Read `code`')?.text, 'Read `code`');
  assert.equal(selection('```sh\ncode\n```', '```sh\ncode\n```')?.text, '```sh\ncode\n```');
  assert.equal(selection('Read `code` today.', 'today')?.text, 'today');
});

test('AI settings serialize overlapping edits and recover after a failed save', async () => {
  const originalStatus = aiStore.status;
  const originalSave = aiControl.saveConfig;
  const config: AiConfig = {
    enabled: true, provider: 'openai-compatible', styleInstructions: '',
    openaiCompatible: {baseUrl:'https://old.example',model:'old'},
    anthropic: {baseUrl:'https://anthropic.example',model:'a'},
    ollama: {baseUrl:'http://localhost:11434',model:'o'},
  };
  const status: AiStatus = {config,keys:[],credentialStore:'mock',defaultStyleInstructions:''};
  aiStore.status = status;
  const pending: Array<{config: AiConfig; resolve: (status: AiStatus) => void}> = [];
  aiControl.saveConfig = (config) => new Promise((resolve) => pending.push({config,resolve}));
  try {
    const url = aiStore.setEndpointField('openai-compatible', 'baseUrl', 'https://new.example');
    const model = aiStore.setEndpointField('openai-compatible', 'model', 'new-model');
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(pending.length, 1);
    pending[0].resolve({...status,config:pending[0].config});
    await url;
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(pending.length, 2);
    assert.equal(pending[1].config.openaiCompatible.baseUrl, 'https://new.example');
    pending[1].resolve({...status,config:pending[1].config});
    await model;
    assert.deepEqual(aiStore.status!.config.openaiCompatible, {baseUrl:'https://new.example',model:'new-model'});

    let calls = 0;
    aiControl.saveConfig = async (config) => {
      if (++calls === 1) throw new Error('disk full');
      return {...status,config};
    };
    const failed = aiStore.setEndpointField('openai-compatible', 'model', 'rejected');
    const next = aiStore.updateConfig((c) => {c.styleInstructions = 'Be direct.';});
    await assert.rejects(failed, /disk full/);
    await next;
    assert.equal(aiStore.status!.config.openaiCompatible.model, 'new-model');
    assert.equal(aiStore.status!.config.styleInstructions, 'Be direct.');
  } finally {
    aiControl.saveConfig = originalSave;
    aiStore.status = originalStatus;
  }
});

test('AI store ignores late results after cancel and reports errors', async () => {
  let release: (r: AiResult) => void = () => {};
  aiControl.run = (req) => new Promise((resolve) => {
    release = (r) => resolve({...r, operationId: req.operationId});
  });
  const pending = aiStore.start({kind:'summarize'}, 'text');
  assert.equal(aiStore.run.phase, 'running');
  aiStore.cancel();
  assert.equal(aiStore.run.phase, 'idle');
  assert.equal(aiControl.cancels.length, 1);
  release({operationId:'', text:'late', provider:'ollama', model:'m', usage:{inputTokens:null, outputTokens:null}});
  await pending;
  assert.equal(aiStore.run.phase, 'idle');

  aiControl.run = async () => {throw 'The provider rejected the API key. (HTTP 401)';};
  await aiStore.start({kind:'rewrite', style:'polish'}, 'text');
  assert.deepEqual(aiStore.run, {phase:'error', message:'The provider rejected the API key. (HTTP 401)'});
  aiStore.panelOperation = 'translate';
  aiStore.targetLanguage = ' German ';
  assert.deepEqual(aiStore.operation(), {kind:'translate', targetLanguage:'German'});
});

test('saved AI rewrite choices migrate Clearer to Polish and retain distinct actions', () => {
  const previous = aiStore.rewriteStyle;
  const operation = aiStore.panelOperation;
  try {
    aiStore.panelOperation = 'rewrite';
    for (const [saved, expected] of [
      ['clearer', 'polish'], ['polish', 'polish'], ['shorter', 'shorter'],
      ['grammar', 'grammar'], ['formal', 'formal'], ['casual', 'casual'],
      [null, 'polish'], ['unknown', 'polish'], ['toString', 'polish'],
    ] as const) {
      aiStore.setRewriteStyle(normalizeRewriteStyle(saved));
      assert.deepEqual(aiStore.operation(), {kind:'rewrite',style:expected});
    }
  } finally {
    aiStore.setRewriteStyle(previous);
    aiStore.panelOperation = operation;
  }
});

test('AI command tabs retain independent results and errors when switching screens', async () => {
  const originalRun = aiControl.run;
  aiStore.closePanel();
  const cancelledBefore = aiControl.cancels.length;
  aiControl.run = async (request) => {
    if (request.operation.kind === 'summarize') throw Error('Summary failed');
    return {operationId:request.operationId,text:`${request.operation.kind} result`,provider:'ollama',model:'m',usage:{inputTokens:null,outputTokens:null}};
  };
  try {
    await aiStore.start({kind:'rewrite',style:'polish'}, 'first selection');
    await aiStore.start({kind:'translate',targetLanguage:'French'}, 'whole note');
    await aiStore.start({kind:'summarize'}, 'whole note');
    assert.deepEqual(aiStore.run,{phase:'error',message:'Error: Summary failed'});
    aiStore.openPanel('rewrite');
    assert.equal(aiStore.run.phase,'done');
    assert.equal(aiStore.run.phase === 'done' && aiStore.run.result.text,'rewrite result');
    aiStore.openPanel('translate');
    assert.equal(aiStore.run.phase === 'done' && aiStore.run.result.text,'translate result');
    aiStore.run = {phase:'idle'};
    aiStore.panelOperation = 'rewrite';
    assert.equal(aiStore.run.phase,'done');
    aiStore.panelOperation = 'summarize';
    assert.equal(aiStore.run.phase,'error');
    assert.equal(aiControl.cancels.length,cancelledBefore);
  } finally {
    aiStore.closePanel();
    aiControl.run = originalRun;
  }
});

test('AI requests finish in their own tabs and closing cancels all hidden requests', async () => {
  const originalRun = aiControl.run;
  aiStore.closePanel();
  const requests: Array<{id:string;resolve:()=>void}> = [];
  aiControl.run = (req) => new Promise((resolve) => requests.push({id:req.operationId,resolve:()=>resolve({
    operationId:req.operationId,text:req.operation.kind,provider:'ollama',model:'m',usage:{inputTokens:null,outputTokens:null},
  })}));
  try {
    const rewrite = aiStore.start({kind:'rewrite',style:'polish'},'text');
    const translate = aiStore.start({kind:'translate',targetLanguage:'French'},'text');
    requests[0].resolve();
    await rewrite;
    assert.equal(aiStore.panelOperation,'translate');
    assert.equal(aiStore.run.phase,'running');
    aiStore.panelOperation = 'rewrite';
    assert.equal(aiStore.run.phase === 'done' && aiStore.run.result.text,'rewrite');
    const summary = aiStore.start({kind:'summarize'},'text');
    aiStore.panelOperation = 'rewrite';
    aiStore.closePanel();
    assert.ok(aiControl.cancels.includes(requests[1].id));
    assert.ok(aiControl.cancels.includes(requests[2].id));
    requests[1].resolve();
    requests[2].resolve();
    await Promise.all([translate,summary]);
    for (const kind of ['rewrite','translate','summarize'] as const) {
      aiStore.panelOperation = kind;
      assert.equal(aiStore.run.phase,'idle');
    }
  } finally {
    aiStore.closePanel();
    aiControl.run = originalRun;
  }
});

test('UI language follows the first supported OS language and falls back to English', () => {
  assert.equal(matchSystemLocale(['es-MX', 'en-US']), 'es');
  assert.equal(matchSystemLocale(['fr-FR', 'en-GB']), 'en');
  assert.equal(matchSystemLocale(['pt-BR']), 'en');
  assert.equal(matchSystemLocale([]), 'en');
});

test('changing the UI language switches messages without a reload', () => {
  try {
    i18n.setPreference('es');
    assert.equal(m.common_cancel(), 'Cancelar');
    assert.equal(m.word_count({ count: 1 }), '1 palabra');
    assert.equal(m.word_count({ count: 3 }), '3 palabras');
    i18n.setPreference('en');
    assert.equal(m.common_cancel(), 'Cancel');
    assert.equal(m.word_count({ count: 1 }), '1 word');
  } finally {
    i18n.setPreference('system');
  }
});

test('markup slots split messages around their placeholders', () => {
  assert.deepEqual(splitSlots(m.search_no_match({ query: SLOT }, { locale: 'en' })), ['No notes matched ', '.']);
  assert.deepEqual(
    splitSlots(m.settings_vault_languages_desc({ lang: SLOT, skip: SLOT }, { locale: 'es' })),
    ['Cámbialos por nota con ', ' u omite una nota con ', '.']
  );
});

test('every English message has a Spanish translation with the same placeholders in the same order', () => {
  const placeholders = (value: unknown) => [...JSON.stringify(value).matchAll(/\{(\w+)\}/g)].map((match) => match[1]);
  const { $schema: _en, ...english } = enMessages as Record<string, unknown>;
  const { $schema: _es, ...spanish } = esMessages as Record<string, unknown>;
  assert.deepEqual(Object.keys(spanish).sort(), Object.keys(english).sort());
  for (const [key, value] of Object.entries(english)) {
    // Templates place markup slots in English order, so the order must match too.
    assert.deepEqual(placeholders(spanish[key]), placeholders(value), key);
  }
});

test('locale refresh rebuilds existing preview widgets without an edit or cursor move', () => {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, 'window');
  Object.defineProperty(globalThis, 'window', { configurable: true, value: new EventTarget() });
  const constructors: any[] = [];
  const original = ViewPlugin.fromClass;
  const capture = mock.method(ViewPlugin, 'fromClass', (cls: any, spec: any) => {
    if (spec?.decorations) constructors.push(cls);
    return original.call(ViewPlugin, cls, spec);
  });
  const plugins: any[] = [];
  const widgets = (set: any) => {
    const result: any[] = [];
    set.between(0, 10000, (_from: number, _to: number, value: any) => {
      if (value.spec.widget?.locale) result.push(value.spec.widget);
    });
    return result;
  };
  try {
    i18n.setPreference('en');
    const extensions = [
      markdown({ base: markdownLanguage }),
      createLivePreviewExtension({ editingMode: 'live', dimInactiveParagraphs: false, typewriterScrolling: false }),
      createCalcExtension(),
    ];
    const state = EditorState.create({
      doc: 'Cursor\n\n- [ ] Task 📅 2026-12-25\n\n[[Target]]\n\n![Image](test.png)\n\n2+2=\n\n```calc\n1/0\n```\n\n$$\nx^2\n$$',
      extensions,
    });
    const view = { state, visibleRanges: [{ from: 0, to: state.doc.length }] };
    plugins.push(...constructors.map((cls) => new cls(view)));
    const before = plugins.flatMap((plugin) => widgets(plugin.decorations));
    const blockWidgets = (s: EditorState) => s.facet(EditorView.decorations)
      .filter((value) => typeof value !== 'function').flatMap(widgets);
    const blocksBefore = blockWidgets(state);
    for (const name of ['TaskCheckboxWidget', 'WikiLinkWidget', 'ImageEmbedWidget', 'DueChipWidget', 'InlineResultWidget', 'BlockResultWidget']) {
      assert.ok(before.some((widget) => widget.constructor.name === name), name);
    }
    assert.ok(blocksBefore.some((widget) => widget.constructor.name === 'RichBlockWidget'));
    i18n.setPreference('es');
    const transaction = state.update({ effects: refreshEditorLocale.of() });
    view.state = transaction.state;
    for (const plugin of plugins) plugin.update({
      view, state: transaction.state, startState: state, transactions: [transaction],
      docChanged: false, selectionSet: false, viewportChanged: false,
    });
    const after = plugins.flatMap((plugin) => widgets(plugin.decorations));
    assert.equal(after.length, before.length);
    for (let index = 0; index < after.length; index++) {
      assert.equal(after[index].locale, 'es');
      assert.equal(before[index].eq(after[index]), false, before[index].constructor.name);
    }
    const blocksAfter = blockWidgets(transaction.state);
    assert.equal(blocksAfter.length, blocksBefore.length);
    assert.equal(blocksBefore[0].eq(blocksAfter[0]), false);
    assert.strictEqual(transaction.state.doc, state.doc);
    assert.strictEqual(transaction.state.selection, state.selection);
  } finally {
    for (const plugin of plugins) plugin.destroy?.();
    capture.mock.restore();
    if (previousWindow) Object.defineProperty(globalThis, 'window', previousWindow);
    else Reflect.deleteProperty(globalThis, 'window');
    i18n.setPreference('system');
  }
});

test('locale refresh preserves unsaved text, undo history, folds, and outline zoom', () => {
  try {
    i18n.setPreference('en');
    let state = EditorState.create({ doc: '- Parent\n  - Child\n\nEnd', extensions: [markdown(), history(), createOutlinerExtension()] });
    state = state.update({ changes: { from: state.doc.length, insert: ' unsaved' } }).state;
    state = state.update(zoomIntoItem(state)!).state;
    const range = foldable(state, 0, state.doc.line(1).to)!;
    state = state.update({ effects: foldEffect.of(range) }).state;
    const before = state;
    i18n.setPreference('es');
    state = state.update({ effects: refreshEditorLocale.of() }).state;
    assert.strictEqual(state.doc, before.doc);
    assert.deepEqual(zoomedRange(state), zoomedRange(before));
    assert.equal(foldedRanges(state).size, 1);
    assert.equal(undoDepth(state), 1);
    const zoomWidgets = state.facet(EditorView.decorations).filter((value) => typeof value !== 'function');
    let translated = false;
    for (const decorations of zoomWidgets) decorations.between(0, state.doc.length, (_from, _to, value) => {
      if (value.spec.widget?.constructor.name === 'ZoomBarWidget') translated = value.spec.widget.locale === 'es';
    });
    assert.equal(translated, true);
  } finally { i18n.setPreference('system'); }
});

test('fold tooltips translate their action and retain the keyboard shortcut', () => {
  try {
    i18n.setPreference('es');
    assert.equal(foldCaretAttributes(false).title, `Plegar el elemento (${isMac ? 'Cmd+Option+[' : 'Ctrl+Shift+['})`);
    assert.equal(foldCaretAttributes(true).title, `Desplegar el elemento (${isMac ? 'Cmd+Option+]' : 'Ctrl+Shift+]'})`);
    const spanish = new FoldCaretWidget(0, 10, false);
    i18n.setPreference('en');
    assert.equal(spanish.eq(new FoldCaretWidget(0, 10, false)), false);
  } finally { i18n.setPreference('system'); }
});

test('Spanish clipboard and selection messages handle one line and one character', () => {
  const locale = { locale: 'es' as const };
  const details = { lines: m.line_count({ count: 1 }, locale), chars: m.character_count({ count: 1 }, locale), mode: 'sin formato' };
  assert.equal(m.status_copied(details, locale), 'Texto copiado: 1 línea (1 carácter) en modo sin formato');
  assert.equal(m.status_cut(details, locale), 'Texto cortado: 1 línea (1 carácter) en modo sin formato');
  assert.equal(m.status_pasted_plain({ ...details, shortcut: 'Ctrl+Shift+V' }, locale), 'Texto pegado: 1 línea (1 carácter) como texto sin formato (Ctrl+Shift+V)');
  assert.equal(m.status_selected({ count: 1 }, locale), '1 seleccionado');
  assert.equal(m.status_selected({ count: 2 }, locale), '2 seleccionados');
  assert.equal(m.character_count({ count: 2 }, locale), '2 caracteres');
  assert.equal(m.line_count({ count: 2 }, locale), '2 líneas');
});

test('diagram error caches keep the language they were rendered in', async () => {
  const source = 'invalid diagram for locale cache regression';
  try {
    i18n.setPreference('en');
    const english = await renderDiagram(source);
    assert.match(english, /Diagram error/);
    i18n.setPreference('es');
    assert.equal(cachedDiagram(source), undefined);
    const spanish = await renderDiagram(source);
    assert.match(spanish, /Error en el diagrama/);
    assert.equal(cachedDiagram(source), spanish);
    i18n.setPreference('en');
    assert.equal(cachedDiagram(source), english);
  } finally { i18n.setPreference('system'); }
});

test('find and replace labels translate and keep the $ placeholders', () => {
  try {
    const phrases = searchPhrases();
    assert.equal(phrases.Find, 'Find');
    assert.equal(phrases['replaced $ matches'], 'Replaced $ matches');
    i18n.setPreference('es');
    const spanish = searchPhrases();
    assert.deepEqual(Object.keys(spanish), Object.keys(phrases));
    assert.equal(spanish.Find, 'Buscar');
    assert.ok(spanish['replaced $ matches'].includes('$'));
    assert.ok(spanish['replaced match on line $'].includes('$'));
  } finally { i18n.setPreference('system'); }
});
