import { spawnOwned, stopOwned, markOwnedReady } from './owned-process.mjs';
import { recordCheck, initializeEvidence } from './native-evidence.mjs';
// Native synthetic qualification for backup/recovery isolation. Project A is
// backed up through the real native Save dialog, while project B owns a held
// loopback SSE discussion. A is then recovered as a new project through the
// real native Open dialog without redirecting or replaying B's work.
import { chromium } from 'playwright-core';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, readFile, realpath, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, relative, sep, isAbsolute, toNamespacedPath } from 'node:path';
import { createServer as createNetServer } from 'node:net';
import { createServer as createHttpServer } from 'node:http';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const executable = process.env.WNS_V3_NATIVE_EXE
  ? resolve(process.env.WNS_V3_NATIVE_EXE)
  : resolve(root, 'target/debug/webnovel-desktop.exe');
await initializeEvidence(executable);
const evidence = resolve(root, '.local/native-results/project-recovery');
await mkdir(evidence, { recursive: true });

function inside(rootPath, childPath) {
  const path = relative(toNamespacedPath(rootPath), toNamespacedPath(childPath));
  return path && !isAbsolute(path) && path !== '..' && !path.startsWith(`..${sep}`);
}

function invoke(page, command, args) {
  return page.evaluate(([name, input]) => input === undefined
    ? window.__TAURI_INTERNALS__.invoke(name)
    : window.__TAURI_INTERNALS__.invoke(name, input), [command, args]);
}

async function reservePort() {
  const server = createNetServer();
  await new Promise(resolvePromise => server.listen(0, '127.0.0.1', resolvePromise));
  const port = server.address().port;
  await new Promise(resolvePromise => server.close(resolvePromise));
  return port;
}

async function launch(data, logs) {
  const port = await reservePort();
  let log = '';
  const app = spawnOwned(executable, [], {
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
  let spawnError;
  app.once('error', error => { spawnError = error; log += `\n${error.stack ?? error}`; });
  app.stdout.on('data', chunk => { log += chunk; });
  app.stderr.on('data', chunk => { log += chunk; });
  logs.push({ data, read: () => log });
  let browser;
  try {
    const deadline = Date.now() + 90_000;
    while (Date.now() < deadline) {
      if (spawnError || app.exitCode !== null) throw new Error(`Native app exited before CDP (${app.exitCode}).\n${log}`);
      try {
        const response = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(2000) });
        if (response.ok && (await response.json()).webSocketDebuggerUrl) {
          browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 10_000 });
          break;
        }
      } catch { /* WebView2 is still starting. */ }
      await new Promise(resolvePromise => setTimeout(resolvePromise, 250));
    }
    if (!browser) throw new Error(`Native app did not expose CDP.\n${log}`);
    const context = browser.contexts()[0];
    const page = context.pages()[0] ?? await context.waitForEvent('page', { timeout: 10_000 });
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor({ timeout: 30_000 });
    const pageErrors = [];
    page.on('pageerror', error => pageErrors.push(error.message));
    const runtime = await invoke(page, 'runtime_info');
  markOwnedReady(app);
    assert.equal(runtime.host, 'Tauri');
    assert.equal(runtime.persistence, true);
    return { app, browser, page, runtime, pageErrors, data };
  } catch (error) {
    await browser?.close().catch(() => {});
    await stopIfAlive(app);
    throw error;
  }
}

async function stopIfAlive(app) { await stopOwned(app); }

async function stopOwnedHelper(helper) {
  if (!helper || helper.exitCode !== null) return;
  helper.kill();
  await new Promise(resolvePromise => {
    const timeout = setTimeout(resolvePromise, 5000);
    helper.once('exit', () => { clearTimeout(timeout); resolvePromise(); });
  });
}

