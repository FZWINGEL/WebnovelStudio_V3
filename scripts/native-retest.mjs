import { createHash } from 'node:crypto';
import { readFile, readdir, stat } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { verifyArtifact } from './native-artifact.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));

export const ALLOWED_HARNESS_PREFIXES = Object.freeze([
  'apps/desktop/scripts/native-',
  'apps/desktop/scripts/owned-process',
  'scripts/native-',
  'scripts/owned-process',
  'tests/native/',
  'docs/',
]);

export const ALLOWED_DOCUMENTS = new Set([
  'AGENTS.md',
  'CLAUDE.md',
  'GEMINI.md',
  'PRODUCT.md',
  'README.md',
]);

export const FORBIDDEN_PREFIXES = Object.freeze([
  'crates/',
  'apps/desktop/src/',
  'apps/desktop/src-tauri/',
  '.github/',
]);

export const FORBIDDEN_FILES = new Set([
  'Cargo.toml',
  'Cargo.lock',
  'package.json',
  'package-lock.json',
  'apps/desktop/package.json',
  'apps/desktop/package-lock.json',
  'apps/desktop/src-tauri/tauri.conf.json',
  'apps/desktop/src-tauri/Cargo.toml',
]);

function fail(message) {
  throw new Error(message);
}

export function parseRunId(value) {
  const text = String(value ?? '').trim();
  if (!/^\d+$/.test(text)) fail(`Run ID must be a positive integer, got '${text || '<missing>'}'.`);
  const number = Number(text);
  if (!Number.isSafeInteger(number) || number <= 0) fail(`Run ID is outside safe positive integer range: ${text}.`);
  return text;
}

