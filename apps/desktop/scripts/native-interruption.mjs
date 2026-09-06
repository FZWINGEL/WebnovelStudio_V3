// Native interruption qualification at durable Save/Apply acknowledgment boundaries. Every project and
// stream in this file is synthetic and lives under a fresh temp directory. The fixture configures no credentials.
import { chromium } from 'playwright-core';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, readFile, realpath, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { isAbsolute, relative, resolve, sep, toNamespacedPath } from 'node:path';
import { createServer as createNetServer } from 'node:net';
import { createServer as createHttpServer } from 'node:http';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';
import { qualifyRefreshShortcuts } from './native-refresh-shortcuts.mjs';
import { qualifyContextMenu } from './native-context-menu-check.mjs';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const executable = process.env.WNS_V3_NATIVE_EXE
  ? resolve(process.env.WNS_V3_NATIVE_EXE)
  : resolve(root, 'target/debug/webnovel-desktop.exe');
const menuOnly = process.argv.includes('--context-menu-only');
const evidence = resolve(root, menuOnly ? '.local/native-results/context-menu' : '.local/native-results/interruption');
await mkdir(evidence, { recursive: true });
const runtimeObservations = [];
const fixtureDirectories = [];
const launchLogs = [];
const screenshots = [];
const ownedApps = [];

async function reservePort() {
  const server = createNetServer();
  await new Promise(resolvePromise => server.listen(0, '127.0.0.1', resolvePromise));
  const port = server.address().port;
  await new Promise(resolvePromise => server.close(resolvePromise));
  return port;
}

function invoke(page, command, args) {
  return page.evaluate(([name, input]) => input === undefined
    ? window.__TAURI_INTERNALS__.invoke(name)
    : window.__TAURI_INTERNALS__.invoke(name, input), [command, args]);
}

async function launch(data) {
  const port = await reservePort();
  let appLog = '';
  const app = spawn(executable, [], {
    cwd: data,
    windowsHide: true,
    stdio: 'pipe',
    env: {
      ...process.env,
      WNS_V3_NATIVE_CDP_PORT: String(port),
      WNS_V3_TRIAL_WEBVIEW_DIR: resolve(data, 'webview'),
      WNS_V3_TEST_DATA_DIR: resolve(data, 'library'),
    },
  });
  ownedApps.push(app);
  let spawnError;
  app.on('error', error => { spawnError = error; appLog += String(error); });
  app.stdout.on('data', chunk => { appLog += chunk; });
  app.stderr.on('data', chunk => { appLog += chunk; });
  launchLogs.push({ dataDirectory: data, read: () => appLog });
  let browser;
  try {
    const started = Date.now();
    while (Date.now() - started < 90_000) {
      if (spawnError || app.exitCode !== null) throw new Error(`Native app exited before CDP (${app.exitCode}).\n${appLog}`);
      try {
        const response = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(2000) });
        if (response.ok && (await response.json()).webSocketDebuggerUrl) {
          browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 10_000 });
          break;
        }
      } catch { /* WebView2 is still starting. */ }
      await new Promise(resolvePromise => setTimeout(resolvePromise, 250));
    }
    if (!browser) throw new Error(`Native app did not expose CDP.\n${appLog}`);
    const context = browser.contexts()[0];
    const page = context.pages()[0] ?? await context.waitForEvent('page', { timeout: 10_000 });
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor({ timeout: 30_000 });
    const pageErrors = [];
    page.on('pageerror', error => pageErrors.push(error.message));
    const runtime = await invoke(page, 'runtime_info');
    assert.equal(runtime.host, 'Tauri');
    assert.equal(runtime.persistence, true);
    runtimeObservations.push({ dataDirectory: data, runtime, pageErrors });
    if (!fixtureDirectories.includes(data)) fixtureDirectories.push(data);
    return { app, browser, page, port, runtime, pageErrors, appLog: () => appLog };
  } catch (error) {
    await browser?.close().catch(() => {});
    await stopIfAlive(app);
    throw error;
  }
}

