import test from 'node:test';
import assert from 'node:assert/strict';

import {
  expectedProductVersions,
  isAllowedRetestPath,
  parseInstallerRunId,
  validateBuildMetadata,
  validateChangedPaths,
  validatePackageRun,
  validateSuccessfulBuildStep,
} from './prepare-package-retest.mjs';

const repository = 'FZWINGEL/WebnovelStudio_V3';
const runId = '34066312408';
const headSha = 'a'.repeat(40);
const digest = 'b'.repeat(64);

function runFixture(overrides = {}) {
  return {
    id: Number(runId),
    status: 'completed',
    conclusion: 'failure',
    event: 'workflow_dispatch',
    path: '.github/workflows/windows-package-smoke.yml',
    head_sha: headSha,
    repository: { full_name: repository },
    ...overrides,
  };
}

function metadataFixture(overrides = {}) {
  return {
    schemaVersion: 1,
    github: { repository, runId, sha: headSha, ref: 'refs/heads/main' },
    source: {
      workspace: 'D:\\a\\workspace',
      gitSha: headSha,
      dirtyStatus: '',
      cargoLockSha256: digest,
      packageLockSha256: digest,
      tauriConfigSha256: digest,
    },
    installer: {
      name: 'WebnovelStudio V3_3.0.0_x64-setup.exe',
      sha256: digest,
      productName: 'WebnovelStudio V3',
      productVersion: '3.0.0',
    },
    ...overrides,
  };
}

test('installer run validation requires a completed manual package run', () => {
  assert.deepEqual(validatePackageRun(runFixture(), { repository, runId, currentSha: headSha }), {
    runId,
    repository,
    headSha,
  });
  assert.throws(() => validatePackageRun(runFixture({ event: 'push' }), { repository, runId, currentSha: headSha }), /manually dispatched/);
  assert.throws(() => validatePackageRun(runFixture({ status: 'in_progress' }), { repository, runId, currentSha: headSha }), /not completed/);
  assert.throws(() => validatePackageRun(runFixture({ repository: {} }), { repository, runId, currentSha: headSha }), /repository identity/);
  assert.throws(() => validatePackageRun(runFixture({ head_sha: 'c'.repeat(40) }), { repository, runId, currentSha: headSha }), /head SHA/);
});

test('build step must complete successfully even when the overall smoke run failed later', () => {
  const jobs = [{
    name: 'package-smoke',
    status: 'completed',
    steps: [{ name: 'Build the locked release NSIS installer', status: 'completed', conclusion: 'success' }],
  }];
  assert.equal(validateSuccessfulBuildStep(jobs).buildStep.conclusion, 'success');
  assert.throws(() => validateSuccessfulBuildStep([{ ...jobs[0], steps: [{ ...jobs[0].steps[0], conclusion: 'failure' }] }]), /did not succeed/);
  assert.throws(() => validateSuccessfulBuildStep([{ ...jobs[0], name: 'other-job' }]), /package-smoke job/);
});

test('original metadata validates installer identity, clean source, locks, and current product version', () => {
  const metadata = metadataFixture();
  assert.equal(validateBuildMetadata(metadata, {
    repository,
    runId,
    headSha,
    installerName: metadata.installer.name,
    installerSha256: digest,
    expectedProductVersion: '3.0.0',
    expectedSourceHashes: {
      cargoLockSha256: digest,
      packageLockSha256: digest,
      tauriConfigSha256: digest,
    },
  }).productVersion, '3.0.0');
  assert.equal(expectedProductVersions('3.0.0').has('3.0.0.0'), false, 'product version matching is exact');
  assert.throws(() => validateBuildMetadata(metadataFixture({ installer: { ...metadata.installer, productVersion: '3.0.0.0' } }), {
    repository, runId, headSha, installerName: metadata.installer.name, installerSha256: digest,
    expectedProductVersion: '3.0.0', expectedSourceHashes: { cargoLockSha256: digest, packageLockSha256: digest, tauriConfigSha256: digest },
  }), /product version/);
  assert.throws(() => validateBuildMetadata(metadataFixture({ source: { ...metadata.source, dirtyStatus: ' M apps/desktop/src/App.tsx' } }), {
    repository, runId, headSha, installerName: metadata.installer.name, installerSha256: digest,
    expectedProductVersion: '3.0.0', expectedSourceHashes: { cargoLockSha256: digest, packageLockSha256: digest, tauriConfigSha256: digest },
  }), /not clean/);
  const missingDirtyStatus = { ...metadata.source };
  delete missingDirtyStatus.dirtyStatus;
  assert.throws(() => validateBuildMetadata(metadataFixture({ source: missingDirtyStatus }), {
    repository, runId, headSha, installerName: metadata.installer.name, installerSha256: digest,
    expectedProductVersion: '3.0.0', expectedSourceHashes: { cargoLockSha256: digest, packageLockSha256: digest, tauriConfigSha256: digest },
  }), /dirtyStatus/);
  assert.throws(() => validateBuildMetadata(metadataFixture({ installer: { ...metadata.installer, sha256: 'c'.repeat(64) } }), {
    repository, runId, headSha, installerName: metadata.installer.name, installerSha256: digest,
    expectedProductVersion: '3.0.0', expectedSourceHashes: { cargoLockSha256: digest, packageLockSha256: digest, tauriConfigSha256: digest },
  }), /Downloaded installer SHA/);
});

test('retest source diff is fail-closed and does not treat lock hashes as proof of a harness-only change', () => {
  assert.doesNotThrow(() => validateChangedPaths([
    'docs/IMPLEMENTATION_STATUS.md',
    'scripts/windows-package-qualification.ps1',
    'scripts/prepare-package-retest.mjs',
    '.github/workflows/windows-package-smoke.yml',
  ]));
  assert.equal(isAllowedRetestPath('Cargo.lock'), false);
  assert.equal(isAllowedRetestPath('apps/desktop/package-lock.json'), false);
  assert.throws(() => validateChangedPaths(['Cargo.lock']), /disallowed files/);
  assert.throws(() => validateChangedPaths(['apps/desktop/src/App.tsx']), /disallowed files/);
  assert.throws(() => validateChangedPaths(['apps/desktop/src-tauri/src/main.rs']), /disallowed files/);
});

test('installer run id rejects malformed and unsafe values', () => {
  assert.equal(parseInstallerRunId(runId), runId);
  assert.throws(() => parseInstallerRunId(''), /positive integer/);
  assert.throws(() => parseInstallerRunId('12.5'), /positive integer/);
  assert.throws(() => parseInstallerRunId('9007199254740992'), /safe positive integer/);
});
