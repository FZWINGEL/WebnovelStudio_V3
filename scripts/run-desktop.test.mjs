import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  findNpm,
  computeInstallSignature,
  isInstallValid,
  recordTiming,
} from './run-desktop.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));

test('findNpm finds npm executable or npm-cli.js', () => {
  const npm = findNpm();
  assert(npm, 'findNpm should return a valid npm path');
  assert(typeof npm === 'string' && npm.length > 0);
});

test('computeInstallSignature computes sha256 hex string for apps/desktop', () => {
  const sig = computeInstallSignature(resolve(root, 'apps/desktop'));
  assert(sig, 'Expected install signature for apps/desktop');
  assert.equal(sig.length, 64);
  assert(/^[0-9a-f]{64}$/.test(sig));

  // Nonexistent directory returns null
  assert.equal(computeInstallSignature(resolve(root, 'nonexistent-directory-xyz')), null);
});

test('isInstallValid checks key file and install signature', () => {
  const desktop = resolve(root, 'apps/desktop');
  const valid = isInstallValid(desktop, 'package.json');
  // If .install-signature is present and valid, returns true
  assert(typeof valid === 'boolean');

  // Nonexistent key file returns false
  assert.equal(isInstallValid(desktop, 'nonexistent-key-file.js'), false);
});

test('recordTiming writes valid JSONL entry into .local/performance/desktop-timings.jsonl', () => {
  const tempDir = mkdtempSync(join(tmpdir(), 'wns-timing-test-'));
  try {
    const entry = {
      timestamp: '2026-09-13T20:00:00.000Z',
      action: 'ensure-frontend',
      args: [],
      frontendDeps: 'reused',
      nativeDeps: 'skipped',
      durationMs: 42,
      status: 'success',
    };

    recordTiming(entry, tempDir);

    const timingFile = resolve(tempDir, '.local/performance/desktop-timings.jsonl');
    assert(existsSync(timingFile), 'desktop-timings.jsonl must exist');

    const content = readFileSync(timingFile, 'utf8').trim();
    const parsed = JSON.parse(content);
    assert.deepEqual(parsed, entry);
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
});

test('updateDepStatus preserves installed over subsequent reused checks', async () => {
  const { dependencyStatus, updateDepStatus } = await import('./run-desktop.mjs');
  
  // Reset for test
  dependencyStatus.frontend = 'skipped';
  
  updateDepStatus('frontend', 'installed');
  assert.equal(dependencyStatus.frontend, 'installed');

  // Second check reports reused: MUST RETAIN 'installed'
  updateDepStatus('frontend', 'reused');
  assert.equal(dependencyStatus.frontend, 'installed', 'installed status must not be overwritten by reused');

  // Failed status test
  dependencyStatus.native = 'skipped';
  updateDepStatus('native', 'failed');
  assert.equal(dependencyStatus.native, 'failed');
  // reused does not clear failure
  updateDepStatus('native', 'reused');
  assert.equal(dependencyStatus.native, 'failed');
});

test('recordPhase links telemetry phases by invocationId', async () => {
  const tempDir = mkdtempSync(join(tmpdir(), 'wns-phase-test-'));
  const { setInvocationId, recordPhase } = await import('./run-desktop.mjs');
  try {
    setInvocationId('test-invocation-12345');

    recordPhase({
      type: 'dependency',
      target: 'frontend',
      status: 'installed',
      durationMs: 123.4,
    }, tempDir);

    recordPhase({
      type: 'command',
      command: 'cargo fmt --all --check',
      cwd: '.',
      exitCode: 0,
      durationMs: 45.6,
      status: 'success',
    }, tempDir);

    const timingFile = resolve(tempDir, '.local/performance', 'desktop-timings.jsonl');
    assert(existsSync(timingFile));

    const lines = readFileSync(timingFile, 'utf8').trim().split('\n').map(l => JSON.parse(l));
    assert.equal(lines.length, 2);
    assert.equal(lines[0].invocationId, 'test-invocation-12345');
    assert.equal(lines[0].type, 'dependency');
    assert.equal(lines[0].status, 'installed');
    assert.equal(lines[1].invocationId, 'test-invocation-12345');
    assert.equal(lines[1].type, 'command');
    assert.equal(lines[1].durationMs, 45.6);
  } finally {
    setInvocationId(null);
    rmSync(tempDir, { recursive: true, force: true });
  }
});

test('resolveWorkerSettings computes effective Vitest and Rust worker telemetry', async () => {
  const { resolveWorkerSettings } = await import('./run-desktop.mjs');
  const settings = resolveWorkerSettings();
  assert.equal(settings.vitestPool, 'threads');
  assert.equal(settings.vitestIsolate, true);
  assert(typeof settings.vitestMaxWorkers === 'number' && settings.vitestMaxWorkers >= 2);
  assert(typeof settings.rustTestThreads === 'string' && settings.rustTestThreads.length > 0);
  assert(typeof settings.cargoBuildJobs === 'string' && settings.cargoBuildJobs.length > 0);
});

