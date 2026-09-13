import { spawnOwned, stopOwned, markOwnedReady } from './owned-process.mjs';
import { recordCheck, initializeEvidence } from './native-evidence.mjs';
// Native reviewed-memory qualification. Default uses only the offline test model.
// --live explicitly opts into one Codex discussion with at most three calls.
import { chromium } from 'playwright-core';
import { setupMemoryLookupFixture } from './native-memory-lookup.mjs';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, realpath, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, relative, isAbsolute, sep, toNamespacedPath } from 'node:path';
import { createServer } from 'node:net';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const live = process.argv.includes('--live');
const evidence = resolve(root, `.local/native-results/memory-lookup-${live ? 'live' : 'mock'}`);
await mkdir(evidence, { recursive: true });
const executable = process.env.WNS_V3_NATIVE_EXE ? resolve(process.env.WNS_V3_NATIVE_EXE) : resolve(root, 'target/debug/webnovel-desktop.exe');
await initializeEvidence(executable);
const info = await stat(executable);
const data = await mkdtemp(resolve(tmpdir(), 'wns-v3-memory-lookup-'));
const server = createServer();
await new Promise(done => server.listen(0, '127.0.0.1', done));
const port = server.address().port;
await new Promise(done => server.close(done));
const app = spawnOwned(executable, [], { cwd: data, windowsHide: true, stdio: 'pipe', env: {
  ...process.env, WNS_V3_NATIVE_CDP_PORT: String(port), WNS_V3_TRIAL_WEBVIEW_DIR: resolve(data, 'webview'), WNS_V3_TEST_DATA_DIR: resolve(data, 'library'),
} });
let appLog = ''; app.stdout.on('data', chunk => { appLog += chunk; }); app.stderr.on('data', chunk => { appLog += chunk; });
let browser, page, db;
const errors = [];
const report = { startedAt: new Date().toISOString(), executable, executableBytes: info.size,
  executableSha256: createHash('sha256').update(await readFile(executable)).digest('hex'),
  dataDirectory: data, live, checks: [], modelCallCount: 0 };
