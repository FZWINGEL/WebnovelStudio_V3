import { readdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';

const root = fileURLToPath(new URL('../', import.meta.url));

export async function discoverToolingTests(baseDir = root) {
  const dirs = [
    resolve(baseDir, 'scripts'),
    resolve(baseDir, 'tests/native'),
  ];
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

export function runToolingTests(baseDir = root, extraArgs = []) {
  return discoverToolingTests(baseDir).then(testFiles => {
    assert(testFiles.length > 0, 'No tooling test files found!');
    const args = ['--test', ...testFiles, ...extraArgs];
    const result = spawnSync(process.execPath, args, {
      cwd: baseDir,
      stdio: 'inherit',
      windowsHide: true,
    });
    return result.status ?? 1;
  });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const exitCode = await runToolingTests(root, process.argv.slice(2));
  process.exitCode = exitCode;
}
