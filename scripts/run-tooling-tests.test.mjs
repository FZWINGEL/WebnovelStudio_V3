import test from 'node:test';
import assert from 'node:assert/strict';
import { readdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { discoverToolingTests } from './run-tooling-tests.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));

test('discoverToolingTests discovers every *.test.mjs file on disk', async () => {
  const discovered = await discoverToolingTests(root);
  
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
