import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { copyFile, mkdir, readFile, realpath, writeFile } from 'node:fs/promises';
import { isAbsolute, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

export const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
export const runtimePins = {
  node: `v${(await readFile(new URL('../.node-version', import.meta.url), 'utf8')).trim()}`,
  rustc: (await readFile(new URL('../rust-toolchain.toml', import.meta.url), 'utf8')).match(/^channel\s*=\s*"([^"]+)"/m)?.[1],
};
assert(runtimePins.rustc, 'Missing pinned Rust toolchain');
const git = (root, ...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8', windowsHide: true }).trim();
export function checkoutIdentity(root) {
  assert.equal(git(root, 'status', '--porcelain', '--untracked-files=normal'), '', 'Artifact qualification requires a clean checkout');
  return git(root, 'rev-parse', 'HEAD');
}
export function validateManifest(manifest, commit) {
  assert.equal(manifest.schemaVersion, 1);
  assert.match(commit, /^[a-f0-9]{40}$/);
  assert.equal(manifest.commit, commit, 'Executable and checkout revision differ');
  assert.equal(manifest.buildMode, 'debug');
  assert.equal(manifest.platform, 'win32');
  assert.equal(manifest.architecture, 'x64');
  assert.equal(manifest.node, runtimePins.node);
  assert.equal(typeof manifest.rustc, 'string');
  assert(manifest.rustc.startsWith(`rustc ${runtimePins.rustc} `), 'Unexpected Rust toolchain');
  assert.equal(manifest.runtime, 'system-webview2');
  assert.equal(manifest.assets, 'embedded');
  assert.equal(manifest.executable, 'webnovel-desktop.exe');
  assert(Array.isArray(manifest.files) && manifest.files.length > 0);
  const names = new Set();
  for (const file of manifest.files) {
    assert.match(file.path, /^[a-zA-Z0-9_.-]+$/, 'Artifact files must be flat safe names');
    assert(!names.has(file.path), 'Duplicate artifact file'); names.add(file.path);
    assert.match(file.sha256, /^[a-f0-9]{64}$/);
    assert(Number.isSafeInteger(file.bytes) && file.bytes > 0);
  }
  assert(names.has(manifest.executable), 'Missing executable');
  return manifest;
}
export async function verifyArtifact(directory, commit) {
  const manifest = validateManifest(JSON.parse(await readFile(resolve(directory, 'manifest.json'), 'utf8')), commit);
  const root = await realpath(directory);
  for (const file of manifest.files) {
    const path = await realpath(resolve(root, file.path));
    const child = relative(root, path);
    assert(child && !isAbsolute(child) && child !== '..' && !child.startsWith(`..${sep}`), 'Artifact escaped its directory');
    const bytes = await readFile(path);
    assert.equal(bytes.length, file.bytes, `Artifact size mismatch: ${file.path}`);
    assert.equal(sha256(bytes), file.sha256, `Artifact hash mismatch: ${file.path}`);
  }
  return manifest;
}
export async function createArtifact(root, directory) {
  const commit = checkoutIdentity(root);
  assert.equal(process.platform, 'win32'); assert.equal(process.arch, 'x64');
  const executable = resolve(root, 'target/debug/webnovel-desktop.exe');
  const rustc = execFileSync('rustc', ['--version'], { encoding: 'utf8', windowsHide: true }).trim();
  const bytes = await readFile(executable);
  const manifest = {
    schemaVersion: 1, commit, buildMode: 'debug', platform: process.platform,
    architecture: process.arch, node: process.version, rustc,
    runtime: 'system-webview2', assets: 'embedded', executable: 'webnovel-desktop.exe',
    files: [{ path: 'webnovel-desktop.exe', bytes: bytes.length, sha256: sha256(bytes) }],
  };
  // Bundled resources or external frontend assets need an explicit manifest extension.
  const config = JSON.parse(await readFile(resolve(root, 'apps/desktop/src-tauri/tauri.conf.json'), 'utf8'));
  assert(!config.bundle?.resources && !config.bundle?.externalBin, 'Declare new runtime files before transferring the app');
  validateManifest(manifest, commit);
  await mkdir(directory, { recursive: true });
  await copyFile(executable, resolve(directory, manifest.executable));
  await writeFile(resolve(directory, 'manifest.json'), JSON.stringify(manifest, null, 2));
  await verifyArtifact(directory, commit);
  return manifest;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = fileURLToPath(new URL('../', import.meta.url));
  const [command, directory] = process.argv.slice(2);
  assert(directory, 'Artifact directory is required');
  if (command === 'create') console.log(JSON.stringify(await createArtifact(root, resolve(directory))));
  else if (command === 'verify') console.log(JSON.stringify(await verifyArtifact(resolve(directory), checkoutIdentity(root))));
  else throw new Error('Expected create or verify');
}
