import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once, EventEmitter } from 'node:events';
import { trackOwnedProcess } from '../apps/desktop/scripts/owned-process.mjs';

for (const signaled of [false, true]) {
  test(`repeated cleanup after ${signaled ? 'signal' : 'normal'} exit`, async () => {
    const child = spawn(process.execPath, ['-e', signaled ? 'setInterval(() => {}, 1000)' : ''], { windowsHide: true });
    const owner = trackOwnedProcess(child);
    const closed = once(child, 'close');
    if (signaled) child.kill();
    await closed;
    await owner.stop(20);
    await owner.stop(20);
  });
}
test('live child concurrent cleanup shares completion and closes streams', async () => {
  const child = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { windowsHide: true });
  const owner = trackOwnedProcess(child);
  const first = owner.stop();
  assert.equal(owner.stop(), first);
  await first;
  assert(child.stdout.destroyed);
  assert(child.exitCode !== null || child.signalCode !== null);
});
test('failed spawn settles without an unhandled error', async () => {
  const child = spawn('missing-owned-process-fixture-executable', [], { windowsHide: true });
  const owner = trackOwnedProcess(child);
  await owner.stop();
  await owner.stop(20);
});
test('unconfirmed stream closure fails explicitly', async () => {
  const child = Object.assign(new EventEmitter(), { exitCode: null, signalCode: 'SIGTERM' });
  const owner = trackOwnedProcess(child);
  await assert.rejects(owner.stop(10), /cleanup unconfirmed/);
  child.emit('close');
  await owner.stop();
  assert.equal(child.listenerCount('error'), 0);
});
