import { spawn, execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync, mkdirSync, appendFileSync } from 'node:fs';
import { createHash, randomUUID } from 'node:crypto';
import { basename, dirname, delimiter, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { homedir, cpus } from 'node:os';

const isMain = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain && process.version !== 'v24.20.0') {
  throw new Error('Run scripts/desktop.ps1 to select Node 24.20.0.');
}

const root = fileURLToPath(new URL('../', import.meta.url));
const desktop = resolve(root, 'apps/desktop');
const environment = { ...process.env, PATH: [dirname(process.execPath), resolve(homedir(), '.cargo/bin'), process.env.PATH].join(delimiter) };
const action = process.argv[2] ?? 'dev';

export function getGitCommit() {
  try {
    return execFileSync('git', ['rev-parse', 'HEAD'], {
      cwd: root,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
      windowsHide: true,
    }).trim();
  } catch {
    return 'unknown';
  }
}

export function findNpm() {
  if (process.env.npm_execpath && existsSync(process.env.npm_execpath)) {
    return process.env.npm_execpath;
  }
  const winCandidate = resolve(dirname(process.execPath), 'node_modules/npm/bin/npm-cli.js');
  if (existsSync(winCandidate)) return winCandidate;
  const nixCandidate = resolve(dirname(process.execPath), '../lib/node_modules/npm/bin/npm-cli.js');
  if (existsSync(nixCandidate)) return nixCandidate;
  if (process.env.PATH) {
    for (const dir of process.env.PATH.split(delimiter)) {
      const candidate = resolve(dir, 'node_modules/npm/bin/npm-cli.js');
      if (existsSync(candidate)) return candidate;
    }
  }
  return null;
}

const npm = findNpm();

let currentInvocationId = null;

export function setInvocationId(id) {
  currentInvocationId = id;
}

export function getInvocationId() {
  return currentInvocationId;
}

export function recordPhase(phaseData, baseDir = root) {
  if (!currentInvocationId) return;
  recordTiming({
    invocationId: currentInvocationId,
    timestamp: new Date().toISOString(),
    ...phaseData,
  }, baseDir);
}

export async function run(executable, args, cwd = root) {
  const t0 = performance.now();
  let exitCode = 0;
  try {
    await new Promise((accept, reject) => {
      const child = spawn(executable, args, { cwd, env: environment, stdio: 'inherit', windowsHide: true });
      child.once('error', reject);
      child.once('exit', code => {
        exitCode = code ?? 0;
        if (code === 0) accept();
        else reject(new Error(`${args.join(' ')} failed (${code}).`));
      });
    });
    recordPhase({
      type: 'command',
      command: [executable, ...args].join(' '),
      cwd: relative(root, cwd).replace(/\\/g, '/') || '.',
      exitCode,
      durationMs: Math.round((performance.now() - t0) * 10) / 10,
      status: 'success',
    });
  } catch (err) {
    recordPhase({
      type: 'command',
      command: [executable, ...args].join(' '),
      cwd: relative(root, cwd).replace(/\\/g, '/') || '.',
      exitCode: exitCode || 1,
      durationMs: Math.round((performance.now() - t0) * 10) / 10,
      status: 'failed',
      error: err?.message || String(err),
    });
    throw err;
  }
}

export async function node(args, cwd = desktop) { await run(process.execPath, args, cwd); }