async function stopIfAlive(app) {
  if (app && app.exitCode === null) {
    app.kill();
    await new Promise(resolvePromise => {
      const timeout = setTimeout(resolvePromise, 5000);
      app.once('exit', () => { clearTimeout(timeout); resolvePromise(); });
    });
  }
}

async function createProject(page, title, documentId, documentTitle, text) {
  const opened = await invoke(page, 'library_create', {
    operationId: `interruption-${title.toLowerCase().replaceAll(/[^a-z0-9]+/g, '-')}`,
    title,
    session: `interruption-${documentId}`,
  });
  await invoke(page, 'create_document', { request: {
    access: opened.access,
    operationId: `create-${documentId}`,
    documentId,
    title: documentTitle,
    kind: 'chapter',
    body: { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: `${documentId}-paragraph` }, content: [{ type: 'text', text }] }] } },
  } });
  return opened;
}

async function configureAnonymousEndpoint(page, apiPort) {
  const settings = await invoke(page, 'endpoint_settings');
  const endpoint = await invoke(page, 'save_endpoint_settings', { request: {
    expectedRevision: settings.revision,
    profileId: null,
    label: 'Synthetic held interruption API',
    baseUrl: `http://127.0.0.1:${apiPort}/interruption-api`,
    enabled: true,
    jsonMode: false,
    manualModelIds: ['test-editor-v1'],
    apiKey: { kind: 'remove' },
  } });
  const profile = endpoint.profiles.find(item => item.label === 'Synthetic held interruption API');
  assert(profile, 'Anonymous interruption fixture endpoint must be persisted.');
  const provider = await invoke(page, 'provider_state');
  // Endpoint profile IDs already carry the openai-compatible: namespace.
  const providerId = profile.id;
  const author = await invoke(page, 'save_model_settings', {
    expectedRevision: provider.settings.revision,
    active: { providerId, modelId: 'test-editor-v1', reasoning: null, serviceTier: null },
    favorites: [],
  });
  assert.equal(author.settings.active.providerId, providerId);
  await page.reload();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  return { providerId };
}

function startHeldApi() {
  const requests = [];
  const server = createHttpServer((request, response) => {
    if (request.method === 'GET' && request.url?.endsWith('/models')) {
      response.writeHead(200, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify({ data: [{ id: 'test-editor-v1' }] }));
      return;
    }
    if (request.method !== 'POST' || !request.url?.endsWith('/chat/completions')) {
      response.writeHead(404); response.end(); return;
    }
    const entry = {
      number: requests.length + 1,
      model: null,
      requestBodyComplete: false,
      responseStarted: false,
      partialContent: false,
      clientDisconnected: false,
      completed: false,
    };
    requests.push(entry);
    const chunks = [];
    request.on('data', chunk => chunks.push(chunk));
    request.on('end', () => {
      try { entry.model = JSON.parse(Buffer.concat(chunks).toString()).model; } catch { entry.model = 'unknown'; }
      entry.requestBodyComplete = true;
      response.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache', Connection: 'keep-alive' });
      entry.responseStarted = true;
      response.write(`data: ${JSON.stringify({ model: entry.model, choices: [{ index: 0, delta: { role: 'assistant', content: `Held synthetic response ${entry.number}. ` } }] })}\n\n`);
      entry.partialContent = true;
      // Keep the response open across renderer replacement. Terminating its
      // native owner must disconnect it without a new provider request.
    });
    response.on('finish', () => { entry.completed = true; });
    response.on('close', () => { if (!entry.completed) entry.clientDisconnected = true; });
  });
  return {
    server,
    requests,
    async listen() {
      await new Promise(resolvePromise => server.listen(0, '127.0.0.1', resolvePromise));
      return server.address().port;
    },
    close() { server.closeAllConnections?.(); server.close(); },
  };
}

