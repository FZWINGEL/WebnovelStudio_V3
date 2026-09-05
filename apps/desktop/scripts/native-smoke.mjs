// Actual Tauri/WebView2 integration checks. No Chromium browser is launched.
import { chromium } from 'playwright-core';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:net';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const executable = process.env.WNS_V3_NATIVE_EXE ? resolve(process.env.WNS_V3_NATIVE_EXE) : resolve(root, 'target/debug/webnovel-desktop.exe');
const output = resolve(root, '.local/native-results');
await mkdir(output, { recursive: true });
const data = await mkdtemp(resolve(tmpdir(), 'wns-v3-native-'));
const server = createServer();
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const port = server.address().port;
await new Promise(resolve => server.close(resolve));
let appLog = '';
let spawnError;
function launch() {
  const process = spawn(executable, [], {
    cwd: root, windowsHide: true, stdio: 'pipe',
    env: { ...globalThis.process.env, WNS_V3_NATIVE_CDP_PORT: String(port), WNS_V3_TRIAL_WEBVIEW_DIR: resolve(data, 'webview'), WNS_V3_TEST_DATA_DIR: resolve(data, 'library') },
  });
  process.stdout.on('data', chunk => { appLog += chunk; });
  process.stderr.on('data', chunk => { appLog += chunk; });
  process.on('error', error => { spawnError = error; appLog += error.stack; });
  return process;
}
let app = launch();
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
      if (response.ok && (await response.json()).webSocketDebuggerUrl) { ready = true; break; }
      lastReadinessError = `Unexpected CDP HTTP status ${response.status}`;
    } catch (error) { lastReadinessError = String(error); }
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  if (!ready) throw new Error(`WebView2 CDP was not ready after ${Date.now() - startup}ms. PID=${app.pid}; exit=${app.exitCode}; ${lastReadinessError}\nNative log:\n${appLog || '(empty)'}`);
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 10000 });
  const context = browser.contexts()[0];
  let page = context.pages()[0];
  if (!page) page = await context.waitForEvent('page', { timeout: 10000 });
  observedPage = page;
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
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
  await page.keyboard.press('Control+c');
  await page.evaluate(() => {
    const editor = document.querySelector('.tiptap').editor;
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
  });
  await page.keyboard.press('Control+v');
  await page.waitForFunction(() => document.querySelector('.tiptap').editor.state.doc.textContent.split('Clipboard 灯火🙂').length === 3);
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
  const libraryBeforeRestart = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('library_snapshot'));
  assert.equal(libraryBeforeRestart.entries.length, 3);
  assert.equal(new Set(libraryBeforeRestart.entries.map(entry => entry.projectId)).size, 3);
  assert.equal(libraryBeforeRestart.pending.length, 0);
  await browser.close(); browser = undefined;
  const exited = new Promise(resolve => app.once('exit', resolve));
  app.kill(); await exited;
  app = launch();
  const restartedAt = Date.now(); let restarted = false;
  while (Date.now() - restartedAt < 90000) {
    if (app.exitCode !== null || spawnError) throw new Error(`Native restart failed: ${appLog}`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(2000) });
      if (response.ok && (await response.json()).webSocketDebuggerUrl) { restarted = true; break; }
    } catch { /* A new WebView2 process is starting. */ }
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  assert(restarted, `Native restart did not expose CDP: ${appLog}`);
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 10000 });
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
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('button', { name: 'Archive Harbour C', exact: true }).click();
  await page.getByRole('button', { name: /^Harbour C Last opened/ }).waitFor({ state: 'detached' });
  await page.getByRole('button', { name: 'Archived', exact: true }).click();
  await page.getByRole('button', { name: 'Unarchive Harbour C', exact: true }).click();
  await page.getByRole('button', { name: 'Show active', exact: true }).click();
  await page.getByRole('button', { name: /^Harbour C Last opened/ }).waitFor();
  checks.push('Native renames preserve the mounted editor; last document and exact caret survive navigation/reload; archive and unarchive preserve the project');
  assert.deepEqual(errors, []);
  await writeFile(resolve(output, 'report.json'), JSON.stringify({ date: new Date().toISOString(), runtime, url: page.url(), authoringLanguage: 'English', checks, errors, executable, limitations: ['Explicit editor trial is session-only; library documents use the Rust persistence path', 'No physical keyboard/dead-key author trial', 'No screen-reader user trial', 'No minimum-window-size or multi-DPI qualification', 'No live provider or durable Apply', 'Native export/recovery dialog journeys remain separate W3 checks'], dataDirectory: data }, null, 2));
  console.log(JSON.stringify({ passed: checks.length, checks, output }, null, 2));
} catch (error) {
  if (observedPage && !observedPage.isClosed()) {
    await observedPage.screenshot({ path: resolve(output, 'failure.png') }).catch(() => {});
    appLog += `\nVisible native state:\n${await observedPage.locator('body').innerText().catch(() => '(unavailable)')}`;
  }
  await writeFile(resolve(output, 'failure.txt'), `${error.stack}\n${appLog}`);
  throw error;
} finally {
  await browser?.close();
  if (app.exitCode === null) app.kill();
}
