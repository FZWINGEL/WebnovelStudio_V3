import { createHash } from 'node:crypto';
import { appendFile, copyFile, mkdir, mkdtemp, readFile, readdir } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import path from 'node:path';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

export const RETEST_ALLOWED_PATHS = Object.freeze([
  '.github/workflows/windows-package-smoke.yml',
  'scripts/windows-package-qualification.ps1',
  'scripts/prepare-package-retest.mjs',
  'scripts/prepare-package-retest.test.mjs',
]);

const ROOT_DOCUMENTS = new Set([
  'AGENTS.md',
  'CHANGELOG.md',
  'DESIGN.md',
  'PRODUCT.md',
  'README.md',
]);

function fail(message) {
  throw new Error(message);
}

function requireString(value, label) {
  if (typeof value !== 'string' || value.trim() === '') fail(`${label} must be a non-empty string.`);
  return value.trim();
}

function requireObject(value, label) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail(`${label} must be an object.`);
  return value;
}

export function parseInstallerRunId(value) {
  const text = String(value ?? '').trim();
  if (!/^\d+$/.test(text)) fail(`installer_run_id must be a positive integer, got '${text || '<missing>'}'.`);
  const number = Number(text);
  if (!Number.isSafeInteger(number) || number <= 0) fail(`installer_run_id is outside the safe positive integer range: ${text}.`);
  return text;
}

export function expectedProductVersions(version) {
  const normalized = requireString(version, 'expected product version');
  return new Set([normalized]);
}

