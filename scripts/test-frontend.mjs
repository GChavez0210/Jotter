import { build, transform } from 'esbuild';
import { compileModule } from 'svelte/compiler';
import { readFile, mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const output = resolve('src-tauri/target/frontend-tests/tests.mjs');
await mkdir(resolve('src-tauri/target/frontend-tests'), {recursive:true});
await build({
  entryPoints:['tests/frontend.test.ts'], outfile:output, bundle:true,
  platform:'node', conditions:['browser'], format:'esm', loader:{'.css':'empty'},
  plugins:[{name:'store-tests',setup(build) {
    build.onResolve({filter:/\/ipc$|^\.\.\/ipc$/}, () => ({path:resolve('tests/ipc.mock.ts')}));
    build.onResolve({filter:/clipboard\.svelte$/}, () => ({path:'clipboard',namespace:'test'}));
    build.onResolve({filter:/settings\.svelte$/}, () => ({path:'settings',namespace:'test'}));
    build.onLoad({filter:/.*/,namespace:'test'}, ({path}) => ({contents:path==='clipboard'
      ? 'export const clipboardStore = {setStatus() {}};'
      : 'export const settingsStore = {dailyFolder:"Daily",dailyTemplatePath:"",summonEnabled:false,summonShortcut:"Ctrl+Alt+J",trayEnabled:false,closeAction:"quit",setSummon(enabled,shortcut=this.summonShortcut) {this.summonEnabled=enabled;this.summonShortcut=shortcut;}};'}));
    build.onLoad({filter:/\.svelte\.ts$/}, async ({path}) => {
      const source = await transform(await readFile(path,'utf8'), {loader:'ts'});
      return {contents:compileModule(source.code,{filename:path,generate:'client'}).js.code,loader:'js'};
    });
  }}],
});
const result = spawnSync(process.execPath, ['--test',output], {stdio:'inherit'});
process.exitCode = result.status ?? 1;
