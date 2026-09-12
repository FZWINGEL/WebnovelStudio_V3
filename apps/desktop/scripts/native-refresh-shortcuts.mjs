// Native WebView2 refresh-accelerator qualification. The caller supplies an
// already opened synthetic chapter. This module never starts a provider job.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const helperPath = resolve(root, 'apps/desktop/scripts/native-refresh-shortcut.ps1');
const shortcuts = [
  { label: 'Control+R', key: 'ControlR', baseKey: 'R', control: true, shift: false },
  { label: 'F5', key: 'F5', baseKey: 'F5', control: false, shift: false },
  { label: 'Shift+F5', key: 'ShiftF5', baseKey: 'F5', control: false, shift: true },
  { label: 'Control+F5', key: 'ControlF5', baseKey: 'F5', control: true, shift: false },
  { label: 'Control+Shift+F5', key: 'ControlShiftF5', baseKey: 'F5', control: true, shift: true },
  { label: 'Control+Shift+R', key: 'ControlShiftR', baseKey: 'R', control: true, shift: true },
];
const timeoutMs = 15_000;
const outputLimit = 32_000;

function appendOutput(current, chunk) {
  const next = current + chunk.toString();
  if (next.length > outputLimit) throw new Error('Refresh shortcut helper output exceeded its bound.');
  return next;
}

