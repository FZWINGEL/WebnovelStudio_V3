import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { sha256, validateManifest, verifyArtifact } from './native-artifact.mjs';

const commit = 'a'.repeat(40);
function fixture() {
  return { schemaVersion: 1, commit, buildMode: 'debug', platform: 'win32', architecture: 'x64',
    node: 'v24.20.0', rustc: 'rustc 1.98.1 (fixture)', runtime: 'system-webview2', assets: 'embedded',
    executable: 'webnovel-desktop.exe', files: [{ path: 'webnovel-desktop.exe', bytes: 3, sha256: sha256('exe') }] };
}
test('manifest rejects stale source, release builds, missing and duplicate files, unsafe paths and missing toolchain', () => {
  validateManifest(fixture(), commit);
  for (const change of [m => m.commit = 'b'.repeat(40), m => m.buildMode = 'release', m => m.files = [],
    m => m.files.push(m.files[0]), m => m.files[0].path = '../escape', m => delete m.rustc]) {
    const manifest = fixture(); change(manifest);
    assert.throws(() => validateManifest(manifest, commit));
  }
});
test('artifact verification checks actual executable bytes and missing files', async () => {
  const directory = await mkdtemp(resolve(tmpdir(), 'native-artifact-test-'));
  try {
    await writeFile(resolve(directory, 'manifest.json'), JSON.stringify(fixture()));
    await assert.rejects(verifyArtifact(directory, commit));
    await writeFile(resolve(directory, 'webnovel-desktop.exe'), 'exe');
    await verifyArtifact(directory, commit);
    await writeFile(resolve(directory, 'webnovel-desktop.exe'), 'bad');
    await assert.rejects(verifyArtifact(directory, commit), /hash mismatch/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
