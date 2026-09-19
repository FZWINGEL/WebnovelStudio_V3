import { spawnOwned, stopOwned, markOwnedReady } from './owned-process.mjs';
import { recordCheck, initializeEvidence } from './native-evidence.mjs';
// Focused native normal-close qualification. Every project, stream, and
// credential in this file is synthetic and lives under a fresh temp directory.
import { chromium } from 'playwright-core';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { createServer as createNetServer } from 'node:net';
import { createServer as createHttpServer } from 'node:http';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const executable = process.env.WNS_V3_NATIVE_EXE
  ? resolve(process.env.WNS_V3_NATIVE_EXE)
  : resolve(root, 'target/debug/webnovel-desktop.exe');
await initializeEvidence(executable);
const evidence = resolve(root, '.local/native-results/app-close');
await mkdir(evidence, { recursive: true });
// The evidence directory is a record. A passing run must not leave the previous
// run's failure capture beside its own, where it reads as a failure that did
// not happen. Only the dirty-close failure pair is clearable — the neighbouring
// dirty-flush and close-work captures are deliberate passing-path evidence.
await rm(resolve(evidence, 'dirty-close-failure.png'), { force: true });
await rm(resolve(evidence, 'dirty-close-failure.txt'), { force: true });
const runtimeObservations = [];
const fixtureDirectories = [];
const launchLogs = [];
const screenshots = [];

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
  await mkdir(resolve(data, 'empty-codex-home'), { recursive: true });
  const app = spawnOwned(executable, [], {
    cwd: data,
    windowsHide: true,
    stdio: 'pipe',
    env: {
      ...process.env,
      CODEX_HOME: resolve(data, 'empty-codex-home'),
      WNS_V3_NATIVE_CDP_PORT: String(port),
      WNS_V3_TRIAL_WEBVIEW_DIR: resolve(data, 'webview'),
      WNS_V3_TEST_DATA_DIR: resolve(data, 'library'),
    },
  });
  app.stdout.on('data', chunk => { appLog += chunk; });
  app.stderr.on('data', chunk => { appLog += chunk; });
  launchLogs.push({ dataDirectory: data, read: () => appLog });
  const started = Date.now();
  let browser;
  while (Date.now() - started < 90_000) {
    if (app.exitCode !== null) throw new Error(`Native app exited before CDP (${app.exitCode}).\n${appLog}`);
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
  const initialProvider = await invoke(page, 'provider_state');
  if (initialProvider.settings.revision === '0') {
    await invoke(page, 'save_model_settings', {
      expectedRevision: initialProvider.settings.revision,
      active: { providerId: 'mock', modelId: 'mock-story-context', reasoning: null, serviceTier: null },
      favorites: [],
    });
    await page.reload();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  }
  // Fresh-library startup may have already admitted a catalog check. Let that
  // bounded check settle before this fixture asserts a no-job close; later
  // launches preserve the explicitly configured local/loopback provider.
  const discoveryDeadline = Date.now() + 60_000;
  while ((await invoke(page, 'provider_state')).codexConnection?.checking) {
    assert(Date.now() < discoveryDeadline, 'Synthetic startup discovery did not settle before close qualification.');
    await new Promise(resolvePromise => setTimeout(resolvePromise, 100));
  }
  let runtime = null;
  try { runtime = await invoke(page, 'runtime_info'); } catch { /* Report the launch failure below. */ }
  markOwnedReady(app);
  runtimeObservations.push({ dataDirectory: data, runtime, pageErrors });
  if (!fixtureDirectories.includes(data)) fixtureDirectories.push(data);
  return { app, browser, page, port, runtime, pageErrors, appLog: () => appLog };
}

async function stopIfAlive(app) { await stopOwned(app); }