function startShortcutHelper(ownerPid, key, data) {
  const helper = spawn('powershell.exe', [
    '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', helperPath,
    '-OwnerPid', String(ownerPid), '-Key', key, '-WaitForSignal',
  ], { cwd: data, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  let output = '';
  let remainder = '';
  let readyResolve;
  let readyReject;
  let exitResolve;
  let exitReject;
  let sentValue = null;
  let settled = false;
  const ready = new Promise((resolveReady, rejectReady) => { readyResolve = resolveReady; readyReject = rejectReady; });
  const exited = new Promise((resolveExit, rejectExit) => { exitResolve = resolveExit; exitReject = rejectExit; });
  // A helper may be stopped while the ready promise has already resolved. Mark
  // the exit rejection handled here; send() still observes the same promise.
  exited.catch(() => {});
  const timer = setTimeout(() => {
    if (settled) return;
    settled = true;
    helper.kill();
    const error = new Error(`Refresh shortcut helper timed out for ${key}.\n${output}`);
    readyReject(error);
    exitReject(error);
  }, timeoutMs);
  const parse = chunk => {
    output = appendOutput(output, chunk);
    remainder += chunk.toString();
    const lines = remainder.split(/\r?\n/u);
    remainder = lines.pop() ?? '';
    for (const line of lines) {
      if (!line.trim()) continue;
      let value;
      try { value = JSON.parse(line); } catch { continue; }
      if (value.ready === true) readyResolve(value);
      if (value.action === 'shortcut-sent') sentValue = value;
    }
  };
  helper.stdout.on('data', parse);
  helper.stderr.on('data', chunk => { output = appendOutput(output, chunk); });
  helper.once('error', error => { if (!settled) { settled = true; clearTimeout(timer); readyReject(error); exitReject(error); } });
  helper.once('exit', code => {
    if (settled) return;
    if (code !== 0) {
      settled = true; clearTimeout(timer);
      const error = new Error(`Refresh shortcut helper failed for ${key} (${code}).\n${output}`);
      readyReject(error); exitReject(error); return;
    }
    settled = true; clearTimeout(timer);
    if (code === 0 && sentValue) exitResolve({ ...sentValue, exitCode: code });
    else exitReject(new Error(`Refresh shortcut helper exited before sending ${key}.\n${output}`));
  });
  return {
    helper,
    ready,
    async send() {
      await ready;
      helper.stdin.end('send\n');
      return exited;
    },
    async stop() {
      if (helper.exitCode !== null) return;
      helper.kill();
      await new Promise(resolveExit => { const timer = setTimeout(resolveExit, 5_000); helper.once('exit', () => { clearTimeout(timer); resolveExit(); }); });
    },
  };
}

async function waitForEditor(page) {
  const current = page.getByRole('textbox', { name: 'Manuscript', exact: true });
  if (await current.count()) { await current.waitFor({ state: 'visible', timeout: 10_000 }); return current; }
  const library = page.getByRole('heading', { name: 'Your stories', exact: true });
  if (!(await library.count())) throw new Error('Refresh shortcut left neither the editor nor the project library visible.');
  const projects = page.locator('.project-open');
  if (!(await projects.count())) throw new Error('Refresh shortcut fixture project was not available after reload.');
  await projects.first().click();
  const reopened = page.getByRole('textbox', { name: 'Manuscript', exact: true });
  await reopened.waitFor({ state: 'visible', timeout: 10_000 });
  return reopened;
}

async function rendererIdentity(page) {
  return page.evaluate(() => {
    const marker = '__wnsRefreshShortcutRenderer';
    if (!window[marker]) window[marker] = crypto.randomUUID();
    return window[marker];
  });
}

async function installKeyRecorder(page) {
  await page.evaluate(() => {
    const keyEvents = [];
    window.__wnsRefreshShortcutKeys = keyEvents;
    window.addEventListener('keydown', event => {
      keyEvents.push({ key: event.key, code: event.code, ctrlKey: event.ctrlKey, shiftKey: event.shiftKey, saveStatus: document.querySelector('.save-status')?.textContent ?? null, activeTag: document.activeElement?.tagName ?? null, activeContentEditable: document.activeElement?.getAttribute('contenteditable') ?? null });
    }, { capture: true });
  });
}

async function holdNextSaveSnapshot(page) {
  await page.evaluate(() => {
    const originalFetch = window.fetch;
    const state = { held: false, released: false, release: null };
    window.__wnsRefreshSaveHold = state;
    window.fetch = async (...args) => {
      if (!state.held && String(args[0]).endsWith('/save_snapshot')) {
        state.held = true;
        await new Promise(resolveRelease => { state.release = resolveRelease; });
        state.released = true;
        window.fetch = originalFetch;
      }
      return originalFetch.apply(window, args);
    };
  });
}

async function releaseSaveSnapshot(page) {
  await page.evaluate(() => window.__wnsRefreshSaveHold?.release?.()).catch(() => {});
}

/**
 * Qualify native refresh accelerators while the caller's synthetic chapter is
 * intentionally dirty. The helper verifies the application PID/window before
 * the caller types and waits for a signal, so helper startup cannot consume
 * the editor's debounce window.
 */
export async function qualifyRefreshShortcuts({ page, app, data, evidence }) {
  assert(page, 'A connected native page is required.');
  assert(app?.pid, 'The owned native application PID is required.');
  assert(data, 'The owned temporary data directory is required.');
  const report = { startedAt: new Date().toISOString(), shortcuts: [], liveModelCalls: 0, limitations: [
    'Synthetic temporary manuscript only; no provider request or user data is used.',
    'The check observes native WebView2 refresh accelerators, not a forced process crash.',
    'A native accelerator may consume the refresh key before a DOM keydown; a separate Control probe verifies the focused WebView input path.',
  ] };
  let navigationCount = 0;
  const mainFrame = page.mainFrame();
  const navigated = frame => { if (frame === mainFrame) navigationCount += 1; };
  page.on('framenavigated', navigated);
  try {
    for (const shortcut of shortcuts) {
      const editor = await waitForEditor(page);
      const beforeText = await editor.innerText();
      const marker = `Refresh shortcut probe · ${shortcut.label} · ${Date.now()}`;
      const beforeNavigation = navigationCount;
      const beforeRenderer = await rendererIdentity(page);
      await installKeyRecorder(page);
      await holdNextSaveSnapshot(page);
      const helper = startShortcutHelper(app.pid, shortcut.key, data);
      const probe = startShortcutHelper(app.pid, 'Probe', data);
      try {
        const [ready, probeReady] = await Promise.all([helper.ready, probe.ready]);
        assert.equal(ready.ownerPid, app.pid, 'Shortcut helper must report the owned application PID.');
        assert.equal(probeReady.ownerPid, app.pid, 'Control probe must report the owned application PID.');
        await editor.fill(marker);
        await page.waitForFunction(expected => document.querySelector('.tiptap')?.textContent === expected, marker, { timeout: 5_000 });
        await page.locator('.save-status').filter({ hasText: 'Saving…' }).waitFor({ state: 'visible', timeout: 5_000 });
        await page.waitForFunction(() => window.__wnsRefreshSaveHold?.held === true, null, { timeout: 10_000 });
        const dirtyText = await editor.innerText();
        assert.equal(dirtyText, marker, `${shortcut.label} fixture must type the dirty marker before sending the key.`);
        const dirtyStatus = await page.locator('.save-status').innerText();
        const probeSent = await probe.send();
        assert.equal(probeSent.ownerPid, app.pid, 'Control probe must report the owned application PID after sending.');
        await new Promise(resolveWait => setTimeout(resolveWait, 100));
        const probeEvents = await page.evaluate(() => window.__wnsRefreshShortcutKeys ?? []).catch(() => []);
        const probeKey = probeEvents.find(event => (event.key === 'Control' || event.code === 'ControlLeft' || event.code === 'ControlRight') && event.ctrlKey === true);
        assert(probeKey, `${shortcut.label} Control probe did not reach the focused WebView.`);
        assert.equal(probeKey.saveStatus, 'Saving…', `${shortcut.label} Control probe was not received while the manuscript was dirty.`);
        assert.equal(probeKey.activeContentEditable, 'true', `${shortcut.label} Control probe did not reach the manuscript editor.`);
        const sent = await helper.send();
        assert.equal(sent.ownerPid, app.pid, 'Shortcut helper must report the owned application PID after sending.');
        await new Promise(resolveWait => setTimeout(resolveWait, 1_500));
        const afterNavigation = navigationCount;
        const afterRenderer = await page.evaluate(() => window.__wnsRefreshShortcutRenderer ?? null).catch(() => null);
        const afterEditor = page.getByRole('textbox', { name: 'Manuscript', exact: true });
        const editorVisible = await afterEditor.count() > 0;
        const afterText = editorVisible ? await afterEditor.innerText().catch(() => null) : null;
        const keyEvents = await page.evaluate(() => window.__wnsRefreshShortcutKeys ?? []).catch(() => []);
        const matchingKey = keyEvents.find(event => {
          const expectedKey = shortcut.baseKey === 'F5'
            ? event.key === 'F5'
            : (event.key === 'r' || event.key === 'R');
          return expectedKey && event.ctrlKey === shortcut.control && event.shiftKey === shortcut.shift;
        });
        const reloaded = afterNavigation !== beforeNavigation || afterRenderer === null || afterRenderer !== beforeRenderer;
        const result = { shortcut: shortcut.label, dirtyMarker: marker, dirtyStatus, beforeNavigation, afterNavigation, navigationCount: afterNavigation - beforeNavigation, beforeRenderer, afterRenderer, reloaded, editorVisible, textRetained: afterText === marker, keyEvents, probeKey, matchingKey: matchingKey ?? null, refreshKeyObservedInRenderer: matchingKey !== undefined, beforeText, afterText };
        report.shortcuts.push(result);
        assert.equal(reloaded, false, `${shortcut.label} unexpectedly reloaded the renderer while the manuscript was dirty.`);
        if (matchingKey) {
          assert.equal(matchingKey.saveStatus, 'Saving…', `${shortcut.label} keydown was not received while the manuscript was dirty.`);
          assert.equal(matchingKey.activeContentEditable, 'true', `${shortcut.label} keydown did not reach the manuscript editor.`);
        }
        assert.equal(result.textRetained, true, `${shortcut.label} did not retain the dirty manuscript marker.`);
        await releaseSaveSnapshot(page);
        await page.locator('.save-status').filter({ hasText: 'Saved' }).waitFor({ state: 'visible', timeout: 10_000 });
      } finally {
        await releaseSaveSnapshot(page);
        await probe.stop().catch(() => {});
        await helper.stop().catch(() => {});
      }
    }
    report.finishedAt = new Date().toISOString();
    report.status = 'passed';
  } catch (error) {
    report.finishedAt = new Date().toISOString();
    report.status = 'failed';
    report.failure = String(error?.stack ?? error);
    throw error;
  } finally {
    page.off('framenavigated', navigated);
    if (evidence) {
      await mkdir(resolve(evidence), { recursive: true });
      await writeFile(resolve(evidence, 'refresh-shortcuts.json'), JSON.stringify(report, null, 2));
    }
  }
  return report;
}
