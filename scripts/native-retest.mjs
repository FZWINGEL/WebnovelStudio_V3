import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

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

export async function computeHarnessSha256(baseDir = root) {
  const hash = createHash('sha256');
  for (const dirName of ['apps/desktop/scripts', 'tests/native', 'scripts']) {
    const dir = resolve(baseDir, dirName);
    try {
      const files = (await readdir(dir)).filter(f => f.startsWith('native-') || f.startsWith('owned-process') || f.endsWith('.json') || f.endsWith('.test.mjs')).sort();
      for (const f of files) {
        const content = await readFile(resolve(dir, f));
        hash.update(f).update(content);
      }
    } catch {
      // directory might not exist in some environments
    }
  }
  return hash.digest('hex');
}

export async function validateNativeRetestContract({
  manifest,
  qualificationCommit,
  changedPaths,
  artifactDirectory,
  baseDir = root,
}) {
  if (!manifest || typeof manifest !== 'object') fail('Missing or malformed artifact manifest.');
  const buildCommit = manifest.commit;
  if (!buildCommit || typeof buildCommit !== 'string' || buildCommit.length !== 40) {
    fail('Artifact manifest has invalid buildCommit.');
  }
  if (!qualificationCommit || typeof qualificationCommit !== 'string' || qualificationCommit.length !== 40) {
    fail('Invalid qualificationCommit.');
  }

  let diffPaths = changedPaths;
  if (!diffPaths) {
    try {
      const stdout = execFileSync('git', ['diff', '--name-only', `${buildCommit}..${qualificationCommit}`], { cwd: baseDir, encoding: 'utf8' });
      diffPaths = stdout.split('\n').map(s => s.trim()).filter(Boolean);
    } catch (err) {
      fail(`Failed to compute git diff between ${buildCommit} and ${qualificationCommit}: ${err.message}`);
    }
  }

  validateNativeRetestDiff(diffPaths);
  const executableFile = manifest.files?.find(f => f.path === manifest.executable);
  if (!executableFile?.sha256) fail('Missing executable entry in artifact manifest.');

  if (artifactDirectory) {
    const exePath = resolve(artifactDirectory, manifest.executable);
    const bytes = await readFile(exePath);
    const actualSha = createHash('sha256').update(bytes).digest('hex');
    if (actualSha !== executableFile.sha256) {
      fail(`Executable bytes on disk do not match artifact manifest sha256 (expected: ${executableFile.sha256}, actual: ${actualSha})`);
    }
  }

  const harnessSha256 = await computeHarnessSha256(baseDir);

  return {
    buildCommit,
    qualificationCommit,
    executableSha256: executableFile.sha256,
    harnessSha256,
  };
}
