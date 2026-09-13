import test from 'node:test';
import assert from 'node:assert/strict';
import {
  classifyChanges,
  planFromClassification,
  deduplicateFilters,
  parseGitStatusPorcelainZ,
  validateCommand,
  PlanRequirements,
  KNOWN_WORKSPACE_PACKAGES,
  setDefaultWorkspacePackages,
  getChangedFiles,
} from './test-plan.mjs';

// Ensure unit tests run instantaneously without spawning real Cargo subprocesses
setDefaultWorkspacePackages(KNOWN_WORKSPACE_PACKAGES);

test('classifies clean working tree', () => {
  const c = classifyChanges([]);
  assert.equal(c.isClean, true);
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'clean');
  assert.equal(plan.outstandingNative, false);
  assert.equal(plan.nativeObligation.required, false);
});

test('classifies documentation changes', () => {
  const c = classifyChanges(['docs/TESTING.md', 'README.md']);
  assert.equal(c.toolingProfiles.has('core'), true);
  assert.equal(c.rustPackages.size, 0);
  assert.equal(c.frontendTypecheck, false);

  const plan = planFromClassification(c);
  assert.equal(plan.category, 'docs');
  assert.equal(plan.commands.length, 1);
  assert.equal(plan.commands[0].executable, 'node');
  assert(plan.commands[0].args.includes('--profile=core'));
  assert.equal(plan.outstandingNative, false);
  assert.equal(plan.nativeObligation.required, false);
});

test('tooling-only change emits only tooling without frontend or rust work', () => {
  const c = classifyChanges(['scripts/check-versions.mjs']);
  assert.equal(c.toolingProfiles.has('core'), true);
  assert.equal(c.rustPackages.size, 0);
  assert.equal(c.frontendTypecheck, false);
  assert.equal(c.frontendFullVitest, false);

  const plan = planFromClassification(c);
  assert.equal(plan.category, 'tooling');
  assert.equal(plan.commands.length, 1);
  assert.equal(plan.commands[0].executable, 'node');
  assert(plan.commands[0].args.includes('--profile=core'));
  assert(!plan.commands.some(cmd => cmd.executable === 'npm'));
  assert(!plan.commands.some(cmd => cmd.executable === 'cargo'));
  assert.equal(plan.outstandingNative, false);
  assert.equal(plan.nativeObligation.required, false);
});

test('contracts + tooling preserves both contract and tooling checks', () => {
  const c = classifyChanges(['contracts/src/lib.rs', 'scripts/check-versions.mjs']);
  assert.equal(c.toolingProfiles.has('core'), true);
  assert.equal(c.rustPackages.has('wns-bindings'), true);
  assert.equal(c.rustPackages.has('contracts'), true);
  assert.equal(c.frontendTypecheck, true);

  const plan = planFromClassification(c);
  // Tooling command MUST be retained!
  assert(plan.commands.some(cmd => cmd.executable === 'node' && cmd.args.includes('--profile=core')), 'Tooling runner must be retained when contracts are added');
  assert(plan.commands.some(cmd => cmd.args.includes('typecheck')));
  assert(plan.commands.some(cmd => cmd.args.includes('structured_contract_golden')));
  assert.equal(plan.outstandingNative, true);
  assert(plan.requiredNativeSuites.includes('main'));
});


test('native harness edits require native qualification and affected suites', () => {
  const c = classifyChanges(['tests/native/native-smoke.mjs']);
  assert.equal(c.nativeOutstanding, true);
  assert.equal(c.nativeSuites.has('main'), true);
  assert.equal(c.toolingProfiles.has('native-preflight'), true);

  const plan = planFromClassification(c);
  assert.equal(plan.outstandingNative, true);
  assert(plan.requiredNativeSuites.includes('main'));
  assert(plan.commands.some(cmd => cmd.args.includes('--profile=all')));
});

