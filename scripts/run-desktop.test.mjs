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