async function waitForHeldRequest(held, count) {
  const deadline = Date.now() + 15_000;
  while ((held.requests.length < count
    || held.requests.slice(0, count).some(request => !request.requestBodyComplete || !request.model))
    && Date.now() < deadline) {
    await new Promise(resolvePromise => setTimeout(resolvePromise, 50));
  }
  assert.equal(held.requests.length, count, `Expected exactly ${count} synthetic provider requests.`);
  assert(held.requests.slice(0, count).every(request => request.requestBodyComplete && request.model),
    `Synthetic request bodies did not settle: ${JSON.stringify(held.requests)}`);
}

async function capture(page, name) {
  const path = resolve(evidence, name);
  await page.screenshot({ path });
  screenshots.push(path);
}

function inspect(path, read) {
  const db = new DatabaseSync(resolve(path, 'project.sqlite3'), { readOnly: true });
  try { return read(db); } finally { db.close(); }
}

async function assertOwnedEntry(entry, data) {
  assert(entry, 'Synthetic project must be indexed.');
  // Windows can expose the same temp directory through an 8.3 alias in the
  // Node process while Rust returns its canonical long path in the library
  // index. Resolve both sides before checking containment.
  const [ownedRoot, projectPath] = await Promise.all([
    realpath(resolve(data)),
    realpath(entry.path),
  ]);
  const inside = relative(toNamespacedPath(ownedRoot), toNamespacedPath(projectPath));
  assert(inside && !isAbsolute(inside) && inside !== '..' && !inside.startsWith(`..${sep}`), 'Inspect only the owned synthetic project.');
}

const manuscript = page => page.getByRole('textbox', { name: 'Manuscript', exact: true });
const readBody = (path, documentId) => inspect(path, db => db.prepare('SELECT working_version,body_json,body_hash FROM documents WHERE id=?').get(documentId));
const readReceipt = (path, operationId) => inspect(path, db => db.prepare('SELECT * FROM command_receipts WHERE operation_id=?').all(operationId));

async function openFixture(page, title, chapter) {
  await page.getByRole('button', { name: new RegExp(`^${title} Last opened`) }).click();
  await page.getByRole('heading', { name: chapter, exact: true }).waitFor();
}

async function chooseMock(page) {
  await page.getByRole('button', { name: /^Choose model:/ }).click();
  await page.locator('.model-choice').filter({ hasText: 'Local test model' }).click();
  await page.getByRole('button', { name: 'Choose model: Local test model', exact: true }).waitFor();
}

/** Hold only the renderer acknowledgment, after the real Rust commit. */
async function holdAcknowledgment(page, command) {
  await page.evaluate(name => {
    const fetch = window.fetch;
    window.fetch = async (...args) => {
      const response = await fetch.apply(window, args);
      if (String(args[0]).endsWith(`/${name}`) && response.headers.get('Tauri-Response') === 'ok') {
        window.fetch = fetch;
        window.interruptedAck = { request: JSON.parse(args[1].body).request, response: await response.clone().json() };
        // Destroying this renderer abandons the promise. It never replays IPC.
        await new Promise(() => {});
      }
      return response;
    };
  }, command);
}

async function settledAck(page) {
  await page.waitForFunction(() => !!window.interruptedAck);
  return page.evaluate(() => window.interruptedAck);
}