const invoke = (command, args) => page.evaluate(([name, value]) => window.__TAURI_INTERNALS__.invoke(name, value), [command, args]);
try {
  const deadline = Date.now() + 90_000;
  while (Date.now() < deadline) {
    assert.equal(app.exitCode, null, `Owned app exited early: ${appLog}`);
    try { const response = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(1000) }); if (response.ok) break; } catch {}
    await new Promise(done => setTimeout(done, 250));
  }
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  page = browser.contexts()[0].pages()[0];
  page.on('pageerror', error => errors.push(error.message));
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  report.runtime = await invoke('runtime_info');
  markOwnedReady(app);
  const fixture = await setupMemoryLookupFixture({ page, data });
  report.fixture = fixture;
  const library = await invoke('library_snapshot');
  const entry = library.entries.find(item => item.title === fixture.title);
  assert(entry);
  const projectPath = await realpath(entry.path);
  const child = relative(toNamespacedPath(await realpath(data)), toNamespacedPath(projectPath));
  assert(child && !isAbsolute(child) && child !== '..' && !child.startsWith(`..${sep}`));
  db = new DatabaseSync(resolve(projectPath, 'project.sqlite3'), { readOnly: true });
  assert.equal(db.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, 0);
  recordCheck(report.checks, 'native-memory-lookup-smoke:01', 'Synthetic reviewed story records created without a model request');
  await page.reload();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.getByRole('button', { name: new RegExp(`^${fixture.title} Last opened`) }).click();
  if (!(await page.getByRole('heading', { name: fixture.targetTitle, exact: true }).isVisible())) {
    await page.getByRole('button', { name: new RegExp(fixture.targetTitle) }).first().click();
  }
  await page.getByRole('heading', { name: fixture.targetTitle, exact: true }).waitFor();
  const original = await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());
  let state = await invoke('provider_state');
  const modelSearch = live ? 'gpt-5.6-luna' : 'Local test model';
  await page.getByRole('button', { name: /^Choose model:/ }).click();
  await page.getByRole('searchbox', { name: 'Search models' }).fill(modelSearch);
  await page.getByRole('searchbox', { name: 'Search models' }).press('Enter');
  if (live) {
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.getByRole('button', { name: 'Check Codex connection', exact: true }).click();
    await page.getByText(/Signed in through Codex/, { exact: false }).first().waitFor({ timeout: 120000 });
    await page.getByRole('button', { name: 'Close settings', exact: true }).click();
    await page.getByRole('combobox', { name: 'Reasoning effort', exact: true }).selectOption('xhigh');
    await page.getByRole('combobox', { name: 'Service tier', exact: true }).selectOption('priority');
  }
  state = await invoke('provider_state');
  report.selection = state.settings.active;
  report.codexConnection = state.codexConnection;
  assert.notEqual(state.codexConnection?.checking, true, 'The startup connection check must settle before author dispatch.');
  if (live) {
    assert.equal(state.dispatch.kind, 'codexCli');
    assert.deepEqual(state.settings.active, { providerId: 'codex', modelId: 'gpt-5.6-luna', reasoning: 'xhigh', serviceTier: 'priority' });
  } else assert.equal(state.settings.active.providerId, 'mock');
  const options = page.locator('details.request-options').first();
  if (await options.count() && await options.getAttribute('open') === null) await options.locator(':scope > summary').click();
  await page.getByLabel('Look up story details when needed', { exact: true }).check();
  const instruction = live
    ? 'Qualify the reviewed-memory lookup interface on this synthetic story. In your first response, request findEntities reads for character Mei, object silver key, and promise Return the silver key. In the next response, use their returned exact IDs to request knowledgeHistory, possessionHistory, and promiseHistory. In your third and final response, explain what Mei believes about the gate, who is recorded holding the key, and the recorded promise. Quote the exact evidence, name the source chapter, and distinguish beliefs and incomplete recorded history from world truth. Keep the final answer under 160 words. Do these read-only qualification steps even if some evidence is already present in the initial packet; do not create edits.'
    : 'Look up reviewed memory for "Mei" and inspect knowledge, promises, and possessions before answering.';
  await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).fill(instruction);
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  const until = Date.now() + (live ? 600000 : 30000);
  let run;
  while (Date.now() < until) {
    run = db.prepare('SELECT id,status,packet_id,dispatch_state FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get();
    if (run && !['queued', 'running', 'stopping'].includes(run.status)) break;
    await new Promise(done => setTimeout(done, 250));
  }
  report.run = run;
  report.invocations = db.prepare('SELECT i.ordinal,i.packet_id,i.state,r.assistant_text,r.outcome,r.confirmed_stdin_bytes,r.usage_json,r.cleanup,r.error FROM discussion_lookup_invocations i LEFT JOIN discussion_lookup_results r ON r.run_id=i.run_id AND r.ordinal=i.ordinal ORDER BY i.ordinal').all();
  report.modelCallCount = live ? report.invocations.filter(row => Number(row.confirmed_stdin_bytes) > 0).length : 0;
  report.packets = report.invocations.map(row => JSON.parse(db.prepare('SELECT packet_json FROM context_packets WHERE id=?').get(row.packet_id).packet_json));
  report.reads = db.prepare('SELECT request_json,result_json FROM discussion_lookup_reads ORDER BY rowid').all().map(row => ({ request: JSON.parse(row.request_json), result: JSON.parse(row.result_json) }));
  report.answer = db.prepare("SELECT content,packet_id FROM discussion_messages WHERE role='assistant' ORDER BY rowid DESC LIMIT 1").get();
  assert.equal(run.status, 'completed');
  assert.equal(report.invocations.length, 3);
  assert(report.packets.every(packet => packet.receipt.lookup.reviewedMemory === 'reviewed-memory.v1'));
  for (const kind of ['findEntities', 'knowledgeHistory', 'possessionHistory', 'promiseHistory']) assert(report.reads.some(row => row.request.kind === kind), `Missing ${kind} read`);
  assert.equal(db.prepare('SELECT count(*) AS n FROM proposals').get().n, 0);
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), original);
  await page.getByLabel('Context for model call', { exact: true }).waitFor();
  const inspector = page.locator('.context-inspector');
  if (await inspector.getAttribute('open') === null) await inspector.locator(':scope > summary').click();
  await inspector.getByRole('region', { name: 'Delivered to model story lookup evidence', exact: true }).waitFor();
  await inspector.getByText(/^Knowledge history/).scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(evidence, 'knowledge-history.png') });
  await inspector.getByRole('button', { name: 'Open exact source', exact: true }).first().click();
  await page.screenshot({ path: resolve(evidence, 'memory-lookups.png') });
  recordCheck(report.checks, 'native-memory-lookup-smoke:02', 'Three bounded model invocations retain identity, knowledge, promise and possession reads; final answer inspected with exact source; no manuscript/proposal change');
  const runCount = report.invocations.length;
  await page.reload();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.getByRole('button', { name: new RegExp(`^${fixture.title} Last opened`) }).click();
  await page.getByRole('heading', { name: fixture.targetTitle, exact: true }).waitFor();
  await page.getByLabel('Context for model call', { exact: true }).waitFor();
  assert.equal(db.prepare('SELECT count(*) AS n FROM discussion_lookup_invocations').get().n, runCount);
  recordCheck(report.checks, 'native-memory-lookup-smoke:03', 'Saved answer and lookup packets reopen without another invocation');
  assert.deepEqual(errors, []);
  report.status = 'passed';
} catch (error) {
  report.status = 'failed'; report.failure = error.stack;
  report.providerStateAtFailure = await invoke('provider_state').catch(() => null);
  await page?.screenshot({ path: resolve(evidence, 'failure.png') }).catch(() => {});
  process.exitCode = 1;
} finally {
  db?.close(); await browser?.close().catch(() => {});
  await stopOwned(app);
  report.cleanup = { ownedPid: app.pid, exitCode: app.exitCode, signalCode: app.signalCode };
  report.finishedAt = new Date().toISOString(); report.pageErrors = errors;
  await writeFile(resolve(evidence, 'app.log'), appLog);
  await writeFile(resolve(evidence, 'qualification.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ status: report.status, failure: report.failure, modelCallCount: report.modelCallCount, checks: report.checks, evidence }));
}
