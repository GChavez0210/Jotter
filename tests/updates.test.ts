import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
import { UpdateStore, CHECK_INTERVAL_MS, FIRST_CHECK_DELAY_MS } from '../src/lib/stores/updates.svelte';
import { plainReleaseNotes } from '../src/lib/releaseNotes';
import { updateControl } from './ipc.mock';
import type { UpdateInfo } from '../src/lib/types';
import { i18n, m } from '../src/lib/i18n.svelte';

const offer: UpdateInfo = {
  version: '0.6.0', currentVersion: '0.5.1', notes: '- Added updates', date: '2026-11-01', canInstall: true,
};

function reset() {
  updateControl.check = async () => null;
  updateControl.install = async () => {};
  updateControl.progress = null;
}

test('a check that finds an update keeps it for the dialog', async () => {
  reset();
  updateControl.check = async () => offer;
  const store = new UpdateStore();
  assert.deepEqual(await store.check(true), offer);
  assert.equal(store.info?.version, '0.6.0');
  assert.equal(store.upToDate, false);
  store.openDialog();
  assert.equal(store.dialogOpen, true);
});

test('a check that finds nothing reports the app as current', async () => {
  reset();
  const store = new UpdateStore();
  assert.equal(await store.check(true), null);
  assert.equal(store.upToDate, true);
  store.openDialog();
  assert.equal(store.dialogOpen, false, 'nothing to show');
});

test('a failed manual check shows the error and an automatic one stays quiet', async () => {
  reset();
  updateControl.check = async () => { throw new Error('offline'); };
  const store = new UpdateStore();
  await store.check(false);
  assert.equal(store.error, '');
  await store.check(true);
  assert.equal(store.error, m.update_check_failed());
  assert.equal(store.checking, false);
});

test('a failed check keeps the update an earlier check found', async () => {
  reset();
  updateControl.check = async () => offer;
  const store = new UpdateStore();
  await store.check(false);
  updateControl.check = async () => { throw new Error('offline'); };
  assert.deepEqual(await store.check(false), offer);
  assert.deepEqual(store.info, offer);
});

test('the install waits for notes to save and stops if they do not', async () => {
  reset();
  updateControl.check = async () => offer;
  let installs = 0;
  updateControl.install = async () => { installs++; };
  const store = new UpdateStore();
  await store.check(false);
  await store.install(async () => false);
  assert.equal(installs, 0);
  assert.notEqual(store.error, '');
  assert.equal(store.installing, false);
  await store.install(async () => true);
  assert.equal(installs, 1);
  assert.equal(store.error, '');
});

test('download progress becomes a fraction and then an installing stage', async () => {
  reset();
  updateControl.check = async () => offer;
  const seen: Array<[string, number | null]> = [];
  const store = new UpdateStore();
  updateControl.install = async () => {
    updateControl.progress?.({ phase: 'downloading', downloaded: 25, total: 100 });
    seen.push([store.stage, store.progress]);
    updateControl.progress?.({ phase: 'downloading', downloaded: 5, total: null });
    seen.push([store.stage, store.progress]);
    updateControl.progress?.({ phase: 'installing' });
    seen.push([store.stage, store.progress]);
  };
  await store.check(false);
  await store.install(async () => true);
  assert.deepEqual(seen, [['downloading', 0.25], ['downloading', null], ['installing', null]]);
  assert.equal(updateControl.progress, null, 'listener removed');
  assert.equal(store.stage, 'idle');
});

test('an install that cannot replace itself never starts', async () => {
  reset();
  updateControl.check = async () => ({ ...offer, canInstall: false });
  let installs = 0;
  updateControl.install = async () => { installs++; };
  const store = new UpdateStore();
  await store.check(false);
  await store.install(async () => true);
  assert.equal(installs, 0);
});

