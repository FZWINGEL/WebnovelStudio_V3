import test from 'node:test';
import assert from 'node:assert/strict';
import { classifyChanges, planFromClassification } from './test-plan.mjs';

test('classifies clean working tree', () => {
  const c = classifyChanges([]);
  assert.equal(c.category, 'none');
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'clean');
});

test('classifies documentation changes', () => {
  const c = classifyChanges(['docs/TESTING.md', 'README.md']);
  assert.equal(c.category, 'docs');
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'docs');
  assert.equal(plan.commands[0].executable, 'node');
});

test('classifies isolated Rust crate changes and maps integration suites', () => {
  const c = classifyChanges(['crates/documents/src/records.rs']);
  assert.equal(c.category, 'isolated-rust');
  assert.equal(c.crates.length, 1);
  assert.equal(c.crates[0].crate, 'wns-documents');

  const plan = planFromClassification(c);
  assert.equal(plan.category, 'isolated-rust');
  const commandStrings = plan.commands.map(cmd => `${cmd.executable} ${cmd.args.join(' ')}`);
  assert(commandStrings.some(s => s.includes('clippy -p wns-documents')));
  assert(commandStrings.some(s => s.includes('test -p wns-documents')));
  assert(commandStrings.some(s => s.includes('test -p webnovel-core --test integration -- document_roles')));
});

test('classifies frontend component changes', () => {
  const c = classifyChanges(['apps/desktop/src/kernel/document.ts']);
  assert.equal(c.category, 'frontend');
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'frontend');
  assert(plan.commands.some(cmd => cmd.args.includes('test:types')));
  assert(plan.commands.some(cmd => cmd.args.includes('src/kernel/')));
});

test('classifies shared contract and bindings changes', () => {
  const c = classifyChanges(['contracts/src/lib.rs', 'apps/desktop/src/kernel/contracts.ts']);
  assert.equal(c.category, 'contracts');
  const plan = planFromClassification(c);
  assert.equal(plan.category, 'contracts');
  assert(plan.commands.some(cmd => cmd.args.includes('structured_contract_golden')));
  assert(plan.commands.some(cmd => cmd.args.includes('test:types')));
});

test('fails closed to broad check for root configs and core changes', () => {
  for (const f of [
    'Cargo.toml',
    'Cargo.lock',
    'apps/desktop/package.json',
    'crates/core/src/lib.rs',
    '.github/workflows/ci.yml',
    'apps/desktop/src-tauri/tauri.conf.json',
  ]) {
    const c = classifyChanges([f]);
    assert.equal(c.category, 'broad', `File ${f} should fail closed to broad check`);
    const plan = planFromClassification(c);
    assert.equal(plan.category, 'broad');
    assert.deepEqual(plan.commands[0], { executable: 'desktop', args: ['check'] });
  }
});