test('database migration SQL files fail closed to broad check', () => {
  const c = classifyChanges(['crates/storage/src/016_memory.sql']);
  assert(c.broadFallback !== null, 'SQL migration must trigger broad fallback');
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'broad');
  assert.equal(plan.outstandingNative, true);
  assert.deepEqual(plan.commands[0], { executable: 'desktop', args: ['check'] });
});

test('batches multi-crate Rust changes into single clippy and test commands', () => {
  const c = classifyChanges([
    'crates/documents/src/records.rs',
    'crates/story/src/lib.rs',
  ]);
  assert.equal(c.rustPackages.size, 2);
  assert.equal(c.rustPackages.has('wns-documents'), true);
  assert.equal(c.rustPackages.has('wns-story'), true);

  const plan = planFromClassification(c);
  assert.equal(plan.category, 'isolated-rust');

  // Single batched clippy command with both packages!
  const clippyCmds = plan.commands.filter(cmd => cmd.args.includes('clippy'));
  assert.equal(clippyCmds.length, 1, 'Expected exactly 1 batched clippy command');
  assert(clippyCmds[0].args.includes('wns-documents'));
  assert(clippyCmds[0].args.includes('wns-story'));

  // Single batched unit test command with both packages!
  const testCmds = plan.commands.filter(cmd => cmd.executable === 'cargo' && cmd.args[0] === 'test' && cmd.args.includes('--lib'));
  assert.equal(testCmds.length, 1, 'Expected exactly 1 batched cargo test command');
  assert(testCmds[0].args.includes('wns-documents'));
  assert(testCmds[0].args.includes('wns-story'));

  // Single batched integration command!
  const integrationCmds = plan.commands.filter(cmd => cmd.args.includes('--test') && cmd.args.includes('integration'));
  assert.equal(integrationCmds.length, 1, 'Expected exactly 1 batched integration command');
});

test('classifies frontend component changes with valid scripts', () => {
  const c = classifyChanges(['apps/desktop/src/kernel/document.ts']);
  assert.equal(c.frontendTypecheck, true);
  assert.equal(c.frontendDirs.has('kernel'), true);

  const plan = planFromClassification(c);
  assert.equal(plan.category, 'frontend');
  assert(plan.commands.some(cmd => cmd.args.includes('typecheck')));
  assert(plan.commands.some(cmd => cmd.args.includes('src/kernel/')));
  assert(plan.commands.some(cmd => cmd.args.includes('src/featureBoundary.test.ts')));
  assert.equal(plan.outstandingNative, true);
  assert.equal(plan.nativeObligation.required, true);
  assert(plan.nativeObligation.suites.includes('main'));
  assert(plan.nativeObligation.suites.includes('recovery'));
  assert(plan.nativeObligation.suites.includes('close'));
});

test('local scope and final native obligations remain distinguishable', () => {
  // 1. Application source edit: local iteration is fast, native qualification required before merge
  const appChange = classifyChanges(['apps/desktop/src/shell/documentWorkspace.ts']);
  const appPlan = planFromClassification(appChange);
  assert(appPlan.commands.some(cmd => cmd.args.includes('typecheck')));
  assert(appPlan.commands.some(cmd => cmd.args.includes('src/shell/')));
  // Fast local commands do NOT schedule the heavy native suite
  assert(!appPlan.commands.some(cmd => cmd.executable === 'desktop' && cmd.args.includes('check')));
  assert(appPlan.exclusions.some(e => e.includes('omitted from fast local iteration')));
  // But native qualification is strictly marked as required for final acceptance!
  assert.equal(appPlan.nativeObligation.required, true);
  assert.equal(appPlan.nativeObligation.status, 'scoped');
  assert.deepEqual(appPlan.nativeObligation.suites, ['chat', 'main', 'workshop']);

  // 2. Native consumer orchestration edit: requires full native qualification
  const runnerChange = classifyChanges(['scripts/native-consumer.mjs']);
  const runnerPlan = planFromClassification(runnerChange);
  assert(runnerPlan.commands.some(cmd => cmd.args.includes('--profile=all')));
  assert.equal(runnerPlan.nativeObligation.required, true);
  assert.deepEqual(runnerPlan.nativeObligation.suites, ['all']);

  // 3. Documentation edit: zero native qualification needed
  const docChange = classifyChanges(['docs/TESTING.md']);
  const docPlan = planFromClassification(docChange);
  assert.equal(docPlan.nativeObligation.required, false);
  assert.equal(docPlan.nativeObligation.status, 'none');
});