async function qualifySaveRendererLoss() {
  const data = await mkdtemp(resolve(tmpdir(), 'wns-v3-interrupt-save-'));
  let run;
  try {
    run = await launch(data);
    const { page } = run;
    const created = await createProject(page, 'Interrupted save', 'save-chapter', 'After the storm', 'The opening survives.');
    const entry = (await invoke(page, 'library_snapshot')).entries.find(item => item.projectId === created.project.projectId);
    await assertOwnedEntry(entry, data);
    await page.reload();
    await openFixture(page, 'Interrupted save', 'After the storm');
    await holdAcknowledgment(page, 'save_snapshot');
    const committedText = 'This save committed before its renderer acknowledgment was lost.';
    await manuscript(page).fill(committedText);
    const held = await settledAck(page);
    assert.equal(readReceipt(entry.path, held.request.operationId).length, 1);
    assert.equal(readBody(entry.path, 'save-chapter').body_hash, held.response.head.bodyHash);
    assert.equal(await page.locator('.save-status').innerText(), 'Saving…');
    // CDP reload is forced renderer replacement, deliberately bypassing app
    // navigation. Controlled keyboard refresh is qualified separately.
    await page.reload();
    await openFixture(page, 'Interrupted save', 'After the storm');
    assert.equal(await manuscript(page).innerText(), committedText);
    const laterText = `${committedText} The author then wrote something newer.`;
    await manuscript(page).fill(laterText);
    await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
    const later = readBody(entry.path, 'save-chapter');
    assert(later.working_version > Number(held.response.head.version));
    const reconciled = await invoke(page, 'reconcile_document', { request: {
      projectId: created.project.projectId, operationNamespace: created.project.operationNamespace,
      session: 'after-renderer-loss', documentId: 'save-chapter', pendingOperationIds: [held.request.operationId],
    } });
    assert.equal(reconciled.receipts.length, 1);
    assert.deepEqual(reconciled.receipts[0].result.head, held.response.head);
    assert.equal(reconciled.document.head.bodyHash, later.body_hash, 'An old receipt must not replace later saved prose.');
    assert.notEqual(reconciled.access.writerLease, held.request.access.writerLease);
    // A delayed renderer is fenced even when it carries a previously valid
    // request. Execution-time lease validation precedes receipt reuse.
    const stale = await page.evaluate(async request => {
      try { await window.__TAURI_INTERNALS__.invoke('save_snapshot', { request }); return null; }
      catch (error) { return error; }
    }, held.request);
    assert.equal(stale?.code, 'WriterLeaseExpired');
    assert.deepEqual(readBody(entry.path, 'save-chapter'), later);
    assert.equal(readReceipt(entry.path, held.request.operationId).length, 1);
    await page.reload();
    await openFixture(page, 'Interrupted save', 'After the storm');
    assert.equal(await manuscript(page).innerText(), laterText);
    await capture(page, 'save-reloaded-with-newer-prose.png');
    return { summary: 'Forced renderer reload after a committed Save retains exact prose; reconciliation returns the old receipt and newer head separately, and rejects the retired writer lease.', dataDirectory: data };
  } finally { await run?.browser.close().catch(() => {}); await stopIfAlive(run?.app); }
}