async function operateBackupDialog(app, data, action, destination, title) {
  const helper = spawn('powershell.exe', [
    '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File',
    resolve(root, 'apps/desktop/scripts/native-backup-dialog.ps1'),
    '-OwnerPid', String(app.pid), '-Action', action, '-TestRoot', data,
    '-Destination', destination, '-DialogTitle', title,
  ], { cwd: data, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  let output = '';
  let timeout;
  helper.stdout.on('data', chunk => { output += chunk; });
  helper.stderr.on('data', chunk => { output += chunk; });
  // Keep late spawn/termination errors handled while the owned helper is
  // being fenced and awaited below.
  helper.on('error', () => {});
  let code;
  try {
    code = await new Promise((resolvePromise, reject) => {
      timeout = setTimeout(() => reject(new Error('The owned native backup dialog helper timed out.')), 30_000);
      helper.once('error', reject);
      helper.once('exit', resolvePromise);
    });
  } finally {
    clearTimeout(timeout);
    await stopOwnedHelper(helper);
  }
  if (code !== 0) throw new Error(`Native backup dialog action failed (${code}).\n${output}`);
  return output.trim();
}

async function createProject(page, title, documentId, documentTitle, text, prefix) {
  const opened = await invoke(page, 'library_create', {
    operationId: `${prefix}-${title.toLowerCase().replaceAll(/[^a-z0-9]+/g, '-')}`,
    title,
    session: `${prefix}-${documentId}`,
  });
  const document = await invoke(page, 'create_document', { request: {
    access: opened.access,
    operationId: `${prefix}-create-${documentId}`,
    documentId,
    title: documentTitle,
    kind: 'chapter',
    body: { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: `${documentId}-paragraph` }, content: [{ type: 'text', text }] }] } },
  } });
  return { ...opened, documents: [document] };
}