test('missing or invalid revision arguments cannot produce an unintended clean-worktree plan', () => {
  // Specifying --head without --base throws explicitly
  assert.throws(
    () => getChangedFiles({ head: 'HEAD' }),
    /Cannot specify head commit without base commit/
  );

  // Invalid base commit returns error object that fails closed to broad plan
  const badBase = getChangedFiles({ base: 'invalid-commit-hash-0123456789' });
  assert(badBase.error);
  const badBasePlan = planFromClassification(classifyChanges(badBase));
  assert.equal(badBasePlan.category, 'broad');
  assert.equal(badBasePlan.outstandingNative, true);
});

test('planner unit tests complete without invoking real Cargo', () => {
  // Passing custom fixture packages validates packages purely in memory
  const customPackages = new Set(['my-custom-pkg']);
  assert.doesNotThrow(() => {
    validateCommand(
      { executable: 'cargo', args: ['clippy', '-p', 'my-custom-pkg'] },
      undefined,
      { workspacePackages: customPackages }
    );
  });

  assert.throws(
    () => validateCommand(
      { executable: 'cargo', args: ['clippy', '-p', 'other-pkg'] },
      undefined,
      { workspacePackages: customPackages }
    ),
    /Package not in workspace Cargo.toml/
  );
});

test('public asset changes in frontend trigger full frontend test', () => {
  const c = classifyChanges(['apps/desktop/public/vite.svg']);
  assert.equal(c.frontendFullVitest, true);
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'frontend');
  const testCmd = plan.commands.find(cmd => cmd.executable === 'npm' && cmd.args[0] === 'test');
  assert.deepEqual(testCmd.args, ['test'], 'Public assets should run full npm test');
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
    assert(c.broadFallback !== null, `File ${f} should fail closed to broad check`);
    const plan = planFromClassification(c);
    assert.equal(plan.category, 'broad');
    assert.deepEqual(plan.commands[0], { executable: 'desktop', args: ['check'] });
    assert.equal(plan.outstandingNative, true);
  }
});

test('fails closed to broad on unknown files', () => {
  const c = classifyChanges(['unknown_script.py']);
  assert(c.broadFallback !== null);
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'broad');
});

test('fails closed to broad on git failure object', () => {
  const c = classifyChanges({ error: 'git not found' });
  assert(c.broadFallback !== null);
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'broad');
});

test('union model invariant holds for pairwise combinations', () => {
  const pairs = [
    ['scripts/native-retest.mjs', 'contracts/src/lib.rs'],
    ['crates/documents/src/records.rs', 'apps/desktop/src/kernel/document.ts'],
    ['docs/TESTING.md', 'crates/story/src/lib.rs'],
  ];

  for (const [f1, f2] of pairs) {
    const r1 = classifyChanges([f1]);
    const r2 = classifyChanges([f2]);
    const rBoth = classifyChanges([f1, f2]);

    // Requirements(A U B) >= Requirements(A) U Requirements(B)
    for (const p of r1.rustPackages) assert(rBoth.rustPackages.has(p));
    for (const p of r2.rustPackages) assert(rBoth.rustPackages.has(p));
    for (const p of r1.toolingProfiles) assert(rBoth.toolingProfiles.has(p));
    for (const p of r2.toolingProfiles) assert(rBoth.toolingProfiles.has(p));
    if (r1.frontendTypecheck || r2.frontendTypecheck) assert(rBoth.frontendTypecheck);
  }
});

