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

import { runtimePins } from './native-artifact.mjs';

test('computeHarnessSha256 produces identical hash regardless of parent directory name', async () => {
  const tempRoot = await mkdtemp(resolve(tmpdir(), 'test-harness-parent-'));
  try {
    const dirNormal = resolve(tempRoot, 'normal-checkout');
    const dirLocal = resolve(tempRoot, '.local/nested-checkout');

    for (const d of [dirNormal, dirLocal]) {
      await mkdir(resolve(d, 'tests/native/fixtures'), { recursive: true });
      await mkdir(resolve(d, 'apps/desktop/scripts'), { recursive: true });
      await mkdir(resolve(d, 'scripts'), { recursive: true });

      await writeFile(resolve(d, 'tests/native/native-smoke.mjs'), 'console.log("smoke");');
      await writeFile(resolve(d, 'tests/native/fixtures/fixture.txt'), 'plain text fixture content');
      await writeFile(resolve(d, 'apps/desktop/scripts/native-smoke.mjs'), 'export const stub = true;');
      await writeFile(resolve(d, 'scripts/native-consumer.mjs'), 'export const consumer = true;');
    }

    const shaNormal = await computeHarnessSha256(dirNormal);
    const shaLocal = await computeHarnessSha256(dirLocal);

    assert.equal(typeof shaNormal, 'string');
    assert.equal(shaNormal.length, 64);
    assert.equal(shaNormal, shaLocal, 'Harness hash must be identical even when parent directory contains .local');
  } finally {
    await rm(tempRoot, { recursive: true, force: true });
  }
});

test('computeHarnessSha256 changes on nested helper or non-code fixture mutation', async () => {
  const tempRoot = await mkdtemp(resolve(tmpdir(), 'test-harness-mutation-'));
  try {
    const dir = resolve(tempRoot, 'checkout');
    await mkdir(resolve(dir, 'tests/native/fixtures'), { recursive: true });
    await mkdir(resolve(dir, 'tests/native/helpers'), { recursive: true });
    await mkdir(resolve(dir, 'apps/desktop/scripts'), { recursive: true });
    await mkdir(resolve(dir, 'scripts'), { recursive: true });

    await writeFile(resolve(dir, 'tests/native/native-smoke.mjs'), 'console.log("smoke");');
    await writeFile(resolve(dir, 'tests/native/fixtures/fixture.txt'), 'initial fixture content');
    await writeFile(resolve(dir, 'tests/native/helpers/util.mjs'), 'export const helper = 1;');
    await writeFile(resolve(dir, 'apps/desktop/scripts/native-smoke.mjs'), 'export const stub = true;');
    await writeFile(resolve(dir, 'scripts/native-consumer.mjs'), 'export const consumer = true;');

    const baseSha = await computeHarnessSha256(dir);

    // 1. Mutate non-code fixture (.txt)
    await writeFile(resolve(dir, 'tests/native/fixtures/fixture.txt'), 'modified fixture content');
    const mutatedTxtSha = await computeHarnessSha256(dir);
    assert.notEqual(baseSha, mutatedTxtSha, 'Mutating non-code fixture .txt must change harness SHA-256');

    // 2. Mutate nested script helper
    await writeFile(resolve(dir, 'tests/native/helpers/util.mjs'), 'export const helper = 2;');
    const mutatedScriptSha = await computeHarnessSha256(dir);
    assert.notEqual(mutatedTxtSha, mutatedScriptSha, 'Mutating nested script must change harness SHA-256');
  } finally {
    await rm(tempRoot, { recursive: true, force: true });
  }
});

