// Library-only experiment. Does not replace or estimate native/test linking.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { createServer } from 'node:net';
import { homedir, hostname } from 'node:os';
import { delimiter, isAbsolute, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const pins = JSON.parse(await readFile(resolve(root, 'ci-performance.json'), 'utf8'));
assert(process.argv[2] && process.argv[3], 'Pass an isolated source checkout and pinned sccache executable');
const source = resolve(process.argv[2]);
const sccache = resolve(process.argv[3]);
const directory = resolve(root, '.local/performance', `sccache-${Date.now()}`);
const target = resolve(directory, 'target');
const childPath = relative(root, target);
assert(childPath.startsWith(`.local${sep}`) && !isAbsolute(childPath) && !childPath.startsWith('..'));
await mkdir(directory, { recursive: true });
const socket = createServer();
await new Promise(done => socket.listen(0, '127.0.0.1', done));
const port = socket.address().port;
await new Promise(done => socket.close(done));
const env = { ...process.env, PATH: [resolve(homedir(), '.cargo/bin'), process.env.PATH].join(delimiter),
  CARGO_INCREMENTAL: '0', CARGO_TERM_COLOR: 'never', SCCACHE_DIR: resolve(directory, 'cache'), SCCACHE_SERVER_PORT: String(port) };
delete env.RUSTC_WRAPPER; delete env.RUSTC_WORKSPACE_WRAPPER;
const records = [];
async function fingerprint() {
  const files = execFileSync('git', ['ls-files', '-co', '--exclude-standard', '-z'], { cwd: source }).toString().split('\0').filter(Boolean);
  const hash = createHash('sha256');
  for (const file of [...new Set(files)].sort()) { hash.update(file); hash.update(await readFile(resolve(source, file))); }
  return hash.digest('hex');
}
async function run(label, exe, args, wrapped = false) {
  const started = performance.now(); let stdout = ''; let stderr = '';
  const code = await new Promise((done, reject) => {
    const child = spawn(exe, args, { cwd: source, windowsHide: true, env: { ...env, ...(wrapped ? { RUSTC_WRAPPER: sccache } : {}) }, stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.on('data', bytes => { stdout += bytes; }); child.stderr.on('data', bytes => { stderr += bytes; });
    child.once('error', reject); child.once('close', done);
  });
  const record = { label, code, durationMs: performance.now() - started, args };
  records.push(record);
  await writeFile(resolve(directory, `${label}.stdout.txt`), stdout);
  await writeFile(resolve(directory, `${label}.stderr.txt`), stderr);
  await writeFile(resolve(directory, 'samples.json'), JSON.stringify(records, null, 2));
  console.log(`${label}: exit ${code}, ${record.durationMs.toFixed(0)} ms`);
  assert.equal(code, 0, `${label} failed; evidence retained`);
  return stdout;
}
const report = { source, directory, runner: hostname(), pins, records, qualified: false,
  commit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: source, encoding: 'utf8' }).trim(),
  scope: 'Warm dependencies; rebuild only webnovel-core library at fixed source; no application/test linking claim' };
try {
  report.sourceHash = await fingerprint();
  assert((await run('version', sccache, ['--version'])).includes(`sccache ${pins.sccache}`));
  const args = ['build', '-p', 'webnovel-core', '--lib', '--frozen', '--target-dir', target];
  await run('prepare-dependencies', 'cargo', args);
  for (let pair = 0; pair < pins.samples; pair++) {
    for (const variant of ['baseline', 'sccache']) {
      await run(`clean-${variant}-${pair}`, 'cargo', ['clean', '-p', 'webnovel-core', '--target-dir', target]);
      await run(`${variant}-${pair}`, 'cargo', args, variant === 'sccache');
      assert.equal(await fingerprint(), report.sourceHash, 'Source changed; samples are not comparable');
    }
  }
  report.cacheStatistics = JSON.parse(await run('cache-statistics', sccache, ['--show-stats', '--stats-format', 'json']));
  report.qualified = true;
} catch (error) { report.failure = String(error.stack ?? error); process.exitCode = 1; }
finally {
  // This study chooses its own server port and cache directory, never the author's server.
  await run('stop-study-server', sccache, ['--stop-server']).catch(error => { report.cleanupFailure = String(error); report.qualified = false; process.exitCode = 1; });
  await writeFile(resolve(directory, 'report.json'), JSON.stringify(report, null, 2));
}