test('deduplicateFilters eliminates redundant substring filters', () => {
  const input = ['scope', 'append_scope', 'document_roles', 'text_replacement', 'scope'];
  const deduped = deduplicateFilters(input);
  assert.deepEqual(deduped, ['scope', 'document_roles', 'text_replacement']);
});

test('parseGitStatusPorcelainZ parses NUL-delimited records including renames', () => {
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

test('frontend test-only change skips native qualification and runs targeted tests', () => {
  const c = classifyChanges(['apps/desktop/src/shell/AppCloseDialog.test.tsx']);
  assert.equal(c.frontendTypecheck, true);
  assert.equal(c.frontendFiles.has('src/shell/AppCloseDialog.test.tsx'), true);
  assert.equal(c.frontendFiles.has('src/featureBoundary.test.ts'), true);
  assert.equal(c.frontendDirs.size, 0);
  assert.equal(c.nativeOutstanding, false);
  assert.equal(c.nativeSuites.size, 0);

  const plan = planFromClassification(c);
  assert.equal(plan.category, 'frontend');
  assert.equal(plan.outstandingNative, false);
  assert.equal(plan.nativeObligation.required, false);
  assert.equal(plan.nativeObligation.status, 'none');

  const vitestCmd = plan.commands.find(cmd => cmd.executable === 'npm' && cmd.args[0] === 'test');
  assert(vitestCmd, 'Should emit targeted npm test command');
  assert(vitestCmd.args.includes('src/shell/AppCloseDialog.test.tsx'));
  assert(vitestCmd.args.includes('src/featureBoundary.test.ts'));
});

test('tooling unit test in tests/native skips native qualification', () => {
  const c = classifyChanges(['tests/native/dependency-resolution.test.mjs']);
  assert.equal(c.toolingProfiles.has('native-preflight'), true);
  assert.equal(c.nativeOutstanding, false);
  assert.equal(c.nativeSuites.size, 0);

  const plan = planFromClassification(c);
  assert.equal(plan.category, 'tooling');
  assert.equal(plan.outstandingNative, false);
  assert.equal(plan.nativeObligation.required, false);
  assert.equal(plan.nativeObligation.status, 'none');
  assert.deepEqual(plan.commands, [
    { executable: 'node', args: ['scripts/run-tooling-tests.mjs', '--profile=native-preflight'] },
  ]);
});

test('tooling unit test in scripts skips native qualification', () => {
  const c = classifyChanges(['scripts/native-artifact.test.mjs']);
  assert.equal(c.toolingProfiles.has('core'), true);
  assert.equal(c.nativeOutstanding, false);
  assert.equal(c.nativeSuites.size, 0);

  const plan = planFromClassification(c);
  assert.equal(plan.category, 'tooling');
  assert.equal(plan.outstandingNative, false);
  assert.equal(plan.nativeObligation.required, false);
  assert.equal(plan.nativeObligation.status, 'none');
  assert.deepEqual(plan.commands, [
    { executable: 'node', args: ['scripts/run-tooling-tests.mjs', '--profile=core'] },
  ]);
});

test('combined frontend production and test file change retains native obligation', () => {
  const c = classifyChanges([
    'apps/desktop/src/shell/AppCloseDialog.tsx',
    'apps/desktop/src/shell/AppCloseDialog.test.tsx',
  ]);
  assert.equal(c.frontendTypecheck, true);
  assert.equal(c.frontendDirs.has('shell'), true);
  assert.equal(c.nativeOutstanding, true);
  assert.equal(c.nativeSuites.has('main'), true);

  const plan = planFromClassification(c);
  assert.equal(plan.category, 'frontend');
  assert.equal(plan.outstandingNative, true);
  assert.equal(plan.nativeObligation.required, true);
  assert.equal(plan.nativeObligation.status, 'scoped');
  assert(plan.nativeObligation.suites.includes('main'));
});


