import { spawn } from 'node:child_process';
import { appendFileSync } from 'node:fs';
const owned = new WeakMap();
export function spawnOwned(...args) {
  const child = spawn(...args);
  owned.set(child, trackOwnedProcess(child));
  return child;
}
export async function stopOwned(child) {
  if (!child) return;
  const record = owned.get(child);
  if (!record) throw new Error('Refuse cleanup of an unowned child');
  await record.stop();
}
export function markOwnedReady(child) {
  const record = owned.get(child);
  if (!record) throw new Error('Refuse readiness for an unowned child');
  record.ready();
}
function trace(value) {
  if (process.env.WNS_V3_NATIVE_TRACE) appendFileSync(process.env.WNS_V3_NATIVE_TRACE, `${JSON.stringify(value)}\n`);
}
// Register immediately after spawn. `close` confirms both exit and stdio closure.
export function trackOwnedProcess(child) {
  let closed = false;
  let spawnError;
  let stopping;
  let settle;
  const started = performance.now();
  let forced = false;
  let shutdownStarted;
  trace({ kind: 'process-start', pid: child.pid ?? null });
  const completion = new Promise(resolve => { settle = resolve; });
  const onError = error => { spawnError = error; };
  child.on('error', onError);
  child.once('close', () => {
    closed = true;
    child.removeListener('error', onError);
    trace({ kind: 'process-close', pid: child.pid ?? null, forced, durationMs: performance.now() - started,
      exitCode: child.exitCode, signalCode: child.signalCode, spawnError: spawnError ? String(spawnError) : null });
    if (shutdownStarted !== undefined) trace({ kind: 'phase', phase: 'teardown', pid: child.pid ?? null,
      elapsedMs: performance.now() - shutdownStarted });
    settle();
  });
  return {
    ready() { trace({ kind: 'phase', phase: 'native-startup', pid: child.pid ?? null, elapsedMs: performance.now() - started }); },
    stop(timeoutMs = 5000) {
      if (closed) return Promise.resolve();
      if (stopping) return stopping;
      stopping = (async () => {
        shutdownStarted = performance.now();
        if (!spawnError && child.pid !== undefined && child.exitCode === null && child.signalCode === null) {
          forced = true;
          if (!child.kill()) throw new Error('Owned process termination was not accepted');
        }
        let timer;
        try {
          await Promise.race([
            completion,
            new Promise((_, reject) => {
              timer = setTimeout(() => reject(new Error('Owned process cleanup unconfirmed: exit/stdio close timeout')), timeoutMs);
            }),
          ]);
        } finally { clearTimeout(timer); }
      })();
      return stopping;
    },
  };
}