export function computeInstallSignature(dir) {
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

export function isInstallValid(dir, keyFile) {
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

export function writeInstallSignature(dir) {
  const sig = computeInstallSignature(dir);
  if (sig) {
    try {
      writeFileSync(resolve(dir, 'node_modules/.install-signature'), sig, 'utf8');
    } catch {
      // Best effort
    }
  }
}

export const dependencyStatus = {
  frontend: 'skipped',
  native: 'skipped',
};

export function updateDepStatus(target, newStatus) {
  const current = dependencyStatus[target];
  if (current === 'installed') {
    // If dependencies were installed in this invocation, retain 'installed'
    return;
  }
  if (newStatus === 'installed') {
    dependencyStatus[target] = 'installed';
  } else if (newStatus === 'failed') {
    dependencyStatus[target] = 'failed';
  } else if (newStatus === 'reused' && current !== 'failed') {
    dependencyStatus[target] = 'reused';
  }
}

export async function ensureFrontendDependencies(force = false) {
  const t0 = performance.now();
  if (force || !isInstallValid(desktop, 'node_modules/@tauri-apps/cli/tauri.js')) {
    const npmBin = findNpm();
    if (!npmBin) {
      updateDepStatus('frontend', 'failed');
      recordPhase({
        type: 'dependency',
        target: 'frontend',
        status: 'failed',
        durationMs: Math.round((performance.now() - t0) * 10) / 10,
        error: 'Could not locate npm runtime.',
      });
      throw new Error('Could not locate npm runtime. Run scripts/desktop.ps1.');
    }
    try {
      await node([npmBin, 'ci']);
      writeInstallSignature(desktop);
      updateDepStatus('frontend', 'installed');
      recordPhase({
        type: 'dependency',
        target: 'frontend',
        status: 'installed',
        durationMs: Math.round((performance.now() - t0) * 10) / 10,
      });
      return 'installed';
    } catch (err) {
      updateDepStatus('frontend', 'failed');
      recordPhase({
        type: 'dependency',
        target: 'frontend',
        status: 'failed',
        durationMs: Math.round((performance.now() - t0) * 10) / 10,
        error: err?.message || String(err),
      });
      throw err;
    }
  }
  updateDepStatus('frontend', 'reused');
  recordPhase({
    type: 'dependency',
    target: 'frontend',
    status: 'reused',
    durationMs: Math.round((performance.now() - t0) * 10) / 10,
  });
  return 'reused';
}

export async function ensureNativeDependencies(force = false) {
  const t0 = performance.now();
  const nativeDir = resolve(root, 'tests/native');
  if (force || !isInstallValid(nativeDir, 'node_modules/playwright-core/package.json')) {
    const npmBin = findNpm();
    if (!npmBin) {
      updateDepStatus('native', 'failed');
      recordPhase({
        type: 'dependency',
        target: 'native',
        status: 'failed',
        durationMs: Math.round((performance.now() - t0) * 10) / 10,
        error: 'Could not locate npm runtime.',
      });
      throw new Error('Could not locate npm runtime. Run scripts/desktop.ps1.');
    }
    try {
      await node([npmBin, 'ci'], nativeDir);
      writeInstallSignature(nativeDir);
      updateDepStatus('native', 'installed');
      recordPhase({
        type: 'dependency',
        target: 'native',
        status: 'installed',
        durationMs: Math.round((performance.now() - t0) * 10) / 10,
      });
      return 'installed';
    } catch (err) {
      updateDepStatus('native', 'failed');
      recordPhase({
        type: 'dependency',
        target: 'native',
        status: 'failed',
        durationMs: Math.round((performance.now() - t0) * 10) / 10,
        error: err?.message || String(err),
      });
      throw err;
    }
  }
  updateDepStatus('native', 'reused');
  recordPhase({
    type: 'dependency',
    target: 'native',
    status: 'reused',
    durationMs: Math.round((performance.now() - t0) * 10) / 10,
  });
  return 'reused';
}

export function recordTiming(entry, baseDir = root) {
  try {
    const dir = resolve(baseDir, '.local/performance');
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true });
    }
    const logPath = resolve(dir, 'desktop-timings.jsonl');
    appendFileSync(logPath, JSON.stringify(entry) + '\n', 'utf8');
  } catch {
    // Best effort logging
  }
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

export function resolveWorkerSettings() {
  const defaultVitestWorkers = Math.min(12, Math.max(2, Math.floor((cpus()?.length || 4) / 2)));
  const rawVitestWorkers = process.env.VITEST_MAX_WORKERS;
  const requestedVitestWorkers = rawVitestWorkers === undefined ? undefined : Number(rawVitestWorkers);
  const vitestMaxWorkers = Number.isInteger(requestedVitestWorkers) && requestedVitestWorkers > 0
    ? requestedVitestWorkers
    : (process.env.CI ? 2 : defaultVitestWorkers);

  return {
    vitestPool: 'threads',
    vitestIsolate: true,
    vitestMaxWorkers,
    rustTestThreads: process.env.RUST_TEST_THREADS || '4 (default)',
    cargoBuildJobs: process.env.CARGO_BUILD_JOBS || 'default',
  };
}