export function isAllowedNativeRetestPath(filePath) {
  const normalized = String(filePath ?? '').replaceAll('\\', '/').replace(/^\.\//, '');
  if (FORBIDDEN_FILES.has(normalized)) return false;
  if (FORBIDDEN_PREFIXES.some(prefix => normalized.startsWith(prefix))) return false;
  if (ALLOWED_DOCUMENTS.has(normalized)) return true;
  return ALLOWED_HARNESS_PREFIXES.some(prefix => normalized.startsWith(prefix));
}

export function validateNativeRetestDiff(changedPaths) {
  if (!Array.isArray(changedPaths)) fail('Changed paths must be an array.');
  const normalized = changedPaths.map(p => String(p).replaceAll('\\', '/').replace(/^\.\//, ''));
  const rejected = normalized.filter(p => !isAllowedNativeRetestPath(p));
  if (rejected.length > 0) {
    fail(`Native artifact reuse is refused because the source diff contains disallowed changes: ${rejected.join(', ')}`);
  }
  return normalized;
}

async function collectHarnessFiles(baseDir) {
  const records = [];

  async function walk(dir) {
    const entries = await readdir(dir, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = resolve(dir, entry.name);
      const relPath = relative(baseDir, fullPath).replaceAll('\\', '/');
      const segments = relPath.split('/');

      if (entry.isSymbolicLink()) {
        fail(`Symbolic links are not allowed in native harness: ${relPath}`);
      }

      if (segments.includes('node_modules') || segments.includes('.git')) {
        continue;
      }

      if (entry.isDirectory()) {
        await walk(fullPath);
      } else if (entry.isFile()) {
        const content = await readFile(fullPath);
        const sha = createHash('sha256').update(content).digest('hex');
        records.push({ path: relPath, type: 'file', sha256: sha });
      }
    }
  }

  const testsNativeDir = resolve(baseDir, 'tests/native');
  if (existsSync(testsNativeDir)) {
    await walk(testsNativeDir);
  } else {
    fail(`tests/native directory does not exist: ${testsNativeDir}`);
  }

  const desktopScriptsDir = resolve(baseDir, 'apps/desktop/scripts');
  if (existsSync(desktopScriptsDir)) {
    const entries = await readdir(desktopScriptsDir, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = resolve(desktopScriptsDir, entry.name);
      const relPath = relative(baseDir, fullPath).replaceAll('\\', '/');
      if (entry.isSymbolicLink()) {
        fail(`Symbolic links are not allowed in harness: ${relPath}`);
      }
      if (entry.isFile() && (entry.name.startsWith('native-') || entry.name.startsWith('owned-process') || entry.name.endsWith('.mjs') || entry.name.endsWith('.ps1'))) {
        const content = await readFile(fullPath);
        const sha = createHash('sha256').update(content).digest('hex');
        records.push({ path: relPath, type: 'file', sha256: sha });
      }
    }
  } else {
    fail(`apps/desktop/scripts directory does not exist: ${desktopScriptsDir}`);
  }

  const rootScriptsDir = resolve(baseDir, 'scripts');
  if (existsSync(rootScriptsDir)) {
    const entries = await readdir(rootScriptsDir, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = resolve(rootScriptsDir, entry.name);
      const relPath = relative(baseDir, fullPath).replaceAll('\\', '/');
      if (entry.isSymbolicLink()) {
        fail(`Symbolic links are not allowed in harness: ${relPath}`);
      }
      if (entry.isFile() && (entry.name.startsWith('native-') || entry.name.startsWith('owned-process') || entry.name.startsWith('run-tooling'))) {
        const content = await readFile(fullPath);
        const sha = createHash('sha256').update(content).digest('hex');
        records.push({ path: relPath, type: 'file', sha256: sha });
      }
    }
  } else {
    fail(`scripts directory does not exist: ${rootScriptsDir}`);
  }

  return records;
}

export async function computeHarnessSha256(baseDir = root) {
  if (!baseDir || !existsSync(baseDir)) {
    fail(`Base directory does not exist: ${baseDir}`);
  }

  const records = await collectHarnessFiles(baseDir);
  records.sort((a, b) => a.path.localeCompare(b.path));

  const hash = createHash('sha256');
  for (const r of records) {
    hash.update(`${r.path}\0${r.type}\0${r.sha256}\n`);
  }
  return hash.digest('hex');
}

export async function verifyNativeRetest({
  buildCommit,
  qualificationCommit,
  artifactDirectory,
  baseDir = root,
}) {
  if (!baseDir || !existsSync(baseDir)) {
    fail(`Base directory does not exist: ${baseDir}`);
  }

  if (!buildCommit || typeof buildCommit !== 'string' || !/^[0-9a-f]{40}$/i.test(buildCommit)) {
    fail(`Invalid buildCommit: '${buildCommit}'. Must be a 40-character commit hash.`);
  }

  if (!qualificationCommit || typeof qualificationCommit !== 'string' || !/^[0-9a-f]{40}$/i.test(qualificationCommit)) {
    fail(`Invalid qualificationCommit: '${qualificationCommit}'. Must be a 40-character commit hash.`);
  }

  if (!artifactDirectory || !existsSync(artifactDirectory)) {
    fail('Artifact directory is required and must exist on disk.');
  }

  try {
    execFileSync('git', ['cat-file', '-e', `${buildCommit}^{commit}`], { cwd: baseDir, stdio: 'ignore', windowsHide: true });
  } catch {
    fail(`buildCommit does not exist in repository: ${buildCommit}`);
  }
  try {
    execFileSync('git', ['cat-file', '-e', `${qualificationCommit}^{commit}`], { cwd: baseDir, stdio: 'ignore', windowsHide: true });
  } catch {
    fail(`qualificationCommit does not exist in repository: ${qualificationCommit}`);
  }

  const headCommit = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: baseDir, encoding: 'utf8', windowsHide: true }).trim();
  if (headCommit !== qualificationCommit) {
    fail(`Working tree HEAD (${headCommit}) does not match qualificationCommit (${qualificationCommit}).`);
  }
  const status = execFileSync('git', ['status', '--porcelain', '-uall'], { cwd: baseDir, encoding: 'utf8', windowsHide: true }).trim();
  if (status !== '') {
    fail('Working tree must be clean to qualify native retest.');
  }

  let diffPaths;
  try {
    const stdout = execFileSync('git', ['diff', '--no-renames', '--name-only', '-z', buildCommit, qualificationCommit], { cwd: baseDir, encoding: 'utf8', windowsHide: true });
    diffPaths = stdout.split('\0').filter(Boolean);
  } catch (err) {
    fail(`Failed to compute git diff between ${buildCommit} and ${qualificationCommit}: ${err.message}`);
  }

  validateNativeRetestDiff(diffPaths);

  const verifiedManifest = await verifyArtifact(artifactDirectory, buildCommit);

  const executableFile = verifiedManifest.files?.find(f => f.path === verifiedManifest.executable);
  if (!executableFile?.sha256) {
    fail('Missing executable entry in artifact manifest.');
  }

  const harnessSha256 = await computeHarnessSha256(baseDir);

  return {
    buildCommit,
    qualificationCommit,
    executableSha256: executableFile.sha256,
    harnessSha256,
  };
}

export const validateNativeRetestContract = verifyNativeRetest;

