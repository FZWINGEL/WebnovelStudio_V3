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

async function collectFilesRecursively(dir, filterFn, ignoreFn) {
  const results = [];
  async function walk(currentDir) {
    const entries = await readdir(currentDir, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = resolve(currentDir, entry.name);
      if (ignoreFn && ignoreFn(fullPath, entry)) continue;
      if (entry.isDirectory()) {
        await walk(fullPath);
      } else if (entry.isFile()) {
        if (!filterFn || filterFn(fullPath, entry)) {
          results.push(fullPath);
        }
      }
    }
  }
  await walk(dir);
  return results;
}

export async function computeHarnessSha256(baseDir = root) {
  if (!existsSync(baseDir)) {
    fail(`Base directory does not exist: ${baseDir}`);
  }

  const testsNativeDir = resolve(baseDir, 'tests/native');
  const desktopScriptsDir = resolve(baseDir, 'apps/desktop/scripts');
  const rootScriptsDir = resolve(baseDir, 'scripts');

  if (!existsSync(testsNativeDir) || !existsSync(desktopScriptsDir) || !existsSync(rootScriptsDir)) {
    fail('One or more required harness directories do not exist for computing harness SHA-256.');
  }

  const filePaths = [];

  // Recurse tests/native (ignore node_modules, .local)
  const nativeFiles = await collectFilesRecursively(
    testsNativeDir,
    p => /\.(mjs|js|json|ps1|ts|md)$/.test(p),
    p => p.includes('node_modules') || p.includes('.local')
  );
  filePaths.push(...nativeFiles);

  // apps/desktop/scripts
  const desktopFiles = await collectFilesRecursively(
    desktopScriptsDir,
    p => /\.(mjs|ps1)$/.test(p),
    p => p.includes('.local')
  );
  filePaths.push(...desktopFiles);

  // scripts
  const rootScriptEntries = await readdir(rootScriptsDir, { withFileTypes: true });
  for (const entry of rootScriptEntries) {
    if (entry.isFile() && (entry.name.startsWith('native-') || entry.name.startsWith('owned-process') || entry.name.startsWith('run-tooling'))) {
      filePaths.push(resolve(rootScriptsDir, entry.name));
    }
  }

  const canonical = filePaths.map(p => relative(baseDir, p).replaceAll('\\', '/')).sort();
  const hash = createHash('sha256');
  for (const relPath of canonical) {
    const content = await readFile(resolve(baseDir, relPath));
    hash.update(relPath).update(content);
  }

  return hash.digest('hex');
}

export async function validateNativeRetestContract({
  manifest,
  buildCommit,
  qualificationCommit,
  artifactDirectory,
  changedPaths,
  baseDir = root,
}) {
  if (!baseDir || !existsSync(baseDir)) {
    fail(`Base directory does not exist: ${baseDir}`);
  }

  const bCommit = buildCommit || manifest?.commit;
  if (!bCommit || typeof bCommit !== 'string' || !/^[0-9a-f]{40}$/i.test(bCommit)) {
    fail(`Invalid buildCommit: '${bCommit}'. Must be a 40-character commit hash.`);
  }

  if (!qualificationCommit || typeof qualificationCommit !== 'string' || !/^[0-9a-f]{40}$/i.test(qualificationCommit)) {
    fail(`Invalid qualificationCommit: '${qualificationCommit}'. Must be a 40-character commit hash.`);
  }

  if (!artifactDirectory || !existsSync(artifactDirectory)) {
    fail('Artifact directory is required and must exist on disk.');
  }

  let diffPaths = changedPaths;
  if (!diffPaths) {
    try {
      const stdout = execFileSync('git', ['diff', '--no-renames', '--name-only', '-z', bCommit, qualificationCommit], { cwd: baseDir, encoding: 'utf8' });
      diffPaths = stdout.split('\0').filter(Boolean);
    } catch (err) {
      fail(`Failed to compute git diff between ${bCommit} and ${qualificationCommit}: ${err.message}`);
    }
  }

  validateNativeRetestDiff(diffPaths);

  const verifiedManifest = manifest && manifest.files && manifest.executable
    ? manifest
    : await verifyArtifact(artifactDirectory, bCommit);

  const executableFile = verifiedManifest.files?.find(f => f.path === verifiedManifest.executable);
  if (!executableFile?.sha256) {
    fail('Missing executable entry in artifact manifest.');
  }

  const exePath = resolve(artifactDirectory, verifiedManifest.executable);
  const bytes = await readFile(exePath);
  const actualSha = createHash('sha256').update(bytes).digest('hex');
  if (actualSha !== executableFile.sha256) {
    fail(`Executable bytes on disk do not match artifact manifest sha256 (expected: ${executableFile.sha256}, actual: ${actualSha})`);
  }

  const harnessSha256 = await computeHarnessSha256(baseDir);

  return {
    buildCommit: bCommit,
    qualificationCommit,
    executableSha256: executableFile.sha256,
    harnessSha256,
  };
}
