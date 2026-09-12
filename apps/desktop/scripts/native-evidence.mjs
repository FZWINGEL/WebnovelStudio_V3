import { appendFileSync } from 'node:fs';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { dirname } from 'node:path';
import { hostname, release, cpus } from 'node:os';
let previous = performance.now();
export async function initializeEvidence(executable) {
  if (!process.env.WNS_V3_NATIVE_TRACE) return;
  await mkdir(dirname(process.env.WNS_V3_NATIVE_TRACE), { recursive: true });
  await writeFile(process.env.WNS_V3_NATIVE_TRACE, `${JSON.stringify({ kind: 'identity', executable,
    executableSha256: createHash('sha256').update(await readFile(executable)).digest('hex'),
    runner: { hostname: hostname(), os: release(), logicalCpus: cpus().length, node: process.version },
    startedAt: new Date().toISOString() })}\n`);
  previous = performance.now();
}
export function recordCheck(checks, id, ...descriptions) {
  const now = performance.now();
  if (process.env.WNS_V3_NATIVE_TRACE) {
    appendFileSync(process.env.WNS_V3_NATIVE_TRACE, `${JSON.stringify({
      kind: 'check', id, descriptions, elapsedMs: now - previous, at: new Date().toISOString(),
    })}\n`);
  }
  previous = now;
  return checks.push(...descriptions);
}
