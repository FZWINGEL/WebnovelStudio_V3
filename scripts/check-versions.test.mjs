import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { collectVersionValues, assertVersionValues } from './check-versions.mjs';

async function fixture() {
  return {
    cargoToml: await readFile(new URL('../Cargo.toml', import.meta.url), 'utf8'),
    coreToml: await readFile(new URL('../crates/core/Cargo.toml', import.meta.url), 'utf8'),
    desktopToml: await readFile(new URL('../apps/desktop/src-tauri/Cargo.toml', import.meta.url), 'utf8'),
    cargoLock: await readFile(new URL('../Cargo.lock', import.meta.url), 'utf8'),
    packageJson: JSON.parse(await readFile(new URL('../apps/desktop/package.json', import.meta.url), 'utf8')),
    packageLock: JSON.parse(await readFile(new URL('../apps/desktop/package-lock.json', import.meta.url), 'utf8')),
    tauriConfig: JSON.parse(await readFile(new URL('../apps/desktop/src-tauri/tauri.conf.json', import.meta.url), 'utf8')),
  };
}

test('accepts synchronized workspace, frontend, lockfile, and inherited Tauri versions', async () => {
  const values = collectVersionValues(await fixture());
  assertVersionValues(values);
});

test('rejects a frontend package lock that would install a different product version', async () => {
  const input = await fixture();
  input.packageLock.packages[''].version = '2.9.9';
  assert.throws(() => assertVersionValues(collectVersionValues(input)), /packageLockRoot=2\.9\.9/);
});

test('rejects a Cargo lock that does not match the workspace package version', async () => {
  const input = await fixture();
  input.cargoLock = input.cargoLock.replace(/(name = "webnovel-core"\r?\nversion = ")3\.0\.0(")/, '$12.9.9$2');
  assert.throws(() => assertVersionValues(collectVersionValues(input)), /cargoLockCore=2\.9\.9/);
});

test('rejects a desktop crate version instead of allowing workspace drift', async () => {
  const input = await fixture();
  input.desktopToml = input.desktopToml.replace('version.workspace = true', 'version = "2.9.9"');
  assert.throws(() => collectVersionValues(input), /apps\/desktop\/src-tauri\/Cargo\.toml must inherit/);
});

test('rejects a duplicated Tauri version instead of allowing config drift', async () => {
  const input = await fixture();
  input.tauriConfig.version = '2.9.9';
  assert.throws(() => collectVersionValues(input), /must omit version/);
});

test('accepts CRLF Cargo.lock files', async () => {
  const input = await fixture();
  input.cargoLock = input.cargoLock.replace(/\n/g, '\r\n');
  assertVersionValues(collectVersionValues(input));
});
