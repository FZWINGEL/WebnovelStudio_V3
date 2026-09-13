import { spawn } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { basename, dirname, delimiter, resolve } from 'node:path';
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

function computeInstallSignature(dir) {
  const pkgPath = resolve(dir, 'package.json');
  const lockPath = resolve(dir, 'package-lock.json');
  if (!existsSync(pkgPath) || !existsSync(lockPath)) return null;
  const pkgContent = readFileSync(pkgPath);
  const lockContent = readFileSync(lockPath);
  return createHash('sha256')
    .update(pkgContent)
    .update(lockContent)
    .update(`${process.version}:${process.platform}:${process.arch}`)
    .digest('hex');
}

function isInstallValid(dir, keyFile) {
  if (!existsSync(resolve(dir, keyFile))) return false;
  const sigFile = resolve(dir, 'node_modules/.install-signature');
  if (!existsSync(sigFile)) return false;
  const expectedSig = computeInstallSignature(dir);
  if (!expectedSig) return false;
  try {
    const actualSig = readFileSync(sigFile, 'utf8').trim();
    return actualSig === expectedSig;
  } catch {
    return false;
  }
}

function writeInstallSignature(dir) {
  const sig = computeInstallSignature(dir);
  if (sig) {
    try {
      writeFileSync(resolve(dir, 'node_modules/.install-signature'), sig, 'utf8');
    } catch {
      // Best effort
    }
  }
}

async function ensureFrontendDependencies() {
  if (!isInstallValid(desktop, 'node_modules/@tauri-apps/cli/tauri.js')) {
    if (!npm) throw new Error('Run scripts/desktop.ps1 so the npm runtime is available.');
    await node([npm, 'ci']);
    writeInstallSignature(desktop);
  }
}

async function ensureNativeDependencies() {
  const nativeDir = resolve(root, 'tests/native');
  if (!isInstallValid(nativeDir, 'node_modules/playwright-core/package.json')) {
    if (!npm) throw new Error('Run scripts/desktop.ps1 so the npm runtime is available.');
    await node([npm, 'ci'], nativeDir);
    writeInstallSignature(nativeDir);
  }
}

if (action === 'setup') {
  if (!npm) throw new Error('Run scripts/desktop.ps1 so the npm runtime is available.');
  await node([npm, 'ci']);
  writeInstallSignature(desktop);
  await node([npm, 'ci'], resolve(root, 'tests/native'));
  writeInstallSignature(resolve(root, 'tests/native'));
  process.exit(0);
}

const tauri = resolve(desktop, 'node_modules/@tauri-apps/cli/tauri.js');
if (['build', 'spike', 'package'].includes(action)) {
  await ensureFrontendDependencies();
  await node([resolve(root, 'scripts/check-versions.mjs')]);
}
async function pruneTarget() {
  const depsDir = resolve(root, 'target/debug/deps');
  if (!existsSync(depsDir)) {
    console.log('No target/debug/deps directory found.');
    return;
  }
  const activeArtifacts = new Set();
  try {
    const { execFileSync } = await import('node:child_process');
    const output = execFileSync('cargo', ['test', '--workspace', '--all-targets', '--no-run', '--message-format=json'], {
      cwd: root,
      env: environment,
      stdio: ['ignore', 'pipe', 'ignore'],
      windowsHide: true,
    }).toString();
    for (const line of output.split('\n')) {
      if (!line.trim()) continue;
      try {
        const msg = JSON.parse(line);
        if (msg.reason === 'compiler-artifact') {
          if (msg.executable) {
            const base = basename(msg.executable);
            activeArtifacts.add(base);
            activeArtifacts.add(base.replace(/\.exe$/i, '.pdb'));
            activeArtifacts.add(base.replace(/\.exe$/i, '.d'));
          }
          if (msg.filenames) {
            for (const f of msg.filenames) {
              const base = basename(f);
              activeArtifacts.add(base);
              if (base.endsWith('.exe')) {
                activeArtifacts.add(base.replace(/\.exe$/i, '.pdb'));
                activeArtifacts.add(base.replace(/\.exe$/i, '.d'));
              }
            }
          }
        }
      } catch {}
    }
  } catch (err) {
    console.log(`Unable to query Cargo for active artifacts: ${err.message}`);
  }
  const { readdirSync, statSync, unlinkSync } = await import('node:fs');
  const files = readdirSync(depsDir);
  const groups = new Map();
  for (const file of files) {
    if (file.endsWith('.pdb') || file.endsWith('.exe')) {
      if (activeArtifacts.has(file)) continue;
      const match = file.match(/^([a-zA-Z0-9_]+)-[0-9a-f]{16}\.(pdb|exe)$/);
      if (match) {
        const key = `${match[1]}.${match[2]}`;
        if (!groups.has(key)) groups.set(key, []);
        const stat = statSync(resolve(depsDir, file));
        groups.get(key).push({ file, mtime: stat.mtimeMs, size: stat.size });
      }
    }
  }
  let prunedCount = 0;
  let prunedBytes = 0;
  for (const list of groups.values()) {
    list.sort((a, b) => b.mtime - a.mtime);
    const toRemove = list.slice(1);
    for (const item of toRemove) {
      try {
        unlinkSync(resolve(depsDir, item.file));
        prunedCount++;
        prunedBytes += item.size;
      } catch {
        // ignore locked files
      }
    }
  }
  const mb = (prunedBytes / (1024 * 1024)).toFixed(1);
  console.log(`Pruned ${prunedCount} orphaned build artifacts (${mb} MB freed).`);
}

