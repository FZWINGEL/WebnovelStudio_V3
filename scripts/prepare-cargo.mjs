// Optional registry experiment: probe source availability offline, fetch only
// missing locked dependencies, and never retry a compiler or test online.
import { spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const attempts = [];
async function fetchDependencies(offline) {
  const args = ['fetch', '--locked', ...(offline ? ['--offline'] : [])];
  const started = performance.now();
  const exitCode = await new Promise((accept, reject) => {
    const child = spawn('cargo', args, { cwd: root, stdio: 'inherit', windowsHide: true });
    child.once('error', reject); child.once('close', accept);
  });
  attempts.push({ args, exitCode, durationMs: performance.now() - started });
  return exitCode;
}
try {
  if (await fetchDependencies(true) !== 0 && await fetchDependencies(false) !== 0) process.exitCode = 1;
} finally {
  await mkdir(resolve(root, '.local/performance'), { recursive: true });
  await writeFile(resolve(root, '.local/performance/dependencies.json'), JSON.stringify({ attempts }, null, 2));
}
