import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));

test('testing handbook documents exist and resolve internal markdown links', () => {
  const docs = [
    'docs/TESTING.md',
    'docs/testing/QUALIFICATION.md',
    'docs/testing/WRITING_TESTS.md',
    'docs/testing/PERFORMANCE.md',
    'tests/native/README.md',
  ];

  for (const docRel of docs) {
    const docPath = resolve(root, docRel);
    assert(existsSync(docPath), `Document must exist: ${docRel}`);
    const content = readFileSync(docPath, 'utf8');
    const docDir = dirname(docPath);

    // Extract markdown links: [text](target)
    const linkRegex = /\[([^\]]+)\]\(([^)]+)\)/g;
    let match;
    while ((match = linkRegex.exec(content)) !== null) {
      const target = match[2].trim();
      // Skip external URLs, mailto, and fragment-only links
      if (/^(https?:|mailto:|#)/.test(target)) continue;

      // Strip fragment (#...) if present
      const targetWithoutFragment = target.split('#')[0];
      if (!targetWithoutFragment) continue;

      const resolved = resolve(docDir, targetWithoutFragment);
      assert(
        existsSync(resolved),
        `Broken link in ${docRel}: "${target}" -> resolved to non-existent path: "${resolved}"`
      );
    }
  }
});

test('documented launcher commands in TESTING.md match scripts/run-desktop.mjs', () => {
  const runDesktopContent = readFileSync(resolve(root, 'scripts/run-desktop.mjs'), 'utf8');
  const desktopPs1Content = readFileSync(resolve(root, 'scripts/desktop.ps1'), 'utf8');

  // Documented core commands from TESTING.md table
  const documentedCommands = [
    'ensure-deps',
    'plan',
    'quick',
    'test',
    'test:watch',
    'check',
    'spike',
    'native',
    'prune',
  ];

  for (const cmd of documentedCommands) {
    const isSupported =
      runDesktopContent.includes(`case '${cmd}':`) ||
      desktopPs1Content.includes(`'${cmd}'`);
    assert(isSupported, `Documented command '${cmd}' must be supported in scripts/run-desktop.mjs or desktop.ps1`);
  }
});

test('documented tooling profiles match scripts/run-tooling-tests.mjs', async () => {
  const { PROFILES } = await import('./run-tooling-tests.mjs');
  const documentedProfiles = ['core', 'native-preflight', 'all'];

  for (const profile of documentedProfiles) {
    assert(
      profile in PROFILES,
      `Documented profile '${profile}' must exist in PROFILES of scripts/run-tooling-tests.mjs`
    );
  }
});

test('documented native suites in runbook match scripts/native-suites.json', () => {
  const suitesManifest = JSON.parse(readFileSync(resolve(root, 'scripts/native-suites.json'), 'utf8'));
  const registeredSuites = Object.keys(suitesManifest.suites);

  const documentedSuites = [
    'main',
    'chat',
    'workshop',
    'close',
    'interruption',
    'recovery',
    'memory',
  ];

  for (const suite of documentedSuites) {
    assert(
      registeredSuites.includes(suite),
      `Suite '${suite}' documented in tests/native/README.md must be declared in scripts/native-suites.json`
    );
  }
});
