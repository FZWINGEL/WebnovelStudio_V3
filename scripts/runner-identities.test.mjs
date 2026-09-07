import { test } from 'node:test';
import assert from 'node:assert/strict';
import { expectedIdentities, verifyLibtestOutput, verifyNextestOutput } from './runner-identities.mjs';
const expected = [{ binary: 'core', name: 'first' }, { binary: 'core::integration', name: 'second' }];
test('equal counts cannot hide a duplicate replacing an omitted test', () => {
  verifyLibtestOutput('test first ... ok\ntest second ... ok\n', expected);
  assert.throws(() => verifyLibtestOutput('test first ... ok\ntest first ... ok\n', expected));
  const output = ' PASS [ 0.1s] (  1/2) core first\n FAIL [ 0.2s] (  2/2) core::integration second\n';
  verifyNextestOutput(output + ' FAIL [ 0.2s] (2/2) core::integration second\n', expected);
  assert.throws(() => verifyNextestOutput(output.replace('second', 'first'), expected));
});
test('non-ignored default-filter exclusions are rejected', () => {
  const inventory = { 'rust-suites': { core: { testcases: { first: { ignored: false, 'filter-match': { status: 'mismatch' } } } } } };
  assert.throws(() => expectedIdentities(inventory));
});
