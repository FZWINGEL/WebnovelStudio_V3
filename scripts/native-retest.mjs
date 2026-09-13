import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

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
  '.github/workflows/ci.yml',
]);

export const FORBIDDEN_PREFIXES = Object.freeze([
  'crates/',
  'apps/desktop/src/',
  'apps/desktop/src-tauri/',
]);

export const FORBIDDEN_FILES = new Set([
  'Cargo.toml',
  'Cargo.lock',
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
  const scriptsDir = resolve(baseDir, 'apps/desktop/scripts');
  const scriptFiles = (await readdir(scriptsDir)).filter(f => f.startsWith('native-') || f.startsWith('owned-process')).sort();
  for (const f of scriptFiles) {
    const content = await readFile(resolve(scriptsDir, f));
    hash.update(f).update(content);
  }
  const rootScriptsDir = resolve(baseDir, 'scripts');
  const rootScriptFiles = (await readdir(rootScriptsDir)).filter(f => f.startsWith('native-')).sort();
  for (const f of rootScriptFiles) {
    const content = await readFile(resolve(rootScriptsDir, f));
    hash.update(f).update(content);
  }
  return hash.digest('hex');
}

export async function validateNativeRetestContract({
  manifest,
  qualificationCommit,
  changedPaths,
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
  validateNativeRetestDiff(changedPaths);
  const executableFile = manifest.files?.find(f => f.path === manifest.executable);
  if (!executableFile?.sha256) fail('Missing executable entry in artifact manifest.');

  const harnessSha256 = await computeHarnessSha256(baseDir);

  return {
    buildCommit,
    qualificationCommit,
    executableSha256: executableFile.sha256,
    harnessSha256,
  };
}
