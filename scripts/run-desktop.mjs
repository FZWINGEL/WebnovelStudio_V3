import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, delimiter, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { homedir } from 'node:os';

if (process.version !== 'v24.20.0') throw new Error('Run scripts/desktop.ps1 to select Node 24.20.0.');
const root = fileURLToPath(new URL('../', import.meta.url));
const desktop = resolve(root, 'apps/desktop');
const environment = { ...process.env, PATH: [dirname(process.execPath), resolve(homedir(), '.cargo/bin'), process.env.PATH].join(delimiter) };
const action = process.argv[2] ?? 'dev';
const npm = process.env.npm_execpath;

async function run(executable, args, cwd = root) {
  await new Promise((accept, reject) => {
    const child = spawn(executable, args, { cwd, env: environment, stdio: 'inherit', windowsHide: true });
    child.once('error', reject);
    child.once('exit', code => code === 0 ? accept() : reject(new Error(`${args.join(' ')} failed (${code}).`)));
  });
}
async function node(args, cwd = desktop) { await run(process.execPath, args, cwd); }
if (action === 'setup' || !existsSync(resolve(desktop, 'node_modules/@tauri-apps/cli/tauri.js'))) {
  if (!npm) throw new Error('Run scripts/desktop.ps1 so the npm runtime is available.');
  await node([npm, 'ci']);
}
if (action === 'setup') process.exit(0);
const tauri = resolve(desktop, 'node_modules/@tauri-apps/cli/tauri.js');
if (['build', 'spike', 'package'].includes(action)) {
  await node([resolve(root, 'scripts/check-versions.mjs')]);
}
switch (action) {
  case 'dev': await node([tauri, 'dev']); break;
  case 'spike': await node([tauri, 'build', '--debug', '--no-bundle', '--', '--locked']); break;
  case 'build': await node([tauri, 'build', '--no-bundle', '--', '--locked']); break;
  case 'package':
    if (process.platform !== 'win32') throw new Error('The initial installer target is Windows x64.');
    await node([tauri, 'build', '--target', 'x86_64-pc-windows-msvc', '--bundles', 'nsis', '--', '--locked']);
    break;
  case 'test': await node(['node_modules/vitest/vitest.mjs', 'run']); break;
  case 'native': await node(['scripts/native-smoke.mjs']); break;
  case 'check':
    await node(['--test', resolve(root, 'scripts/check-versions.test.mjs'), resolve(root, 'scripts/prepare-package-retest.test.mjs')]);
    await run('cargo', ['fmt', '--all', '--check']);
    await run('cargo', ['clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings']);
    await run('cargo', ['test', '--workspace', '--locked']);
    await node(['scripts/build.mjs']);
    await node(['node_modules/vitest/vitest.mjs', 'run']);
    break;
  default: throw new Error(`Unknown desktop command: ${action}`);
}
