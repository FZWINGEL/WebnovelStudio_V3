import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

export const STABLE_IDENTIFIER = 'com.webnovelstudio.v3';

const root = fileURLToPath(new URL('../', import.meta.url));

function readCargoVersion(cargoToml, label) {
  const workspace = cargoToml.match(/\[workspace\.package\]([\s\S]*?)(?:\n\[|$)/);
  const version = workspace?.[1]?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
  if (!version) throw new Error(`${label} is missing [workspace.package].version.`);
  return version;
}

function readWorkspaceVersion(crateToml, label) {
  const version = crateToml.match(/^version\.workspace\s*=\s*true\s*$/m);
  if (!version) throw new Error(`${label} must inherit version.workspace = true.`);
}

function readLockedPackageVersion(cargoLock, name) {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const packageBlock = cargoLock.match(new RegExp(`\\[\\[package\\]\\]\\r?\\nname = "${escaped}"\\r?\\nversion = "([^"]+)"`));
  if (!packageBlock) throw new Error(`Cargo.lock is missing the ${name} package entry.`);
  return packageBlock[1];
}

export function collectVersionValues({ cargoToml, coreToml, desktopToml, cargoLock, packageJson, packageLock, tauriConfig }) {
  const packageLockRoot = packageLock.packages?.[''];
  if (!packageLockRoot) throw new Error('apps/desktop/package-lock.json is missing its root package entry.');
  if (Object.prototype.hasOwnProperty.call(tauriConfig, 'version')) {
    throw new Error('apps/desktop/src-tauri/tauri.conf.json must omit version; Cargo.toml is authoritative.');
  }
  if (tauriConfig.identifier !== STABLE_IDENTIFIER) {
    throw new Error(`Tauri identifier must remain ${STABLE_IDENTIFIER}.`);
  }
  readWorkspaceVersion(coreToml, 'crates/core/Cargo.toml');
  readWorkspaceVersion(desktopToml, 'apps/desktop/src-tauri/Cargo.toml');
  return {
    workspaceCargo: readCargoVersion(cargoToml, 'Cargo.toml'),
    coreCargo: 'workspace',
    desktopCargo: 'workspace',
    cargoLockCore: readLockedPackageVersion(cargoLock, 'webnovel-core'),
    cargoLockDesktop: readLockedPackageVersion(cargoLock, 'webnovel-desktop'),
    packageJson: packageJson.version,
    packageLock: packageLock.version,
    packageLockRoot: packageLockRoot.version,
    tauriConfig: 'workspace',
  };
}

export function assertVersionValues(values) {
  const expected = values.workspaceCargo;
  const mismatches = Object.entries(values)
    .filter(([, value]) => value !== 'workspace' && value !== expected)
    .map(([name, value]) => `${name}=${value ?? '<missing>'}`);
  if (mismatches.length) {
    throw new Error(`Version mismatch; expected ${expected}: ${mismatches.join(', ')}`);
  }
  return values;
}

export async function checkVersions({ baseDir = root } = {}) {
  const read = name => readFile(path.join(baseDir, name), 'utf8');
  const [cargoToml, coreToml, desktopToml, cargoLockText, packageJsonText, packageLockText, tauriConfigText] = await Promise.all([
    read('Cargo.toml'),
    read('crates/core/Cargo.toml'),
    read('apps/desktop/src-tauri/Cargo.toml'),
    read('Cargo.lock'),
    read('apps/desktop/package.json'),
    read('apps/desktop/package-lock.json'),
    read('apps/desktop/src-tauri/tauri.conf.json'),
  ]);
  const values = collectVersionValues({
    cargoToml,
    coreToml,
    desktopToml,
    cargoLock: cargoLockText,
    packageJson: JSON.parse(packageJsonText),
    packageLock: JSON.parse(packageLockText),
    tauriConfig: JSON.parse(tauriConfigText),
  });
  return assertVersionValues(values);
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))) {
  try {
    const values = await checkVersions();
    console.log(`Version check passed: ${values.workspaceCargo} (Tauri inherits Cargo workspace version).`);
    if (process.argv.includes('--verbose')) console.log(JSON.stringify(values, null, 2));
  } catch (error) {
    console.error(`Version check failed: ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  }
}
