import { spawnOwned, stopOwned, markOwnedReady } from './owned-process.mjs';
import { recordCheck, initializeEvidence } from './native-evidence.mjs';
// Synthetic native Settings qualification. This flow never starts a writing,
// Workshop, memory, or provider request; it only persists and reloads the
// Codex transport preference through the real Tauri/WebView2 UI and IPC.
import assert from 'node:assert/strict';
import { chromium } from 'playwright-core';
import { createHash } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import { mkdtemp, mkdir, realpath, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { createServer } from 'node:net';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const executable = process.env.WNS_V3_NATIVE_EXE
  ? resolve(process.env.WNS_V3_NATIVE_EXE)
  : resolve(root, 'target/debug/webnovel-desktop.exe');
const output = resolve(root, '.local/native-results/app-server-transport');
const data = await mkdtemp(resolve(tmpdir(), 'wns-v3-app-server-transport-'));
await mkdir(output, { recursive: true });
// The output directory is evidence. A passing run must not leave the previous
// run's failure artifacts beside its own report, where they read as a failure
// that did not happen.
await rm(resolve(output, 'failure.json'), { force: true });
await rm(resolve(output, 'failure.txt'), { force: true });
// This settings-only qualification has no access to an author's Codex login.
await mkdir(resolve(data, 'empty-codex-home'));
await initializeEvidence(executable);

const report = {
  startedAt: new Date().toISOString(),
  executable,
  executableBytes: (await readFile(executable)).byteLength,
  executableSha256: createHash('sha256').update(await readFile(executable)).digest('hex'),
  dataDirectory: data,
  checks: [],
  launches: [],
};

let browser;
let page;
let app;
let database;
let appLog = '';
let pageErrors = [];
const allPageErrors = [];

function invoke(command, args) {
  return page.evaluate(([name, value]) => value === undefined
    ? window.__TAURI_INTERNALS__.invoke(name)
    : window.__TAURI_INTERNALS__.invoke(name, value), [command, args]);
}

async function reservePort() {
  const server = createServer();
  await new Promise(resolvePromise => server.listen(0, '127.0.0.1', resolvePromise));
  const port = server.address().port;
  await new Promise(resolvePromise => server.close(resolvePromise));
  return port;
}

async function waitFor(description, check, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError = '';
  while (Date.now() < deadline) {
    try {
      const value = await check();
      if (value) return value;
    } catch (error) {
      lastError = String(error);
    }
    await new Promise(resolvePromise => setTimeout(resolvePromise, 250));
  }
  throw new Error(`Timed out waiting for ${description}${lastError ? `: ${lastError}` : ''}`);
}

async function launch(label) {
  const port = await reservePort();
  appLog = '';
  pageErrors = [];
  app = spawnOwned(executable, [], {
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
  app.stdout.on('data', chunk => { appLog += chunk; if (appLog.length > 64_000) appLog = appLog.slice(-64_000); });
  app.stderr.on('data', chunk => { appLog += chunk; if (appLog.length > 64_000) appLog = appLog.slice(-64_000); });

  await waitFor(`${label} WebView2 CDP`, async () => {
    if (app.exitCode !== null) throw new Error(`Native app exited (${app.exitCode}): ${appLog}`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(1_000) });
      return response.ok && (await response.json()).webSocketDebuggerUrl;
    } catch {
      return false;
    }
  }, 90_000);
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 10_000 });
  const context = browser.contexts()[0];
  page = context.pages()[0] ?? await context.waitForEvent('page', { timeout: 10_000 });
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor({ timeout: 30_000 });
  page.on('pageerror', error => {
    pageErrors.push(error.message);
    allPageErrors.push({ label, message: error.message });
  });
  markOwnedReady(app);
  report.launches.push({ label, port, runtime: await invoke('runtime_info') });
}

async function stopCurrent() {
  database?.close();
  database = undefined;
  await browser?.close().catch(() => {});
  browser = undefined;
  page = undefined;
  if (app) await stopOwned(app);
  app = undefined;
}

function selectionOf(state) {
  return state.settings.active;
}

function assertNoProviderWork(label) {
  const counts = {
    discussionRuns: Number(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n),
    memoryJobs: Number(database.prepare('SELECT count(*) AS n FROM memory_jobs').get().n),
  };
  assert.deepEqual(counts, { discussionRuns: 0, memoryJobs: 0 }, `${label} must not create provider work`);
  return counts;
}

