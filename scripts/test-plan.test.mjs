import test from 'node:test';
import assert from 'node:assert/strict';
import {
  classifyChanges,
  planFromClassification,
  deduplicateFilters,
  parseGitStatusPorcelainZ,
  validateCommand,
} from './test-plan.mjs';

test('classifies clean working tree', () => {
  const c = classifyChanges([]);
  assert.equal(c.category, 'none');
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'clean');
  assert.equal(plan.outstandingNative, false);
});

test('classifies documentation changes', () => {
  const c = classifyChanges(['docs/TESTING.md', 'README.md']);
  assert.equal(c.category, 'docs');
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'docs');
  assert.equal(plan.commands[0].executable, 'node');
  assert(plan.commands[0].args.includes('scripts/run-tooling-tests.mjs'));
  assert.equal(plan.outstandingNative, false);
});

test('classifies isolated Rust crate changes and batches integration suites', () => {
  const c = classifyChanges(['crates/documents/src/records.rs']);
  assert.equal(c.category, 'isolated-rust');
  assert.equal(c.crates.length, 1);
  assert.equal(c.crates[0], 'wns-documents');

  const plan = planFromClassification(c);
  assert.equal(plan.category, 'isolated-rust');
  const commandStrings = plan.commands.map(cmd => `${cmd.executable} ${cmd.args.join(' ')}`);
  assert(commandStrings.some(s => s.includes('clippy -p wns-documents')));
  assert(commandStrings.some(s => s.includes('test -p wns-documents')));

  // Batched integration test: exactly 1 integration test command
  const integrationCmds = plan.commands.filter(cmd => cmd.args.includes('--test') && cmd.args.includes('integration'));
  assert.equal(integrationCmds.length, 1, 'Expected exactly 1 batched cargo integration command');
  const integrationArgs = integrationCmds[0].args;
  assert(integrationArgs.includes('document_roles'));
  assert(integrationArgs.includes('scope'));
  // scope subsumes append_scope
  assert(!integrationArgs.includes('append_scope'), 'append_scope should be subsumed by scope');
  assert(integrationArgs.includes('text_replacement'));
  assert.equal(plan.outstandingNative, false);
});

test('classifies frontend component changes with valid scripts', () => {
  const c = classifyChanges(['apps/desktop/src/kernel/document.ts']);
  assert.equal(c.category, 'frontend');
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'frontend');
  assert(plan.commands.some(cmd => cmd.args.includes('typecheck')), 'Must emit typecheck, not test:types');
  assert(!plan.commands.some(cmd => cmd.args.includes('test:types')), 'Must never emit non-existent test:types');
  assert(plan.commands.some(cmd => cmd.args.includes('src/kernel/')));
  assert(plan.commands.some(cmd => cmd.args.includes('src/featureBoundary.test.ts')));
  assert.equal(plan.outstandingNative, false);
});

test('public asset changes in frontend trigger full frontend test', () => {
  const c = classifyChanges(['apps/desktop/public/vite.svg']);
  assert.equal(c.category, 'frontend');
  assert.equal(c.fullFrontend, true);
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'frontend');
  const testCmd = plan.commands.find(cmd => cmd.executable === 'npm' && cmd.args[0] === 'test');
  assert.deepEqual(testCmd.args, ['test'], 'Public assets should run full npm test');
});

test('classifies shared contract and bindings changes', () => {
  const c = classifyChanges(['contracts/src/lib.rs', 'apps/desktop/src/kernel/contracts.ts']);
  assert.equal(c.category, 'contracts');
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'contracts');
  assert(plan.commands.some(cmd => cmd.args.includes('structured_contract_golden')));
  assert(plan.commands.some(cmd => cmd.args.includes('typecheck')));
  assert.equal(plan.outstandingNative, false);
});

test('fails closed to broad check for root configs, manifests, and core changes', () => {
  for (const f of [
    'Cargo.toml',
    'Cargo.lock',
    'package.json',
    'package-lock.json',
    '.node-version',
    'apps/desktop/package.json',
    'apps/desktop/package-lock.json',
    'crates/core/src/lib.rs',
    'crates/architecture/src/lib.rs',
    '.github/workflows/ci.yml',
    'apps/desktop/src-tauri/tauri.conf.json',
    'apps/desktop/src-tauri/Cargo.toml',
    'crates/documents/Cargo.toml',
  ]) {
    const c = classifyChanges([f]);
    assert.equal(c.category, 'broad', `File ${f} should fail closed to broad check`);
    const plan = planFromClassification(c);
    assert.equal(plan.category, 'broad');
    assert.deepEqual(plan.commands[0], { executable: 'desktop', args: ['check'] });
    assert.equal(plan.outstandingNative, true);
  }
});

test('fails closed to broad on unknown files', () => {
  const c = classifyChanges(['unknown_script.py']);
  assert.equal(c.category, 'broad');
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'broad');
  assert.equal(plan.outstandingNative, true);
});

test('fails closed to broad on git failure object', () => {
  const c = classifyChanges({ error: 'git not found' });
  assert.equal(c.category, 'broad');
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'broad');
  assert.equal(plan.outstandingNative, true);
});

test('union model never drops checks when adding files', () => {
  // Adding an unknown file to contracts forces broad instead of staying contracts
  const c1 = classifyChanges(['contracts/src/lib.rs', 'something_unrecognized.txt']);
  assert.equal(c1.category, 'broad');

  // Combining isolated rust and frontend results in cross-cutting
  const c2 = classifyChanges(['crates/documents/src/records.rs', 'apps/desktop/src/kernel/document.ts']);
  assert.equal(c2.category, 'cross-cutting');
  const plan = planFromClassification(c2);
  assert(plan.commands.some(cmd => cmd.args.includes('wns-documents')));
  assert(plan.commands.some(cmd => cmd.args.includes('typecheck')));
});

test('deduplicateFilters eliminates redundant substring filters', () => {
  const input = ['scope', 'append_scope', 'document_roles', 'text_replacement', 'scope'];
  const deduped = deduplicateFilters(input);
  assert.deepEqual(deduped, ['scope', 'document_roles', 'text_replacement']);
});

test('parseGitStatusPorcelainZ parses NUL-delimited records including renames', () => {
  // Normal entry: " M path/file.txt\0"
  // Rename entry: "R  new/path.rs\0old/path.rs\0"
  const raw = ' M crates/documents/src/records.rs\0R  crates/story/src/new.rs\0crates/story/src/old.rs\0?? untracked.md\0';
  const parsed = parseGitStatusPorcelainZ(raw);
  assert.deepEqual(parsed, [
    'crates/documents/src/records.rs',
    'crates/story/src/new.rs',
    'crates/story/src/old.rs',
    'untracked.md',
  ]);
});

test('validateCommand throws on invalid npm scripts or unknown cargo packages', () => {
  assert.throws(
    () => validateCommand({ executable: 'npm', args: ['run', 'nonexistent-script'], cwd: 'apps/desktop' }),
    /Script does not exist/
  );

  assert.throws(
    () => validateCommand({ executable: 'cargo', args: ['test', '-p', 'nonexistent-crate'] }),
    /Package not in workspace Cargo.toml/
  );

  assert.doesNotThrow(
    () => validateCommand({ executable: 'npm', args: ['run', 'typecheck'], cwd: 'apps/desktop' })
  );

  assert.doesNotThrow(
    () => validateCommand({ executable: 'cargo', args: ['test', '-p', 'wns-documents'] })
  );
});
