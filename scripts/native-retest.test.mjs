import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import {
  isAllowedNativeRetestPath,
  validateNativeRetestDiff,
  validateNativeRetestContract,
  computeHarnessSha256,
  parseRunId,
} from './native-retest.mjs';

const buildCommit = '1'.repeat(40);
const qualificationCommit = '2'.repeat(40);

test('native retest path filtering allows harness and docs only', () => {
  assert.equal(isAllowedNativeRetestPath('apps/desktop/scripts/native-smoke.mjs'), true);
  assert.equal(isAllowedNativeRetestPath('scripts/native-consumer.mjs'), true);
  assert.equal(isAllowedNativeRetestPath('tests/native/package.json'), true);
  assert.equal(isAllowedNativeRetestPath('tests/native/helpers/subhelper.mjs'), true);
  assert.equal(isAllowedNativeRetestPath('docs/TESTING.md'), true);
  assert.equal(isAllowedNativeRetestPath('AGENTS.md'), true);

  // .github/workflows/ci.yml must NOT be allowed
  assert.equal(isAllowedNativeRetestPath('.github/workflows/ci.yml'), false);

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

  assert.throws(() => validateNativeRetestDiff([
    '.github/workflows/ci.yml',
  ]), /disallowed changes/);
});

test('automatic git diff with --no-renames detects removed rust file on rename to doc', async () => {
  // Create a temporary git repo to simulate a rename from rust source to doc
  const tempRepo = await mkdtemp(resolve(tmpdir(), 'test-retest-rename-'));
  try {
    execFileSync('git', ['init'], { cwd: tempRepo });
    execFileSync('git', ['config', 'user.email', 'test@test.com'], { cwd: tempRepo });
    execFileSync('git', ['config', 'user.name', 'Test'], { cwd: tempRepo });

    const rustDir = resolve(tempRepo, 'crates/kernel/src');
    await mkdir(rustDir, { recursive: true });
    await writeFile(resolve(rustDir, 'renamed.rs'), 'fn test() {}');
    execFileSync('git', ['add', '.'], { cwd: tempRepo });
    execFileSync('git', ['commit', '-m', 'initial'], { cwd: tempRepo });
    const c1 = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: tempRepo, encoding: 'utf8' }).trim();

    // Now rename rust file to doc
    const docsDir = resolve(tempRepo, 'docs');
    await mkdir(docsDir, { recursive: true });
    await rm(resolve(rustDir, 'renamed.rs'));
    await writeFile(resolve(docsDir, 'renamed.md'), '# Doc');
    execFileSync('git', ['add', '.'], { cwd: tempRepo });
    execFileSync('git', ['commit', '-m', 'renamed to doc'], { cwd: tempRepo });
    const c2 = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: tempRepo, encoding: 'utf8' }).trim();

    // Verify that diff with --no-renames -z exposes the deleted rust path
    const stdout = execFileSync('git', ['diff', '--no-renames', '--name-only', '-z', c1, c2], { cwd: tempRepo, encoding: 'utf8' });
    const paths = stdout.split('\0').filter(Boolean);
    assert(paths.includes('crates/kernel/src/renamed.rs'), 'Must include deleted rust path');
    assert(paths.includes('docs/renamed.md'), 'Must include added doc path');

    // validateNativeRetestDiff MUST reject this diff
    assert.throws(() => validateNativeRetestDiff(paths), /disallowed changes: crates\/kernel\/src\/renamed\.rs/);
  } finally {
    await rm(tempRepo, { recursive: true, force: true });
  }
});

test('computeHarnessSha256 recursively hashes tests/native and catches nested helper changes', async () => {
  const hash1 = await computeHarnessSha256();
  assert.equal(typeof hash1, 'string');
  assert.equal(hash1.length, 64);

  // Missing directory fails explicitly
  await assert.rejects(
    () => computeHarnessSha256('nonexistent-dir-12345'),
    /does not exist/
  );
});

test('validateNativeRetestContract mandates artifactDirectory and verifies executable bytes', async () => {
  const dir = await mkdtemp(resolve(tmpdir(), 'test-retest-art-'));
  try {
    const exeDir = resolve(dir, 'target/debug');
    await mkdir(exeDir, { recursive: true });
    const exePath = resolve(exeDir, 'webnovel-desktop.exe');
    const content = Buffer.from('mock executable binary content');
    const sha = createHash('sha256').update(content).digest('hex');
    await writeFile(exePath, content);

    const validManifest = {
      schemaVersion: 1,
      commit: buildCommit,
      executable: 'target/debug/webnovel-desktop.exe',
      files: [
        { path: 'target/debug/webnovel-desktop.exe', sha256: sha, bytes: content.length }
      ]
    };

    // Valid check with artifactDirectory
    const result = await validateNativeRetestContract({
      manifest: validManifest,
      buildCommit,
      qualificationCommit,
      changedPaths: ['tests/native/native-smoke.mjs'],
      artifactDirectory: dir,
    });
    assert.equal(result.buildCommit, buildCommit);
    assert.equal(result.qualificationCommit, qualificationCommit);
    assert.equal(result.executableSha256, sha);

    // Missing artifactDirectory fails explicitly
    await assert.rejects(() => validateNativeRetestContract({
      manifest: validManifest,
      buildCommit,
      qualificationCommit,
      changedPaths: ['tests/native/native-smoke.mjs'],
    }), /Artifact directory is required/);

    // Mismatched executable bytes fails explicitly
    const mismatchedManifest = {
      ...validManifest,
      files: [{ path: 'target/debug/webnovel-desktop.exe', sha256: 'f'.repeat(64), bytes: content.length }]
    };
    await assert.rejects(() => validateNativeRetestContract({
      manifest: mismatchedManifest,
      buildCommit,
      qualificationCommit,
      changedPaths: ['tests/native/native-smoke.mjs'],
      artifactDirectory: dir,
    }), /Executable bytes on disk do not match/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test('validateNativeRetestContract fails closed on invalid commits', async () => {
  await assert.rejects(() => validateNativeRetestContract({
    buildCommit: 'short',
    qualificationCommit,
    artifactDirectory: 'some-dir',
  }), /Invalid buildCommit/);

  await assert.rejects(() => validateNativeRetestContract({
    buildCommit,
    qualificationCommit: 'not-hex-commit-hash-value-12345678901234',
    artifactDirectory: 'some-dir',
  }), /Invalid qualificationCommit/);
});

test('parseRunId verifies positive integer run IDs', () => {
  assert.equal(parseRunId('12345'), '12345');
  assert.throws(() => parseRunId(''), /positive integer/);
  assert.throws(() => parseRunId('abc'), /positive integer/);
  assert.throws(() => parseRunId('-5'), /positive integer/);
});
