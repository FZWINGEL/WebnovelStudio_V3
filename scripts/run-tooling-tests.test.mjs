import test from 'node:test';
import assert from 'node:assert/strict';
import { readdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { discoverToolingTests, parseProfileArg, PROFILES } from './run-tooling-tests.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));

test('discoverToolingTests discovers every *.test.mjs file for all profile', async () => {
  const discovered = await discoverToolingTests(root, 'all');

  const expected = [];
  for (const dir of ['scripts', 'tests/native']) {
    const files = await readdir(resolve(root, dir));
    for (const f of files) {
      if (f.endsWith('.test.mjs')) {
        expected.push(resolve(root, dir, f));
      }
    }
  }
  expected.sort();

  assert.deepEqual(discovered, expected);
  assert(discovered.length >= 10, 'Expected at least 10 tooling test files');
});

test('discoverToolingTests filters by profile correctly', async () => {
  const coreTests = await discoverToolingTests(root, 'core');
  assert(coreTests.length > 0);
  for (const file of coreTests) {
    const normalized = file.replaceAll('\\', '/');
    assert(normalized.includes('scripts/'), `Core test ${file} must be in scripts/`);
    assert(!normalized.includes('tests/native/'), `Core test ${file} must not be in tests/native/`);
  }

  const nativeTests = await discoverToolingTests(root, 'native-preflight');
  assert(nativeTests.length > 0);
  for (const file of nativeTests) {
    const normalized = file.replaceAll('\\', '/');
    assert(normalized.includes('tests/native/'), `Native test ${file} must be in tests/native/`);
  }

  const allTests = await discoverToolingTests(root, 'all');
  assert.deepEqual(allTests, [...coreTests, ...nativeTests].sort());
});

test('parseProfileArg parses --profile flags and extracts remaining arguments', () => {
  assert.deepEqual(parseProfileArg(['--profile=core', '--test-reporter=tap']), {
    profile: 'core',
    remainingArgs: ['--test-reporter=tap'],
  });

  assert.deepEqual(parseProfileArg(['--profile', 'native-preflight', 'extra']), {
    profile: 'native-preflight',
    remainingArgs: ['extra'],
  });

  assert.deepEqual(parseProfileArg(['regular-arg']), {
    profile: 'all',
    remainingArgs: ['regular-arg'],
  });
});

test('discoverToolingTests throws on unknown profile', async () => {
  await assert.rejects(
    () => discoverToolingTests(root, 'invalid-profile'),
    /Unknown tooling test profile/
  );
});
