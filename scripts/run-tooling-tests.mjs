import { readdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';

const root = fileURLToPath(new URL('../', import.meta.url));

export const PROFILES = Object.freeze({
  core: ['scripts'],
  'native-preflight': ['tests/native'],
  all: ['scripts', 'tests/native'],
});

export async function discoverToolingTests(baseDir = root, profile = 'all') {
  const dirNames = PROFILES[profile];
  assert(dirNames, `Unknown tooling test profile: '${profile}'. Supported profiles: ${Object.keys(PROFILES).join(', ')}`);
  const dirs = dirNames.map(d => resolve(baseDir, d));
  const testFiles = [];
  for (const dir of dirs) {
    try {
      const files = await readdir(dir);
      for (const file of files) {
        if (file.endsWith('.test.mjs')) {
          testFiles.push(resolve(dir, file));
        }
      }
    } catch {
      // Directory might not exist in some environments
    }
  }
  return testFiles.sort();
}

export function parseProfileArg(args = []) {
  let profile = 'all';
  const remainingArgs = [];
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg.startsWith('--profile=')) {
      profile = arg.slice('--profile='.length);
    } else if (arg === '--profile' && i + 1 < args.length) {
      profile = args[++i];
    } else {
      remainingArgs.push(arg);
    }
  }
  return { profile, remainingArgs };
}

export async function runToolingTests(baseDir = root, args = []) {
  const { profile, remainingArgs } = parseProfileArg(args);
  const testFiles = await discoverToolingTests(baseDir, profile);
  assert(testFiles.length > 0, `No tooling test files found for profile '${profile}'!`);
  const nodeArgs = ['--test', ...testFiles, ...remainingArgs];
  const result = spawnSync(process.execPath, nodeArgs, {
    cwd: baseDir,
    stdio: 'inherit',
    windowsHide: true,
  });
  return result.status ?? 1;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const exitCode = await runToolingTests(root, process.argv.slice(2));
  process.exitCode = exitCode;
}
