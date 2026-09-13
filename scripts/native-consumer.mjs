import assert from 'node:assert/strict';
import { readFile, mkdir, writeFile, unlink } from 'node:fs/promises';
import { hostname, release } from 'node:os';
import { resolve, delimiter } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnOwned, stopOwned } from '../tests/native/owned-process.mjs';
import { checkoutIdentity, verifyArtifact, sha256, runtimePins } from './native-artifact.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
export const suiteManifest = JSON.parse(await readFile(new URL('./native-suites.json', import.meta.url), 'utf8'));
export function validateSuiteResult(result, expected, identity) {
  assert.equal(result.status, 'passed', `Suite failed: ${result.suite}`);
  assert.equal(result.failure, undefined, 'Passed suite retained a failure');
  assert.equal(result.commit, identity.commit);
  assert.equal(result.executableSha256, identity.executableSha256);
  assert.equal(result.exitCode, 0);
  assert(Number.isFinite(result.durationMs) && result.durationMs >= 0);
  assert(Array.isArray(result.events));
  const identities = result.events.filter(event => event.kind === 'identity');
  assert.equal(identities.length, 1, 'Missing or duplicate executable identity');
  assert.equal(identities[0].executableSha256, identity.executableSha256);
  assert.equal(identities[0].runner.node, runtimePins.node);
  const actual = result.events.filter(event => event.kind === 'check').map(event => event.id);
  assert.equal(new Set(actual).size, actual.length, 'Duplicate native check');
  assert.deepEqual([...actual].sort(), [...expected.checks].sort(), 'Native check identities differ');
  for (const event of result.events.filter(event => event.kind === 'check')) {
    assert(Number.isFinite(event.elapsedMs) && event.elapsedMs >= 0);
  }
  const starts = result.events.filter(event => event.kind === 'process-start');
  const closes = result.events.filter(event => event.kind === 'process-close');
  assert(starts.length > 0, 'No native process evidence');
  assert.equal(starts.length, closes.length, 'Unconfirmed native cleanup');
  assert.equal(new Set(starts.map(event => event.pid)).size, starts.length, 'Duplicate native process identity');
  for (const started of starts) {
    assert(Number.isSafeInteger(started.pid) && started.pid > 0);
    const matching = closes.filter(event => event.pid === started.pid);
    assert.equal(matching.length, 1, 'Missing or duplicate process closure');
    const closed = matching[0];
    assert.equal(closed.spawnError, null, 'Native spawn failed');
    assert(closed.exitCode === null || Number.isInteger(closed.exitCode), 'Malformed native exit code');
    assert(closed.signalCode === null || typeof closed.signalCode === 'string' && closed.signalCode.length > 0, 'Malformed native exit signal');
    assert.equal(typeof closed.forced, 'boolean', 'Missing termination method');
    assert(closed.exitCode !== null || closed.signalCode !== null, 'Native exit unconfirmed');
    if (!closed.forced) {
      assert.equal(closed.exitCode, 0, 'Native process exited abnormally without an owned stop');
      assert.equal(closed.signalCode, null, 'Native process received an unexpected signal');
    }
    if (result.suite === 'close') assert.equal(closed.forced, false, 'Normal-close qualification required a forced stop');
  }
}
export function getConsumerSuites(name) {
  return suiteManifest.topologies?.parallel?.[name]
    ?? suiteManifest.topologies?.serial?.[name]
    ?? suiteManifest.consumers?.[name];
}