async function qualifyApplyProcessLoss() {
  const data = await mkdtemp(resolve(tmpdir(), 'wns-v3-interrupt-apply-'));
  let run;
  try {
    run = await launch(data);
    let page = run.page;
    await chooseMock(page);
    const created = await createProject(page, 'Interrupted Apply', 'apply-chapter', 'The guarded ending', 'Mira held the lantern. The ending stays intact.');
    const entry = (await invoke(page, 'library_snapshot')).entries.find(item => item.projectId === created.project.projectId);
    await assertOwnedEntry(entry, data);
    await page.reload();
    await openFixture(page, 'Interrupted Apply', 'The guarded ending');
    await page.evaluate(() => document.querySelector('.tiptap').editor.commands.setTextSelection({ from: 1, to: 5 }));
    await page.getByRole('button', { name: 'Discuss selection', exact: true }).click();
    await page.getByRole('button', { name: 'Suggest edits', exact: true }).click();
    await page.getByRole('textbox', { name: 'Request edits for this passage', exact: true }).fill('Change who holds the lantern, preserving all other words.');
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    await page.waitForFunction(() => document.querySelectorAll('.proposal-card').length === 3);
    let chosen = page.locator('.proposal-card').filter({ hasText: 'Mock clarity option' });
    await chosen.getByRole('textbox', { name: 'Replacement wording', exact: true }).fill('Her sister');
    await chosen.getByRole('button', { name: 'Preview', exact: true }).click();
    await chosen.locator('.after-text').filter({ hasText: /^Her sister$/ }).waitFor();
    await holdAcknowledgment(page, 'apply_proposal');
    await chosen.getByRole('button', { name: 'Apply', exact: true }).click();
    const held = await settledAck(page);
    const receipt = readReceipt(entry.path, held.request.operationId);
    assert.equal(receipt.length, 1);
    assert.equal(receipt[0].operation_kind, 'apply');
    assert.equal(await manuscript(page).innerText(), 'Mira held the lantern. The ending stays intact.', 'Editor transaction must still await Rust acknowledgment.');
    const durable = readBody(entry.path, 'apply-chapter');
    assert.equal(durable.body_hash, held.response.result.head.bodyHash);
    const decision = held.response.result.applied;
    const decisionsBefore = inspect(entry.path, db => db.prepare('SELECT * FROM proposal_decisions ORDER BY id').all());
    assert.equal(decisionsBefore.length, 1);
    assert.equal(decisionsBefore[0].id, decision.decisionId);
    const beforeHistory = inspect(entry.path, db => db.prepare('SELECT id,body_hash FROM revisions WHERE id IN (?,?) ORDER BY id').all(decision.beforeRevisionId, decision.afterRevisionId));
    assert.equal(beforeHistory.length, 2, 'Both immutable revisions must exist before the acknowledgment.');
    const frozenBefore = inspect(entry.path, db => ({
      packets: db.prepare('SELECT * FROM context_packets ORDER BY id').all(),
      snapshots: db.prepare('SELECT * FROM story_snapshots ORDER BY id').all(),
      runs: db.prepare('SELECT id,status,packet_id FROM discussion_runs ORDER BY id').all(),
    }));
    await stopIfAlive(run.app); // Terminate only the child this fixture spawned.
    await run.browser.close();
    run = await launch(data); page = run.page;
    await openFixture(page, 'Interrupted Apply', 'The guarded ending');
    const appliedText = 'Her sister held the lantern. The ending stays intact.';
    assert.equal(await manuscript(page).innerText(), appliedText);
    chosen = page.locator('.proposal-card').filter({ hasText: 'Mock clarity option' });
    await chosen.locator('.proposal-status').filter({ hasText: /^Applied$/ }).waitFor();
    assert.equal(await page.locator('.proposal-stale').count(), 2);
    await manuscript(page).focus();
    await page.keyboard.press('Control+z');
    assert.equal(await manuscript(page).innerText(), appliedText, 'Durable history must not masquerade as an editor undo stack after restart.');
    assert.deepEqual(readReceipt(entry.path, held.request.operationId), receipt);
    assert.deepEqual(inspect(entry.path, db => db.prepare('SELECT * FROM proposal_decisions ORDER BY id').all()), decisionsBefore);
    const frozenAfter = inspect(entry.path, db => ({
      packets: db.prepare('SELECT * FROM context_packets ORDER BY id').all(),
      snapshots: db.prepare('SELECT * FROM story_snapshots ORDER BY id').all(),
      runs: db.prepare('SELECT id,status,packet_id FROM discussion_runs ORDER BY id').all(),
    }));
    assert.deepEqual(frozenAfter, frozenBefore, 'Restart must preserve exact frozen context/policy/epoch/packet records and not regenerate.');
    assert.deepEqual(inspect(entry.path, db => db.prepare('SELECT id,body_hash FROM revisions WHERE id IN (?,?) ORDER BY id').all(decision.beforeRevisionId, decision.afterRevisionId)), beforeHistory);
    await capture(page, 'apply-process-reopened.png');
    return { summary: 'Process termination after committed Apply but before editor acknowledgment reopens the exact applied body, one decision/receipt, immutable before/after revisions and frozen context; no durable undo stack or model replay is invented.', dataDirectory: data };
  } finally { await run?.browser.close().catch(() => {}); await stopIfAlive(run?.app); }
}