test('operational verifyNativeRetest requires real commits, real on-disk manifest, and rejects prohibited changes', async () => {
  const tempRepo = await mkdtemp(resolve(tmpdir(), 'test-operational-retest-'));
  const tempArtifact = await mkdtemp(resolve(tmpdir(), 'test-operational-art-'));

  try {
    execFileSync('git', ['init'], { cwd: tempRepo });
    execFileSync('git', ['config', 'user.email', 'test@test.com'], { cwd: tempRepo });
    execFileSync('git', ['config', 'user.name', 'Test'], { cwd: tempRepo });

    // Create required harness directories
    await mkdir(resolve(tempRepo, 'tests/native'), { recursive: true });
    await mkdir(resolve(tempRepo, 'apps/desktop/scripts'), { recursive: true });
    await mkdir(resolve(tempRepo, 'scripts'), { recursive: true });
    await mkdir(resolve(tempRepo, 'docs'), { recursive: true });

    await writeFile(resolve(tempRepo, 'tests/native/native-smoke.mjs'), '// smoke');
    await writeFile(resolve(tempRepo, 'apps/desktop/scripts/native-smoke.mjs'), '// smoke');
    await writeFile(resolve(tempRepo, 'scripts/native-consumer.mjs'), '// consumer');
    await writeFile(resolve(tempRepo, 'docs/README.md'), '# Initial');

    execFileSync('git', ['add', '.'], { cwd: tempRepo });
    execFileSync('git', ['commit', '-m', 'commit-1'], { cwd: tempRepo });
    const c1 = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: tempRepo, encoding: 'utf8' }).trim();

    // Create real on-disk executable and manifest in artifact directory
    const exeName = 'webnovel-desktop.exe';
    const exeContent = Buffer.from('test binary content for executable');
    const exeSha = createHash('sha256').update(exeContent).digest('hex');
    await writeFile(resolve(tempArtifact, exeName), exeContent);

    const validManifest = {
      schemaVersion: 1,
      commit: c1,
      buildMode: 'debug',
      platform: 'win32',
      architecture: 'x64',
      node: runtimePins.node,
      rustc: `rustc ${runtimePins.rustc} (mock)`,
      runtime: 'system-webview2',
      assets: 'embedded',
      executable: exeName,
      files: [
        { path: exeName, sha256: exeSha, bytes: exeContent.length }
      ]
    };
    await writeFile(resolve(tempArtifact, 'manifest.json'), JSON.stringify(validManifest));

    // Commit 2: Valid allowed doc change
    await writeFile(resolve(tempRepo, 'docs/README.md'), '# Updated Docs');
    execFileSync('git', ['add', '.'], { cwd: tempRepo });
    execFileSync('git', ['commit', '-m', 'commit-2 docs update'], { cwd: tempRepo });
    const c2 = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: tempRepo, encoding: 'utf8' }).trim();

    // Operational verification succeeds with real git commits and on-disk manifest
    const res = await validateNativeRetestContract({
      baseDir: tempRepo,
      artifactDirectory: tempArtifact,
      buildCommit: c1,
      qualificationCommit: c2,
    });
    assert.equal(res.buildCommit, c1);
    assert.equal(res.qualificationCommit, c2);
    assert.equal(res.executableSha256, exeSha);

    // Commit 3: Disallowed change to rust source
    await mkdir(resolve(tempRepo, 'crates/kernel/src'), { recursive: true });
    await writeFile(resolve(tempRepo, 'crates/kernel/src/lib.rs'), 'fn disallowed() {}');
    execFileSync('git', ['add', '.'], { cwd: tempRepo });
    execFileSync('git', ['commit', '-m', 'commit-3 prohibited change'], { cwd: tempRepo });
    const c3 = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: tempRepo, encoding: 'utf8' }).trim();

    // Operational verification MUST reject prohibited change - cannot be bypassed by caller
    await assert.rejects(
      () => validateNativeRetestContract({
        baseDir: tempRepo,
        artifactDirectory: tempArtifact,
        buildCommit: c1,
        qualificationCommit: c3,
        // Even if an adversarial caller passes changedPaths: [] or custom manifest, it is ignored
        changedPaths: [],
      }),
      /disallowed changes: crates\/kernel\/src\/lib\.rs/
    );

    // Non-existent commit fails explicitly
    await assert.rejects(
      () => validateNativeRetestContract({
        baseDir: tempRepo,
        artifactDirectory: tempArtifact,
        buildCommit: 'f'.repeat(40),
        qualificationCommit: c2,
      }),
      /buildCommit does not exist in repository/
    );

    // Missing manifest.json in artifact directory fails explicitly
    execFileSync('git', ['checkout', c2], { cwd: tempRepo });
    const emptyArtDir = await mkdtemp(resolve(tmpdir(), 'test-empty-art-'));
    try {
      await assert.rejects(
        () => validateNativeRetestContract({
          baseDir: tempRepo,
          artifactDirectory: emptyArtDir,
          buildCommit: c1,
          qualificationCommit: c2,
        }),
        /ENOENT/
      );
    } finally {
      await rm(emptyArtDir, { recursive: true, force: true });
    }

  } finally {
    await rm(tempRepo, { recursive: true, force: true });
    await rm(tempArtifact, { recursive: true, force: true });
  }
});

test('parseRunId verifies positive integer run IDs', () => {
  assert.equal(parseRunId('12345'), '12345');
  assert.throws(() => parseRunId(''), /positive integer/);
  assert.throws(() => parseRunId('abc'), /positive integer/);
  assert.throws(() => parseRunId('-5'), /positive integer/);
});