if (isMain) {
  const startTime = performance.now();
  const extraArgs = process.argv.slice(3);
  const invocationId = typeof randomUUID === 'function' ? randomUUID() : `inv-${Date.now()}`;
  setInvocationId(invocationId);

  recordTiming({
    type: 'invocation',
    invocationId,
    timestamp: new Date().toISOString(),
    source: getGitCommit(),
    action,
    args: extraArgs,
    nodeVersion: process.version,
    platform: process.platform,
    arch: process.arch,
    workers: resolveWorkerSettings(),
  });

  try {
    const tauri = resolve(desktop, 'node_modules/@tauri-apps/cli/tauri.js');
    if (['build', 'spike', 'package'].includes(action)) {
      await ensureFrontendDependencies();
      await node([resolve(root, 'scripts/check-versions.mjs')]);
    }

    switch (action) {
      case 'ensure-frontend':
        await ensureFrontendDependencies();
        break;
      case 'ensure-native':
        await ensureNativeDependencies();
        break;
      case 'ensure-deps':
        await ensureFrontendDependencies();
        await ensureNativeDependencies();
        break;
      case 'setup':
        await ensureFrontendDependencies(true);
        await ensureNativeDependencies(true);
        break;
      case 'dev':
        await ensureFrontendDependencies();
        await node([tauri, 'dev']);
        break;
      case 'spike':
        await node([tauri, 'build', '--debug', '--no-bundle', '--', '--locked']);
        break;
      case 'build':
        await node([tauri, 'build', '--no-bundle', '--', '--locked']);
        break;
      case 'package':
        if (process.platform !== 'win32') throw new Error('The initial installer target is Windows x64.');
        await node([tauri, 'build', '--target', 'x86_64-pc-windows-msvc', '--bundles', 'nsis', '--', '--locked']);
        break;
      case 'test':
        await ensureFrontendDependencies();
        if (!npm) throw new Error('Could not locate npm runtime. Run scripts/desktop.ps1.');
        await node([npm, 'test', ...(extraArgs.length ? ['--', ...extraArgs] : [])]);
        break;
      case 'test:watch':
        await ensureFrontendDependencies();
        if (!npm) throw new Error('Could not locate npm runtime. Run scripts/desktop.ps1.');
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
          if (!npm) throw new Error('Could not locate npm runtime. Run scripts/desktop.ps1.');
          await node([npm, 'run', 'typecheck']);
        }
        break;
      }
      case 'plan':
        await node([resolve(root, 'scripts/test-plan.mjs'), ...extraArgs]);
        break;
      case 'prune':
        await pruneTarget();
        break;
      case 'check':
        await node([resolve(root, 'scripts/run-tooling-tests.mjs'), '--profile=core']);
        await run('cargo', ['fmt', '--all', '--check']);
        await run('cargo', ['clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings']);
        await run('cargo', ['test', '--workspace', '--locked']);
        await ensureFrontendDependencies();
        await node(['scripts/build.mjs']);
        if (!npm) throw new Error('Could not locate npm runtime. Run scripts/desktop.ps1.');
        await node([npm, 'test']);
        await ensureNativeDependencies();
        await node([resolve(root, 'scripts/run-tooling-tests.mjs'), '--profile=native-preflight']);
        break;
      default:
        throw new Error(`Unknown desktop command: ${action}`);
    }

    recordTiming({
      type: 'completion',
      invocationId,
      timestamp: new Date().toISOString(),
      action,
      args: extraArgs,
      frontendDeps: dependencyStatus.frontend,
      nativeDeps: dependencyStatus.native,
      durationMs: Math.round((performance.now() - startTime) * 10) / 10,
      status: 'success',
    });
  } catch (err) {
    recordTiming({
      type: 'completion',
      invocationId,
      timestamp: new Date().toISOString(),
      action,
      args: extraArgs,
      frontendDeps: dependencyStatus.frontend,
      nativeDeps: dependencyStatus.native,
      durationMs: Math.round((performance.now() - startTime) * 10) / 10,
      status: 'failed',
      error: err?.message || String(err),
    });
    throw err;
  }
}
