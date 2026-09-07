import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { suiteManifest, reconcileConsumers } from './native-consumer.mjs';
import { sha256 } from './native-artifact.mjs';
const identity = { commit: 'a'.repeat(40), executableSha256: 'b'.repeat(64) };
function fixture() {
  return Object.entries(suiteManifest.consumers).map(([consumer, suites], index) => ({
    consumer, ...identity, status: 'passed', runner: { hostname: `vm-${index}` },
    manifestSha256: sha256(JSON.stringify(suiteManifest)),
    suites: suites.map(suite => ({ suite, ...identity, status: 'passed', exitCode: 0, durationMs: 1,
      events: [{ kind: 'process-start', pid: 123 }, { kind: 'process-close', pid: 123, spawnError: null, exitCode: 0, signalCode: null, forced: false },
        { kind: 'identity', executableSha256: identity.executableSha256, runner: { node: 'v24.20.0', hostname: `vm-${index}` } },
        ...suiteManifest.suites[suite].checks.map(id => ({ kind: 'check', id, elapsedMs: 1 }))] })),
  }));
}
test('reconciliation requires every check identity and separate native desktops', () => {
  reconcileConsumers(fixture(), identity);
  for (const alter of [x => x.pop(), x => x.push(x[0]), x => x[1].runner.hostname = x[0].runner.hostname,
    x => x[0].status = 'skipped', x => x[0].suites.pop(), x => x[0].suites[0].events.pop(),
    x => x[0].suites[0].events.push(x[0].suites[0].events.at(-1)),
    x => x[0].suites[0].events.splice(1, 1), x => x[0].suites[0].exitCode = 1,
    x => x[0].suites[0].executableSha256 = 'c'.repeat(64),
    x => x[1].suites.find(s => s.suite === 'close').events[1].forced = true,
    x => x[0].manifestSha256 = 'c'.repeat(64)]) {
    const consumers = fixture(); alter(consumers);
    assert.throws(() => reconcileConsumers(consumers, identity));
  }
});
test('malformed closure and contradictory success evidence cannot pass', () => {
  for (const alter of [
    x => delete x[0].suites[0].events[1].exitCode,
    x => delete x[0].suites[0].events[1].signalCode,
    x => delete x[0].suites[0].events[1].forced,
    x => x[0].suites[0].events[1].exitCode = 1,
    x => x[0].suites[0].failure = 'cleanup failed',
    x => x[0].failure = 'consumer failed',
    x => x[0].runner.hostname = '',
    x => x[0].suites[0].events[2].runner.hostname = 'another-host',
  ]) {
    const consumers = fixture(); alter(consumers);
    assert.throws(() => reconcileConsumers(consumers, identity));
  }
});
test('registered native checkpoint ids exist in source exactly once', async () => {
  const ids = Object.values(suiteManifest.suites).flatMap(suite => suite.checks);
  assert.equal(new Set(ids).size, ids.length);
  for (const id of ids.filter(id => id.startsWith('native-'))) {
    const module = id.split(':')[0];
    const source = await readFile(new URL(`../apps/desktop/scripts/${module}.mjs`, import.meta.url), 'utf8');
    assert.equal(source.split(`'${id}'`).length - 1, 1, id);
  }
  const scripts = new URL('../apps/desktop/scripts/', import.meta.url);
  for (const name of (await readdir(scripts)).filter(name => name.startsWith('native-') && name.endsWith('.mjs'))) {
    const source = await readFile(new URL(name, scripts), 'utf8');
    for (const match of source.matchAll(/recordCheck\([^,]+, '([^']+)'/g)) {
      assert(ids.includes(match[1]), `Unregistered native checkpoint ${match[1]}`);
    }
  }
});