async function qualifyRunningRequestProcessLoss() {
  const data = await mkdtemp(resolve(tmpdir(), 'wns-v3-interrupt-request-'));
  const held = startHeldApi();
  await held.listen();
  let run;
  try {
    run = await launch(data);
    let page = run.page;
    await configureAnonymousEndpoint(page, held.server.address().port);
    const created = await createProject(page, 'Interrupted reply', 'reply-chapter', 'A frozen request', 'The pendant was left under the old bridge.');
    const entry = (await invoke(page, 'library_snapshot')).entries.find(item => item.projectId === created.project.projectId);
    await assertOwnedEntry(entry, data);
    await page.reload();
    await openFixture(page, 'Interrupted reply', 'A frozen request');
    await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).fill('Where was the pendant left? Keep the story unchanged.');
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    await waitForHeldRequest(held, 1);
    const before = inspect(entry.path, db => ({
      packets: db.prepare('SELECT * FROM context_packets ORDER BY id').all(),
      snapshots: db.prepare('SELECT * FROM story_snapshots ORDER BY id').all(),
      runs: db.prepare('SELECT id,project_id,operation_namespace,packet_id,status FROM discussion_runs').all(),
    }));
    assert.equal(before.runs.length, 1);
    assert.equal(before.runs[0].status, 'running');
    const prose = readBody(entry.path, 'reply-chapter');
    // Replacing only the renderer must leave native job ownership intact.
    await page.reload();
    await openFixture(page, 'Interrupted reply', 'A frozen request');
    assert.equal(held.requests.length, 1);
    assert.equal(held.requests[0].clientDisconnected, false);
    assert.equal(inspect(entry.path, db => db.prepare('SELECT status FROM discussion_runs').get()).status, 'running');
    await stopIfAlive(run.app);
    await run.browser.close();
    const disconnectDeadline = Date.now() + 5000;
    while (!held.requests[0].clientDisconnected && Date.now() < disconnectDeadline) await new Promise(resolvePromise => setTimeout(resolvePromise, 50));
    assert.equal(held.requests[0].clientDisconnected, true, 'Terminating the native owner must close its HTTP stream.');
    run = await launch(data); page = run.page;
    await openFixture(page, 'Interrupted reply', 'A frozen request');
    const after = inspect(entry.path, db => ({
      packets: db.prepare('SELECT * FROM context_packets ORDER BY id').all(),
      snapshots: db.prepare('SELECT * FROM story_snapshots ORDER BY id').all(),
      runs: db.prepare('SELECT id,project_id,operation_namespace,packet_id,status FROM discussion_runs').all(),
      providerResults: db.prepare('SELECT * FROM provider_results').all(),
    }));
    assert.equal(after.runs.length, 1);
    assert.deepEqual({ ...after.runs[0] }, { ...before.runs[0], status: 'interrupted' });
    assert.deepEqual(after.packets, before.packets);
    assert.deepEqual(after.snapshots, before.snapshots);
    assert.deepEqual(readBody(entry.path, 'reply-chapter'), prose);
    assert.equal(after.providerResults.length, 0, 'Missing terminal provider evidence must remain unknown after process loss.');
    assert.equal(held.requests.length, 1, 'Reopen must never resubmit the provider request.');
    await capture(page, 'interrupted-provider-reopened.png');
    return { summary: 'A held native HTTP reply survives forced renderer reload; process termination disconnects it, and reopen retains one interrupted run and the exact frozen packet/source/policy without resending or inventing terminal provider evidence.', dataDirectory: data, syntheticRequests: held.requests };
  } finally { await run?.browser.close().catch(() => {}); await stopIfAlive(run?.app); held.close(); }
}