export function isAllowedRetestPath(filePath) {
  const normalized = String(filePath ?? '').replaceAll('\\', '/').replace(/^\.\//, '');
  return RETEST_ALLOWED_PATHS.includes(normalized)
    || normalized.startsWith('docs/')
    || ROOT_DOCUMENTS.has(normalized);
}

export function validateChangedPaths(paths) {
  if (!Array.isArray(paths)) fail('git diff paths must be an array.');
  const normalized = paths.map(filePath => String(filePath).replaceAll('\\', '/'));
  const rejected = normalized.filter(filePath => !isAllowedRetestPath(filePath));
  if (rejected.length > 0) {
    fail(`Installer reuse is refused because the original build differs in disallowed files: ${rejected.join(', ')}`);
  }
  return normalized;
}

export function validatePackageRun(run, { repository, runId, currentSha } = {}) {
  requireObject(run, 'workflow run');
  const expectedRepository = requireString(repository, 'repository');
  const expectedRunId = parseInstallerRunId(runId);
  const expectedSha = requireGitSha(currentSha || run.head_sha, 'workflow head SHA');
  if (String(run.id) !== expectedRunId) fail(`Workflow run id does not match installer_run_id: ${run.id}.`);
  if (run.status !== 'completed') fail(`Workflow run ${expectedRunId} is not completed: ${run.status ?? '<missing>'}.`);
  if (!['success', 'failure'].includes(String(run.conclusion))) {
    fail(`Workflow run ${expectedRunId} has an unusable conclusion: ${run.conclusion ?? '<missing>'}.`);
  }
  if (run.event !== 'workflow_dispatch') fail(`Workflow run ${expectedRunId} was not manually dispatched: ${run.event ?? '<missing>'}.`);
  if (run.path !== '.github/workflows/windows-package-smoke.yml') {
    fail(`Workflow run ${expectedRunId} did not use the package smoke workflow: ${run.path ?? '<missing>'}.`);
  }
  if (String(run.head_sha ?? '').toLowerCase() !== expectedSha) {
    fail(`Workflow run ${expectedRunId} head SHA does not match the requested source: ${run.head_sha ?? '<missing>'}.`);
  }
  const runRepository = run.repository?.full_name;
  if (!runRepository) fail('Workflow run repository identity is missing.');
  if (runRepository !== expectedRepository) fail(`Workflow run repository is ${runRepository}, expected ${expectedRepository}.`);
  return { runId: expectedRunId, repository: expectedRepository, headSha: expectedSha };
}

export function validateSuccessfulBuildStep(jobs) {
  if (!Array.isArray(jobs)) fail('workflow jobs response must contain an array.');
  const packageJob = jobs.find(job => job?.name === 'package-smoke');
  if (!packageJob) fail('The package-smoke job was not present in the installer run.');
  if (packageJob.status !== 'completed') fail(`The package-smoke job was not completed: ${packageJob.status ?? '<missing>'}.`);
  const steps = Array.isArray(packageJob.steps) ? packageJob.steps : [];
  const buildStep = steps.find(step => step?.name === 'Build the locked release NSIS installer');
  if (!buildStep) fail('The locked release NSIS installer build step was not present.');
  if (buildStep.status !== 'completed' || buildStep.conclusion !== 'success') {
    fail(`The locked release NSIS installer build step did not succeed: ${buildStep.status ?? '<missing>'}/${buildStep.conclusion ?? '<missing>'}.`);
  }
  return { packageJob, buildStep };
}

function requireSha(value, label) {
  const sha = requireString(value, label).toLowerCase();
  if (!/^[0-9a-f]{64}$/.test(sha)) fail(`${label} must be a SHA-256 hex digest.`);
  return sha;
}

function requireGitSha(value, label) {
  const sha = requireString(value, label).toLowerCase();
  if (!/^[0-9a-f]{40}$/.test(sha)) fail(`${label} must be a full Git commit SHA.`);
  return sha;
}

export function validateBuildMetadata(metadata, {
  repository,
  runId,
  headSha,
  installerName,
  installerSha256,
  expectedProductVersion,
  expectedSourceHashes,
} = {}) {
  requireObject(metadata, 'build metadata');
  const github = requireObject(metadata.github, 'build metadata github');
  const source = requireObject(metadata.source, 'build metadata source');
  const installer = requireObject(metadata.installer, 'build metadata installer');
  const expectedRepo = requireString(repository, 'repository');
  const expectedRun = parseInstallerRunId(runId);
  const expectedHead = requireGitSha(headSha, 'workflow head SHA');
  if (github.repository !== expectedRepo) fail(`Build metadata repository is ${github.repository ?? '<missing>'}, expected ${expectedRepo}.`);
  if (String(github.runId) !== expectedRun) fail(`Build metadata run id is ${github.runId ?? '<missing>'}, expected ${expectedRun}.`);
  if (String(github.sha ?? '').toLowerCase() !== expectedHead) fail('Build metadata SHA does not match the workflow head SHA.');
  if (String(source.gitSha ?? '').toLowerCase() !== expectedHead) fail('Build metadata source git SHA does not match the workflow head SHA.');
  if (!Object.prototype.hasOwnProperty.call(source, 'dirtyStatus') || typeof source.dirtyStatus !== 'string' || source.dirtyStatus.trim() !== '') {
    fail('The original installer build source was not clean or did not record a clean dirtyStatus.');
  }
  const actualInstallerName = requireString(installer.name, 'build metadata installer name');
  if (installerName && actualInstallerName !== installerName) fail(`Build metadata installer name is ${actualInstallerName}, expected ${installerName}.`);
  if (installer.productName !== 'WebnovelStudio V3') fail(`Build metadata installer product name is ${installer.productName ?? '<missing>'}, expected WebnovelStudio V3.`);
  const actualInstallerSha = requireSha(installer.sha256, 'build metadata installer SHA-256');
  if (installerSha256 && actualInstallerSha !== requireSha(installerSha256, 'downloaded installer SHA-256')) {
    fail('Downloaded installer SHA-256 does not match build metadata.');
  }
  const productVersion = requireString(installer.productVersion, 'build metadata product version');
  if (!expectedProductVersions(expectedProductVersion).has(productVersion)) {
    fail(`Build metadata product version ${productVersion} does not exactly match expected ${expectedProductVersion}.`);
  }
  const hashes = requireObject(expectedSourceHashes, 'expected source hashes');
  for (const [field, expected] of Object.entries(hashes)) {
    const actual = requireSha(source[field], `build metadata source ${field}`);
    if (actual !== requireSha(expected, `expected source ${field}`)) {
      fail(`Build metadata source hash ${field} does not match the current checkout.`);
    }
  }
  return {
    repository: expectedRepo,
    runId: expectedRun,
    headSha: expectedHead,
    installerName: actualInstallerName,
    installerSha256: actualInstallerSha,
    productVersion,
  };
}

export async function sha256File(filePath) {
  const hash = createHash('sha256');
  hash.update(await readFile(filePath));
  return hash.digest('hex');
}

async function walkFiles(root) {
  const entries = await readdir(root, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const entryPath = path.join(root, entry.name);
    if (entry.isDirectory()) files.push(...await walkFiles(entryPath));
    else if (entry.isFile()) files.push(entryPath);
  }
  return files;
}

async function jsonFile(filePath, label) {
  try {
    return JSON.parse((await readFile(filePath, 'utf8')).replace(/^\uFEFF/, ''));
  } catch (error) {
    fail(`${label} is not valid JSON: ${error instanceof Error ? error.message : String(error)}`);
  }
}

async function runFile(command, args, { cwd, env } = {}) {
  try {
    return await execFileAsync(command, args, {
      cwd,
      env,
      windowsHide: true,
      maxBuffer: 8 * 1024 * 1024,
    });
  } catch (error) {
    const detail = [error?.stderr, error?.stdout, error?.message].filter(Boolean).join('\n').trim();
    fail(`${command} ${args.join(' ')} failed${detail ? `: ${detail}` : '.'}`);
  }
}

async function ghJson(repository, args, env, cwd) {
  const { stdout } = await runFile('gh', ['api', ...args], { cwd, env });
  return jsonFileFromText(stdout, `gh api ${args.join(' ')}`);
}

function jsonFileFromText(text, label) {
  try {
    return JSON.parse(String(text).replace(/^\uFEFF/, ''));
  } catch (error) {
    fail(`${label} returned invalid JSON: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function outputValue(value) {
  return String(value).replaceAll('%', '%25').replaceAll('\r', '%0D').replaceAll('\n', '%0A');
}

export async function preparePackageRetest({ env = process.env, argv = process.argv.slice(2) } = {}) {
  if (env.GITHUB_ACTIONS !== 'true') fail('Package retest orchestration requires GITHUB_ACTIONS=true.');
  const cliIndex = argv.indexOf('--installer-run-id');
  const cliRunId = cliIndex >= 0 ? argv[cliIndex + 1] : undefined;
  const runId = parseInstallerRunId(cliRunId || env.INSTALLER_RUN_ID || env.INPUT_INSTALLER_RUN_ID);
  const repository = requireString(env.GITHUB_REPOSITORY, 'GITHUB_REPOSITORY');
  const currentSha = requireGitSha(env.GITHUB_SHA, 'GITHUB_SHA');
  const workspaceRoot = path.resolve(requireString(env.GITHUB_WORKSPACE, 'GITHUB_WORKSPACE'));
  const runnerTemp = path.resolve(requireString(env.RUNNER_TEMP, 'RUNNER_TEMP'));
  if (!env.GH_TOKEN && !env.GITHUB_TOKEN) fail('A GitHub token is required for package artifact orchestration.');
  const childEnv = { ...env, GH_TOKEN: env.GH_TOKEN || env.GITHUB_TOKEN };
  const run = await ghJson(repository, [`repos/${repository}/actions/runs/${runId}`], childEnv, workspaceRoot);
  const runIdentity = validatePackageRun(run, { repository, runId });
  const jobs = await ghJson(repository, [`repos/${repository}/actions/runs/${runId}/jobs?per_page=100`], childEnv, workspaceRoot);
  validateSuccessfulBuildStep(jobs.jobs);

  const { stdout: dirtyOutput } = await runFile('git', ['status', '--porcelain=v1', '--untracked-files=all'], { cwd: workspaceRoot, env: childEnv });
  if (dirtyOutput.trim() !== '') fail('The qualification checkout must be clean before reusing an installer.');
  const originalSha = String(run.head_sha).toLowerCase();
  try {
    await runFile('git', ['cat-file', '-e', `${originalSha}^{commit}`], { cwd: workspaceRoot, env: childEnv });
  } catch {
    await runFile('git', ['fetch', '--no-tags', '--depth=1', 'origin', originalSha], { cwd: workspaceRoot, env: childEnv });
  }
  const { stdout: diffOutput } = await runFile('git', ['diff', '--name-only', '--diff-filter=ACDMRTUXB', originalSha, currentSha], { cwd: workspaceRoot, env: childEnv });
  validateChangedPaths(diffOutput.split(/\r?\n/).map(value => value.trim()).filter(Boolean));

  const packageJson = await jsonFile(path.join(workspaceRoot, 'apps/desktop/package.json'), 'apps/desktop/package.json');
  const expectedProductVersion = requireString(packageJson.version, 'apps/desktop/package.json version');
  const expectedSourceHashes = {
    cargoLockSha256: await sha256File(path.join(workspaceRoot, 'Cargo.lock')),
    packageLockSha256: await sha256File(path.join(workspaceRoot, 'apps/desktop/package-lock.json')),
    tauriConfigSha256: await sha256File(path.join(workspaceRoot, 'apps/desktop/src-tauri/tauri.conf.json')),
  };

  const retestRoot = await mkdtemp(path.join(runnerTemp, 'webnovel-package-retest-'));
  const installerRoot = path.join(retestRoot, 'installer');
  const evidenceRoot = path.join(retestRoot, 'evidence');
  await runFile('gh', ['run', 'download', runId, '--repo', repository, '--name', 'windows-installer', '--dir', installerRoot], { cwd: workspaceRoot, env: childEnv });
  await runFile('gh', ['run', 'download', runId, '--repo', repository, '--name', 'windows-package-smoke-evidence', '--dir', evidenceRoot], { cwd: workspaceRoot, env: childEnv });
  const [installerFiles, metadataFiles] = await Promise.all([
    walkFiles(installerRoot).then(files => files.filter(filePath => /-setup\.exe$/i.test(path.basename(filePath)))),
    walkFiles(evidenceRoot).then(files => files.filter(filePath => path.basename(filePath).toLowerCase() === 'build-metadata.json')),
  ]);
  if (installerFiles.length !== 1) fail(`Expected exactly one downloaded setup installer, found ${installerFiles.length}.`);
  if (metadataFiles.length !== 1) fail(`Expected exactly one original build-metadata.json artifact, found ${metadataFiles.length}.`);
  const installerPath = installerFiles[0];
  const metadataPath = metadataFiles[0];
  const installerSha256 = await sha256File(installerPath);
  const metadata = await jsonFile(metadataPath, 'original build metadata');
  const validated = validateBuildMetadata(metadata, {
    ...runIdentity,
    installerName: path.basename(installerPath),
    installerSha256,
    expectedProductVersion,
    expectedSourceHashes,
  });
  await mkdir(path.join(workspaceRoot, '.local'), { recursive: true });
  const workspaceRetestRoot = await mkdtemp(path.join(workspaceRoot, '.local', 'webnovel-package-retest-'));
  const workspaceInstallerPath = path.join(workspaceRetestRoot, path.basename(installerPath));
  await copyFile(installerPath, workspaceInstallerPath);
  if (await sha256File(workspaceInstallerPath) !== installerSha256) {
    fail('The workspace-staged installer changed while it was being copied.');
  }
  const outputPath = env.GITHUB_OUTPUT;
  if (!outputPath) fail('GITHUB_OUTPUT is required to pass validated retest paths to the workflow.');
  await appendFile(outputPath, `installer_path=${outputValue(workspaceInstallerPath)}\noriginal_metadata_path=${outputValue(metadataPath)}\n`, 'utf8');
  return { installerPath: workspaceInstallerPath, metadataPath, metadata, validated, originalSha, currentSha, diffPaths: diffOutput.split(/\r?\n/).filter(Boolean) };
}

if (import.meta.url === `file://${process.argv[1]?.replaceAll('\\', '/')}` || process.argv[1]?.endsWith('prepare-package-retest.mjs')) {
  try {
    const result = await preparePackageRetest();
    console.log(JSON.stringify({
      installerPath: result.installerPath,
      originalMetadataPath: result.metadataPath,
      originalSha: result.originalSha,
      currentSha: result.currentSha,
      diffPaths: result.diffPaths,
    }, null, 2));
  } catch (error) {
    console.error(`Package retest preparation failed: ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  }
}
