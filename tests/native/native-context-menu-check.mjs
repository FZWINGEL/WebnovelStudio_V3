// Native WebView2 context-menu qualification. The caller supplies an already
// opened synthetic chapter; this module never starts a provider job.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const helperPath = resolve(root, 'tests/native/native-context-menu.ps1');
const timeoutMs = 20_000;
const outputLimit = 32_000;

function appendOutput(current, chunk) {
  const next = current + chunk.toString();
  if (next.length > outputLimit) throw new Error('Context-menu helper output exceeded its bound.');
  return next;
}

function startMenuHelper(ownerPid, data) {
  const helper = spawn('powershell.exe', [
    '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', helperPath,
    '-OwnerPid', String(ownerPid), '-Action', 'Read', '-WaitForSignal',
  ], { cwd: data, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  let output = '';
  let remainder = '';
  let readyResolve;
  let readyReject;
  let resultResolve;
  let resultReject;
  let settled = false;
  let result = null;
  const ready = new Promise((resolveReady, rejectReady) => { readyResolve = resolveReady; readyReject = rejectReady; });
  const exited = new Promise((resolveResult, rejectResult) => { resultResolve = resolveResult; resultReject = rejectResult; });
  exited.catch(() => {});
  const timer = setTimeout(() => {
    if (settled) return;
    settled = true;
    helper.kill();
    const error = new Error(`Native context-menu helper timed out.\n${output}`);
    readyReject(error);
    resultReject(error);
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
      if (value.menuVisible !== undefined) result = value;
    }
  };
  helper.stdout.on('data', parse);
  helper.stderr.on('data', chunk => { output = appendOutput(output, chunk); });
  helper.once('error', error => {
    if (settled) return;
    settled = true;
    clearTimeout(timer);
    readyReject(error);
    resultReject(error);
  });
  helper.once('exit', code => {
    if (settled) return;
    settled = true;
    clearTimeout(timer);
    if (code !== 0) {
      const error = new Error(`Native context-menu helper failed (${code}).\n${output}`);
      readyReject(error);
      resultReject(error);
    } else if (result) {
      resultResolve(result);
    } else {
      resultReject(new Error(`Native context-menu helper exited without a JSON snapshot.\n${output}`));
    }
  });
  return {
    ready,
    async inspectAt(point) {
      await ready;
      helper.stdin.end(JSON.stringify(point));
      return exited;
    },
    async stop() {
      if (helper.exitCode !== null) return;
      helper.kill();
      await new Promise(resolveExit => {
        const timer = setTimeout(resolveExit, 5_000);
        helper.once('exit', () => { clearTimeout(timer); resolveExit(); });
      });
    },
  };
}

async function dismissMenu(ownerPid, data) {
  const helper = spawn('powershell.exe', [
    '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', helperPath,
    '-OwnerPid', String(ownerPid), '-Action', 'Dismiss',
  ], { cwd: data, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  let output = '';
  return new Promise((resolveDismiss, rejectDismiss) => {
    const timer = setTimeout(() => {
      helper.kill();
      rejectDismiss(new Error(`Native context-menu dismissal timed out.\n${output}`));
    }, 5_000);
    const append = chunk => { output = appendOutput(output, chunk); };
    helper.stdout.on('data', append);
    helper.stderr.on('data', append);
    helper.once('error', error => { clearTimeout(timer); rejectDismiss(error); });
    helper.once('exit', code => {
      clearTimeout(timer);
      if (code === 0) resolveDismiss(output.trim());
      else rejectDismiss(new Error(`Native context-menu dismissal failed (${code}).\n${output}`));
    });
  });
}

function normalize(value) {
  const compact = String(value ?? '').replace(/[\u2026…]/gu, '...').replace(/\s+/gu, ' ').trim().replace(/\.{3}\s*$/u, '');
  return compact
    .replace(/\s*(?:[([]\s*)?(?:(?:ctrl|control|shift|alt|win|cmd)\s*\+\s*)+(?:[a-z0-9]+|f\d+)\s*[)\]]?\s*$/iu, '')
    .replace(/\s*(?:[([]\s*)?f\d+\s*[)\]]?\s*$/iu, '')
    .trim();
}

function isReloadCommand(value) {
  return /^(?:reload|reload page|refresh|refresh page)$/iu.test(normalize(value));
}

function isNativeEditingCommand(value) {
  return /^(?:cut|copy|paste|select all|undo|redo)$/iu.test(normalize(value));
}

async function contextPoint(page, target, ready) {
  const box = await target.boundingBox();
  assert(box, 'The native context-menu target must be visible.');
  const viewport = await page.evaluate(() => ({ width: window.innerWidth, height: window.innerHeight }));
  const webView = ready.webView;
  assert(webView?.clientWidth > 0 && webView?.clientHeight > 0, 'The native helper did not report an owned WebView client area.');
  const scaleX = webView.clientWidth / viewport.width;
  const scaleY = webView.clientHeight / viewport.height;
  return {
    x: Math.round((box.x + box.width / 2) * scaleX),
    y: Math.round((box.y + box.height / 2) * scaleY),
  };
}

async function rightClickRenderer(page, target) {
  const box = await target.boundingBox();
  assert(box, 'The renderer context-menu target must be visible.');
  const x = Math.max(box.x + 24, Math.min(box.x + box.width - 24, box.x + box.width / 2));
  const y = Math.max(box.y + 24, Math.min(box.y + box.height - 24, box.y + box.height / 2));
  await page.mouse.click(x, y, { button: 'right' });
}

async function closeNativeContextMenu(page, helper, ownerPid, data) {
  await helper.stop().catch(() => {});
  // The read helper has exited after producing its snapshot. Dismiss through a
  // second PID-scoped helper so the native popup cannot remain under the next
  // renderer interaction; Escape is only a last-resort renderer cleanup.
  await dismissMenu(ownerPid, data).catch(() => {});
  await page.keyboard.press('Escape').catch(() => {});
}

/**
 * Qualify native browser-menu removal while retaining the renderer's
 * selected-text feedback menu. The fixture never reads or changes clipboard
 * contents and only inspects menus belonging to the owned process tree.
 */
export async function qualifyContextMenu({ page, app, data, evidence }) {
  assert(page, 'A connected native page is required.');
  assert(app?.pid, 'The owned native application PID is required.');
  assert(data, 'The owned temporary data directory is required.');
  const report = {
    startedAt: new Date().toISOString(),
    status: 'running',
    nativeHeading: null,
    nativeEditor: null,
    selected: null,
    liveModelCalls: 0,
    limitations: [
      'Synthetic temporary manuscript only; no provider request or user data is used.',
      'Native menu labels are read through Windows UI Automation and are English-locale dependent.',
      'Clipboard contents are never read or changed by this qualifier.',
    ],
  };
  let helper;
  try {
    const editor = page.locator('.tiptap').first();
    await editor.waitFor({ state: 'visible', timeout: 10_000 });
    const heading = page.locator('.document-heading h1').first();
    await heading.waitFor({ state: 'visible', timeout: 10_000 });
    assert.equal(await heading.getAttribute('contenteditable'), null, 'The chapter heading must be noneditable for the browser-menu probe.');
    assert.equal(await heading.evaluate(element => element.isContentEditable), false, 'The chapter heading must not be contenteditable.');
    await page.keyboard.press('Escape').catch(() => {});
    await page.evaluate(() => {
      const instance = document.querySelector('.tiptap')?.editor;
      instance?.commands.setTextSelection({ from: 1, to: 1 });
      instance?.commands.focus();
    });
    await page.waitForFunction(() => !document.querySelector('[role="menu"]'));

    // A heading is outside the editor's custom selection menu. This is the
    // browser-menu surface where WebView2 exposes Reload on an unfiltered
    // build, so testing only the editor would be a false positive.
    helper = startMenuHelper(app.pid, data);
    const ready = await helper.ready;
    assert.equal(ready.ownerPid, app.pid, 'The context-menu helper must report the owned application PID.');
    const nativeHeadingPoint = await contextPoint(page, heading, ready);
    const nativeHeading = await helper.inspectAt(nativeHeadingPoint);
    nativeHeading.inputTarget = { ...ready.webView, point: nativeHeadingPoint };
    report.nativeHeading = nativeHeading;
    const headingLabels = nativeHeading.items.map(item => normalize(item.name)).filter(Boolean);
    assert(nativeHeading.menuVisible, 'Windows UI Automation did not expose an owned native context menu on the chapter heading.');
    assert(!headingLabels.some(isReloadCommand), `Native heading context menu still exposes Reload/Refresh: ${headingLabels.join(', ')}`);
    const browserCommands = headingLabels.filter(label => /^(?:back|forward|save(?: page)? as|print|cast|translate|view source|inspect|open link)$/iu.test(label));
    assert(browserCommands.length > 0, `The heading menu did not expose a recognizable browser command; labels: ${headingLabels.join(', ')}`);
    report.nativeHeading.labels = headingLabels;
    report.nativeHeading.browserCommands = browserCommands;
    await closeNativeContextMenu(page, helper, app.pid, data);
    helper = null;

    // A collapsed editor selection should retain native edit commands such as
    // Copy/Paste/Select all even though the browser Reload item is removed.
    await page.evaluate(() => {
      const instance = document.querySelector('.tiptap')?.editor;
      instance?.commands.setTextSelection({ from: 1, to: 1 });
      instance?.commands.focus();
    });
    helper = startMenuHelper(app.pid, data);
    const editorReady = await helper.ready;
    const nativeEditorPoint = await contextPoint(page, editor, editorReady);
    const nativeEditor = await helper.inspectAt(nativeEditorPoint);
    nativeEditor.inputTarget = { ...editorReady.webView, point: nativeEditorPoint };
    report.nativeEditor = nativeEditor;
    const editorLabels = nativeEditor.items.map(item => normalize(item.name)).filter(Boolean);
    assert(nativeEditor.menuVisible, 'Windows UI Automation did not expose an owned native context menu on the editor.');
    assert(!editorLabels.some(isReloadCommand), `Native editor context menu still exposes Reload/Refresh: ${editorLabels.join(', ')}`);
    const editingCommands = editorLabels.filter(isNativeEditingCommand);
    assert(editingCommands.length > 0, `Native editor context menu exposed no editing command to preserve: ${editorLabels.join(', ')}`);
    report.nativeEditor.labels = editorLabels;
    report.nativeEditor.editingCommands = editingCommands;
    await closeNativeContextMenu(page, helper, app.pid, data);
    helper = null;

    await page.evaluate(() => {
      const instance = document.querySelector('.tiptap')?.editor;
      const end = Math.min(8, Math.max(2, instance?.state.doc.content.size - 1 ?? 2));
      instance?.commands.setTextSelection({ from: 1, to: end });
      instance?.commands.focus();
    });
    const selectedHelperMenu = page.locator('[role="menu"]').filter({ hasText: /feedback|discuss|selection/iu }).last();
    await rightClickRenderer(page, editor);
    await selectedHelperMenu.waitFor({ state: 'visible', timeout: 5_000 });
    const selectedLabels = (await selectedHelperMenu.getByRole('menuitem').allTextContents()).map(normalize);
    assert(selectedLabels.some(label => /feedback|discuss|selection/iu.test(label)), `Selected-text menu did not expose feedback action: ${selectedLabels.join(', ')}`);
    report.selected = { customMenuVisible: true, labels: selectedLabels };
    if (evidence) await page.screenshot({ path: resolve(evidence, 'context-menu-selection.png') });
    await page.keyboard.press('Escape').catch(() => {});
    report.finishedAt = new Date().toISOString();
    report.status = 'passed';
  } catch (error) {
    if (helper) await closeNativeContextMenu(page, helper, app.pid, data);
    report.finishedAt = new Date().toISOString();
    report.status = 'failed';
    report.failure = String(error?.stack ?? error);
    report.limitations.push('This run did not establish the native-menu contract; see the recorded failure.');
    if (evidence) {
      await mkdir(resolve(evidence), { recursive: true });
      await writeFile(resolve(evidence, 'context-menu.json'), JSON.stringify(report, null, 2));
    }
    throw error;
  }
  if (evidence) {
    await mkdir(resolve(evidence), { recursive: true });
    await writeFile(resolve(evidence, 'context-menu.json'), JSON.stringify(report, null, 2));
  }
  return report;
}

// Short alias for native-smoke callers that use one generic qualification
// entrypoint per fixture.
export const qualifier = qualifyContextMenu;