async function qualifyDirtyRefreshKeys() {
  const data = await mkdtemp(resolve(tmpdir(), 'wns-v3-interrupt-shortcuts-'));
  let run;
  try {
    run = await launch(data);
    await createProject(run.page, 'Refresh shortcuts', 'shortcut-chapter', 'Keep this writing', 'The saved opening.');
    await run.page.reload();
    await openFixture(run.page, 'Refresh shortcuts', 'Keep this writing');
    const result = await qualifyRefreshShortcuts({ page: run.page, app: run.app, data, evidence });
    assert.equal(result.status, 'passed');
    assert.equal(result.shortcuts.length, 6);
    await capture(run.page, 'refresh-shortcuts-retained.png');
    return { summary: 'Six native refresh shortcuts preserve the dirty manuscript and renderer while its save is held before dispatch; ordinary saving completes after release.', dataDirectory: data, shortcuts: result.shortcuts };
  } finally { await run?.browser.close().catch(() => {}); await stopIfAlive(run?.app); }
}

async function qualifyNativeContextMenus() {
  const data = await mkdtemp(resolve(tmpdir(), 'wns-v3-interrupt-menus-'));
  let run;
  try {
    run = await launch(data);
    await createProject(run.page, 'Safe context menus', 'menu-chapter', 'A familiar writing menu', 'The lantern waited beside the old bridge.');
    await run.page.reload();
    await openFixture(run.page, 'Safe context menus', 'A familiar writing menu');
    const result = await qualifyContextMenu({ page: run.page, app: run.app, data, evidence });
    assert.equal(result.status, 'passed');
    return { summary: 'Native context menus exclude browser Reload, retain editing commands, and preserve the selected-text feedback menu.', dataDirectory: data, menus: result };
  } finally { await run?.browser.close().catch(() => {}); await stopIfAlive(run?.app); }
}

const info = await stat(executable);
const report = {
  startedAt: new Date().toISOString(), executable, executableLength: info.size,
  executableSha256: createHash('sha256').update(await readFile(executable)).digest('hex'),
  checks: [], fixtureRoots: fixtureDirectories, runtimeObservations, screenshots, liveModelCalls: 0,
  limitations: [
    'Save and Apply interruption occurs after a confirmed database commit; statement-level rollback and storage fault injection are separate core tests.',
    'Forced renderer reload uses CDP; this is not a physical renderer crash or application-controlled keyboard refresh.',
    'Unsaved debounce/composition buffers and in-memory provider chunks are not promised durable after forced process loss.',
    'The fixture configures no provider credentials and uses anonymous loopback HTTP; native startup may inspect locally installed CLI state, while installed-release and real-provider qualification remain separate.',
  ],
};
try {
  const qualifications = menuOnly
    ? [qualifyNativeContextMenus]
    : [qualifySaveRendererLoss, qualifyApplyProcessLoss, qualifyRunningRequestProcessLoss, qualifyDirtyRefreshKeys];
  for (const qualify of qualifications) {
    const result = await qualify();
    report.checks.push(result);
    console.log(result.summary);
  }
  assert(runtimeObservations.every(observation => observation.pageErrors.length === 0), 'Native renderer must not emit uncaught errors.');
  report.status = 'passed';
} catch (error) {
  report.status = 'failed'; report.failure = String(error?.stack ?? error); throw error;
} finally {
  for (const app of ownedApps) await stopIfAlive(app);
  report.finishedAt = new Date().toISOString();
  report.launchLogs = launchLogs.map(entry => ({ dataDirectory: entry.dataDirectory, tail: entry.read().slice(-8000) }));
  await writeFile(resolve(evidence, 'qualification.json'), JSON.stringify(report, null, 2));
}
console.log(JSON.stringify({ status: report.status, checks: report.checks.length, evidence }, null, 2));
