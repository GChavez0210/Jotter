// Builds the `jotter` CLI and copies it to src-tauri/binaries/ under the name Tauri expects for
// an external binary: `jotter-<target triple>[.exe]`. `tauri build --config
// src-tauri/tauri.cli.conf.json` runs this before it compiles the app, so the installers ship
// the CLI next to the desktop executable.
//
// The CLI builds into its own target directory. Its binary is also named `jotter`, and Tauri
// copies the sidecar into the app's target directory, so sharing one directory would let the two
// writes collide.
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', 'src-tauri');
const targetDir = join(root, 'target', 'cli-sidecar');

function hostTriple() {
  const info = execFileSync('rustc', ['-vV'], { encoding: 'utf8' });
  const host = info.match(/^host: (\S+)$/m);
  if (!host) throw new Error('Cannot read the host target from `rustc -vV`.');
  return host[1];
}

// Tauri sets these for beforeBuildCommand. Without them, build a release for the host.
const triple = process.env.TAURI_ENV_TARGET_TRIPLE || hostTriple();
const debug = process.env.TAURI_ENV_DEBUG === 'true';
const profile = debug ? 'debug' : 'release';
const exe = triple.includes('windows') ? '.exe' : '';

const args = ['build', '--locked', '-p', 'jotter-cli', '--target', triple];
if (!debug) args.push('--release');
console.log(`Building the jotter CLI for ${triple} (${profile})`);
execFileSync('cargo', args, {
  cwd: root,
  stdio: 'inherit',
  env: { ...process.env, CARGO_TARGET_DIR: targetDir },
});

const binaries = join(root, 'binaries');
mkdirSync(binaries, { recursive: true });
const dest = join(binaries, `jotter-${triple}${exe}`);
copyFileSync(join(targetDir, triple, profile, `jotter${exe}`), dest);
console.log(`Copied the CLI to ${dest}`);
