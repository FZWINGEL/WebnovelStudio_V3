import test from 'node:test';
import assert from 'node:assert/strict';
import {
  isAllowedNativeRetestPath,
  validateNativeRetestDiff,
  validateNativeRetestContract,
  parseRunId,
} from './native-retest.mjs';

const buildCommit = '1'.repeat(40);
const qualificationCommit = '2'.repeat(40);
const exeSha = '3'.repeat(64);

const sampleManifest = {
  schemaVersion: 1,
  commit: buildCommit,
  executable: 'target/debug/webnovel-desktop.exe',
  files: [
    { path: 'target/debug/webnovel-desktop.exe', sha256: exeSha, bytes: 12345 }
  ]
};

test('native retest path filtering allows harness and docs only', () => {
  assert.equal(isAllowedNativeRetestPath('apps/desktop/scripts/native-smoke.mjs'), true);
  assert.equal(isAllowedNativeRetestPath('scripts/native-consumer.mjs'), true);
  assert.equal(isAllowedNativeRetestPath('tests/native/package.json'), true);
  assert.equal(isAllowedNativeRetestPath('docs/TESTING.md'), true);
  assert.equal(isAllowedNativeRetestPath('AGENTS.md'), true);
  assert.equal(isAllowedNativeRetestPath('.github/workflows/ci.yml'), true);

  assert.equal(isAllowedNativeRetestPath('Cargo.toml'), false);
  assert.equal(isAllowedNativeRetestPath('Cargo.lock'), false);
  assert.equal(isAllowedNativeRetestPath('crates/core/src/lib.rs'), false);
  assert.equal(isAllowedNativeRetestPath('apps/desktop/src/App.tsx'), false);
  assert.equal(isAllowedNativeRetestPath('apps/desktop/package.json'), false);
  assert.equal(isAllowedNativeRetestPath('apps/desktop/src-tauri/tauri.conf.json'), false);
});

test('validateNativeRetestDiff rejects any forbidden source changes', () => {
  assert.doesNotThrow(() => validateNativeRetestDiff([
    'apps/desktop/scripts/native-smoke.mjs',
    'scripts/native-consumer.mjs',
    'docs/TESTING.md',
  ]));

  assert.throws(() => validateNativeRetestDiff([
    'apps/desktop/scripts/native-smoke.mjs',
    'crates/kernel/src/lib.rs',
  ]), /disallowed changes/);

  assert.throws(() => validateNativeRetestDiff([
    'apps/desktop/src/kernel/document.ts',
  ]), /disallowed changes/);

  assert.throws(() => validateNativeRetestDiff([
    'Cargo.lock',
  ]), /disallowed changes/);
});

test('validateNativeRetestContract returns separate identities and verifies harness SHA', async () => {
  const contract = await validateNativeRetestContract({
    manifest: sampleManifest,
    qualificationCommit,
    changedPaths: ['apps/desktop/scripts/native-smoke.mjs'],
  });

  assert.equal(contract.buildCommit, buildCommit);
  assert.equal(contract.qualificationCommit, qualificationCommit);
  assert.equal(contract.executableSha256, exeSha);
  assert.equal(typeof contract.harnessSha256, 'string');
  assert.equal(contract.harnessSha256.length, 64);
});

test('validateNativeRetestContract fails closed on invalid manifest or commits', async () => {
  await assert.rejects(() => validateNativeRetestContract({
    manifest: null,
    qualificationCommit,
    changedPaths: [],
  }), /Missing or malformed artifact manifest/);

  await assert.rejects(() => validateNativeRetestContract({
    manifest: { ...sampleManifest, commit: 'short' },
    qualificationCommit,
    changedPaths: [],
  }), /invalid buildCommit/);

  await assert.rejects(() => validateNativeRetestContract({
    manifest: sampleManifest,
    qualificationCommit: 'short',
    changedPaths: [],
  }), /Invalid qualificationCommit/);
});

test('parseRunId verifies positive integer run IDs', () => {
  assert.equal(parseRunId('12345'), '12345');
  assert.throws(() => parseRunId(''), /positive integer/);
  assert.throws(() => parseRunId('abc'), /positive integer/);
  assert.throws(() => parseRunId('-5'), /positive integer/);
});