try {
  await launch('initial');
  const initialTransport = await invoke('codex_transport_settings');
  assert.equal(initialTransport.transport, 'exec', 'A fresh synthetic library must default to Codex exec');
  assert.equal(initialTransport.revision, '0');

  // Wait for the offline/reference catalog before choosing the author model.
  await waitFor('GPT-5.6-Luna in the saved catalog', async () => {
    const state = await invoke('provider_state');
    return state.catalog.models.some(model => model.key.providerId === 'codex' && model.key.modelId === 'gpt-5.6-luna');
  });

  // Persist the local test choice before creating the synthetic project. The
  // native provider catalog may finish its offline Codex discovery after the
  // first renderer load; an explicit nonzero settings revision makes this
  // qualification independent of that startup timing and still performs no
  // generation.
  await page.getByRole('button', { name: /^Choose model:/ }).click();
  await page.locator('.model-choice').filter({ hasText: 'Local test model' }).click();
  await page.getByRole('button', { name: 'Choose model: Local test model', exact: true }).waitFor();
  assert.notEqual((await invoke('provider_state')).settings.revision, '0');

  const created = await invoke('library_create', {
    operationId: 'app-server-transport-smoke-create',
    title: 'App-server transport smoke project',
    session: 'app-server-transport-smoke-session',
  });
  assert(created?.project?.projectId, 'Synthetic project must be created through the library command');
  const library = await invoke('library_snapshot');
  const entry = library.entries.find(item => item.title === 'App-server transport smoke project');
  assert(entry, 'Synthetic project must be indexed');
  const projectPath = await realpath(entry.path);
  database = new DatabaseSync(resolve(projectPath, 'project.sqlite3'), { readOnly: true });
  assertNoProviderWork('Initial settings state');

  // Choose the author model only after the synthetic project exists and the
  // explicit local choice has fenced startup discovery.
  await page.getByRole('button', { name: /^Choose model:/ }).click();
  await page.locator('#model-search').fill('gpt-5.6-luna');
  await page.locator('.model-choice').filter({ hasText: 'GPT-5.6-Luna' }).click();
  await page.getByRole('button', { name: 'Choose model: GPT-5.6-Luna', exact: true }).waitFor();
  await page.getByRole('combobox', { name: 'Reasoning effort', exact: true }).selectOption('xhigh');
  await waitFor('Extra high reasoning to persist', async () => (await invoke('provider_state')).settings.active.reasoning === 'xhigh');
  await page.getByRole('combobox', { name: 'Service tier', exact: true }).selectOption('priority');
  await waitFor('Fast service tier to persist', async () => (await invoke('provider_state')).settings.active.serviceTier === 'priority');

  let state = await invoke('provider_state');
  const preservedSelection = selectionOf(state);
  assert.deepEqual(preservedSelection, {
    providerId: 'codex', modelId: 'gpt-5.6-luna', reasoning: 'xhigh', serviceTier: 'priority',
  });
  assert.equal(state.codexConnection?.ready, false, 'The synthetic run must not claim a live Codex connection');
  recordCheck(report.checks, 'native-app-server-smoke:01', 'The author model, Extra high reasoning, and Fast service tier are saved before transport changes while live readiness remains false');

  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const transport = page.locator('#codex-transport');
  await transport.waitFor();
  assert.equal(await transport.inputValue(), 'exec');
  await transport.selectOption('appServer');
  await waitFor('app-server preference to persist', async () => (await invoke('codex_transport_settings')).transport === 'appServer');
  assert.equal(await transport.inputValue(), 'appServer');
  await page.screenshot({ path: resolve(output, 'settings-app-server.png') });

  state = await invoke('provider_state');
  assert.deepEqual(selectionOf(state), preservedSelection, 'Changing transport must preserve the author picker selection and traits');
  assert.equal(state.codexConnection?.ready, false, 'Selecting app-server must not fabricate a connected server');
  assert.equal(state.dispatch.kind, 'blocked', 'Unqualified app-server transport must remain blocked');
  assert((state.codexConnection?.detail ?? '').length > 0, 'Unavailable transport must explain its state');
  assertNoProviderWork('After selecting app-server');
  recordCheck(report.checks, 'native-app-server-smoke:02', 'Settings saves App-server without generation, preserves the author binding, and reports qualification required instead of fake readiness');

  await page.getByRole('button', { name: 'Close settings', exact: true }).click();
  await stopCurrent();
  await launch('restart');
  database = new DatabaseSync(resolve(projectPath, 'project.sqlite3'), { readOnly: true });
  const afterRestartTransport = await invoke('codex_transport_settings');
  assert.equal(afterRestartTransport.transport, 'appServer', 'App-server preference must survive a synthetic app restart');
  state = await invoke('provider_state');
  assert.deepEqual(selectionOf(state), preservedSelection, 'The author model and traits must survive restart');
  assert.equal(state.codexConnection?.ready, false, 'Restart must not turn an unqualified app-server into a connected state');
  assert.equal(state.dispatch.kind, 'blocked');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.locator('#codex-transport').waitFor();
  assert.equal(await page.locator('#codex-transport').inputValue(), 'appServer');
  await page.screenshot({ path: resolve(output, 'settings-after-restart.png') });
  recordCheck(report.checks, 'native-app-server-smoke:03', 'A second synthetic native launch restores the App-server preference and author traits while keeping readiness blocked');

  await page.locator('#codex-transport').selectOption('exec');
  await waitFor('exec preference to persist', async () => (await invoke('codex_transport_settings')).transport === 'exec');
  state = await invoke('provider_state');
  assert.deepEqual(selectionOf(state), preservedSelection, 'Returning to Exec must preserve the author model and traits');
  assertNoProviderWork('After returning to exec');
  await page.screenshot({ path: resolve(output, 'settings-exec.png') });
  recordCheck(report.checks, 'native-app-server-smoke:04', 'Returning to Exec is a preference-only operation with no generation or background jobs');

  assert.deepEqual(allPageErrors, [], `Native transport Settings page errors: ${allPageErrors.map(error => `${error.label}: ${error.message}`).join('; ')}`);
  report.finalTransport = await invoke('codex_transport_settings');
  report.finalProviderState = await invoke('provider_state');
  report.finalCounts = assertNoProviderWork('Final settings state');
  report.status = 'passed';
  report.finishedAt = new Date().toISOString();
  await writeFile(resolve(output, 'report.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ status: report.status, checks: report.checks.length, output }, null, 2));
} catch (error) {
  report.status = 'failed';
  report.finishedAt = new Date().toISOString();
  report.failure = String(error);
  report.appLog = appLog;
  report.pageErrors = allPageErrors;
  await writeFile(resolve(output, 'failure.json'), JSON.stringify(report, null, 2));
  await writeFile(resolve(output, 'failure.txt'), `${error.stack ?? error}\n${appLog}`);
  throw error;
} finally {
  await stopCurrent();
}
