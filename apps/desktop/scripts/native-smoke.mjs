// Actual Tauri/WebView2 integration checks. No Chromium browser is launched.
import { chromium } from 'playwright-core';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, readFile, realpath, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { isAbsolute, relative, resolve, sep, toNamespacedPath } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createServer } from 'node:net';
import { DatabaseSync } from 'node:sqlite';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const executable = process.env.WNS_V3_NATIVE_EXE ? resolve(process.env.WNS_V3_NATIVE_EXE) : resolve(root, 'target/debug/webnovel-desktop.exe');
const output = resolve(root, '.local/native-results');
await mkdir(output, { recursive: true });
const data = await mkdtemp(resolve(tmpdir(), 'wns-v3-native-'));
async function reservePort() {
  const server = createServer();
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const port = server.address().port;
  await new Promise(resolve => server.close(resolve));
  return port;
}
let port = await reservePort();
let appLog = '';
let spawnError;
function launch() {
  const process = spawn(executable, [], {
    cwd: data, windowsHide: true, stdio: 'pipe',
    env: { ...globalThis.process.env, WNS_V3_NATIVE_CDP_PORT: String(port), WNS_V3_TRIAL_WEBVIEW_DIR: resolve(data, 'webview'), WNS_V3_TEST_DATA_DIR: resolve(data, 'library') },
  });
  process.stdout.on('data', chunk => { appLog += chunk; });
  process.stderr.on('data', chunk => { appLog += chunk; });
  process.on('error', error => { spawnError = error; appLog += error.stack; });
  return process;
}
let app = launch();
async function operateSaveDialog(action, destination = '', reviewed = false, expectNoFile = false) {
  const args = ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', resolve(root, 'apps/desktop/scripts/native-save-dialog.ps1'),
    '-OwnerPid', String(app.pid), '-Action', action, '-TestRoot', data];
  if (destination) args.push('-Destination', destination);
  if (reviewed) args.push('-DialogTitle', 'Save author-reviewed snapshot as a new file');
  if (expectNoFile) args.push('-ExpectNoFile');
  return new Promise((accept, reject) => {
    const helper = spawn('powershell.exe', args, { windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
    let output = '';
    const timeout = setTimeout(() => { helper.kill(); reject(new Error('The owned native Save dialog helper timed out.')); }, 30_000);
    const append = chunk => { output += chunk; if (output.length > 64_000) { helper.kill(); reject(new Error('Native dialog diagnostics exceeded the bound.')); } };
    helper.stdout.on('data', append); helper.stderr.on('data', append);
    helper.once('error', error => { clearTimeout(timeout); reject(error); });
    helper.once('exit', code => { clearTimeout(timeout); code === 0 ? accept(output.trim()) : reject(new Error(`Native Save dialog action failed (${code}): ${output}`)); });
  });
}
let browser;
let observedPage;
const checks = [];
try {
  const startup = Date.now();
  let ready = false;
  let lastReadinessError = '';
  console.log(`Starting native WebView2 qualification (PID ${app.pid}); waiting up to 90 seconds for CDP.`);
  while (Date.now() - startup < 90_000) {
    if (spawnError || app.exitCode !== null) throw new Error(`Native app did not start (exit ${app.exitCode}): ${appLog}`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(2000) });
      if (response.ok && (await response.json()).webSocketDebuggerUrl) {
        browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 10000 });
        ready = true; break;
      }
      lastReadinessError = `Unexpected CDP HTTP status ${response.status}`;
    } catch (error) { lastReadinessError = String(error); }
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  if (!ready) throw new Error(`WebView2 CDP was not ready after ${Date.now() - startup}ms. PID=${app.pid}; exit=${app.exitCode}; ${lastReadinessError}\nNative log:\n${appLog || '(empty)'}`);
  const context = browser.contexts()[0];
  let page = context.pages()[0];
  if (!page) page = await context.waitForEvent('page', { timeout: 10000 });
  observedPage = page;
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.getByRole('button', { name: 'Choose model: Local test model', exact: true }).click();
  await page.getByRole('button', { name: 'Favorite GPT-5.6-Luna', exact: true }).click();
  await page.getByRole('button', { name: 'Unfavorite GPT-5.6-Luna', exact: true }).waitFor();
  await page.getByRole('searchbox', { name: 'Search models' }).fill('luna');
  assert.equal(await page.locator('.model-choice').count(), 1);
  await page.getByRole('searchbox', { name: 'Search models' }).press('Enter');
  await page.getByRole('button', { name: 'Choose model: GPT-5.6-Luna', exact: true }).waitFor();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByLabel('Reasoning', { exact: true }).selectOption('high');
  await page.waitForFunction(async () => (await window.__TAURI_INTERNALS__.invoke('provider_state')).settings.active.reasoning === 'high');
  await page.screenshot({ path: resolve(output, 'model-settings.png') });
  await page.getByRole('button', { name: 'Close settings', exact: true }).click();
  await page.reload();
  await page.getByRole('button', { name: 'Choose model: GPT-5.6-Luna', exact: true }).waitFor();
  const savedProvider = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('provider_state'));
  assert.equal(savedProvider.dispatch.kind, 'blocked');
  assert.deepEqual(savedProvider.settings.active, { providerId: 'codex', modelId: 'gpt-5.6-luna', reasoning: 'high', serviceTier: 'priority' });
  assert.deepEqual(savedProvider.settings.favorites, [{ providerId: 'codex', modelId: 'gpt-5.6-luna' }]);
  await page.getByRole('button', { name: 'Choose model: GPT-5.6-Luna', exact: true }).click();
  await page.locator('.model-choice').filter({ hasText: 'Local test model' }).click();
  await page.getByRole('button', { name: 'Choose model: Local test model', exact: true }).waitFor();
  checks.push('Model choice, favorites and traits survive reload; unavailable Codex remains blocked without substitution');
  await page.getByRole('button', { name: 'Open editor trial', exact: true }).click();
  await page.getByRole('textbox', { name: 'Chapter manuscript' }).waitFor();
  // The diagnostic is intentionally hidden at smaller native window widths.
  // Qualification reads the real runtime through IPC below, not CSS visibility.
  await page.getByText(/Tauri · WebView2/).waitFor({ state: 'attached' });
  assert.equal(new URL(page.url()).hostname, 'tauri.localhost');
  const runtime = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('runtime_info'));
  assert.equal(runtime.host, 'Tauri');
  assert.equal(runtime.persistence, true);
  checks.push(`Real Tauri IPC, WebView2 ${runtime.webviewVersion}`);
  await page.getByRole('button', { name: 'Check with Rust', exact: true }).click();
  await page.getByRole('status').filter({ hasText: 'Fingerprints match' }).waitFor();
  checks.push('Editor/Rust canonical JSON and SHA-256 match over real IPC');
  await page.screenshot({ path: resolve(output, 'desktop.png') });

  // An immutable Tiptap instance is exposed on its DOM element by Tiptap itself.
  const before = await page.evaluate(() => {
    const editor = document.querySelector('.tiptap').editor;
    window.nativeTrialEditor = editor;
    return editor.getJSON();
  });
  await page.evaluate(() => document.querySelector('.tiptap').editor.commands.setTextSelection({ from: 1, to: 8 }));
  await page.getByRole('button', { name: 'Selection feedback', exact: true }).click();
  await page.getByRole('textbox', { name: 'Your feedback on this passage' }).waitFor();
  assert.equal(await page.locator('.quoted-scope blockquote').textContent(), 'By dusk');
  assert.equal(await page.evaluate(() => document.activeElement.id), 'feedback-input');
  await page.getByRole('textbox', { name: 'Your feedback on this passage' }).fill('Make this opening more immediate.');
  await page.getByRole('button', { name: 'Keep feedback', exact: true }).click();
  assert(await page.evaluate(() => document.querySelector('.tiptap').editor === window.nativeTrialEditor));
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), before);
  checks.push('Selection quote and composer focus survive chat updates without replacing the editor');

  await page.getByRole('button', { name: 'Try your own replacement' }).click();
  await page.getByRole('textbox', { name: 'Replacement text' }).fill('At nightfall');
  await page.getByRole('button', { name: 'Preview replacement' }).click();
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), before);
  await page.getByRole('button', { name: 'Reject', exact: true }).click();
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), before);
  await page.getByRole('button', { name: 'Preview replacement' }).click();
  await page.locator('.replacement-preview').scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(output, 'selection-preview.png') });
  await page.getByRole('button', { name: 'Apply replacement' }).click();
  await page.getByRole('status').filter({ hasText: 'Replacement applied' }).waitFor();
  const after = await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());
  assert.equal(after.content[0].content[0].text, 'At nightfall, every lantern in the harbour had gone dark. All but one.');
  assert.deepEqual(after.content.slice(1), before.content.slice(1));
  assert.equal(after.content[0].attrs.id, before.content[0].attrs.id);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), before);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), after);
  checks.push('Preview/Reject do not mutate; strict local Apply, undo and redo preserve surrounding nodes');

  const manuscript = page.getByRole('textbox', { name: 'Chapter manuscript' });
  await manuscript.click();
  await page.keyboard.press('ArrowRight');
  const focusStyle = await manuscript.evaluate(element => ({ style: getComputedStyle(element).outlineStyle, width: getComputedStyle(element).outlineWidth }));
  assert.equal(focusStyle.style, 'solid');
  assert.equal(focusStyle.width, '2px');
  await page.screenshot({ path: resolve(output, 'editor-focused.png') });
  checks.push('Manuscript keyboard focus is visibly indicated in the native window');
  await page.keyboard.press('Control+End');
  await page.keyboard.press('Enter');
  await page.keyboard.insertText('灯火🙂 灯火🙂 e\u0301 👩‍🚀');
  assert((await manuscript.textContent()).includes('灯火🙂 灯火🙂 e\u0301 👩‍🚀'));
  await page.getByRole('button', { name: 'Check with Rust', exact: true }).click();
  await page.getByRole('status').filter({ hasText: 'Fingerprints match' }).waitFor();
  checks.push('Synthetic Unicode edge cases survive native input: non-Latin text, emoji, combining marks and ZWJ');
  await page.evaluate(() => {
    const editor = document.querySelector('.tiptap').editor;
    editor.commands.setTextSelection({ from: 1, to: 8 });
    editor.commands.focus();
  });
  await page.waitForFunction(() => document.activeElement.classList.contains('tiptap'));
  await page.keyboard.press('Control+Shift+f');
  await page.getByRole('textbox', { name: 'Your feedback on this passage' }).waitFor();
  // The label can already exist from an earlier selection. Wait for the
  // requestAnimationFrame focus handoff itself, not just that existing label.
  await page.waitForFunction(() => document.activeElement?.id === 'feedback-input');
  assert.equal(await page.evaluate(() => document.activeElement.id), 'feedback-input');
  checks.push('Ctrl+Shift+F captures selection and transfers focus');

  await page.evaluate(() => {
    const editor = document.querySelector('.tiptap').editor;
    editor.commands.insertContentAt(1, 'Changed ');
  });
  await page.getByText('The chapter changed. This quotation is kept as a reference.', { exact: false }).waitFor();
  assert(await page.getByRole('button', { name: 'Try your own replacement' }).isDisabled());
  checks.push('Intervening manuscript edits make captured replacement scope stale');

  await page.getByRole('button', { name: 'Whole chapter', exact: true }).click();
  const clipboardSource = await page.evaluate(() => {
    const editor = document.querySelector('.tiptap').editor;
    editor.commands.setContent({ type: 'doc', content: [
      { type: 'paragraph', attrs: { id: 'clipboard-left' }, content: [{ type: 'text', text: 'Clipboard 灯火🙂', marks: [{ type: 'bold' }] }] },
      { type: 'paragraph', attrs: { id: 'clipboard-right' }, content: [{ type: 'text', text: '尾声', marks: [{ type: 'italic' }] }] },
      { type: 'paragraph', attrs: { id: 'clipboard-destination' }, content: [{ type: 'text', text: 'Destination ' }] },
    ] });
    editor.commands.setTextSelection({ from: 1, to: editor.state.doc.child(0).nodeSize + 1 + editor.state.doc.child(1).content.size });
    editor.commands.focus();
    return editor.getJSON();
  });
  await page.waitForFunction(() => document.activeElement.classList.contains('tiptap'));
  await page.evaluate(() => {
    window.nativeClipboardProbe = { focused: document.hasFocus(), copied: 0, pasted: 0, matched: false };
    document.addEventListener('copy', event => {
      window.nativeClipboardProbe.copied++;
      window.nativeClipboardProbe.copySelectionMatches = window.getSelection()?.toString().includes('Clipboard 灯火🙂') ?? false;
      window.nativeClipboardProbe.copyDataMatches = event.clipboardData?.getData('text/plain').includes('Clipboard 灯火🙂') ?? false;
      window.nativeClipboardProbe.copyPrevented = event.defaultPrevented;
    }, { once: true });
    document.addEventListener('paste', event => {
      window.nativeClipboardProbe.pasted++;
      window.nativeClipboardProbe.matched = event.clipboardData?.getData('text/plain').includes('Clipboard 灯火🙂') ?? false;
      window.nativeClipboardProbe.pasteTypes = [...event.clipboardData.types];
      window.nativeClipboardProbe.pasteLength = event.clipboardData.getData('text/plain').length;
    }, { once: true });
  });
  await page.waitForFunction(() => window.getSelection()?.toString().includes('Clipboard 灯火🙂'));
  await page.keyboard.press('Control+c');
  await page.evaluate(() => {
    const editor = document.querySelector('.tiptap').editor;
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
  });
  await page.keyboard.press('Control+v');
  await page.waitForFunction(() => document.querySelector('.tiptap').editor.state.doc.textContent.split('Clipboard 灯火🙂').length === 3).catch(async error => {
    appLog += `\nClipboard event probe: ${JSON.stringify(await page.evaluate(() => ({ ...window.nativeClipboardProbe, focusedAfter: document.hasFocus(), selected: window.getSelection()?.toString().length })))}`;
    throw error;
  });
  const pasted = await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());
  assert.deepEqual(pasted.content.slice(0, 2), clipboardSource.content.slice(0, 2));
  assert.equal(new Set(pasted.content.map(block => block.attrs.id)).size, pasted.content.length);
  assert(pasted.content.slice(2).some(block => block.content?.some(node => node.text?.includes('Clipboard 灯火🙂') && node.marks?.some(mark => mark.type === 'bold'))));
  assert(pasted.content.slice(2).some(block => block.content?.some(node => node.text?.includes('尾声') && node.marks?.some(mark => mark.type === 'italic'))));
  await page.getByRole('button', { name: 'Check with Rust', exact: true }).click();
  await page.getByRole('status').filter({ hasText: 'Fingerprints match' }).waitFor();
  checks.push('Native WebView2 Ctrl+C/Ctrl+V preserves formatted Unicode paragraphs and unique IDs');
  await page.evaluate(() => {
    const editor = document.querySelector('.tiptap').editor;
    editor.commands.setTextSelection({ from: 1, to: 10 });
    editor.commands.focus();
  });
  await page.waitForFunction(() => document.activeElement.classList.contains('tiptap'));
  await page.locator('.tiptap p').first().click({ button: 'right', position: { x: 20, y: 10 } });
  await page.getByRole('menuitem', { name: 'Give feedback on selection' }).click();
  await page.getByRole('textbox', { name: 'Your feedback on this passage' }).waitFor();
  assert.equal(await page.locator('.quoted-scope blockquote').textContent(), 'Clipboard');
  checks.push('Right-click selection menu captures the intended passage');
  // W2 exercises shipping project commands against synthetic, file-backed data.
  // This is transport qualification; the visible W0 manuscript is still session-only.
  const projectPath = resolve(data, 'persistence-project');
  const persisted = await page.evaluate(async ({ projectPath }) => {
    const invoke = (command, args) => window.__TAURI_INTERNALS__.invoke(command, args);
    const opened = await invoke('create_project', { path: projectPath, title: 'Native persistence fixture', session: 'native-session-one' });
    const body = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'native-paragraph' } }] } };
    const created = await invoke('create_document', { request: { access: opened.access, operationId: 'create-native', documentId: 'native-chapter', title: 'An empty harbour', kind: 'chapter', body } });
    body.body.content[0].content = [{ type: 'text', text: 'Saved in Rust. Mei waited beneath the lantern. 👩‍🚀' }];
    const request = { access: opened.access, operationId: 'native-save', expected: created.head, localGeneration: '1', body, cause: 'typing' };
    const ack = await invoke('save_snapshot', { request });
    const replay = await invoke('save_snapshot', { request });
    const revision = await invoke('checkpoint_document', { request: { access: opened.access, expected: ack.head, reason: 'manual' } });
    const reconciled = await invoke('reconcile_document', { request: { projectId: opened.project.projectId, operationNamespace: opened.project.operationNamespace, session: 'native-session-two', documentId: 'native-chapter', pendingOperationIds: ['native-save'] } });
    let staleError;
    try { await invoke('save_snapshot', { request }); } catch (error) { staleError = error; }
    const reopened = await invoke('open_project', { path: projectPath, session: 'native-session-three' });
    return { opened, created, ack, replay, revision, reconciled, staleError, reopened };
  }, { projectPath });
  assert.equal(persisted.ack.head.version, '1');
  assert.deepEqual(persisted.replay, persisted.ack);
  assert.deepEqual(persisted.reconciled.document.head, persisted.ack.head);
  assert.equal(persisted.reconciled.receipts[0].operationId, 'native-save');
  assert.equal(persisted.staleError.code, 'WriterLeaseExpired');
  assert.equal(persisted.reopened.documents[0].body.body.content[0].content[0].text, 'Saved in Rust. Mei waited beneath the lantern. 👩‍🚀');
  assert.equal(persisted.reopened.documents[0].lastCheckpointId, persisted.revision.id);
  checks.push('Real project IPC creates file-backed prose, saves once, checkpoints, fences stale writers and reopens the latest head');
  const contextProof = await page.evaluate(async opened => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const access = opened.access;
    const head = opened.documents[0].head;
    const epochs = await invoke('context_epochs', { access });
    const frozen = await invoke('freeze_story_context', { request: {
      access, operationId: 'native-context-freeze', expected: head, basis: 'working', purpose: 'storyQuestion',
      policy: { version: epochs.policy, audience: 'authorRoom', readerFrontier: null, characterId: null, characterGrants: [], allowAlternatives: false, allowHistorical: false },
    } });
    const prepared = await invoke('prepare_story_context', { request: {
      access, operationId: 'native-context-prepare', snapshotId: frozen.snapshot.snapshotId,
      instruction: 'Who waited beneath the lantern?', mandatoryHandles: [], scope: null,
      budget: { modelId: 'mock-story-context', contextWindowTokens: '20000', reservedOutputTokens: '1000', reservedProtocolTokens: '1000' },
    } });
    if (prepared.status !== 'prepared') throw new Error(JSON.stringify(prepared));
    const packet = await invoke('prepared_story_context', { access, packetId: prepared.packet.receipt.packetId });
    await invoke('save_snapshot', { request: {
      access, operationId: 'native-context-change', expected: head, localGeneration: '2', cause: 'typing',
      body: { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'p1' }, content: [{ type: 'text', text: 'Lian now waits beside the river.' }] }] } },
    } });
    const current = await invoke('prepared_story_context_is_current', { access, packetId: packet.receipt.packetId });
    const oldEvidence = await invoke('search_story_context', { request: { access, snapshotId: frozen.snapshot.snapshotId, query: 'Mei', mode: 'literal', limit: 10 } });
    await invoke('revoke_story_context', { access, expectedPolicy: epochs.policy });
    let revoked;
    try { await invoke('prepared_story_context', { access, packetId: packet.receipt.packetId }); } catch (error) { revoked = error; }
    return { prepared, packet, current, oldEvidence, revoked };
  }, persisted.reopened);
  assert.equal(contextProof.prepared.current, true);
  assert.deepEqual(contextProof.packet, contextProof.prepared.packet);
  assert.equal(contextProof.packet.messages.at(-1).content, 'Who waited beneath the lantern?');
  assert.equal(contextProof.packet.options.modelId, 'mock-story-context');
  assert.equal(contextProof.current, false);
  assert.equal(contextProof.oldEvidence.hits.length, 1);
  assert.equal(contextProof.revoked.code, 'ContextPolicyChanged');
  checks.push('Native context IPC freezes exact sources, persists a mock-only packet receipt, marks it stale after editing and revokes further reads on policy change');

  // F2-B reviewed continuation stays an explicit restricted-writing basis:
  // the current target remains working prose, while only the exact earlier
  // reviewed prefix can enter the frozen manifest. Keep this fixture outside
  // the visible writer/library flow so it cannot change the UI document count.
  const reviewedContinuation = await page.evaluate(async ({ reviewedPath }) => {
    const invoke = (command, args) => window.__TAURI_INTERNALS__.invoke(command, args);
    const body = (blockId, text) => ({ schemaVersion: 1, body: { type: 'doc', content: [{
      type: 'paragraph', attrs: { id: blockId }, content: text ? [{ type: 'text', text }] : [],
    }] } });
    const create = (access, operationId, documentId, title, kind, text) => invoke('create_document', { request: {
      access, operationId, documentId, title, kind, body: body(documentId, text),
    } });
    const opened = await invoke('create_project', {
      path: reviewedPath, title: 'Native reviewed continuation fixture', session: 'reviewed-continuation-session',
    });
    const first = await create(opened.access, 'reviewed-create-first', 'reviewed-first', 'First reviewed chapter', 'chapter', 'First reviewed prose.');
    const firstStage = await invoke('stage_author_review', { request: {
      access: opened.access, operationId: 'reviewed-stage-first', expected: first.head,
    } });
    const firstBundle = await invoke('mark_ready', { request: {
      access: opened.access, operationId: 'reviewed-ready-first', stageId: firstStage.id,
    } });
    const target = await create(opened.access, 'reviewed-create-target', 'reviewed-target', 'Current continuation', 'chapter', 'Current target prose.');
    const future = await create(opened.access, 'reviewed-create-future', 'reviewed-future', 'Future private chapter', 'chapter', 'Future chapter prose must stay private.');
    const privateNote = await create(opened.access, 'reviewed-create-note', 'reviewed-private-note', 'Private author note', 'note', 'Author room note must stay private.');
    const epochs = await invoke('context_epochs', { access: opened.access });
    const policy = {
      version: epochs.policy, audience: 'restrictedWriting', readerFrontier: '1', characterId: null,
      characterGrants: [], allowAlternatives: false, allowHistorical: false,
    };
    const frozen = await invoke('freeze_reviewed_continuation', { request: {
      access: opened.access, operationId: 'reviewed-freeze-target', expected: target.head, policy,
    } });
    const byDocument = new Map(frozen.snapshot.sources.map(source => [source.source.documentId, source]));
    const targetDescriptor = byDocument.get(target.head.documentId);
    const earlierDescriptor = byDocument.get(first.head.documentId);
    const futureDescriptor = byDocument.get(future.head.documentId) ?? null;
    const privateDescriptor = byDocument.get(privateNote.head.documentId) ?? null;
    const prefix = frozen.snapshot.reviewedBasis?.prefix ?? [];
    const scope = await invoke('capture_story_scope', {
      access: opened.access, snapshotId: frozen.snapshot.snapshotId, kind: 'wholeDocument', start: null, end: null,
    });
    const prepared = await invoke('prepare_story_context', { request: {
      access: opened.access, operationId: 'reviewed-prepare-target', snapshotId: frozen.snapshot.snapshotId,
      instruction: 'Continue the current chapter from the reviewed earlier story.', mandatoryHandles: [], scope,
      budget: { modelId: 'mock-story-context', contextWindowTokens: '20000', reservedOutputTokens: '1000', reservedProtocolTokens: '1000' },
    } });
    if (prepared.status !== 'prepared') throw new Error(JSON.stringify(prepared));
    const preparedCurrentBeforeEdit = await invoke('prepared_story_context_is_current', {
      access: opened.access, packetId: prepared.packet.receipt.packetId,
    });
    const editedFirst = await invoke('save_snapshot', { request: {
      access: opened.access, operationId: 'reviewed-edit-first', expected: first.head, localGeneration: '1', cause: 'typing',
      body: body(first.head.documentId, 'Changed first prose makes the old reviewed basis stale.'),
    } });
    const oldPacket = await invoke('prepared_story_context', {
      access: opened.access, packetId: prepared.packet.receipt.packetId,
    });
    const staleCurrent = await invoke('prepared_story_context_is_current', {
      access: opened.access, packetId: prepared.packet.receipt.packetId,
    });
    const oldEvidence = await invoke('search_story_context', { request: {
      access: opened.access, snapshotId: frozen.snapshot.snapshotId, query: 'First reviewed prose.', mode: 'literal', limit: 10,
    } });
    let staleFreeze;
    try {
      await invoke('freeze_reviewed_continuation', { request: {
        access: opened.access, operationId: 'reviewed-freeze-after-edit', expected: target.head, policy,
      } });
    } catch (error) { staleFreeze = error; }
    const revokedEpochs = await invoke('revoke_story_context', {
      access: opened.access, expectedPolicy: policy.version,
    });
    let revokedRead;
    try {
      await invoke('prepared_story_context', {
        access: opened.access, packetId: prepared.packet.receipt.packetId,
      });
    } catch (error) { revokedRead = error; }
    return {
      projectId: opened.project.projectId,
      firstBundle,
      targetHead: target.head,
      editedFirstHead: editedFirst.head,
      snapshotBasis: frozen.snapshot.basis,
      snapshotTarget: frozen.snapshot.target,
      snapshotSources: frozen.snapshot.sources.map(source => ({
        documentId: source.source.documentId, handle: source.handle, source: source.source,
        kind: source.kind, current: source.current,
      })),
      policy: frozen.policy,
      targetDescriptor: targetDescriptor ? {
        handle: targetDescriptor.handle, source: targetDescriptor.source,
        kind: targetDescriptor.kind, current: targetDescriptor.current,
      } : null,
      earlierDescriptor: earlierDescriptor ? {
        handle: earlierDescriptor.handle, source: earlierDescriptor.source,
        kind: earlierDescriptor.kind, current: earlierDescriptor.current,
      } : null,
      futureDescriptor: futureDescriptor ? { kind: futureDescriptor.kind, current: futureDescriptor.current } : null,
      privateDescriptor: privateDescriptor ? { kind: privateDescriptor.kind, current: privateDescriptor.current } : null,
      prefix,
      scopeKind: scope.kind,
      scopeSourceHash: scope.sourceHash,
      preparedCurrentBeforeEdit,
      preparedPacketId: prepared.packet.receipt.packetId,
      preparedSourceHandles: prepared.packet.receipt.sourceHandles,
      oldPacketId: oldPacket.receipt.packetId,
      oldEvidenceHits: oldEvidence.hits.length,
      oldEvidenceText: oldEvidence.hits[0]?.passage?.text ?? null,
      staleCurrent,
      staleFreeze,
      revokedPolicy: revokedEpochs.policy,
      revokedRead,
    };
  }, { reviewedPath: resolve(data, 'reviewed-continuation-project') });
  assert.equal(reviewedContinuation.snapshotBasis, 'reviewed');
  assert.equal(reviewedContinuation.policy.audience, 'restrictedWriting');
  assert.equal(reviewedContinuation.policy.readerFrontier, '1');
  const sortedSnapshotSources = [...reviewedContinuation.snapshotSources].sort((left, right) => left.documentId.localeCompare(right.documentId));
  assert.deepEqual(sortedSnapshotSources.map(({ documentId, kind, current }) => ({ documentId, kind, current })), [
    { documentId: 'reviewed-first', kind: 'reviewedAuthority', current: true },
    { documentId: 'reviewed-target', kind: 'currentDraft', current: true },
  ]);
  assert.equal(reviewedContinuation.targetDescriptor.kind, 'currentDraft');
  assert.equal(reviewedContinuation.earlierDescriptor.kind, 'reviewedAuthority');
  assert.deepEqual(
    reviewedContinuation.snapshotSources.find(source => source.documentId === 'reviewed-target'),
    reviewedContinuation.targetDescriptor && {
      documentId: 'reviewed-target', handle: reviewedContinuation.targetDescriptor.handle,
      source: reviewedContinuation.targetDescriptor.source, kind: 'currentDraft', current: true,
    },
  );
  assert.deepEqual(
    reviewedContinuation.snapshotSources.find(source => source.documentId === 'reviewed-first'),
    reviewedContinuation.earlierDescriptor && {
      documentId: 'reviewed-first', handle: reviewedContinuation.earlierDescriptor.handle,
      source: reviewedContinuation.earlierDescriptor.source, kind: 'reviewedAuthority', current: true,
    },
  );
  assert.equal(reviewedContinuation.futureDescriptor, null);
  assert.equal(reviewedContinuation.privateDescriptor, null);
  assert.equal(reviewedContinuation.prefix.length, 1);
  assert.equal(reviewedContinuation.prefix[0].documentId, 'reviewed-first');
  assert.equal(reviewedContinuation.prefix[0].bundleId, reviewedContinuation.firstBundle.id);
  assert.equal(reviewedContinuation.scopeKind, 'wholeDocument');
  assert.equal(reviewedContinuation.scopeSourceHash, reviewedContinuation.targetHead.bodyHash);
  assert.equal(reviewedContinuation.preparedCurrentBeforeEdit, true);
  assert(reviewedContinuation.preparedSourceHandles.includes(reviewedContinuation.targetDescriptor.handle));
  assert(reviewedContinuation.preparedSourceHandles.includes(reviewedContinuation.earlierDescriptor.handle));
  assert.equal(reviewedContinuation.oldPacketId, reviewedContinuation.preparedPacketId);
  assert.equal(reviewedContinuation.oldEvidenceHits, 1);
  assert.equal(reviewedContinuation.oldEvidenceText, 'First reviewed prose.');
  assert.equal(reviewedContinuation.staleCurrent, false);
  assert.equal(reviewedContinuation.staleFreeze?.code, 'ReviewBasisUnavailable');
  assert.equal(reviewedContinuation.revokedPolicy, '1');
  assert.equal(reviewedContinuation.revokedRead?.code, 'ContextPolicyChanged');
  checks.push('Native reviewed continuation freezes the current target separately from its exact reviewed prefix, excludes future/private sources, preserves stale evidence read-only, and revokes old reads after policy change');
  await page.getByRole('button', { name: 'Back to library', exact: true }).click();
  async function fillManuscript(text) {
    await page.getByRole('textbox', { name: 'Manuscript', exact: true }).fill(text);
    // Playwright's contenteditable fill may finish before ProseMirror's DOM
    // observer has applied the transaction. Exercise pending autosave only
    // after the actual editor model (not just its DOM) contains the new text.
    await page.waitForFunction(expected => document.querySelector('.tiptap')?.editor?.getText() === expected, text);
  }
  async function createWritingProject(title, kind, documentTitle, text) {
    await page.getByRole('button', { name: 'New project', exact: true }).click();
    await page.getByRole('textbox', { name: 'Project title', exact: true }).fill(title);
    await page.getByRole('button', { name: 'Create project', exact: true }).click();
    await page.getByRole('button', { name: 'Add your first document', exact: true }).click();
    await page.getByLabel('Start with', { exact: true }).selectOption(kind);
    await page.getByRole('textbox', { name: 'Title', exact: true }).fill(documentTitle);
    await page.getByRole('button', { name: 'Create', exact: true }).click();
    await page.getByRole('heading', { name: documentTitle, exact: true }).waitFor();
    await fillManuscript(text);
    await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  }
  await createWritingProject('Harbour A', 'character', 'Mei', 'Mei keeps the brass key. Her voice is quiet.');
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await createWritingProject('Harbour B', 'chapter', 'The empty pier', 'The tide carries a red lantern towards the pier.');
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('button', { name: /^Harbour A Last opened/ }).click();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), 'Mei keeps the brass key. Her voice is quiet.');
  await fillManuscript('Mei keeps the brass key. She has made her choice.');
  // Navigate while debounce is pending: the lifecycle guard must drain it.
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('button', { name: /^Harbour B Last opened/ }).click();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), 'The tide carries a red lantern towards the pier.');
  await page.reload();
  await page.getByRole('button', { name: /^Harbour A Last opened/ }).click();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), 'Mei keeps the brass key. She has made her choice.');
  await page.screenshot({ path: resolve(output, 'persistent-workspace.png') });
  checks.push('Native library creates character-first and chapter-first projects; typing, detach-after-flush switching and renderer reload retain isolated prose');
  await page.getByRole('button', { name: 'Duplicate', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('.trial-label')?.textContent.includes('Harbour A copy') || !!document.querySelector('[role="alert"]'));
  assert.equal(await page.getByRole('alert').count(), 0, await page.getByRole('alert').allTextContents().then(text => text.join('\n')));
  await page.locator('.trial-label').filter({ hasText: 'Harbour A copy' }).waitFor();
  await fillManuscript('Only the independent copy changes.');
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  // A click acknowledges the gesture, not asynchronous detach/flush. Kill only
  // after the Library is shown, so this scenario tests acknowledged retention.
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  const libraryBeforeRestart = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('library_snapshot'));
  assert.equal(libraryBeforeRestart.entries.length, 3);
  assert.equal(new Set(libraryBeforeRestart.entries.map(entry => entry.projectId)).size, 3);
  assert.equal(libraryBeforeRestart.pending.length, 0);
  await browser.close(); browser = undefined;
  const exited = new Promise(resolve => app.once('exit', resolve));
  app.kill(); await exited;
  // An exiting WebView child can briefly retain the old debugging endpoint.
  // A fresh port makes readiness belong to this new application process.
  const previousPort = port;
  do { port = await reservePort(); } while (port === previousPort);
  app = launch();
  const restartedAt = Date.now(); let restarted = false;
  while (Date.now() - restartedAt < 90000) {
    if (app.exitCode !== null || spawnError) throw new Error(`Native restart failed: ${appLog}`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(2000) });
      if (response.ok && (await response.json()).webSocketDebuggerUrl) {
        browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 10000 });
        restarted = true; break;
      }
    } catch { /* A new WebView2 process is starting. */ }
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  assert(restarted, `Native restart did not expose CDP: ${appLog}`);
  const restartedContext = browser.contexts()[0];
  page = restartedContext.pages()[0] ?? await restartedContext.waitForEvent('page', { timeout: 10000 });
  observedPage = page;
  page.on('pageerror', error => errors.push(error.message));
  await page.getByRole('button', { name: /^Harbour A Last opened/ }).click();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), 'Mei keeps the brass key. She has made her choice.');
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('button', { name: /^Harbour A copy Last opened/ }).click();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), 'Only the independent copy changes.');
  checks.push('Native duplicate uses an independent project; original and copy reopen with distinct prose after the desktop process is killed and restarted');

  // Continuation is qualified in its own synthetic project so the later
  // Harbour C passage/review checks retain their original fixture and counts.
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await createWritingProject('Continuation story', 'chapter', 'First chapter', 'The tide carried the lantern away.');
  await page.evaluate(() => document.querySelector('.tiptap').editor.commands.setContent({ type: 'doc', content: [
    { type: 'paragraph', attrs: { id: 'continuation-opening' }, content: [{ type: 'text', text: 'The tide carried the lantern away.' }] },
    { type: 'paragraph', attrs: { id: 'continuation-ending' }, content: [{ type: 'text', text: 'At the pier, Mei waited for an answer.' }] },
  ] }));
  await page.waitForFunction(() => document.querySelector('.tiptap').editor.getJSON().content.length === 2);
  await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  const continuationBefore = await page.evaluate(() => {
    window.continuationEditor = document.querySelector('.tiptap').editor;
    return window.continuationEditor.getJSON();
  });
  await page.getByRole('button', { name: 'Continue chapter', exact: true }).click();
  await page.getByRole('textbox', { name: 'What should happen next?', exact: true }).waitFor();
  await page.getByLabel('Story basis', { exact: true }).selectOption('reviewed');
  await page.getByRole('textbox', { name: 'What should happen next?', exact: true }).fill('Continue from the chapter ending with a quiet reveal.');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await page.getByRole('alert').filter({ hasText: 'There is no earlier reviewed chapter' }).waitFor();
  await page.getByLabel('Story basis', { exact: true }).selectOption('working');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  const continuationCard = page.locator('.proposal-card').filter({ hasText: 'Local test continuation' });
  await continuationCard.waitFor();
  const typedContinuation = await continuationCard.getByRole('textbox', { name: 'Continuation paragraphs', exact: true }).inputValue();
  assert.equal(typedContinuation.split('\n\n').length, 2);
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), continuationBefore);
  const editedContinuation = 'The door opened beneath the rain.\n\nThe lantern answered from the hall.';
  await continuationCard.getByRole('textbox', { name: 'Continuation paragraphs', exact: true }).fill(editedContinuation);
  await page.evaluate(() => {
    const fetch = window.fetch;
    window.continuationPrepareRequests = [];
    window.continuationPrepareLostAck = false;
    window.continuationPrepareRetryAck = false;
    window.continuationProposalReadAfterLoss = false;
    window.fetch = async (...args) => {
      const url = String(args[0]);
      const init = args[1];
      if (url.endsWith('/prepare_continuation')) {
        let parsed = null;
        try { parsed = typeof init?.body === 'string' ? JSON.parse(init.body) : null; } catch { /* The native command will report malformed input. */ }
        const request = parsed?.request ?? parsed;
        window.continuationPrepareRequests.push(structuredClone(request));
        const response = await fetch.apply(window, args);
        if (!window.continuationPrepareLostAck && response.headers.get('Tauri-Response') === 'ok') {
          window.continuationPrepareLostAck = true;
          return new Response(JSON.stringify({ code: 'UncertainOutcome', detail: 'Synthetic lost continuation preparation acknowledgment after commit' }),
            { headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' } });
        }
        if (window.continuationPrepareLostAck && response.headers.get('Tauri-Response') === 'ok') {
          window.continuationPrepareRetryAck = true;
          window.fetch = fetch;
        }
        return response;
      }
      const response = await fetch.apply(window, args);
      // ProposalPanel reconciles an uncertain preparation immediately. Hide
      // only that first read-back so the visible Check preview action proves
      // the exact request is safely replayed rather than merely reread.
      if (url.endsWith('/proposals') && window.continuationPrepareLostAck && !window.continuationProposalReadAfterLoss
        && response.headers.get('Tauri-Response') === 'ok') {
        const payload = await response.clone().json();
        if (Array.isArray(payload)) {
          window.continuationProposalReadAfterLoss = true;
          const hidden = payload.map(item => item.id === window.continuationPrepareRequests[0]?.proposalId ? { ...item, prepared: null } : item);
          return new Response(JSON.stringify(hidden), { headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'ok' } });
        }
      }
      return response;
    };
  });
  await continuationCard.getByRole('button', { name: 'Preview', exact: true }).click();
  await page.waitForFunction(() => window.continuationPrepareLostAck === true);
  await continuationCard.getByRole('button', { name: 'Check preview', exact: true }).click();
  await continuationCard.locator('.continuation-after p').nth(0).filter({ hasText: 'The door opened beneath the rain.' }).waitFor();
  await continuationCard.locator('.continuation-after p').nth(1).filter({ hasText: 'The lantern answered from the hall.' }).waitFor();
  await page.waitForFunction(() => window.continuationPrepareRequests?.length === 2 && window.continuationPrepareRetryAck === true);
  const continuationPrepareRequests = await page.evaluate(() => window.continuationPrepareRequests);
  assert.equal(continuationPrepareRequests.length, 2);
  assert.equal(continuationPrepareRequests[1].operationId, continuationPrepareRequests[0].operationId);
  assert.equal(continuationPrepareRequests[1].proposalId, continuationPrepareRequests[0].proposalId);
  assert.deepEqual(continuationPrepareRequests[1].paragraphs, continuationPrepareRequests[0].paragraphs);
  assert.deepEqual(continuationPrepareRequests[1].body, continuationPrepareRequests[0].body);
  const preparedParagraphIds = continuationPrepareRequests[0].body.body.content.slice(-2).map(block => block.attrs.id);
  assert.deepEqual(continuationPrepareRequests[1].body.body.content.slice(-2).map(block => block.attrs.id), preparedParagraphIds);
  const continuationProposalId = (await continuationCard.getAttribute('data-testid')).replace(/^proposal-/, '');
  const continuationLibrary = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('library_snapshot'));
  const continuationEntry = continuationLibrary.entries.find(entry => entry.title === 'Continuation story');
  assert(continuationEntry, 'Continuation project must remain in the local library');
  const continuationProjectPath = await realpath(continuationEntry.path);
  const continuationDatabase = new DatabaseSync(resolve(continuationProjectPath, 'project.sqlite3'));
  try {
    assert.equal(continuationDatabase.prepare('SELECT count(*) AS count FROM proposal_versions WHERE proposal_id=?').get(continuationProposalId).count, 1);
    assert.equal(continuationDatabase.prepare('SELECT count(*) AS count FROM proposal_receipts WHERE operation_id=? AND kind=\'prepare\'').get(continuationPrepareRequests[0].operationId).count, 1);
  } finally { continuationDatabase.close(); }
  assert(await page.evaluate(() => document.querySelector('.tiptap').editor === window.continuationEditor));
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), continuationBefore);
  await page.screenshot({ path: resolve(output, 'continuation-preview.png') });
  await continuationCard.getByRole('button', { name: 'Apply', exact: true }).click();
  await continuationCard.locator('.proposal-status').filter({ hasText: /^Applied$/ }).waitFor();
  const continuationAfter = await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());
  assert(await page.evaluate(() => document.querySelector('.tiptap').editor === window.continuationEditor));
  assert.deepEqual(continuationAfter.content.slice(0, continuationBefore.content.length), continuationBefore.content);
  assert.deepEqual(continuationAfter.content.slice(-2).map(block => block.content?.[0]?.text), ['The door opened beneath the rain.', 'The lantern answered from the hall.']);
  await page.screenshot({ path: resolve(output, 'continuation-post-apply.png') });
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('.tiptap').editor.getJSON().content.length === 2);
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), continuationBefore);
  assert(await page.evaluate(() => document.querySelector('.tiptap').editor === window.continuationEditor));
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('.tiptap').editor.getJSON().content.length === 4);
  await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  const continuationFinal = await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());
  assert.deepEqual(continuationFinal, continuationAfter);
  assert(await page.evaluate(() => document.querySelector('.tiptap').editor === window.continuationEditor));
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.reload();
  await page.getByRole('button', { name: /^Continuation story Last opened/ }).click();
  await page.getByRole('heading', { name: 'First chapter', exact: true }).waitFor();
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), continuationFinal);
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await page.getByRole('heading', { name: 'Saved versions', exact: true }).waitFor();
  const continuationVersions = page.getByRole('combobox', { name: 'Saved version', exact: true });
  await page.waitForFunction(() => document.querySelector('#saved-version')?.options.length > 1);
  const continuationLabels = await continuationVersions.locator('option').allTextContents();
  const appliedContinuation = continuationLabels.find(label => label.includes('Applied edit'));
  assert(appliedContinuation, 'The applied continuation must remain in saved history');
  await continuationVersions.selectOption({ label: appliedContinuation });
  await page.locator('.history-preview .saved-prose').filter({ hasText: 'The door opened beneath the rain.' }).waitFor();
  const retainedContinuation = await page.locator('.history-preview .saved-prose').innerText();
  assert(retainedContinuation.includes('At the pier, Mei waited for an answer.'));
  assert(retainedContinuation.includes('The lantern answered from the hall.'));
  await page.screenshot({ path: resolve(output, 'continuation-applied.png') });
  await page.getByRole('button', { name: 'Back to writing', exact: true }).click();
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('button', { name: /^Harbour A copy Last opened/ }).click();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), 'Only the independent copy changes.');
  checks.push('Native continuation refuses an unavailable reviewed prefix, requires explicit working-draft fallback, retries one lost preparation acknowledgment with the exact operation/body/IDs and one stored version, previews typed paragraphs without mutating the mounted editor, applies after the unchanged ending, survives visible undo/redo, and retains the new body in history after reload');
  const editorBeforeRename = await page.evaluate(() => { window.editorBeforeRename = document.querySelector('.tiptap').editor; return window.editorBeforeRename.getJSON(); });
  await page.getByRole('button', { name: 'Rename', exact: true }).click();
  await page.getByRole('textbox', { name: 'Project title', exact: true }).fill('Harbour C');
  await page.getByRole('button', { name: 'Save title', exact: true }).click();
  await page.locator('.trial-label').filter({ hasText: /^Harbour C$/ }).waitFor();
  await page.getByRole('button', { name: 'Rename document', exact: true }).click();
  await page.getByRole('textbox', { name: 'Document title', exact: true }).fill("Mei's voice");
  await page.getByRole('button', { name: 'Save document title', exact: true }).click();
  await page.getByRole('heading', { name: "Mei's voice", exact: true }).waitFor();
  assert(await page.evaluate(() => document.querySelector('.tiptap').editor === window.editorBeforeRename));
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), editorBeforeRename);
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await page.getByLabel('Start with', { exact: true }).selectOption('note');
  await page.getByRole('textbox', { name: 'Title', exact: true }).fill('Ending to protect');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await page.getByRole('heading', { name: 'Ending to protect', exact: true }).waitFor();
  await fillManuscript('Keep the final lantern burning.');
  await page.evaluate(() => document.querySelector('.tiptap').editor.commands.setTextSelection(7));
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('button', { name: /^Harbour C Last opened/ }).click();
  await page.getByRole('heading', { name: 'Ending to protect', exact: true }).waitFor();
  assert.equal(await page.evaluate(() => document.querySelector('.tiptap').editor.state.selection.anchor), 7);
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), 'Keep the final lantern burning.');
  await page.reload();
  await page.getByRole('button', { name: /^Harbour C Last opened/ }).click();
  await page.getByRole('heading', { name: 'Ending to protect', exact: true }).waitFor();
  assert.equal(await page.evaluate(() => document.querySelector('.tiptap').editor.state.selection.anchor), 7);
  await page.locator('.persistent-source-pins>summary').click();
  await page.getByRole('combobox', { name: 'Story source', exact: true }).selectOption({ label: "Mei's voice" });
  await page.getByRole('button', { name: 'Keep source', exact: true }).click();
  await page.locator('.persistent-source-list li').filter({ hasText: "Mei's voice" }).waitFor();
  await page.getByRole('combobox', { name: 'Story source', exact: true }).selectOption({ label: 'Ending to protect' });
  await page.getByRole('combobox', { name: 'Use in discussions for', exact: true }).selectOption('project');
  await page.getByRole('button', { name: 'Keep source', exact: true }).click();
  await page.locator('.persistent-source-list li').filter({ hasText: 'Ending to protectThis project' }).waitFor();
  await page.screenshot({ path: resolve(output, 'persistent-source-pins.png') });
  const beforeDiscussion = await page.evaluate(() => { window.discussionEditor = document.querySelector('.tiptap').editor; window.discussionEditor.commands.setTextSelection({ from: 10, to: 23 }); return window.discussionEditor.getJSON(); });
  await page.getByRole('button', { name: 'Discuss selection', exact: true }).click();
  await page.locator('.persistent-feedback .quoted-scope blockquote').filter({ hasText: /^final lantern$/ }).waitFor();
  await page.getByRole('textbox', { name: 'Discuss this passage', exact: true }).fill('Keep this image, but make its meaning less obvious.');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await page.locator('.persistent-feedback article').filter({ hasText: 'This test confirms discussion and context handling' }).waitFor();
  assert(await page.evaluate(() => document.querySelector('.tiptap').editor === window.discussionEditor));
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), beforeDiscussion);
  await page.locator('.context-inspector>summary').click();
  await page.locator('.context-inspector summary').filter({ hasText: /^Used/ }).waitFor();
  await page.waitForFunction(() => [...document.querySelectorAll('.context-inspector details[open] .context-detail')].filter(item => item.textContent === 'Required source').length === 1);
  await page.locator('.context-inspector details[open]').getByRole('button', { name: "Keep Mei's voice for future discussions", exact: true }).click();
  // The inspector opens confirmation through a React adoption effect. A
  // completed click does not guarantee that the controlled chooser committed.
  await page.waitForFunction(() => document.querySelector('.source-pin-form select')?.selectedOptions[0]?.text === "Mei's voice");
  assert.equal(await page.getByRole('combobox', { name: 'Story source', exact: true }).evaluate(element => element.selectedOptions[0].text), "Mei's voice");
  assert.equal(await page.locator('.persistent-source-list li').count(), 2, 'Opening source confirmation must not save another pin');
  await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).fill('Remember the promise from this scene.');
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('button', { name: /^Harbour C Last opened/ }).click();
  await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).waitFor();
  assert.equal(await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).inputValue(), 'Remember the promise from this scene.');
  await page.locator('.persistent-feedback article').filter({ hasText: /^You/ }).filter({ hasText: 'Keep this image, but make its meaning less obvious.' }).waitFor();
  await page.reload();
  await page.getByRole('button', { name: /^Harbour C Last opened/ }).click();
  await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).waitFor();
  assert.equal(await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).inputValue(), 'Remember the promise from this scene.');
  await page.locator('.persistent-feedback article').filter({ hasText: 'This test confirms discussion and context handling' }).waitFor();
  await page.screenshot({ path: resolve(output, 'persistent-discussion.png') });
  checks.push('Native selected discussion retains its exact quote, mock response and context receipt without replacing prose; unsent composer and conversation survive switching and renderer reload');
  await page.locator('.persistent-source-pins>summary').click();
  await page.locator('.persistent-source-list li').filter({ hasText: "Mei's voiceThis document" }).waitFor();
  await page.locator('.persistent-source-list li').filter({ hasText: 'Ending to protectThis project' }).waitFor();
  checks.push('Persistent document/project source choices survive native navigation/reload and are marked required in the frozen discussion packet without changing prose');
  const beforeGuidance = await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());
  await page.locator('.persistent-feedback article').filter({ hasText: /^You/ }).filter({ hasText: 'Keep this image, but make its meaning less obvious.' }).getByRole('button', { name: 'Keep as guidance', exact: true }).click();
  await page.getByRole('textbox', { name: 'Direction', exact: true }).fill('Preserve the final lantern image and keep the ending intact.');
  await page.getByRole('combobox', { name: 'Apply to', exact: true }).selectOption('document');
  await page.getByRole('button', { name: 'Save guidance', exact: true }).click();
  await page.locator('.guidance-item p').filter({ hasText: /^Preserve the final lantern image and keep the ending intact\.$/ }).waitFor({ state: 'attached' });
  await page.locator('.guidance-item').scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(output, 'author-guidance.png') });
  await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).fill('Discuss the effect of the closing image.');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await page.waitForFunction(() => [...document.querySelectorAll('.persistent-feedback article')].filter(item => item.textContent.includes('This test confirms discussion and context handling')).length === 2);
  if (await page.locator('.context-inspector').getAttribute('open') === null) await page.locator('.context-inspector>summary').click();
  await page.locator('.context-inspector details[open] .context-guidance p').filter({ hasText: /^Preserve the final lantern image and keep the ending intact\.$/ }).waitFor();
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), beforeGuidance);
  await page.locator('.context-inspector details[open] .context-guidance').scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(output, 'guidance-receipt.png') });
  const earlierExchange = page.locator('.context-inspector > details[open] .context-turn').first();
  await earlierExchange.locator('summary').click();
  await earlierExchange.getByText('Keep this image, but make its meaning less obvious.', { exact: true }).waitFor();
  await earlierExchange.getByText('This test confirms discussion and context handling', { exact: false }).waitFor();
  await earlierExchange.scrollIntoViewIfNeeded();
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), beforeGuidance);
  await page.screenshot({ path: resolve(output, 'discussion-context.png') });
  checks.push('Native follow-up discussion receives and displays the exact earlier complete exchange separately from saved guidance without changing the manuscript');
  await page.reload();
  await page.getByRole('button', { name: /^Harbour C Last opened/ }).click();
  await page.locator('.guidance-item p').filter({ hasText: /^Preserve the final lantern image and keep the ending intact\.$/ }).waitFor({ state: 'attached' });
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), beforeGuidance);
  checks.push('Native Keep as guidance stores the confirmed document instruction, supplies its exact version in the next packet, survives reload and never changes manuscript text');
  await page.getByRole('button', { name: 'Add guidance', exact: true }).click();
  await page.getByRole('textbox', { name: 'Direction', exact: true }).fill('Keep the promise intact for this attempt.');
  await page.getByRole('combobox', { name: 'Apply to', exact: true }).selectOption('request');
  await page.getByRole('button', { name: 'Save guidance', exact: true }).click();
  await page.locator('.guidance-item p').filter({ hasText: /^Keep the promise intact for this attempt\.$/ }).waitFor({ state: 'attached' });
  await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).fill('Discuss the old promise without changing the ending.');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await page.getByRole('button', { name: 'Stop response', exact: true }).click();
  await page.locator('.discussion-state').filter({ hasText: 'This response is stopped.' }).waitFor();
  await page.getByRole('button', { name: 'Prepare another attempt', exact: true }).click();
  await page.locator('.retry-notice').waitFor();
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.reload();
  await page.getByRole('button', { name: /^Harbour C Last opened/ }).click();
  await page.locator('.retry-notice').waitFor();
  assert.equal(await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).inputValue(), 'Discuss the old promise without changing the ending.');
  await page.screenshot({ path: resolve(output, 'discussion-retry.png') });
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await page.locator('.retry-notice').waitFor({ state: 'detached' });
  if (await page.locator('.context-inspector').getAttribute('open') === null) await page.locator('.context-inspector>summary').click();
  await page.locator('.context-inspector summary').filter({ hasText: /^Used/ }).waitFor();
  await page.locator('.context-inspector details[open] .context-guidance p').filter({ hasText: /^Keep the promise intact for this attempt\.$/ }).waitFor();
  await page.getByRole('button', { name: 'Stop response', exact: true }).waitFor({ state: 'detached' });
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), beforeGuidance);
  checks.push('Native stopped discussion retry retains one-use guidance and the saved retry choice across navigation/reload, then supplies the exact instruction without changing prose');
  const retryLibrary = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('library_snapshot'));
  const retryProjectPath = await realpath(retryLibrary.entries.find(entry => entry.title === 'Harbour C').path);
  const retryRelative = relative(toNamespacedPath(await realpath(data)), toNamespacedPath(retryProjectPath));
  assert(retryRelative && !isAbsolute(retryRelative) && retryRelative !== '..' && !retryRelative.startsWith(`..${sep}`), 'Fault injection must stay in this synthetic run directory');
  const retryDatabase = new DatabaseSync(resolve(retryProjectPath, 'project.sqlite3'));
  try {
    retryDatabase.exec("CREATE TRIGGER native_terminal_failure BEFORE INSERT ON discussion_messages WHEN NEW.role='assistant' BEGIN SELECT RAISE(ABORT,'synthetic terminal save fault'); END;");
    await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).fill('Check local saving recovery without changing this ending.');
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    await page.getByRole('button', { name: 'Retry saving response', exact: true }).waitFor();
    const runsBeforeLocalRetry = retryDatabase.prepare('SELECT count(*) AS count FROM discussion_runs').get().count;
    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
    await page.reload();
    await page.getByRole('button', { name: /^Harbour C Last opened/ }).click();
    await page.getByRole('button', { name: 'Retry saving response', exact: true }).waitFor();
    await page.screenshot({ path: resolve(output, 'response-save-recovery.png') });
    retryDatabase.exec('DROP TRIGGER native_terminal_failure;');
    await page.getByRole('button', { name: 'Retry saving response', exact: true }).click();
    await page.getByRole('button', { name: 'Retry saving response', exact: true }).waitFor({ state: 'detached' });
    assert.equal(retryDatabase.prepare('SELECT count(*) AS count FROM discussion_runs').get().count, runsBeforeLocalRetry);
    const recoveredRun = retryDatabase.prepare('SELECT status,output_text FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get();
    assert.equal(recoveredRun.status, 'completed');
    assert(recoveredRun.output_text.includes('Check local saving recovery'));
    assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), beforeGuidance);
  } finally {
    retryDatabase.exec('DROP TRIGGER IF EXISTS native_terminal_failure;');
    retryDatabase.close();
  }
  checks.push('Native failed response save stays visible across navigation and renderer reload; local retry commits the retained response without another run or manuscript change');
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await page.getByLabel('Start with', { exact: true }).selectOption('chapter');
  await page.getByRole('textbox', { name: 'Title', exact: true }).fill('The promise on the pier');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await page.getByRole('heading', { name: 'The promise on the pier', exact: true }).waitFor();
  const originalProse = 'Mei held the lantern. The ending stays unchanged.';
  const appliedProse = 'Her sister held the lantern. The ending stays unchanged.';
  await fillManuscript(originalProse);
  await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  const privatePlan = 'Private plan: the mentor stole the lantern years ago. Do not reveal the theft.';
  await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).fill(privatePlan);
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await page.locator('.feedback-note p').filter({ hasText: privatePlan }).first().waitFor();
  await page.waitForFunction(() => document.querySelector('#discussion-composer')?.value === '');
  await page.getByRole('button', { name: 'Stop response', exact: true }).waitFor({ state: 'detached' });
  await page.evaluate(() => document.querySelector('.tiptap').editor.commands.setTextSelection({ from: 1, to: 4 }));
  await page.getByRole('button', { name: 'Discuss selection', exact: true }).click();
  await page.locator('.persistent-feedback .quoted-scope blockquote').filter({ hasText: /^Mei$/ }).waitFor();
  await page.getByRole('button', { name: 'Suggest edits', exact: true }).click();
  await page.locator('.feedback-note').filter({ hasText: privatePlan }).first().getByRole('button', { name: 'Adapt as writing brief', exact: true }).click();
  assert.equal(await page.getByRole('button', { name: 'Send', exact: true }).isEnabled(), false);
  const approvedBrief = 'Let her sister hold the lantern. Preserve the ending.';
  await page.getByRole('textbox', { name: 'Directions for this edit request', exact: true }).fill(approvedBrief);
  await page.getByRole('button', { name: 'Approve this brief', exact: true }).click();
  await page.getByRole('textbox', { name: 'Request edits for this passage', exact: true }).fill('Change who holds the lantern. Preserve everything outside the selected name.');
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.reload();
  await page.getByRole('button', { name: /^Harbour C Last opened/ }).click();
  assert.equal(await page.getByRole('textbox', { name: 'Directions for this edit request', exact: true }).inputValue(), approvedBrief);
  await page.getByText('Writing brief approved', { exact: true }).waitFor();
  await page.locator('.safe-brief-editor').scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(output, 'writing-brief-approved.png') });
  await page.evaluate(() => {
    const fetch = window.fetch;
    window.fetch = async (...args) => {
      const response = await fetch.apply(window, args);
      if (String(args[0]).endsWith('/start_discussion') && response.headers.get('Tauri-Response') === 'ok') {
        window.fetch = fetch;
        window.nativeBriefPacket = (await response.clone().json()).packet;
      }
      return response;
    };
  });
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await page.waitForFunction(() => document.querySelectorAll('.proposal-card').length === 3);
  const briefPacket = await page.evaluate(() => window.nativeBriefPacket);
  assert.equal(briefPacket.receipt.safeBrief.text, approvedBrief);
  assert.equal(JSON.parse(briefPacket.messages[1].content).approvedWritingBrief, approvedBrief);
  assert.equal(JSON.stringify(briefPacket.messages).includes(privatePlan), false);
  assert.equal(JSON.stringify(briefPacket.messages).includes(briefPacket.receipt.safeBrief.originMessageId), false);
  if (await page.locator('.context-inspector').getAttribute('open') === null) await page.locator('.context-inspector>summary').click();
  await page.locator('.context-safe-brief p').filter({ hasText: approvedBrief }).waitFor();
  checks.push('Native writing-brief adoption requires explicit approval, survives draft reload, supplies only exact approved directions rather than private planning, and remains inspectable without changing prose');
  let chosen = page.locator('.proposal-card').filter({ hasText: 'Mock clarity option' });
  await chosen.getByRole('textbox', { name: 'Replacement wording', exact: true }).fill('Her sister');
  await chosen.getByRole('button', { name: 'Preview', exact: true }).click();
  await chosen.locator('.after-text').filter({ hasText: /^Her sister$/ }).waitFor();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), originalProse);
  await chosen.scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(output, 'proposal-preview.png') });
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.reload();
  await page.getByRole('button', { name: /^Harbour C Last opened/ }).click();
  chosen = page.locator('.proposal-card').filter({ hasText: 'Mock clarity option' });
  await chosen.locator('.after-text').filter({ hasText: /^Her sister$/ }).waitFor();
  assert.equal(await chosen.getByRole('textbox', { name: 'Replacement wording', exact: true }).inputValue(), 'Her sister');
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), originalProse);
  await page.evaluate(() => { window.beforeDurableApply = document.querySelector('.tiptap').editor; });
  await chosen.getByRole('button', { name: 'Apply', exact: true }).click();
  await chosen.locator('.proposal-status').filter({ hasText: /^Applied$/ }).waitFor();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), appliedProse);
  assert(await page.evaluate(() => document.querySelector('.tiptap').editor === window.beforeDurableApply));
  assert.equal(await page.locator('.proposal-stale').count(), 2);
  const rejected = page.locator('.proposal-card').filter({ hasText: 'Mock focus option' });
  assert.equal(await rejected.getByRole('button', { name: 'Apply', exact: true }).isEnabled(), false);
  await rejected.getByRole('button', { name: 'Reject', exact: true }).click();
  await rejected.locator('.proposal-status').filter({ hasText: /^Rejected$/ }).waitFor();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), appliedProse);
  await chosen.scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(output, 'proposal-applied.png') });
  checks.push('Native chapter passage suggestions retain three alternatives and edited previews across reload; explicit Apply preserves the mounted editor and protected ending, leaves other options pending/stale, and Reject does not change prose');
  await page.getByRole('textbox', { name: 'Manuscript', exact: true }).focus();
  // Hold a real background caret-save acknowledgement while the author uses
  // Redo. Remembering a reading position must not disable or blur the editor.
  await page.evaluate(() => {
    const fetch = window.fetch;
    window.fetch = async (...args) => {
      const response = await fetch.apply(window, args);
      if (String(args[0]).endsWith('/save_view_state') && response.headers.get('Tauri-Response') === 'ok') {
        window.fetch = fetch;
        window.backgroundViewHeld = true;
        await new Promise(resolve => { window.releaseBackgroundView = resolve; });
        window.backgroundViewHeld = false;
      }
      return response;
    };
  });
  await page.keyboard.press('Control+z');
  await page.waitForFunction(text => document.querySelector('.tiptap').editor.getText() === text, originalProse);
  await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  await page.waitForFunction(() => window.backgroundViewHeld === true);
  assert(await page.evaluate(() => document.activeElement === document.querySelector('.tiptap') && document.querySelector('.tiptap').isContentEditable));
  await page.keyboard.press('Control+Shift+z');
  await page.waitForFunction(text => document.querySelector('.tiptap').editor.getText() === text, appliedProse);
  await page.evaluate(() => window.releaseBackgroundView());
  await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  checks.push('Delayed real caret-save acknowledgement leaves manuscript focus and editing available; native Redo succeeds while the background save is pending');
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.reload();
  await page.getByRole('button', { name: /^Harbour C Last opened/ }).click();
  await page.getByRole('heading', { name: 'The promise on the pier', exact: true }).waitFor();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), appliedProse);
  await page.locator('.proposal-card').filter({ hasText: 'Mock clarity option' }).locator('.proposal-status').filter({ hasText: /^Applied$/ }).waitFor();
  await page.locator('.proposal-card').filter({ hasText: 'Mock focus option' }).locator('.proposal-status').filter({ hasText: /^Rejected$/ }).waitFor();
  checks.push('Native durable Apply is one undo event; undo and redo save through Rust and the final body plus explicit decisions survive navigation and renderer reload');
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await page.getByRole('heading', { name: 'Saved versions', exact: true }).waitFor();
  const versions = page.getByRole('combobox', { name: 'Saved version', exact: true });
  await page.waitForFunction(() => document.querySelector('#saved-version')?.options.length > 1);
  const labels = await versions.locator('option').allTextContents();
  const originalVersion = labels.find(label => label.endsWith('Version 1'));
  assert(originalVersion, 'The original writing must be available in saved versions');
  await versions.selectOption({ label: originalVersion });
  await page.locator('.history-preview .saved-prose').filter({ hasText: originalProse }).waitFor();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), appliedProse);
  await page.screenshot({ path: resolve(output, 'history-comparison.png') });
  // Drop only this renderer acknowledgement after the real Rust command has
  // committed. The subsequent reconciliation still calls the native actor.
  await page.evaluate(() => {
    window.beforeDurableRestore = document.querySelector('.tiptap').editor;
    const fetch = window.fetch;
    window.fetch = async (...args) => {
      const response = await fetch.apply(window, args);
      if (String(args[0]).endsWith('/restore_revision') && response.headers.get('Tauri-Response') === 'ok') {
        const committed = await response.clone().json();
        if (!committed.result?.restored) throw new Error('Expected a real committed restore receipt');
        window.fetch = fetch; window.restoreAcknowledgmentDropped = true;
        // Return a synthetic IPC error after COMMIT. Rejecting fetch itself
        // would exercise Tauri's alternate transport replay instead.
        return new Response(JSON.stringify({ code: 'UncertainOutcome', detail: 'Synthetic lost acknowledgment after restore commit' }),
          { headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' } });
      }
      return response;
    };
  });
  await page.getByRole('button', { name: 'Restore this version', exact: true }).click();
  await page.getByRole('button', { name: 'Check saved version', exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.restoreAcknowledgmentDropped), true);
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), appliedProse);
  await page.getByRole('button', { name: 'Check saved version', exact: true }).click();
  await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), originalProse);
  assert(await page.evaluate(() => document.querySelector('.tiptap').editor === window.beforeDurableRestore));
  await page.getByRole('button', { name: 'Back to writing', exact: true }).click();
  await page.getByRole('textbox', { name: 'Manuscript', exact: true }).focus();
  await page.keyboard.press('Control+z');
  await page.waitForFunction(text => document.querySelector('.tiptap').editor.getText() === text, appliedProse);
  await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  await page.keyboard.press('Control+Shift+z');
  await page.waitForFunction(text => document.querySelector('.tiptap').editor.getText() === text, originalProse);
  await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.reload();
  await page.getByRole('button', { name: /^Harbour C Last opened/ }).click();
  await page.getByRole('heading', { name: 'The promise on the pier', exact: true }).waitFor();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), originalProse);
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await page.waitForFunction(() => [...document.querySelector('#saved-version')?.options ?? []].some(option => option.text.endsWith('Version 4')));
  const restoredLabels = await page.getByRole('combobox', { name: 'Saved version', exact: true }).locator('option').allTextContents();
  await page.getByRole('combobox', { name: 'Saved version', exact: true }).selectOption({ label: restoredLabels.find(label => label.endsWith('Version 4')) });
  await page.locator('.history-preview .saved-prose').filter({ hasText: appliedProse }).waitFor();
  await page.screenshot({ path: resolve(output, 'history-after-restore.png') });
  await page.getByRole('button', { name: 'Back to writing', exact: true }).click();
  checks.push('Native saved-version comparison is read-only; explicit restore recovers a lost commit acknowledgment through the same mounted editor, is one undo event, and retains both versions after reload');
  await page.evaluate(() => document.querySelector('.tiptap').editor.commands.setTextSelection({ from: 1, to: 4 }));
  await page.getByRole('button', { name: 'Bold', exact: true }).click();
  await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  const exportSource = await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());
  await page.getByRole('button', { name: 'Export draft', exact: true }).click();
  const exportDialog = page.getByRole('dialog', { name: 'Export draft', exact: true });
  await exportDialog.getByRole('button', { name: 'Choose destination…', exact: true }).waitFor();
  await page.waitForFunction(() => document.querySelector('.export-preview')?.textContent.includes('**Mei**'));
  await page.screenshot({ path: resolve(output, 'export-preview.png') });
  await exportDialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption('plainText');
  await page.waitForFunction(text => document.querySelector('.export-preview')?.textContent === text, originalProse);
  await exportDialog.getByRole('button', { name: 'Choose destination…', exact: true }).click();
  await operateSaveDialog('Cancel');
  await exportDialog.getByRole('status').filter({ hasText: 'No destination chosen' }).waitFor();
  await exportDialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption('markdown');
  await page.waitForFunction(() => document.querySelector('.export-preview')?.textContent.includes('**Mei**'));
  const exportedText = await exportDialog.getByLabel('Exported file preview', { exact: true }).textContent();
  const exportDestination = resolve(data, 'chapter-draft.md');
  await exportDialog.getByRole('button', { name: 'Choose destination…', exact: true }).click();
  await operateSaveDialog('Save', exportDestination);
  await exportDialog.getByRole('button', { name: 'Done', exact: true }).waitFor();
  assert.equal(await readFile(exportDestination, 'utf8'), exportedText);
  await page.screenshot({ path: resolve(output, 'export-completed.png') });
  await exportDialog.getByRole('button', { name: 'Done', exact: true }).click();
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), exportSource);
  assert.equal(await page.evaluate(() => document.activeElement.textContent), 'Export draft');
  checks.push('Native draft export previews exact frozen Markdown/plain text, cancels the real Save dialog without writing, saves through native UI Automation, and preserves manuscript and return focus');
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('button', { name: 'Archive Harbour C', exact: true }).click();
  await page.getByRole('button', { name: /^Harbour C Last opened/ }).waitFor({ state: 'detached' });
  await page.getByRole('button', { name: 'Archived', exact: true }).click();
  await page.getByRole('button', { name: 'Unarchive Harbour C', exact: true }).click();
  await page.getByRole('button', { name: 'Show active', exact: true }).click();
  await page.getByRole('button', { name: /^Harbour C Last opened/ }).waitFor();
  checks.push('Native renames preserve the mounted editor; last document and exact caret survive navigation/reload; archive and unarchive preserve the project');
  const { qualifyReviewedExport } = await import(pathToFileURL(resolve(root, 'apps/desktop/scripts/native-reviewed-export.mjs')).href);
  await qualifyReviewedExport({ page, data, output, operateSaveDialog, createWritingProject, checks });
  const { qualifyReviewedEvidence } = await import(pathToFileURL(resolve(root, 'apps/desktop/scripts/native-reviewed-evidence.mjs')).href);
  await qualifyReviewedEvidence({ page, data, output, createWritingProject, checks });
  const { qualifyStructuredSuggestions } = await import(pathToFileURL(resolve(root, 'apps/desktop/scripts/native-structured-suggestions.mjs')).href);
  await qualifyStructuredSuggestions({ page, data, output, createWritingProject, checks });
  const { runPromiseHistoryFlow } = await import(pathToFileURL(resolve(root, 'apps/desktop/scripts/native-promise-history.mjs')).href);
  await runPromiseHistoryFlow({ page, data, output, createWritingProject, checks });
  const { qualifyContextLookup } = await import(pathToFileURL(resolve(root, 'apps/desktop/scripts/native-context-lookup.mjs')).href);
  await qualifyContextLookup({ page, output, testRoot: data, createWritingProject, checks });
  await createWritingProject('Review story', 'chapter', 'The gate', 'Mei left the key beside the gate.');
  await page.getByRole('button', { name: 'Story review', exact: true }).click();
  await page.getByRole('button', { name: 'Review saved chapter', exact: true }).click();
  await page.getByLabel('Chapter under review', { exact: true }).getByText('Mei left the key beside the gate.', { exact: true }).waitFor();
  await page.screenshot({ path: resolve(output, 'author-review.png') });
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.reload();
  await page.getByRole('button', { name: /^Review story Last opened/ }).click();
  await page.getByRole('button', { name: 'Story review', exact: true }).click();
  await page.getByRole('button', { name: 'Resume saved review', exact: true }).click();
  await page.getByLabel('Chapter under review', { exact: true }).getByText('Mei left the key beside the gate.', { exact: true }).waitFor();
  await page.evaluate(() => {
    window.reviewEditor = document.querySelector('.tiptap').editor;
    const fetch = window.fetch;
    window.fetch = async (...args) => {
      const response = await fetch.apply(window, args);
      if (String(args[0]).endsWith('/mark_ready') && response.headers.get('Tauri-Response') === 'ok') {
        const committed = await response.clone().json();
        if (!committed.id || !committed.stageId) throw new Error('Expected a real saved review');
        window.fetch = fetch; window.reviewAcknowledgmentDropped = true;
        return new Response(JSON.stringify({ code: 'UncertainOutcome', detail: 'Synthetic lost review acknowledgment after commit' }),
          { headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' } });
      }
      return response;
    };
  });
  await page.getByRole('button', { name: 'Mark this version reviewed', exact: true }).click();
  await page.getByRole('button', { name: 'Check review save', exact: true }).click();
  await page.getByRole('heading', { name: 'Reviewed version is current', exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.reviewAcknowledgmentDropped), true);
  assert.equal(await page.evaluate(() => window.reviewEditor === document.querySelector('.tiptap').editor), true);
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), 'Mei left the key beside the gate.');
  await page.getByRole('button', { name: 'Back to writing', exact: true }).click();
  assert.equal(await page.evaluate(() => document.activeElement.textContent), 'Story review');
  checks.push('Native author review previews exact prose, resumes an unaccepted stage after restart, requires explicit acceptance, reconciles a lost commit acknowledgment, preserves the mounted editor and restores keyboard focus');
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await page.getByLabel('Start with', { exact: true }).selectOption('chapter');
  await page.getByRole('textbox', { name: 'Title', exact: true }).fill('The return');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await page.getByRole('heading', { name: 'The return', exact: true }).waitFor();
  await fillManuscript('Ren returned to the empty gate.');
  await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  await page.getByRole('button', { name: 'Story review', exact: true }).click();
  await page.getByRole('button', { name: 'Review saved chapter', exact: true }).click();
  await page.locator('.review-basis li').filter({ hasText: /^The gate$/ }).waitFor();
  await page.locator('.review-basis summary').filter({ hasText: /^The gate$/ }).click();
  await page.locator('.review-earlier-prose').getByText('Mei left the key beside the gate.', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'Mark this version reviewed', exact: true }).click();
  await page.getByRole('heading', { name: 'Reviewed version is current', exact: true }).waitFor();
  await page.getByRole('button', { name: 'Back to writing', exact: true }).click();
  await page.locator('.document-sidebar nav button').filter({ hasText: /^The gate/ }).click();
  await page.getByRole('heading', { name: 'The gate', exact: true }).waitFor();
  await fillManuscript('Mei carried the key away from the gate.');
  await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  await page.locator('.document-sidebar nav button').filter({ hasText: /^The return/ }).click();
  await page.getByRole('heading', { name: 'The return', exact: true }).waitFor();
  await page.getByRole('button', { name: 'Story review', exact: true }).click();
  await page.getByRole('heading', { name: 'Earlier story needs review', exact: true }).waitFor();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), 'Ren returned to the empty gate.');
  await page.screenshot({ path: resolve(output, 'review-earlier-change.png') });
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.reload();
  await page.getByRole('button', { name: /^Review story Last opened/ }).click();
  await page.getByRole('button', { name: 'Story review', exact: true }).click();
  await page.getByRole('heading', { name: 'Earlier story needs review', exact: true }).waitFor();
  assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), 'Ren returned to the empty gate.');
  checks.push('Native review binds the earlier reviewed prefix; changing an earlier chapter marks the later review unavailable while preserving later prose across reopen');
  // C4 uses only the explicit local test model. Observe the real project DB;
  // dropping an IPC acknowledgment must not create a second memory job.
  const memoryLibrary = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('library_snapshot'));
  const memoryProjectPath = await realpath(memoryLibrary.entries.find(entry => entry.title === 'Review story').path);
  const memoryRelative = relative(toNamespacedPath(await realpath(data)), toNamespacedPath(memoryProjectPath));
  assert(memoryRelative && !isAbsolute(memoryRelative) && memoryRelative !== '..' && !memoryRelative.startsWith(`..${sep}`), 'Memory fixtures must stay in this synthetic run directory');
  const memoryDatabase = new DatabaseSync(resolve(memoryProjectPath, 'project.sqlite3'));
  try {
    assert.equal(memoryDatabase.prepare('SELECT count(*) AS count FROM memory_jobs').get().count, 0);
    await page.getByRole('button', { name: 'Story memory', exact: true }).click();
    await page.getByRole('heading', { name: 'No story memory yet', exact: true }).waitFor();
    assert.equal(memoryDatabase.prepare('SELECT count(*) AS count FROM memory_jobs').get().count, 0);
    const beforeMemory = await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());
    await page.evaluate(() => {
      window.memoryEditor = document.querySelector('.tiptap').editor;
      const fetch = window.fetch;
      window.fetch = async (...args) => {
        const response = await fetch.apply(window, args);
        if (String(args[0]).endsWith('/start_memory') && response.headers.get('Tauri-Response') === 'ok') {
          const committed = await response.clone().json();
          if (!committed.id || !committed.snapshotId) throw new Error('Expected a real durable memory job');
          window.fetch = fetch; window.memoryAcknowledgmentDropped = true;
          return new Response(JSON.stringify({ code: 'UncertainOutcome', detail: 'Synthetic lost memory start acknowledgment after commit' }),
            { headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' } });
        }
        return response;
      };
    });
    await page.getByRole('button', { name: 'Refresh story memory', exact: true }).click();
    await page.getByRole('button', { name: 'Check saved result', exact: true }).click();
    await page.getByRole('heading', { name: 'Current story memory', exact: true }).waitFor();
    assert.equal(await page.evaluate(() => window.memoryAcknowledgmentDropped), true);
    assert.equal(memoryDatabase.prepare('SELECT count(*) AS count FROM memory_jobs').get().count, 1);
    assert.equal(memoryDatabase.prepare('SELECT count(*) AS count FROM memory_results').get().count, 1);
    assert.equal(memoryDatabase.prepare('SELECT count(*) AS count FROM memory_views').get().count, 1);
    assert.equal(await page.evaluate(() => window.memoryEditor === document.querySelector('.tiptap').editor), true);
    assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), beforeMemory);
    const memoryJob = memoryDatabase.prepare('SELECT snapshot_id,packet_id,context_source_epoch FROM memory_jobs').get();
    const memoryFrozen = JSON.parse(memoryDatabase.prepare('SELECT manifest_json FROM story_snapshots WHERE id=?').get(memoryJob.snapshot_id).manifest_json);
    assert.equal(memoryFrozen.purpose, 'memoryAnalysis');
    assert.equal(memoryFrozen.snapshot.sources.length, 1);
    assert.equal(memoryDatabase.prepare('SELECT context_source_epoch FROM project').get().context_source_epoch, memoryJob.context_source_epoch);
    await page.getByText('Show source evidence', { exact: true }).click();
    await page.locator('.memory-evidence blockquote').filter({ hasText: /^Ren returned to the empty gate\.$/ }).waitFor();
    await page.getByRole('button', { name: 'Inspect source', exact: true }).click();
    await page.getByLabel('Saved story source', { exact: true }).getByText('Ren returned to the empty gate.', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Close source', exact: true }).click();
    await page.screenshot({ path: resolve(output, 'chapter-memory.png') });
    await page.getByRole('button', { name: 'Back to writing', exact: true }).click();
    assert.equal(await page.evaluate(() => document.activeElement.textContent), 'Story memory');
    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
    await page.reload();
    await page.getByRole('button', { name: /^Review story Last opened/ }).click();
    await page.getByRole('button', { name: 'Story memory', exact: true }).click();
    await page.getByRole('heading', { name: 'Current story memory', exact: true }).waitFor();
    assert.equal(memoryDatabase.prepare('SELECT count(*) AS count FROM memory_jobs').get().count, 1);
    await page.getByRole('button', { name: 'Back to writing', exact: true }).click();
    await fillManuscript('Ren returned to the gate and found a broken chain.');
    await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
    await page.getByRole('button', { name: 'Story memory', exact: true }).click();
    await page.getByRole('heading', { name: 'Changed source', exact: true }).waitFor();
    assert.equal(memoryDatabase.prepare('SELECT count(*) AS count FROM memory_jobs').get().count, 1);
    await page.getByText('Show source evidence', { exact: true }).click();
    await page.locator('.memory-evidence blockquote').filter({ hasText: /^Ren returned to the empty gate\.$/ }).waitFor();
    await page.screenshot({ path: resolve(output, 'chapter-memory-changed.png') });
    checks.push('Native explicit chapter memory persists one source-bound result across a lost start acknowledgment and reopen, preserves the editor, does no autosave analysis, and retains changed-source evidence');

    memoryDatabase.exec("CREATE TRIGGER native_memory_terminal_failure BEFORE INSERT ON memory_results BEGIN SELECT RAISE(ABORT,'synthetic memory save fault'); END;");
    await page.getByRole('button', { name: 'Refresh story memory', exact: true }).click();
    await page.getByRole('button', { name: 'Check saved result', exact: true }).waitFor();
    const jobsBeforeMemoryRetry = memoryDatabase.prepare('SELECT count(*) AS count FROM memory_jobs').get().count;
    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
    await page.reload();
    await page.getByRole('button', { name: /^Review story Last opened/ }).click();
    await page.getByRole('button', { name: 'Story memory', exact: true }).click();
    await page.getByRole('button', { name: 'Check saved result', exact: true }).waitFor();
    memoryDatabase.exec('DROP TRIGGER native_memory_terminal_failure;');
    await page.getByRole('button', { name: 'Check saved result', exact: true }).click();
    await page.getByRole('heading', { name: 'Current story memory', exact: true }).waitFor();
    assert.equal(memoryDatabase.prepare('SELECT count(*) AS count FROM memory_jobs').get().count, jobsBeforeMemoryRetry);
    assert.equal(memoryDatabase.prepare('SELECT count(*) AS count FROM memory_results').get().count, 2);
    assert.equal(memoryDatabase.prepare('SELECT count(*) AS count FROM memory_views').get().count, 2);
    assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), 'Ren returned to the gate and found a broken chain.');
    checks.push('Native memory terminal-save failure remains visible across navigation and renderer reload; explicit local retry installs the retained result without another model job or manuscript change');
  } finally {
    memoryDatabase.exec('DROP TRIGGER IF EXISTS native_memory_terminal_failure;');
    memoryDatabase.close();
  }
  const memoryPolicy = await page.evaluate(async path => {
    const invoke = (command, args) => window.__TAURI_INTERNALS__.invoke(command, args);
    const opened = await invoke('create_project', { path, title: 'Memory policy fixture', session: 'memory-policy-native' });
    const chapter = await invoke('create_document', { request: {
      access: opened.access, operationId: 'memory-policy-chapter', documentId: 'memory-policy-chapter', title: 'A promise', kind: 'chapter',
      body: { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'promise' }, content: [{ type: 'text', text: 'Mei promised to return the silver key.' }] }] } },
    } });
    const modelSelection = (await invoke('provider_state')).settings.active;
    const request = { access: opened.access, operationId: 'memory-policy-refresh', expected: chapter.head, modelSelection,
      budget: { modelId: 'mock-story-context', contextWindowTokens: '200000', reservedOutputTokens: '4096', reservedProtocolTokens: '1024' } };
    const job = await invoke('start_memory', { request });
    let read;
    const deadline = Date.now() + 10000;
    do {
      read = await invoke('read_memory', { access: opened.access, documentId: chapter.head.documentId });
      if (read.views.length) break;
      await new Promise(resolve => setTimeout(resolve, 50));
    } while (Date.now() < deadline);
    if (!read.views.length) throw new Error('The native memory fixture did not finish');
    const source = await invoke('read_story_context_source', { access: opened.access, snapshotId: job.snapshotId, handle: job.source.revisionId });
    await invoke('revoke_story_context', { access: opened.access, expectedPolicy: job.disclosurePolicyVersion });
    const revoked = await invoke('read_memory', { access: opened.access, documentId: chapter.head.documentId });
    let blocked;
    try { await invoke('read_story_context_source', { access: opened.access, snapshotId: job.snapshotId, handle: job.source.revisionId }); }
    catch (error) { blocked = error; }
    return { source: source.passages.map(passage => passage.text), revoked, blocked };
  }, resolve(data, 'memory-policy-project'));
  assert.deepEqual(memoryPolicy.source, ['Mei promised to return the silver key.']);
  assert.equal(memoryPolicy.revoked.jobs.length, 1);
  assert.equal(memoryPolicy.revoked.views.length, 1);
  assert.equal(memoryPolicy.revoked.views[0].policyAvailable, false);
  assert.equal(memoryPolicy.revoked.views[0].current, false);
  assert.equal(memoryPolicy.revoked.views[0].candidate, null);
  assert.equal(memoryPolicy.revoked.jobs[0].result.rawOutput, null);
  assert.equal(memoryPolicy.revoked.jobs[0].result.candidate, null);
  assert.equal(memoryPolicy.blocked?.code, 'ContextPolicyChanged');
  checks.push('Native memory inspection resolves the exact saved chapter; disclosure revocation hides retained generated text and evidence and blocks further source lookup');
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  const navigationFixture = await page.evaluate(async () => {
    const invoke = (command, args) => window.__TAURI_INTERNALS__.invoke(command, args);
    const opened = await invoke('library_create', { operationId: 'navigation-native-project', title: 'Remembered story', session: 'navigation-native' });
    const create = (id, title, text) => invoke('create_document', { request: {
      access: opened.access, operationId: `create-${id}`, documentId: id, title, kind: 'chapter',
      body: { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id }, content: [{ type: 'text', text }] }] } },
    } });
    const target = await create('navigation-target', 'The return', 'Mei waited for Ren to explain what happened to the silver key.');
    const old = await create('navigation-old', 'The old promise', 'Ren promised to return the silver key. '.repeat(3000));
    await create('navigation-middle', 'The long journey', 'The road wound through the valley. '.repeat(3600));
    const modelSelection = (await invoke('provider_state')).settings.active;
    const job = await invoke('start_memory', { request: { access: opened.access, operationId: 'navigation-memory', expected: old.head,
      modelSelection, budget: { modelId: 'mock-story-context', contextWindowTokens: '200000', reservedOutputTokens: '4096', reservedProtocolTokens: '1024' } } });
    const deadline = Date.now() + 15000;
    let read;
    do {
      read = await invoke('read_memory', { access: opened.access, documentId: old.head.documentId });
      if (read.views.length) break;
      await new Promise(resolve => setTimeout(resolve, 50));
    } while (Date.now() < deadline);
    if (!read.views.length) throw new Error(`Navigation memory fixture failed: ${JSON.stringify(read.jobs)}`);
    return { target, view: read.views[0], projectId: opened.project.projectId, jobId: job.id };
  });
  await page.reload();
  await page.getByRole('button', { name: /^Remembered story Last opened/ }).click();
  await page.getByRole('heading', { name: 'The return', exact: true }).waitFor();
  const beforeNavigation = await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());
  await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).fill('What was the old promise about the silver key?');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await page.locator('.persistent-feedback article').filter({ hasText: 'This test confirms discussion and context handling' }).waitFor();
  if (await page.locator('.context-inspector').getAttribute('open') === null) await page.locator('.context-inspector>summary').click();
  await page.locator('.context-inspector > details[open] > summary').filter({ hasText: /Used .*1 generated summary/ }).waitFor();
  const navigationRow = page.locator('.context-inspector > details[open] .context-navigation').first();
  await navigationRow.locator('details > summary').first().click();
  await navigationRow.getByText('Unreviewed chapter memory.', { exact: false }).waitFor();
  await navigationRow.scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(output, 'navigation-context.png') });
  await navigationRow.getByText('Evidence for this summary', { exact: true }).click();
  await navigationRow.locator('blockquote').filter({ hasText: 'Ren promised to return the silver key.' }).waitFor();
  await navigationRow.getByRole('button', { name: 'Open original evidence · The old promise', exact: true }).click();
  const originalNavigationText = await page.getByRole('region', { name: 'Saved story source' }).innerText();
  assert(originalNavigationText.includes('Ren promised to return the silver key.'));
  assert(originalNavigationText.length > 100000, 'Inspection must read the full retained source, not just the digest quote');
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), beforeNavigation);
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  const navigationRetention = await page.evaluate(async fixture => {
    const invoke = (command, args) => window.__TAURI_INTERNALS__.invoke(command, args);
    const entry = (await invoke('library_snapshot')).entries.find(item => item.projectId === fixture.projectId);
    const opened = await invoke('open_project', { path: entry.path, session: 'navigation-retention' });
    const discussion = await invoke('read_discussion', { access: opened.access, documentId: fixture.target.head.documentId });
    const run = discussion.runs[0];
    const packet = await invoke('prepared_story_context', { access: opened.access, packetId: run.packetId });
    const frozen = await invoke('story_context_snapshot', { access: opened.access, snapshotId: packet.receipt.snapshotId });
    const middle = opened.documents.find(item => item.head.documentId === 'navigation-middle');
    await invoke('save_snapshot', { request: { access: opened.access, operationId: 'change-navigation-unrelated', expected: middle.head, localGeneration: '1', cause: 'typing',
      body: { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'navigation-middle' }, content: [{ type: 'text', text: 'The road turned beneath a different moon. '.repeat(3600) }] }] } } } });
    const originalStaleAfterUnrelated = await invoke('prepared_story_context_is_current', { access: opened.access, packetId: run.packetId });
    const modelSelection = (await invoke('provider_state')).settings.active;
    const followUp = await invoke('start_discussion', { request: {
      access: opened.access, operationId: 'navigation-follow-up', expected: fixture.target.head,
      instruction: 'What did the earlier promise establish?', scope: null, pinnedDocumentIds: [], safeBrief: null,
      budget: { modelId: 'mock-story-context', contextWindowTokens: '200000', reservedOutputTokens: '4096', reservedProtocolTokens: '1024' }, previousRunId: null,
    }, modelSelection });
    // The native command returns after durable begin, while the local mock
    // worker appends its chunks asynchronously.  Drain that worker here so
    // the later source edit/reload assertions cannot observe a queued run.
    const followUpDeadline = Date.now() + 15000;
    let followUpStored;
    do {
      const latest = await invoke('read_discussion', { access: opened.access, documentId: fixture.target.head.documentId });
      followUpStored = latest.runs.find(item => item.id === followUp.run.id);
      if (followUpStored?.status === 'completed') break;
      await new Promise(resolve => setTimeout(resolve, 50));
    } while (Date.now() < followUpDeadline);
    if (followUpStored?.status !== 'completed') {
      throw new Error(`Follow-up mock discussion did not complete before the navigation checks: ${JSON.stringify(followUpStored)}`);
    }
    const followUpPacket = followUp.packet;
    const followUpFrozen = await invoke('story_context_snapshot', { access: opened.access, snapshotId: followUpPacket.receipt.snapshotId });
    const followUpCurrent = await invoke('prepared_story_context_is_current', { access: opened.access, packetId: followUpPacket.receipt.packetId });
    const followUpView = (followUpFrozen.navigationViews ?? []).find(view => view.reference.viewId === fixture.view.id);
    if (!followUpView) throw new Error(`Fresh discussion did not retain the unchanged chapter memory view: ${JSON.stringify(followUpFrozen.navigationViews)}`);
    const followUpEvidence = await invoke('read_story_context_source', { access: opened.access, snapshotId: followUpFrozen.snapshot.snapshotId, handle: followUpView.dependencies[0].revisionId });
    const old = opened.documents.find(item => item.head.documentId === fixture.view.documentId);
    await invoke('save_snapshot', { request: { access: opened.access, operationId: 'change-navigation-source', expected: old.head, localGeneration: '1', cause: 'typing',
      body: { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'navigation-old' }, content: [{ type: 'text', text: 'The key was returned.' }] }] } } } });
    const historical = await invoke('prepared_story_context', { access: opened.access, packetId: run.packetId });
    const current = await invoke('prepared_story_context_is_current', { access: opened.access, packetId: run.packetId });
    const fresh = await invoke('freeze_story_context', { request: { access: opened.access, operationId: 'navigation-after-change', expected: fixture.target.head, basis: 'working', purpose: 'discuss', policy: frozen.policy } });
    let projectMemoryJobs = 0;
    for (const document of opened.documents) {
      const read = await invoke('read_memory', { access: opened.access, documentId: document.head.documentId });
      projectMemoryJobs += read.jobs.length;
    }
    return { packet, frozen, originalStaleAfterUnrelated, followUpPacket, followUpFrozen, followUpCurrent, followUpView, followUpEvidence, historical, current, freshViews: fresh.navigationViews ?? [], projectMemoryJobs };
  }, navigationFixture);
  assert.equal(navigationRetention.packet.receipt.navigationViews.length, 1);
  assert.equal(navigationRetention.packet.receipt.navigationViews[0].viewId, navigationFixture.view.id);
  assert.equal(navigationRetention.frozen.navigationViews[0].candidate.source.revisionId, navigationFixture.view.source.revisionId);
  assert.equal(navigationRetention.originalStaleAfterUnrelated, false);
  assert.equal(navigationRetention.followUpCurrent, true);
  assert.equal(navigationRetention.followUpPacket.receipt.navigationViews.length, 1);
  assert.equal(navigationRetention.followUpPacket.receipt.navigationViews[0].viewId, navigationFixture.view.id);
  assert.equal(navigationRetention.followUpView.sourceContextEpoch, navigationFixture.view.contextSourceEpoch);
  assert(BigInt(navigationRetention.followUpView.sourceContextEpoch) < BigInt(navigationRetention.followUpFrozen.snapshot.contextSourceEpoch), 'Fresh discussion must preserve the old view generation epoch while freezing the newer request epoch');
  assert.deepEqual(navigationRetention.followUpView.candidate, navigationFixture.view.candidate);
  assert.deepEqual(navigationRetention.followUpView.dependencies, [navigationFixture.view.source]);
  assert(navigationRetention.followUpEvidence.passages.some(passage => passage.text.includes('Ren promised to return the silver key.')));
  assert(navigationRetention.followUpEvidence.passages.reduce((total, passage) => total + passage.text.length, 0) > 100000, 'Fresh discussion must retain exact full evidence for the unchanged memory source');
  const followUpEnvelope = JSON.parse(navigationRetention.followUpPacket.messages[1].content);
  assert.equal(followUpEnvelope.derivedViews.views.length, 1);
  assert.equal(followUpEnvelope.derivedViews.views[0].reference.viewId, navigationFixture.view.id);
  const navigationEnvelope = JSON.parse(navigationRetention.packet.messages[1].content);
  assert.equal(navigationEnvelope.derivedViews.views.length, 1);
  assert.equal(navigationEnvelope.derivedViews.coverage, 'unreviewedGenerated');
  assert.equal(navigationEnvelope.derivedViews.representation, 'digest');
  assert.deepEqual(navigationRetention.historical, navigationRetention.packet);
  assert.equal(navigationRetention.current, false);
  assert.deepEqual(navigationRetention.freshViews, []);
  assert.equal(navigationRetention.projectMemoryJobs, 1);
  await page.reload();
  await page.getByRole('button', { name: /^Remembered story Last opened/ }).click();
  if (await page.locator('.context-inspector').getAttribute('open') === null) await page.locator('.context-inspector>summary').click();
  await page.locator('.context-inspector .stale-notice').filter({ hasText: 'Needs refresh' }).waitFor();
  await page.locator('.context-inspector > details[open] .context-navigation').waitFor();
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), beforeNavigation);
  checks.push('Native ordinary discussion automatically supplies existing generated memory when full prose exceeds its allowance, exposes exact evidence, preserves the manuscript and historical packet across changes/reload, and excludes stale views without another memory job');
  assert.deepEqual(errors, []);
  await writeFile(resolve(output, 'report.json'), JSON.stringify({ date: new Date().toISOString(), runtime, url: page.url(), authoringLanguage: 'English', checks, errors, executable, limitations: ['Explicit editor trial is session-only; library documents use the Rust persistence path', 'No physical keyboard/dead-key author trial', 'No screen-reader user trial', 'No minimum-window-size or multi-DPI qualification', 'This flow uses only the local test model; live-provider qualification is separate. Durable Apply supports scoped passage replacements, explicit complete-block and whole-chapter replacements, and append-only continuation', 'Backup/recovery dialog journeys remain separate W3 checks; this flow covers native draft Save/Cancel'], dataDirectory: data }, null, 2));
  console.log(JSON.stringify({ passed: checks.length, checks, output }, null, 2));
} catch (error) {
  if (observedPage && !observedPage.isClosed()) {
    await observedPage.screenshot({ path: resolve(output, 'failure.png') }).catch(() => {});
    appLog += `\nVisible native state:\n${await observedPage.locator('body').innerText().catch(() => '(unavailable)')}`;
  }
  await writeFile(resolve(output, 'failure.txt'), `${error.stack}\n${appLog}`);
  await writeFile(resolve(output, 'failure.json'), JSON.stringify({ date: new Date().toISOString(), executable, checks, failure: String(error), status: 'failed' }, null, 2));
  throw error;
} finally {
  await browser?.close();
  if (app.exitCode === null) app.kill();
}
