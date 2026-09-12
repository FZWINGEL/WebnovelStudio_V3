// Isolated reproduction of repeated ChildProcess cleanup, not a Windows/Tauri test.
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { performance } from 'node:perf_hooks';

async function originalStopIfAlive(app) {
  if (app && app.exitCode === null) {
    app.kill();
    await new Promise(resolve => {
      const timeout = setTimeout(resolve, 5000);
      app.once('exit', () => { clearTimeout(timeout); resolve(); });
    });
  }
}
const child = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], {stdio: 'ignore'});
await once(child, 'spawn');
const exit = once(child, 'exit');
child.kill('SIGTERM');
await exit;
const before = performance.now();
await originalStopIfAlive(child);
const originalMs = performance.now() - before;
const guardedBefore = performance.now();
// Illustrates the already-exited guard only; not a complete process-tree cleanup helper.
if (child.exitCode === null && child.signalCode === null) await originalStopIfAlive(child);
const guardedMs = performance.now() - guardedBefore;
console.log(JSON.stringify({
  scope: 'Isolated Node process on Linux; not Windows native CI',
  node: process.version,
  platform: process.platform,
  exitCode: child.exitCode,
  signalCode: child.signalCode,
  originalRepeatedCleanupMs: Number(originalMs.toFixed(2)),
  guardedRepeatedCleanupMs: Number(guardedMs.toFixed(2))
}, null, 2));