export function reconcileConsumers(consumers, identity, topology = 'parallel') {
  const expectedPlan = suiteManifest.topologies?.[topology] ?? suiteManifest.consumers;
  assert(expectedPlan, `Unknown topology: ${topology}`);
  assert.equal(consumers.length, Object.keys(expectedPlan).length);
  for (const consumer of consumers) {
    assert.equal(typeof consumer.runner?.hostname, 'string', 'Missing consumer host identity');
    assert(consumer.runner.hostname.trim(), 'Empty consumer host identity');
    assert.equal(consumer.failure, undefined, 'Passed consumer retained a failure');
  }
  assert.equal(new Set(consumers.map(item => item.consumer)).size, consumers.length, 'Duplicate consumer');
  if (topology === 'parallel') {
    assert.equal(new Set(consumers.map(item => item.runner.hostname)).size, consumers.length, 'Native consumers shared a desktop');
  }
  for (const [name, expected] of Object.entries(expectedPlan)) {
    const consumer = consumers.find(item => item.consumer === name);
    assert(consumer, `Missing consumer ${name}`);
    assert.equal(consumer.status, 'passed');
    assert.equal(consumer.commit, identity.commit);
    assert.equal(consumer.executableSha256, identity.executableSha256);
    assert.equal(consumer.manifestSha256, sha256(JSON.stringify(suiteManifest)));
    assert.deepEqual(consumer.suites.map(item => item.suite), expected);
    for (const result of consumer.suites) {
      validateSuiteResult(result, suiteManifest.suites[result.suite], identity);
      const header = result.events.find(event => event.kind === 'identity');
      assert.equal(header.runner.hostname, consumer.runner.hostname, 'Suite and consumer host identities differ');
    }
  }
}
async function runSuite(suite, executable, trace) {
  const nodePaths = [resolve(root, 'tests/native/node_modules'), resolve(root, 'apps/desktop/node_modules'), process.env.NODE_PATH].filter(Boolean).join(delimiter);
  const child = spawnOwned(process.execPath, [resolve(root, 'tests/native', suite.script)], {
    cwd: resolve(root, 'tests/native'), windowsHide: true, stdio: 'inherit',
    env: { ...process.env, NODE_PATH: nodePaths, WNS_V3_NATIVE_EXE: executable, WNS_V3_NATIVE_TRACE: trace },
  });
  let timer;
  try {
    return await Promise.race([
      new Promise((accept, reject) => { child.once('error', reject); child.once('close', code => accept(code)); }),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Native suite exceeded 15 minutes')), 900_000); }),
    ]);
  } finally { clearTimeout(timer); await stopOwned(child); }
}
async function consume(name, directory) {
  const suites = getConsumerSuites(name);
  assert(suites, `Unknown native consumer: ${name}`);
  const evidence = resolve(root, '.local/native-results');
  await mkdir(evidence, { recursive: true });
  const output = resolve(evidence, `consumer-${name}.json`);
  const report = { consumer: name, status: 'failed', suites: [], runner: { hostname: hostname(), os: release(), node: process.version },
    manifestSha256: sha256(JSON.stringify(suiteManifest)), startedAt: new Date().toISOString() };
  try {
    report.commit = checkoutIdentity(root);
    const manifest = await verifyArtifact(directory, report.commit);
    report.executableSha256 = manifest.files.find(file => file.path === manifest.executable).sha256;
    for (const id of suites) {
      const suite = suiteManifest.suites[id];
      const trace = resolve(evidence, `${id}.events.jsonl`);
      await writeFile(trace, '');
      const nativeReport = resolve(evidence, suite.report);
      await unlink(nativeReport).catch(error => { if (error.code !== 'ENOENT') throw error; });
      const started = performance.now();
      const result = { suite: id, status: 'failed', commit: report.commit, executableSha256: report.executableSha256 };
      report.suites.push(result);
      try {
        result.exitCode = await runSuite(suite, resolve(directory, manifest.executable), trace);
        const native = JSON.parse(await readFile(nativeReport, 'utf8'));
        assert(!native.failure && (!native.status || native.status === 'passed'), 'Native report failed');
        assert(Array.isArray(native.checks) && native.checks.length > 0, 'Missing native report checks');
        assert(!native.cleanupFailures?.length, 'Native cleanup failed');
        assert(!native.errors?.length && !native.pageErrors?.length, 'Native page errors');
        result.status = 'passed';
      } catch (error) { result.failure = String(error.stack ?? error); }
      finally {
        result.durationMs = performance.now() - started;
        result.events = (await readFile(trace, 'utf8')).trim().split('\n').filter(Boolean).map(line => JSON.parse(line));
      }
      validateSuiteResult(result, suite, report);
    }
    report.status = 'passed';
  } catch (error) { report.failure = String(error.stack ?? error); process.exitCode = 1; }
  finally { report.finishedAt = new Date().toISOString(); await writeFile(output, JSON.stringify(report, null, 2)); }
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, directory, artifactDirectory, topologyArg] = process.argv.slice(2);
  assert(directory, 'Artifact/evidence directory required');
  if (command === 'aggregate') {
    const topology = topologyArg || 'parallel';
    const plan = suiteManifest.topologies?.[topology];
    assert(plan, `Unknown topology: ${topology}`);
    const consumers = await Promise.all(Object.keys(plan).map(name =>
      readFile(resolve(directory, `consumer-${name}.json`), 'utf8').then(JSON.parse)));
    const commit = checkoutIdentity(root);
    assert(artifactDirectory, 'Producer artifact directory required');
    const artifact = await verifyArtifact(resolve(artifactDirectory), commit);
    reconcileConsumers(consumers, { commit, executableSha256: artifact.files.find(file => file.path === artifact.executable).sha256 }, topology);
    console.log('Every scheduled native check and owned process closure was verified.');
  } else await consume(command, resolve(directory));
}