async function requestNativeClose(app, data, onReady) {
  const helper = spawn('powershell.exe', [
    '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File',
    resolve(root, 'tests/native/native-app-close.ps1'),
    '-OwnerPid', String(app.pid),
    ...(onReady ? ['-WaitForSignal'] : []),
  ], { cwd: data, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  let output = '';
  let readyResolve;
  let readyReject;
  const ready = onReady ? new Promise((resolvePromise, reject) => { readyResolve = resolvePromise; readyReject = reject; }) : null;
  let stdoutRemainder = '';
  helper.stdout.on('data', chunk => {
    output += chunk;
    if (!ready) return;
    stdoutRemainder += chunk;
    const lines = stdoutRemainder.split(/\r?\n/);
    stdoutRemainder = lines.pop() ?? '';
    for (const line of lines) {
      if (!line.trim()) continue;
      try {
        const value = JSON.parse(line);
        if (value.ready === true) { readyResolve(value); return; }
      } catch { /* Wait for the complete JSON line. */ }
    }
  });
  helper.stderr.on('data', chunk => { output += chunk; });
  const codePromise = new Promise((resolvePromise, reject) => {
    const timeout = setTimeout(() => { helper.kill(); reject(new Error(`WM_CLOSE helper timed out.\n${output}`)); }, 15_000);
    helper.once('error', error => { clearTimeout(timeout); readyReject?.(error); reject(error); });
    helper.once('exit', exitCode => { clearTimeout(timeout); if (exitCode !== 0) readyReject?.(new Error(`WM_CLOSE helper failed (${exitCode}).\n${output}`)); resolvePromise(exitCode); });
  });
  if (ready) {
    await ready;
    try {
      await onReady();
    } catch (error) {
      helper.kill();
      throw error;
    }
    helper.stdin.end('close\n');
  }
  const code = await codePromise;
  if (code !== 0) throw new Error(`WM_CLOSE helper failed (${code}).\n${output}`);
  const jsonLines = output.trim().split(/\r?\n/).filter(Boolean).map(line => { try { return JSON.parse(line); } catch { return null; } }).filter(Boolean);
  return jsonLines.findLast(value => value.action === 'WM_CLOSE') ?? jsonLines.at(-1);
}

async function waitForExit(app, timeoutMs = 20_000) {
  if (app.exitCode !== null) return app.exitCode;
  return new Promise((resolvePromise, reject) => {
    const timeout = setTimeout(() => reject(new Error(`Native app PID ${app.pid} did not exit after normal close.`)), timeoutMs);
    app.once('exit', code => { clearTimeout(timeout); resolvePromise(code); });
  });
}

async function createProject(page, title, documentId, documentTitle, text) {
  const opened = await invoke(page, 'library_create', {
    operationId: `close-${title.toLowerCase().replaceAll(/[^a-z0-9]+/g, '-')}`,
    title,
    session: `close-${documentId}`,
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
    label: 'Synthetic held close API',
    baseUrl: `http://127.0.0.1:${apiPort}/close-api`,
    enabled: true,
    jsonMode: false,
    manualModelIds: ['test-editor-v1', 'gpt-6-astra'],
    apiKey: { kind: 'remove' },
  } });
  const profile = endpoint.profiles.find(item => item.label === 'Synthetic held close API');
  assert(profile, 'Anonymous close fixture endpoint must be persisted.');
  const provider = await invoke(page, 'provider_state');
  // Endpoint profile IDs already carry the openai-compatible: namespace.
  const providerId = profile.id;
  const author = await invoke(page, 'save_model_settings', {
    expectedRevision: provider.settings.revision,
    active: { providerId, modelId: 'test-editor-v1', reasoning: null, serviceTier: null },
    favorites: [],
  });
  assert.equal(author.settings.active.providerId, providerId);
  const memory = await invoke(page, 'save_story_memory_provider', {
    expectedRevision: author.storyMemory.revision,
    providerId,
  });
  assert.equal(memory.storyMemory.providerId, providerId);
  assert.equal(memory.storyMemory.ready, true, 'Anonymous synthetic endpoint must be usable for memory.');
  await page.reload();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  return { providerId };
}

function startHeldApi() {
  const requests = [];
  const server = createHttpServer((request, response) => {
    if (request.method === 'GET' && request.url?.endsWith('/models')) {
      response.writeHead(200, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify({ data: [{ id: 'test-editor-v1' }, { id: 'gpt-6-astra' }] }));
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
      // Deliberately leave the SSE response open. App close must cancel the
      // exact discussion and memory requests instead of replaying them.
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
  try {
    await page.screenshot({ path });
    screenshots.push(path);
  } catch { /* A successful native close can detach the page immediately. */ }
}

async function qualifyDirtyFlush() {
  const data = await mkdtemp(resolve(tmpdir(), 'wns-v3-close-dirty-'));
  let first;
  let second;
  try {
    first = await launch(data);
    const { page } = first;
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
    await createProject(page, 'Close dirty flush', 'close-dirty-chapter', 'The unsaved chapter', 'The durable opening.');
    const entry = (await invoke(page, 'library_snapshot')).entries.find(item => item.title === 'Close dirty flush');
    assert(entry, 'Dirty flush fixture must be indexed.');
    // Direct IPC fixture creation updates the native library, while the
    // already-mounted React Library list is intentionally not pushed. Reload
    // before clicking the newly indexed project.
    await page.reload();
    await page.getByRole('button', { name: /^Close dirty flush Last opened/ }).click();
    await page.getByRole('heading', { name: 'The unsaved chapter', exact: true }).waitFor();
    const manuscript = page.getByRole('textbox', { name: 'Manuscript', exact: true });
    const baselineDb = new DatabaseSync(resolve(entry.path, 'project.sqlite3'), { readOnly: true });
    const baseline = baselineDb.prepare('SELECT working_version, body_hash FROM documents WHERE id=?').get('close-dirty-chapter');
    assert(baseline, 'Dirty flush fixture must have a durable baseline before typing.');
    baselineDb.close();
    await requestNativeClose(first.app, data, async () => {
      // The helper has verified the real native window and is waiting on stdin.
      // Type only after that point, then release WM_CLOSE in the same turn so
      // the normal-close flush owns the still-dirty generation.
      await manuscript.fill('This text exists only in the live editor until normal close flushes it.');
      await page.waitForFunction(expected => document.querySelector('.tiptap')?.editor?.getText() === expected,
        'This text exists only in the live editor until normal close flushes it.');
      assert.equal(await page.locator('.save-status').innerText(), 'Saving…', 'The close fixture must request close while the editor is still dirty.');
      const beforeCloseDb = new DatabaseSync(resolve(entry.path, 'project.sqlite3'), { readOnly: true });
      const beforeClose = beforeCloseDb.prepare('SELECT working_version, body_hash FROM documents WHERE id=?').get('close-dirty-chapter');
      beforeCloseDb.close();
      assert.deepEqual(beforeClose, baseline, 'The typed body must remain unsaved until normal close starts.');
    });
    try { await waitForExit(first.app); }
    catch (error) {
      await capture(page, 'dirty-close-failure.png');
      await writeFile(resolve(evidence, 'dirty-close-failure.txt'), await page.locator('body').innerText().catch(() => 'Native page unavailable.'));
      throw error;
    }
    await first.browser.close(); first = undefined;

    second = await launch(data);
    const secondPage = second.page;
    await secondPage.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
    await secondPage.getByRole('button', { name: /^Close dirty flush Last opened/ }).click();
    await secondPage.getByRole('heading', { name: 'The unsaved chapter', exact: true }).waitFor();
    assert.equal(await secondPage.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(),
      'This text exists only in the live editor until normal close flushes it.');
    await capture(secondPage, 'dirty-flush-reopened.png');
    // The reopened app must also leave by normal close: a teardown kill counts
    // as a forced stop against this suite's process-cleanliness audit.
    await requestNativeClose(second.app, data);
    await waitForExit(second.app);
    await second.browser.close(); second = undefined;
    return {
      summary: 'Normal WM_CLOSE flushes a verified dirty live manuscript before a no-job close and the exact prose survives a fresh native reopen.',
      dataDirectory: data,
      runtime: first?.runtime ?? second?.runtime,
    };
  } finally {
    await first?.browser.close().catch(() => {}); await second?.browser.close().catch(() => {});
    await stopIfAlive(first?.app); await stopIfAlive(second?.app);
  }
}

async function qualifyHeldWorkClose() {
  const data = await mkdtemp(resolve(tmpdir(), 'wns-v3-close-work-'));
  const held = startHeldApi();
  await held.listen();
  let run;
  try {
    run = await launch(data);
    const { page } = run;
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
    const { providerId } = await configureAnonymousEndpoint(page, held.server.address().port);
    const first = await createProject(page, 'Close discussion project', 'close-discussion-chapter', 'The held reply', 'The discussion project body.');
    const second = await createProject(page, 'Close memory project', 'close-memory-chapter', 'The held memory', 'The story memory project body.');
    const entries = (await invoke(page, 'library_snapshot')).entries;
    const firstEntry = entries.find(item => item.title === 'Close discussion project');
    const secondEntry = entries.find(item => item.title === 'Close memory project');
    assert(firstEntry && secondEntry);
    await page.reload();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();

    await page.getByRole('button', { name: /^Close discussion project Last opened/ }).click();
    await page.getByRole('heading', { name: 'The held reply', exact: true }).waitFor();
    const discussion = page.getByRole('textbox', { name: 'Discuss this document', exact: true });
    if (!(await discussion.isVisible())) await page.getByRole('button', { name: 'Discussion', exact: true }).click();
    await discussion.fill('Keep this discussion request open for the close coordinator.');
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    await waitForHeldRequest(held, 1);
    assert.equal(held.requests[0].model, 'test-editor-v1');

    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await page.getByRole('button', { name: /^Close memory project Last opened/ }).click();
    await page.getByRole('heading', { name: 'The held memory', exact: true }).waitFor();
    await page.getByRole('button', { name: 'Story memory', exact: true }).click();
    await page.getByRole('button', { name: 'Refresh story memory', exact: true }).click();
    await waitForHeldRequest(held, 2);
    assert.equal(held.requests[1].model, 'gpt-6-astra');

    await requestNativeClose(run.app, data);
    await page.getByRole('heading', { name: 'Finish closing WebnovelStudio?', exact: true }).waitFor();
    assert.match(await page.locator('.app-close-dialog').innerText(), /AI replies or story memory refreshes|replies or story memory/);
    await capture(page, 'close-work-prompt.png');
    const requestsBeforeStayOpen = held.requests.length;
    await page.getByRole('button', { name: 'Stay open', exact: true }).click();
    await page.getByRole('heading', { name: 'The held memory', exact: true }).waitFor();
    assert.equal(held.requests.length, requestsBeforeStayOpen, 'Stay open must not replay either held request.');
    assert(held.requests.slice(0, 2).every(request => request.requestBodyComplete && request.responseStarted && request.partialContent && !request.clientDisconnected && !request.completed),
      `Stay open must preserve both held SSE responses: ${JSON.stringify(held.requests)}`);
    assert(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).isVisible(), 'Stay open must preserve the mounted editor.');

    await requestNativeClose(run.app, data);
    await page.getByRole('heading', { name: 'Finish closing WebnovelStudio?', exact: true }).waitFor();
    await page.getByRole('button', { name: 'Stop replies and close', exact: true }).click();
    await waitForExit(run.app);
    await run.browser.close(); run = undefined;

    const cancellationDeadline = Date.now() + 5000;
    while (held.requests.some(request => !request.clientDisconnected) && Date.now() < cancellationDeadline) {
      await new Promise(resolvePromise => setTimeout(resolvePromise, 50));
    }
    assert(held.requests.every(request => request.requestBodyComplete && request.partialContent && request.clientDisconnected),
      `Stop and close must cancel both partial SSE responses: ${JSON.stringify(held.requests)}`);

    const library = new DatabaseSync(resolve(data, 'library/library.sqlite3'), { readOnly: true });
    const paths = library.prepare('SELECT title, path FROM entries WHERE title IN (?, ?) ORDER BY title').all('Close discussion project', 'Close memory project');
    library.close();
    assert.equal(paths.length, 2);
    const outcomes = [];
    for (const entry of paths) {
      const db = new DatabaseSync(resolve(entry.path, 'project.sqlite3'), { readOnly: true });
      const discussionRows = db.prepare('SELECT outcome FROM provider_results ORDER BY rowid').all();
      const memoryRows = db.prepare('SELECT outcome FROM memory_results ORDER BY rowid').all();
      outcomes.push({ title: entry.title, discussion: discussionRows.map(row => row.outcome), memory: memoryRows.map(row => row.outcome) });
      db.close();
    }
    const discussionProject = outcomes.find(item => item.title === 'Close discussion project');
    const memoryProject = outcomes.find(item => item.title === 'Close memory project');
    assert(discussionProject && memoryProject);
    assert.equal(discussionProject.discussion.length, 1, 'Stopping close must persist one discussion outcome.');
    assert.equal(memoryProject.memory.length, 1, 'Stopping close must persist one memory outcome.');
    assert.equal(discussionProject.discussion[0], 'stopped');
    assert.equal(memoryProject.memory[0], 'stopped');
    assert.equal(held.requests.length, 2, 'Stopping close must not replay held provider requests.');
    return {
      summary: `Normal close showed work from both projects, Stay open preserved the editor without replay, and Stop and close settled one discussion (${discussionProject.discussion[0]}) plus one story-memory request (${memoryProject.memory[0]}) with exactly two provider requests.`,
      dataDirectory: data,
      runtime: runtimeObservations.find(observation => observation.dataDirectory === data)?.runtime ?? null,
    };
  } finally {
    await run?.browser.close().catch(() => {});
    await stopIfAlive(run?.app);
    held.close();
  }
}

const executableInfo = await stat(executable);
const executableSha256 = createHash('sha256').update(await readFile(executable)).digest('hex');
const report = {
  startedAt: new Date().toISOString(),
  executable,
  executableLength: executableInfo.size,
  executableSha256,
  checks: [],
  liveModelCalls: 0,
  fixtureRoots: fixtureDirectories,
  runtimeObservations,
  screenshots,
  limitations: [
    'Synthetic anonymous loopback SSE is used; no live provider or credential is touched.',
    'Native pending-result fault blocking is not covered by this fixture; core and frontend close regressions cover the coordinator boundary.',
  ],
};
try {
  const dirty = await qualifyDirtyFlush();
  recordCheck(report.checks, 'close:dirty-flush', dirty.summary);
  const heldWork = await qualifyHeldWorkClose();
  recordCheck(report.checks, 'close:held-work', heldWork.summary);
  report.fixtureResults = [dirty, heldWork];
  report.status = 'passed';
} catch (error) {
  report.status = 'failed';
  report.failure = String(error?.stack ?? error);
  throw error;
} finally {
  report.launchLogs = launchLogs.map(entry => ({ dataDirectory: entry.dataDirectory, tail: entry.read().slice(-8000) }));
  report.finishedAt = new Date().toISOString();
  await writeFile(resolve(evidence, 'qualification.json'), JSON.stringify(report, null, 2));
}
console.log(JSON.stringify(report, null, 2));
