import { build, transform } from 'esbuild';
import { compileModule } from 'svelte/compiler';
import { readFile, mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

await mkdir(resolve('src-tauri/target/frontend-tests'), {recursive:true});
const outputs = [];
for (const suite of ['frontend', 'settings']) {
const output = resolve(`src-tauri/target/frontend-tests/${suite}.mjs`);
outputs.push(output);
await build({
  entryPoints:[`tests/${suite}.test.ts`], outfile:output, bundle:true,
  platform:'node', conditions:['browser'], format:'esm', loader:{'.css':'empty'},
  plugins:[{name:'store-tests',setup(build) {
    build.onResolve({filter:/\/ipc$|^\.\.\/ipc$/}, () => ({path:resolve('tests/ipc.mock.ts')}));
    build.onResolve({filter:/clipboard\.svelte$/}, () => ({path:'clipboard',namespace:'test'}));
    build.onResolve({filter:/^@tauri-apps\/api\/window$/}, () => ({path:resolve('tests/window.mock.ts')}));
    build.onLoad({filter:/.*/,namespace:'test'}, () => ({contents:'export const clipboardStore = {setStatus() {}};'}));
    build.onLoad({filter:/\.svelte\.ts$/}, async ({path}) => {
      const source = await transform(await readFile(path,'utf8'), {loader:'ts'});
      return {contents:compileModule(source.code,{filename:path,generate:'client'}).js.code,loader:'js'};
    });
  }}],
});
}
const result = spawnSync(process.execPath, ['--test',...outputs], {stdio:'inherit'});
process.exitCode = result.status ?? 1;
