import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { readFile, access } from 'node:fs/promises';
import { resolve } from 'node:path';

const testsNativeDir = fileURLToPath(new URL('./', import.meta.url));
const suiteManifest = JSON.parse(await readFile(new URL('../../scripts/native-suites.json', import.meta.url), 'utf8'));

test('playwright-core resolves from tests/native without NODE_PATH or desktop node_modules', () => {
  const env = { ...process.env };
  delete env.NODE_PATH;

  const code = `
    import { chromium } from 'playwright-core';
    if (typeof chromium?.launch !== 'function' && typeof chromium?.connectOverCDP !== 'function') {
      throw new Error('playwright-core does not export chromium connectOverCDP');
    }
    console.log('resolved');
  `;

  const res = spawnSync(process.execPath, ['--input-type=module', '-e', code], {
    cwd: testsNativeDir,
    env,
    encoding: 'utf8',
  });

  assert.equal(res.status, 0, `Failed to resolve playwright-core from tests/native: ${res.stderr || res.stdout}`);
  assert(res.stdout.includes('resolved'));
});

test('all registered native suite scripts exist in tests/native', async () => {
  for (const [name, suite] of Object.entries(suiteManifest.suites)) {
    const scriptPath = resolve(testsNativeDir, suite.script);
    await assert.doesNotReject(access(scriptPath), `Suite ${name} script ${suite.script} missing in tests/native`);
  }
});

test('all native scripts in tests/native have valid syntax', async () => {
  const env = { ...process.env };
  delete env.NODE_PATH;

  for (const suite of Object.values(suiteManifest.suites)) {
    const res = spawnSync(process.execPath, ['--check', resolve(testsNativeDir, suite.script)], {
      cwd: testsNativeDir,
      env,
      encoding: 'utf8',
    });
    assert.equal(res.status, 0, `Syntax error in tests/native/${suite.script}: ${res.stderr}`);
  }
});