test('an in-flight check excludes installation and an open dialog keeps its offer', async () => {
  reset();
  const store = new UpdateStore();
  store.info = offer;
  let resolveCheck!: (info: UpdateInfo) => void;
  updateControl.check = () => new Promise((resolve) => { resolveCheck = resolve; });
  let saves = 0;
  let installs = 0;
  updateControl.install = async () => { installs++; };
  const checking = store.check(false);
  await store.install(async () => { saves++; return true; });
  assert.equal(saves, 0);
  assert.equal(installs, 0);
  resolveCheck({ ...offer, version: '0.7.0' });
  await checking;
  store.openDialog();
  updateControl.check = async () => { throw new Error('must not check while confirming'); };
  assert.equal((await store.check(false))?.version, '0.7.0');
  assert.equal(store.info?.version, '0.7.0');
  updateControl.install = async (version) => { assert.equal(version, '0.7.0'); installs++; };
  await store.install(async () => true);
  assert.equal(installs, 1);
});

test('save and installation failures leave no listener and show translated errors', async () => {
  reset();
  const previous = i18n.preference;
  i18n.setPreference('es');
  try {
    const store = new UpdateStore();
    store.info = offer;
    let installs = 0;
    updateControl.install = async () => { installs++; throw new Error('private OS detail'); };
    await store.install(async () => { throw new Error('save failed'); });
    assert.equal(installs, 0);
    assert.equal(store.error, m.update_save_failed());
    await store.install(async () => true);
    assert.equal(store.error, m.update_install_failed());
    assert.equal(store.installing, false);
    assert.equal(store.stage, 'idle');
    assert.equal(updateControl.progress, null);
    updateControl.install = async () => { throw 'update_offer_changed'; };
    await store.install(async () => true);
    assert.equal(store.error, m.update_offer_changed());
  } finally {
    i18n.setPreference(previous);
  }
});

test('a second install request while one runs is ignored', async () => {
  reset();
  updateControl.check = async () => offer;
  let installs = 0;
  let release!: () => void;
  updateControl.install = () => { installs++; return new Promise<void>((resolve) => { release = resolve; }); };
  const store = new UpdateStore();
  await store.check(false);
  const first = store.install(async () => true);
  await new Promise((resolve) => setTimeout(resolve, 0));
  await store.install(async () => true);
  store.closeDialog();
  release();
  await first;
  assert.equal(installs, 1);
});

test('the dialog stays open while an install runs', async () => {
  reset();
  updateControl.check = async () => offer;
  let release!: () => void;
  updateControl.install = () => new Promise<void>((resolve) => { release = resolve; });
  const store = new UpdateStore();
  await store.check(false);
  store.openDialog();
  const running = store.install(async () => true);
  await new Promise((resolve) => setTimeout(resolve, 0));
  store.closeDialog();
  assert.equal(store.dialogOpen, true);
  release();
  await running;
  store.closeDialog();
  assert.equal(store.dialogOpen, false);
});

test('the automatic schedule checks after a delay and then daily, and stops when cancelled', async () => {
  const settle = () => new Promise((resolve) => setImmediate(resolve));
  reset();
  mock.timers.enable({ apis: ['setTimeout', 'setInterval'] });
  try {
    let checks = 0;
    updateControl.check = async () => { checks++; return null; };
    const stop = new UpdateStore().startAutoCheck();
    mock.timers.tick(FIRST_CHECK_DELAY_MS - 1);
    assert.equal(checks, 0);
    mock.timers.tick(1);
    assert.equal(checks, 1);
    await settle();
    mock.timers.tick(CHECK_INTERVAL_MS);
    assert.equal(checks, 2);
    stop();
    mock.timers.tick(CHECK_INTERVAL_MS * 3);
    assert.equal(checks, 2);
  } finally {
    mock.timers.reset();
  }
});

test('release notes lose their Markdown syntax but keep their structure', () => {
  const notes = [
    '### Added',
    '',
    '- **Split view.** Press `Ctrl+Shift+\` for [help](https://example.com).',
    '  - A nested point',
    '',
    '',
    '',
    '### Fixed',
    '- One fix',
  ].join('\r\n');
  assert.equal(
    plainReleaseNotes(notes),
    'Added\n\n• Split view. Press Ctrl+Shift+\ for help.\n  • A nested point\n\nFixed\n• One fix',
  );
});