function startHeldApi() {
  const requests = [];
  const server = createHttpServer((request, response) => {
    if (request.method === 'GET' && request.url?.endsWith('/models')) {
      response.writeHead(200, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify({ data: [{ id: 'recovery-held-v1' }] }));
      return;
    }
    if (request.method !== 'POST' || !request.url?.endsWith('/chat/completions')) {
      response.writeHead(404); response.end(); return;
    }
    const entry = { number: requests.length + 1, model: null, requestBodyComplete: false, responseStarted: false, partialContent: false, clientDisconnected: false, completed: false };
    requests.push(entry);
    const chunks = [];
    request.on('data', chunk => chunks.push(chunk));
    request.on('end', () => {
      try { entry.model = JSON.parse(Buffer.concat(chunks).toString()).model; } catch { entry.model = 'unknown'; }
      entry.requestBodyComplete = true;
      response.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache', Connection: 'keep-alive' });
      entry.responseStarted = true;
      response.write(`data: ${JSON.stringify({ model: entry.model, choices: [{ index: 0, delta: { role: 'assistant', content: 'Held recovery response. ' } }] })}\n\n`);
      entry.partialContent = true;
      // Keep the stream open until the fixture explicitly stops project B.
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

async function configureEndpoint(page, port) {
  const endpointSettings = await invoke(page, 'endpoint_settings');
  const saved = await invoke(page, 'save_endpoint_settings', { request: {
    expectedRevision: endpointSettings.revision,
    profileId: null,
    label: 'Synthetic recovery held API',
    baseUrl: `http://127.0.0.1:${port}/recovery-api`,
    enabled: true,
    jsonMode: false,
    manualModelIds: ['recovery-held-v1'],
    apiKey: { kind: 'remove' },
  } });
  const profile = saved.profiles.find(item => item.label === 'Synthetic recovery held API');
  assert(profile, 'Synthetic recovery endpoint must be persisted.');
  const provider = await invoke(page, 'provider_state');
  const active = await invoke(page, 'save_model_settings', { expectedRevision: provider.settings.revision,
    active: { providerId: profile.id, modelId: 'recovery-held-v1', reasoning: null, serviceTier: null }, favorites: [] });
  assert.equal(active.settings.active.providerId, profile.id);
  await page.reload();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  return { providerId: profile.id, modelSelection: { providerId: profile.id, modelId: 'recovery-held-v1', reasoning: null, serviceTier: null } };
}

async function waitForHeld(requests) {
  const deadline = Date.now() + 15_000;
  while ((requests.length < 1 || !requests[0].requestBodyComplete || !requests[0].model || !requests[0].responseStarted || !requests[0].partialContent)
    && Date.now() < deadline) {
    await new Promise(resolvePromise => setTimeout(resolvePromise, 50));
  }
  assert.equal(requests.length, 1, `Expected one held provider request, got ${JSON.stringify(requests)}`);
  assert(requests[0].requestBodyComplete && requests[0].model && requests[0].responseStarted && requests[0].partialContent,
    `Held provider request did not reach partial SSE state: ${JSON.stringify(requests)}`);
}

async function waitForRun(dbPath, runId, predicate) {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const db = new DatabaseSync(dbPath, { readOnly: true });
    db.exec('PRAGMA busy_timeout=5000');
    const row = db.prepare('SELECT id, project_id, operation_namespace, status, dispatch_state, target_document_id, output_text FROM discussion_runs WHERE id=?').get(runId);
    db.close();
    if (row && predicate(row)) return row;
    await new Promise(resolvePromise => setTimeout(resolvePromise, 50));
  }
  throw new Error(`Discussion run ${runId} did not reach the expected state.`);
}

function snapshotProject(projectPath, documentId) {
  const db = new DatabaseSync(resolve(projectPath, 'project.sqlite3'), { readOnly: true });
  try {
    db.exec('PRAGMA busy_timeout=5000');
    const project = db.prepare('SELECT id, operation_namespace, title, format_version FROM project WHERE singleton=1').get();
    const document = db.prepare('SELECT id, title, kind, working_version, body_json, body_hash, last_checkpoint_id FROM documents WHERE id=?').get(documentId);
    const revisions = db.prepare('SELECT id, source_working_version, body_json, body_hash, parent_id, reason FROM revisions WHERE document_id=? ORDER BY source_working_version,id').all(documentId);
    const runs = db.prepare('SELECT id, project_id, operation_namespace, status, dispatch_state, target_document_id, output_text FROM discussion_runs ORDER BY rowid').all();
    const providerResults = db.prepare('SELECT run_id, packet_id, outcome, assistant_text, cleanup, error FROM provider_results ORDER BY rowid').all();
    const contextPackets = db.prepare('SELECT id, project_id, operation_namespace, operation_id, snapshot_id, packet_hash, input_hash FROM context_packets ORDER BY rowid').all();
    const storySnapshots = db.prepare('SELECT id, project_id, operation_namespace, operation_id, context_source_epoch, disclosure_policy_epoch, manifest_hash FROM story_snapshots ORDER BY rowid').all();
    const snapshotSources = db.prepare('SELECT snapshot_id, handle, document_id, revision_id, body_hash FROM snapshot_sources ORDER BY rowid').all();
    assert(project && document, `Project snapshot is missing ${projectPath}`);
    return { project, document, revisions, runs, providerResults, contextPackets, storySnapshots, snapshotSources };
  } finally {
    db.close();
  }
}

async function main() {
  const executableInfo = await stat(executable);
  const executableSha256 = createHash('sha256').update(await readFile(executable)).digest('hex');
  const data = await mkdtemp(resolve(tmpdir(), 'wns-v3-project-recovery-'));
  const ownedRoot = await realpath(data);
  const held = startHeldApi();
  const port = await held.listen();
  const logs = [];
  let run;
  const checks = [];
  try {
    run = await launch(data, logs);
    const { page } = run;
    const { providerId, modelSelection } = await configureEndpoint(page, port);
    const projectA = await createProject(page, 'Recovery source A', 'recovery-a-chapter', 'A chapter', 'Project A keeps this exact prose and history.', 'recovery');
    const projectB = await createProject(page, 'Active work B', 'recovery-b-chapter', 'B chapter', 'Project B remains active while A is recovered.', 'recovery');
    const libraryBefore = await invoke(page, 'library_snapshot');
    const entryA = libraryBefore.entries.find(item => item.projectId === projectA.project.projectId);
    const entryB = libraryBefore.entries.find(item => item.projectId === projectB.project.projectId);
    assert(entryA && entryB, 'Both source projects must be indexed in the owned library.');
    // Node's temporary root may use a Windows short-path alias while Rust
    // returns a canonical long path. Compare both resolved directory identities.
    const ownedLibrary = await realpath(resolve(data, 'library'));
    assert(inside(ownedLibrary, await realpath(entryA.path)) && inside(ownedLibrary, await realpath(entryB.path)), 'Source projects must stay inside the synthetic library.');
    await invoke(page, 'checkpoint_document', { request: { access: projectA.access, expected: projectA.documents[0].head, reason: 'manual' } });
    const beforeA = snapshotProject(entryA.path, 'recovery-a-chapter');
    assert(beforeA.revisions.length >= 1, 'Project A must have an immutable checkpoint before backup.');
    const started = await invoke(page, 'start_discussion', { request: {
      access: projectB.access,
      operationId: 'recovery-held-discussion-b',
      expected: projectB.documents[0].head,
      instruction: 'Keep this synthetic discussion open while project A is recovered.',
      scope: null,
      intent: 'discuss',
      basis: null,
      pinnedDocumentIds: [],
      safeBrief: null,
      lookup: null,
      budget: { modelId: 'mock-story-context', contextWindowTokens: '200000', reservedOutputTokens: '4096', reservedProtocolTokens: '1024' },
      previousRunId: null,
    }, modelSelection });
    assert.equal(started.run.owner.projectId, projectB.project.projectId);
    assert.equal(started.run.owner.operationNamespace, projectB.project.operationNamespace);
    await waitForHeld(held.requests);
    const bDbPath = resolve(entryB.path, 'project.sqlite3');
    // HTTP workers keep dispatch_state=claimed while an SSE response is
    // still open; the terminal transition marks the request delivered. The
    // durable running status plus partial output is the active-work proof.
    await waitForRun(bDbPath, started.run.id, row => row.status === 'running' && row.output_text.length > 0);
    const activeB = snapshotProject(entryB.path, 'recovery-b-chapter');
    assert.equal(activeB.runs.length, 1);
    assert.equal(activeB.runs[0].status, 'running');
    assert.equal(activeB.providerResults.length, 0);
    assert(activeB.contextPackets.some(packet => packet.id === started.run.packetId), 'B packet must be durably retained before recovery.');
    recordCheck(checks, 'native-project-recovery:01', 'Project B reached one running held SSE request with durable partial output before backup/recovery');

    const backupPath = resolve(data, 'backup', 'recovery-source-a.wnsbackup');
    await mkdir(resolve(data, 'backup'), { recursive: true });
    await page.evaluate(access => {
      window.__nativeRecoveryBackup = window.__TAURI_INTERNALS__.invoke('project_backup', { access });
    }, projectA.access);
    await operateBackupDialog(run.app, data, 'Save', backupPath, 'Save project backup');
    const backupResult = await page.evaluate(() => window.__nativeRecoveryBackup);
    assert.equal(await realpath(backupResult), await realpath(backupPath));
    const backupStat = await stat(backupPath);
    assert(backupStat.size > 0, 'Native project backup must create a non-empty archive.');
    assert.equal(held.requests.length, 1, 'Backing up A must not replay B provider work.');
    recordCheck(checks, 'native-project-recovery:02', 'Project A backup completed through the PID-owned native Save dialog while B remained active');

    const recoveryTitle = 'Recovered source A';
    await page.evaluate(({ operationId, title }) => {
      window.__nativeRecoveryOpen = window.__TAURI_INTERNALS__.invoke('library_recover', { operationId, title, session: 'recovered-source-a' });
    }, { operationId: 'recovery-a-from-backup', title: recoveryTitle });
    await operateBackupDialog(run.app, data, 'Open', backupPath, 'Recover a backup as a new project');
    const recovered = await page.evaluate(() => window.__nativeRecoveryOpen);
    assert(recovered, 'Native backup recovery must return an opened recovered project.');
    assert.notEqual(recovered.project.projectId, projectA.project.projectId);
    assert.notEqual(recovered.project.projectId, projectB.project.projectId);
    assert.notEqual(recovered.project.operationNamespace, projectA.project.operationNamespace);
    assert.notEqual(recovered.project.operationNamespace, projectB.project.operationNamespace);
    assert.equal(recovered.project.title, recoveryTitle);
    assert.equal(recovered.documents.length, 1);
    assert.deepEqual(recovered.documents[0].body, projectA.documents[0].body);
    assert.deepEqual(recovered.documents[0].head, projectA.documents[0].head);
    assert.equal(held.requests.length, 1, 'Recovering A must not submit a second provider request for B.');
    const libraryAfter = await invoke(page, 'library_snapshot');
    assert.equal(libraryAfter.entries.length, 3, 'Recovery must retain A and B and add one independent entry.');
    const entryRecovered = libraryAfter.entries.find(item => item.projectId === recovered.project.projectId);
    assert(entryRecovered, 'Recovered project must be registered in the native library.');
    assert(inside(ownedLibrary, await realpath(entryRecovered.path)), 'Recovered project must stay inside the synthetic library.');
    const afterA = snapshotProject(entryA.path, 'recovery-a-chapter');
    const recoveredSnapshot = snapshotProject(entryRecovered.path, 'recovery-a-chapter');
    const afterB = snapshotProject(entryB.path, 'recovery-b-chapter');
    assert.deepEqual(afterA.document, beforeA.document, 'Recovery must not mutate source A document head/body.');
    assert.deepEqual(afterA.revisions, beforeA.revisions, 'Recovery must not mutate source A history.');
    assert.equal(afterA.runs.length, 0, 'Source A must not receive project B runs.');
    assert.equal(afterA.providerResults.length, 0, 'Source A must not receive project B provider results.');
    assert.equal(recoveredSnapshot.document.body_json, beforeA.document.body_json);
    assert.deepEqual(recoveredSnapshot.revisions, beforeA.revisions, 'Recovered A must retain source revision history.');
    assert.equal(recoveredSnapshot.runs.length, 0, 'Recovered A must not receive project B runs.');
    assert.equal(recoveredSnapshot.providerResults.length, 0, 'Recovered A must not receive project B provider results.');
    assert.deepEqual(afterB.document, activeB.document, 'Recovery must not change project B document state after its discussion source checkpoint.');
    assert.deepEqual(afterB.revisions, activeB.revisions, 'Recovery must not change project B revision history after its discussion source checkpoint.');
    assert.equal(afterB.runs[0].id, started.run.id);
    assert.equal(afterB.runs[0].project_id, projectB.project.projectId);
    assert.equal(afterB.runs[0].operation_namespace, projectB.project.operationNamespace);
    assert.equal(afterB.runs[0].status, 'running');
    assert.equal(afterB.providerResults.length, 0);
    assert.deepEqual(afterB.contextPackets, activeB.contextPackets, 'Recovery must not change B context packets.');
    assert.deepEqual(afterB.storySnapshots, activeB.storySnapshots, 'Recovery must not change B story snapshots.');
    assert.deepEqual(afterB.snapshotSources, activeB.snapshotSources, 'Recovery must not change B snapshot sources.');
    assert.equal(afterB.contextPackets.find(packet => packet.id === started.run.packetId)?.project_id, projectB.project.projectId);
    recordCheck(checks, 'native-project-recovery:03', 'Recovery returned A with a fresh identity and exact document/history, retained source A/B, and left B running with no replay or result redirect');

    await invoke(page, 'stop_discussion', { access: projectB.access, runId: started.run.id });
    await waitForRun(bDbPath, started.run.id, row => row.status === 'stopped');
    const stoppedB = snapshotProject(entryB.path, 'recovery-b-chapter');
    assert.equal(stoppedB.providerResults.length, 1);
    assert.equal(stoppedB.providerResults[0].outcome, 'stopped');
    assert.equal(stoppedB.providerResults[0].packet_id, started.run.packetId);
    assert.equal(stoppedB.runs[0].status, 'stopped');
    assert.deepEqual(stoppedB.contextPackets, activeB.contextPackets, 'Stopping B must not mutate its frozen context packet.');
    assert.deepEqual(stoppedB.storySnapshots, activeB.storySnapshots, 'Stopping B must not mutate its frozen story snapshot.');
    assert.deepEqual(stoppedB.snapshotSources, activeB.snapshotSources, 'Stopping B must not mutate its frozen snapshot sources.');
    const stopDeadline = Date.now() + 5000;
    while (!held.requests[0].clientDisconnected && Date.now() < stopDeadline) await new Promise(resolvePromise => setTimeout(resolvePromise, 50));
    assert(held.requests[0].clientDisconnected, 'Stopping B must close the exact held SSE connection.');
    assert.equal(held.requests.length, 1, 'Stopping B must not replay the provider request.');
    recordCheck(checks, 'native-project-recovery:04', 'Project B stopped once after recovery, persisted one stopped outcome, closed its held SSE, and did not replay');
    assert.deepEqual(run.pageErrors, [], 'Native recovery fixture must produce no page errors.');
    return { status: 'passed', checks, dataDirectory: data, runtime: run.runtime, pageErrors: run.pageErrors, syntheticProviderRequests: held.requests.length, backupPath, sourceProjectId: projectA.project.projectId, recoveredProjectId: recovered.project.projectId, activeProjectId: projectB.project.projectId };
  } finally {
    await run?.browser.close().catch(() => {});
    await stopIfAlive(run?.app);
    held.close();
  }
}

const startedAt = new Date().toISOString();
let report;
try {
  report = await main();
} catch (error) {
  report = { status: 'failed', failure: String(error?.stack ?? error) };
  throw error;
} finally {
  report = {
    startedAt,
    executable,
    executableLength: (await stat(executable)).size,
    executableSha256: createHash('sha256').update(await readFile(executable)).digest('hex'),
    ...report,
    finishedAt: new Date().toISOString(),
    limitations: [
      'The fixture configures no provider credentials and uses loopback SSE with a fresh temporary library; native startup may inspect locally installed CLI state.',
      'This fixture validates one active discussion in project B; it does not qualify multiple simultaneous jobs or app-close interruption.',
    ],
  };
  await writeFile(resolve(evidence, 'qualification.json'), JSON.stringify(report, null, 2));
}
console.log(JSON.stringify(report, null, 2));