const extraArgs = process.argv.slice(3);

switch (action) {
  case 'dev':
    await ensureFrontendDependencies();
    await node([tauri, 'dev']);
    break;
  case 'spike':
    await ensureFrontendDependencies();
    await node([tauri, 'build', '--debug', '--no-bundle', '--', '--locked']);
    break;
  case 'build':
    await ensureFrontendDependencies();
    await node([tauri, 'build', '--no-bundle', '--', '--locked']);
    break;
  case 'package':
    if (process.platform !== 'win32') throw new Error('The initial installer target is Windows x64.');
    await ensureFrontendDependencies();
    await node([tauri, 'build', '--target', 'x86_64-pc-windows-msvc', '--bundles', 'nsis', '--', '--locked']);
    break;
  case 'test':
    await ensureFrontendDependencies();
    await node([npm, 'test', ...(extraArgs.length ? ['--', ...extraArgs] : [])]);
    break;
  case 'test:watch':
    await ensureFrontendDependencies();
    await node([npm, 'run', 'test:watch', ...(extraArgs.length ? ['--', ...extraArgs] : [])]);
    break;
  case 'native':
    await ensureNativeDependencies();
    await node(['native-smoke.mjs'], resolve(root, 'tests/native'));
    break;
  case 'quick': {
    const pkg = extraArgs[0];
    const targetFlag = pkg ? ['-p', pkg] : ['--workspace'];
    await run('cargo', ['fmt', '--all', '--check']);
    await run('cargo', ['clippy', ...targetFlag, '--all-targets', '--locked', '--', '-D', 'warnings']);
    await run('cargo', ['test', ...targetFlag, '--lib', '--bins', '--locked']);
    if (!pkg) {
      await ensureFrontendDependencies();
      await node([npm, 'run', 'typecheck']);
    }
    break;
  }
  case 'plan':
    await node([resolve(root, 'scripts/test-plan.mjs'), ...extraArgs]);
    break;
  case 'prune': await pruneTarget(); break;
  case 'check':
    await node([resolve(root, 'scripts/run-tooling-tests.mjs'), '--profile=core']);
    await run('cargo', ['fmt', '--all', '--check']);
    await run('cargo', ['clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings']);
    await run('cargo', ['test', '--workspace', '--locked']);
    await ensureFrontendDependencies();
    await node(['scripts/build.mjs']);
    await node([npm, 'test']);
    await ensureNativeDependencies();
    await node([resolve(root, 'scripts/run-tooling-tests.mjs'), '--profile=native-preflight']);
    break;
  default: throw new Error(`Unknown desktop command: ${action}`);
}
