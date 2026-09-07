import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { homedir, hostname, cpus } from 'node:os';
import { delimiter, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expectedIdentities, verifyLibtestOutput, verifyNextestOutput } from './runner-identities.mjs';
const root = fileURLToPath(new URL('../', import.meta.url));
const pins = JSON.parse(await readFile(resolve(root, 'ci-performance.json'), 'utf8'));
const nextest = resolve(process.argv[2] ?? `.local/nextest-${pins.nextest}/cargo-nextest.exe`);
const output = resolve(root, '.local/performance', `runners-${Date.now()}`);
await mkdir(output, { recursive: true });
const env = { ...process.env, PATH: [resolve(homedir(), '.cargo/bin'), process.env.PATH].join(delimiter),
  RUST_TEST_THREADS: String(pins.rustTestThreads), CARGO_TERM_COLOR: 'never' };
const samples = [];
async function sourceHash() {
  const files = execFileSync('git', ['ls-files', '-co', '--exclude-standard', '-z'], { cwd: root }).toString().split('\0').filter(Boolean);
  const hash = createHash('sha256');
  for (const file of [...new Set(files)].sort()) { hash.update(file); hash.update(await readFile(resolve(root, file))); }
  return hash.digest('hex');
}
async function run(label, executable, args) {
  const started = performance.now(); let stdout = ''; let stderr = '';
  const exitCode = await new Promise((accept, reject) => {
    const child = spawn(executable, args, { cwd: root, env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.on('data', bytes => { stdout += bytes; }); child.stderr.on('data', bytes => { stderr += bytes; });
    child.once('error', reject); child.once('close', accept);
  });
  const record = { label, executable, args, exitCode, durationMs: performance.now() - started };
  samples.push(record);
  await writeFile(resolve(output, `${label}.stdout.txt`), stdout);
  await writeFile(resolve(output, `${label}.stderr.txt`), stderr);
  await writeFile(resolve(output, 'samples.json'), JSON.stringify(samples, null, 2));
  console.log(`${label}: exit ${exitCode}, ${record.durationMs.toFixed(0)} ms`);
  return { ...record, stdout, stderr };
}
const report = { runner: hostname(), logicalCpus: cpus().length, pins, samples, output, qualified: false,
  commit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim() };
try {
  report.sourceHash = await sourceHash();
  const version = await run('nextest-version', nextest, ['nextest', '--version']);
  assert(version.stdout.includes(`cargo-nextest ${pins.nextest}`), 'Unexpected nextest version');
  const listing = await run('nextest-list', nextest, ['nextest', 'list', '--workspace', '--locked', '--ignore-default-filter', '--message-format', 'json']);
  assert.equal(listing.exitCode, 0);
  const nextestList = JSON.parse(listing.stdout);
  report.testInventory = nextestList;
  const expected = expectedIdentities(nextestList);
  // Compare each nextest binary's complete and ignored lists with libtest itself.
  for (const [id, suite] of Object.entries(nextestList['rust-suites'])) {
    const binary = suite['binary-path'];
    const listed = await run(`list-${Object.keys(report.testInventory['rust-suites']).indexOf(id)}`, binary, ['--list']);
    const names = listed.stdout.split(/\r?\n/).filter(line => line.endsWith(': test')).map(line => line.slice(0, -6)).sort();
    assert.deepEqual(names, Object.keys(suite.testcases).sort(), `Test identity mismatch: ${id}`);
    const ignored = await run(`ignored-${Object.keys(report.testInventory['rust-suites']).indexOf(id)}`, binary, ['--ignored', '--list']);
    const ignoredNames = ignored.stdout.split(/\r?\n/).filter(line => line.endsWith(': test')).map(line => line.slice(0, -6)).sort();
    assert.deepEqual(ignoredNames, Object.entries(suite.testcases).filter(([, value]) => value.ignored).map(([name]) => name).sort());
  }
  for (let pair = 0; pair < pins.samples; pair++) {
    const baseline = await run(`baseline-${pair}`, 'cargo', ['test', '--workspace', '--locked']);
    if (baseline.exitCode === 0) verifyLibtestOutput(baseline.stdout, expected);
    const candidate = await run(`nextest-${pair}`, nextest, ['nextest', 'run', '--workspace', '--locked', '--ignore-default-filter', '--test-threads', String(pins.rustTestThreads), '--retries', '0', '--no-fail-fast']);
    verifyNextestOutput(candidate.stderr, expected);
    await run(`doctests-${pair}`, 'cargo', ['test', '--workspace', '--doc', '--locked']);
    assert.equal(await sourceHash(), report.sourceHash, 'Source changed during comparison; samples are not comparable');
  }
  report.qualified = samples.every(sample => sample.exitCode === 0);
  if (!report.qualified) process.exitCode = 1;
} catch (error) { report.failure = String(error.stack ?? error); process.exitCode = 1; }
finally { await writeFile(resolve(output, 'report.json'), JSON.stringify(report, null, 2)); }
console.log(output);
