// Actual Tauri/WebView2 qualification with synthetic projects and a local mock.
// Run on an isolated Windows desktop when another author instance is open.
import assert from 'node:assert/strict';
import { chromium } from 'playwright-core';
import { DatabaseSync } from 'node:sqlite';
import { createServer } from 'node:net';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtemp, mkdir, realpath, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, relative, isAbsolute, sep, toNamespacedPath } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnOwned, stopOwned, markOwnedReady } from './owned-process.mjs';
import { initializeEvidence, recordCheck } from './native-evidence.mjs';
import { qualifyChatFailures } from './native-chat-failures.mjs';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const executable = resolve(process.env.WNS_V3_NATIVE_EXE ?? resolve(root, 'target/debug/webnovel-desktop.exe'));
const output = resolve(root, '.local/native-results/chat');
await mkdir(output, { recursive: true });
await writeFile(resolve(output, 'report.json'), JSON.stringify({ status: 'started', checks: [] }));
await initializeEvidence(executable);
const data = await realpath(await mkdtemp(resolve(tmpdir(), 'wns-v3-chat-native-')));
const server = createServer();
await new Promise(done => server.listen(0, '127.0.0.1', done));
const port = server.address().port;
await new Promise(done => server.close(done));
let appLog = ''; let spawnError;
function launchApp(zoom = 1) {
const child = spawnOwned(executable, [], { cwd: data, windowsHide: true, stdio: 'pipe', env: {
  ...process.env, WNS_V3_NATIVE_CDP_PORT: String(port),
  WNS_V3_TRIAL_ZOOM_FACTOR: String(zoom),
  WNS_V3_TRIAL_WEBVIEW_DIR: resolve(data, 'webview'), WNS_V3_TEST_DATA_DIR: resolve(data, 'library'),
} });
child.stdout.on('data', chunk => { appLog += chunk; });
child.stderr.on('data', chunk => { appLog += chunk; });
child.on('error', error => { spawnError = error; });
return child;
}
let app = launchApp();
let browser, page, database, runtime;
const checks = [], pageErrors = [];
const sleep = ms => new Promise(done => setTimeout(done, ms));
const invoke = (command, args) => page.evaluate(([name, value]) => window.__TAURI_INTERNALS__.invoke(name, value), [command, args]);
async function until(predicate, label, timeout = 20_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) { if (await predicate()) return; await sleep(70); }
  throw new Error(`Timed out: ${label}`);
}

function count(sql) { return Number(database.prepare(sql).get().n); }
function epoch() { return Number(database.prepare('SELECT context_source_epoch AS n FROM project').get().n); }
function ordinary() { return database.prepare("SELECT id,body_hash,working_version FROM documents WHERE role='ordinary' AND trashed=0 ORDER BY id").all().map(row => ({ ...row })); }
async function createProject(title) {
  await page.getByRole('button', { name: 'New project', exact: true }).click();
  await page.getByRole('textbox', { name: 'Project title', exact: true }).fill(title);
  await page.getByRole('button', { name: 'Create project', exact: true }).click();
  await page.getByRole('button', { name: 'Start a conversation · trial', exact: true }).click();
  await page.getByRole('textbox', { name: 'Message the project assistant', exact: true }).waitFor();
}

async function exerciseBlankEntryPath(title, instruction, checkId, description) {
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await createProject(title);
  const entry = (await invoke('library_snapshot')).entries.find(item => item.title === title);
  assert(entry, `Synthetic entry-path project ${title} must exist in the library`);
  const entryDatabase = new DatabaseSync(resolve(entry.path, 'project.sqlite3'), { readOnly: true });
  try {
    const entryComposer = page.getByRole('textbox', { name: 'Message the project assistant', exact: true });
    assert.equal(await entryComposer.inputValue(), '');
    assert.equal(await page.locator('.chat-welcome').count(), 1, `${title} starts on the conversation welcome surface`);
    assert.equal(await page.locator('.story-workshop').count(), 0, `${title} does not require the Workshop setup form`);
    assert.equal(Number(entryDatabase.prepare('SELECT COUNT(*) AS n FROM discussion_runs').get().n), 0);
    assert.equal(Number(entryDatabase.prepare('SELECT COUNT(*) AS n FROM documents WHERE role=\'ordinary\' AND trashed=0').get().n), 0);
    await entryComposer.fill(instruction);
    await until(async () => await page.getByRole('button', { name: /^Send/ }).isEnabled(), `${title} send ready`);
    await page.getByRole('button', { name: /^Send/ }).click();
    await until(() => Number(entryDatabase.prepare('SELECT COUNT(*) AS n FROM discussion_runs').get().n) === 1, `${title} request accepted`);
    await until(() => Number(entryDatabase.prepare("SELECT COUNT(*) AS n FROM discussion_runs WHERE status='completed'").get().n) === 1, `${title} request completed`);
    await until(() => Number(entryDatabase.prepare('SELECT COUNT(*) AS n FROM assistant_drafts').get().n) === 2, `${title} planning drafts materialized`);
    assert.equal(Number(entryDatabase.prepare('SELECT COUNT(*) AS n FROM documents WHERE role=\'ordinary\' AND trashed=0').get().n), 0, `${title} remains draft-only`);
    const run = entryDatabase.prepare('SELECT id FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get();
    assert.equal(Number(entryDatabase.prepare('SELECT COUNT(*) AS n FROM assistant_drafts WHERE origin_run_id=?').get(run.id).n), 2);
  } finally {
    entryDatabase.close();
  }
  recordCheck(checks, checkId, description);
}

async function connectApp() {
  await until(async () => {
    if (spawnError || app.exitCode !== null) throw new Error(`Native launch failed: ${spawnError ?? appLog}`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(1500) });
      return response.ok && !!(await response.json()).webSocketDebuggerUrl;
    } catch { return false; }
  }, 'WebView2 CDP startup', 90_000);
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  page = browser.contexts()[0].pages()[0];
  page.on('pageerror', error => pageErrors.push(error.message));
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  markOwnedReady(app);
}

try {
  await connectApp();
  await page.getByRole('button', { name: /^Choose model:/ }).click();
  await page.locator('.model-choice').filter({ hasText: 'Local test model' }).click();
  const provider = await invoke('provider_state');
  assert.equal(provider.settings.active.providerId, 'mock');
  assert.equal(provider.dispatch.kind, 'localMock');
  assert.notEqual(provider.settings.revision, '0');
  runtime = await invoke('runtime_info');
  assert.equal(runtime.host, 'Tauri');
  assert.equal(new URL(page.url()).hostname, 'tauri.localhost');
  markOwnedReady(app);
  recordCheck(checks, 'native-chat-smoke:01', 'Native Tauri/WebView2 uses a durably selected local mock before generation');

  const title = 'Chat journey synthetic story';
  await createProject(title);
  const library = await invoke('library_snapshot');
  const entry = library.entries.find(item => item.title === title);
  assert(entry);
  const projectPath = await realpath(entry.path);
  const child = relative(toNamespacedPath(data), toNamespacedPath(projectPath));
  assert(child && !isAbsolute(child) && child !== '..' && !child.startsWith(`..${sep}`));
  database = new DatabaseSync(resolve(projectPath, 'project.sqlite3'), { readOnly: true });
  assert.equal(ordinary().length, 0);
  assert.equal(count('SELECT COUNT(*) AS n FROM discussion_runs'), 0);
  const startingEpoch = epoch();
  const composer = page.getByRole('textbox', { name: 'Message the project assistant', exact: true });
  await composer.fill('A healer sells memories in a city above the clouds. Develop the world and protagonist without a setup interview.');
  await composer.press('Control+Enter');
  await until(() => count('SELECT COUNT(*) AS n FROM discussion_runs') === 1, 'first durable request');
  await composer.fill('Keep the ending hopeful. This is my next unsent request.');
  await until(() => count("SELECT COUNT(*) AS n FROM discussion_runs WHERE status='completed'") === 1, 'first completed response');
  await until(() => count('SELECT COUNT(*) AS n FROM assistant_drafts') === 2, 'two materialized mock drafts');
  assert.equal(await composer.inputValue(), 'Keep the ending hopeful. This is my next unsent request.');
  assert.equal(ordinary().length, 0);
  assert.equal(epoch(), startingEpoch);
  await page.locator('.chat-message-user').filter({ hasText: 'A healer sells memories' }).waitFor();
  await page.screenshot({ path: resolve(output, 'first-response.png') });
  recordCheck(checks, 'native-chat-smoke:02', 'Ordinary request creates retained isolated drafts; newer composer typing survives terminal response without story changes');

  await page.getByRole('button', { name: /^Review drafts/ }).click();
  await page.getByRole('button', { name: 'Edit this draft', exact: true }).first().click();
  const draftEditor = page.locator('.chat-draft-editor .ProseMirror');
  await draftEditor.fill('An author-edited world draft. The city floats above a sea of clouds.');
  await until(() => count("SELECT COUNT(*) AS n FROM documents WHERE role='assistantDraft' AND working_version>0") === 1, 'draft autosave');
  for (const checkbox of await page.getByRole('checkbox', { name: 'Include in this adoption', exact: true }).all()) await checkbox.check();
  await page.getByRole('button', { name: 'Prepare grouped preview', exact: true }).click();
  await page.getByRole('region', { name: 'Exact adoption preview', exact: true }).waitFor();
  await page.getByRole('region', { name: 'Exact adoption preview', exact: true }).scrollIntoViewIfNeeded();
  assert.equal(ordinary().length, 0);
  assert.equal(epoch(), startingEpoch);
  await page.screenshot({ path: resolve(output, 'exact-preview.png') });
  await page.getByRole('button', { name: /^Adopt all 2 documents/ }).click();
  await until(() => ordinary().length === 2, 'atomic two-document adoption');
  assert.equal(epoch(), startingEpoch + 1);
  assert.equal(count("SELECT COUNT(*) AS n FROM assistant_drafts WHERE disposition='adopted'"), 2);
  assert.equal(count("SELECT COUNT(*) AS n FROM command_receipts WHERE operation_kind='adoptChatPreview'"), 1);
  const firstProjectOrdinary = ordinary();
  const firstProjectRuns = count('SELECT COUNT(*) AS n FROM discussion_runs');
  const firstProjectDrafts = count('SELECT COUNT(*) AS n FROM assistant_drafts');
  recordCheck(checks, 'native-chat-smoke:03', 'Author draft edits save independently; exact before/after preview precedes grouped adoption and one source-epoch advance');

  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await createProject('Second isolated chat story');
  assert.equal(await composer.inputValue(), '');
  assert.equal(await page.locator('.chat-message-user').count(), 0);
  const secondEntry = (await invoke('library_snapshot')).entries.find(item => item.title === 'Second isolated chat story');
  assert(secondEntry);
  const secondDatabase = new DatabaseSync(resolve(secondEntry.path, 'project.sqlite3'), { readOnly: true });
  try {
    await page.getByRole('button', { name: 'Bring a note', exact: true }).click();
    const noteEditor = page.getByRole('textbox', { name: 'Manuscript', exact: true });
    await noteEditor.waitFor();
    assert.equal(Number(secondDatabase.prepare('SELECT COUNT(*) AS n FROM discussion_runs').get().n), 0);
    await noteEditor.fill('A courier keeps a silver letter in the harbor tower.');
    await noteEditor.press('Control+s');
    await until(() => Number(secondDatabase.prepare("SELECT COUNT(*) AS n FROM documents WHERE kind='note' AND role='ordinary' AND working_version>0").get().n) === 1, 'ordinary imported note saved');
    await until(async () => await noteEditor.getAttribute('contenteditable') === 'true', 'note checkpoint releases input');
    const originalNote = secondDatabase.prepare("SELECT id,working_version,body_hash FROM documents WHERE kind='note' AND role='ordinary'").get();
    await page.locator(`[data-document-id="${originalNote.id}"]`).getByRole('button', { name: 'Use Untitled note as a source', exact: true }).click();
    await composer.fill('Organize this note, keeping the original unchanged.');
    await composer.press('Control+Enter');
    await until(() => Number(secondDatabase.prepare("SELECT COUNT(*) AS n FROM discussion_runs WHERE status='completed'").get().n) === 1, 'saved-note reply completed');
    await until(() => Number(secondDatabase.prepare('SELECT COUNT(*) AS n FROM assistant_drafts').get().n) > 0, 'saved-note isolated drafts');
    assert.deepEqual(secondDatabase.prepare('SELECT id,working_version,body_hash FROM documents WHERE id=?').get(originalNote.id), originalNote);
    await noteEditor.fill('The courier now keeps a gold letter in the inland tower.');
    await noteEditor.press('Control+s');
    await until(() => Number(secondDatabase.prepare('SELECT working_version FROM documents WHERE id=?').get(originalNote.id).working_version) > Number(originalNote.working_version), 'later author note revision saved');
    await page.locator('.chat-request-status').getByText('Based on older sources', { exact: true }).waitFor();
    const inspector = page.locator('.chat-context-inspection').first();
    await inspector.locator(':scope > summary').click();
    await inspector.locator('.context-inspector > summary').click();
    const versions = inspector.locator('.context-source-versions').filter({ has: page.getByRole('button', { name: 'Version discussed of Untitled note', exact: true }) }).first();
    await versions.getByRole('button', { name: 'Version discussed of Untitled note', exact: true }).click();
    await until(async () => (await versions.textContent()).includes('A courier keeps a silver letter'), 'original discussed note retained');
    await versions.getByRole('button', { name: 'Current version of Untitled note', exact: true }).click();
    await until(async () => (await versions.textContent()).includes('The courier now keeps a gold letter'), 'current source version independently read');
    assert(!(await versions.textContent()).includes('A courier keeps a silver letter'));
    assert.equal(Number(secondDatabase.prepare('SELECT COUNT(*) AS n FROM discussion_runs').get().n), 1);
    const recap = page.locator('.chat-document-save-recap');
    await recap.locator(':scope > summary').click();
    const savedVersion = String(secondDatabase.prepare('SELECT working_version FROM documents WHERE id=?').get(originalNote.id).working_version);
    await recap.getByRole('button', { name: `Inspect saved version ${savedVersion}`, exact: true }).click();
    await until(async () => (await recap.getByRole('region', { name: 'Saved document event', exact: true }).textContent()).includes('The courier now keeps a gold letter'), 'return recap reads the exact retained author checkpoint');
    assert(!(await recap.getByRole('region', { name: 'Saved document event', exact: true }).textContent()).includes('A courier keeps a silver letter'));
    assert.equal(Number(secondDatabase.prepare('SELECT COUNT(*) AS n FROM discussion_runs').get().n), 1);
    await recap.getByRole('button', { name: 'Close saved version', exact: true }).click();
    await recap.locator(':scope > summary').click();
    recordCheck(checks, 'native-chat-smoke:12', 'Blank-project note entry saves ordinary author text; organization preserves it, and discussed/current versions resolve independently without dispatch');

    const noteBeforeDecision = secondDatabase.prepare('SELECT id,working_version,body_hash FROM documents WHERE id=?').get(originalNote.id);
    const secondRunsBeforeDecision = Number(secondDatabase.prepare('SELECT COUNT(*) AS n FROM discussion_runs').get().n);
    const draftsBeforeDecision = Number(secondDatabase.prepare('SELECT COUNT(*) AS n FROM assistant_drafts').get().n);
    await page.getByRole('button', { name: /^Review drafts/ }).click();
    await page.getByRole('heading', { name: 'Drafts to review', exact: true }).waitFor();
    await page.getByRole('button', { name: 'Reject', exact: true }).first().click();
    await until(() => Number(secondDatabase.prepare("SELECT COUNT(*) AS n FROM assistant_drafts WHERE disposition='rejected'").get().n) === 1, 'one retained draft rejected');
    assert.deepEqual(secondDatabase.prepare('SELECT id,working_version,body_hash FROM documents WHERE id=?').get(originalNote.id), noteBeforeDecision);
    assert.equal(Number(secondDatabase.prepare('SELECT COUNT(*) AS n FROM discussion_runs').get().n), secondRunsBeforeDecision);

    // The back button is only rendered for the narrow/mobile review surface.
    // Native qualification runs the desktop split view, where the right-mode
    // tabs are the canonical way to leave review mode.
    await page.locator('.chat-right-mode-tabs').getByRole('button', { name: 'Document', exact: true }).click();
    const question = page.locator('.chat-question').first();
    await question.waitFor();
    const notNowBefore = Number(secondDatabase.prepare("SELECT COUNT(*) AS n FROM conversation_items WHERE kind='chatDisposition' AND json_extract(payload_json,'$.disposition')='notNow'").get().n);
    await question.getByLabel('Scope').selectOption('task');
    await question.getByRole('button', { name: 'Not now', exact: true }).click();
    await until(() => Number(secondDatabase.prepare("SELECT COUNT(*) AS n FROM conversation_items WHERE kind='chatDisposition' AND json_extract(payload_json,'$.disposition')='notNow'").get().n) === notNowBefore + 1, 'scoped question disposition saved');
    const disposition = JSON.parse(secondDatabase.prepare("SELECT payload_json FROM conversation_items WHERE kind='chatDisposition' AND json_extract(payload_json,'$.disposition')='notNow' ORDER BY sequence DESC LIMIT 1").get().payload_json);
    assert.equal(disposition.disposition, 'notNow');
    assert.equal(disposition.scope.kind, 'task');
    assert.equal(Number(secondDatabase.prepare('SELECT COUNT(*) AS n FROM discussion_runs').get().n), secondRunsBeforeDecision);

    assert.equal(await composer.inputValue(), '');
    await composer.fill('A fresh idea unrelated to the harbor note.');
    assert.equal(Number(secondDatabase.prepare('SELECT COUNT(*) AS n FROM discussion_runs').get().n), secondRunsBeforeDecision);
    await composer.press('Control+Enter');
    await until(() => Number(secondDatabase.prepare("SELECT COUNT(*) AS n FROM discussion_runs WHERE status='completed'").get().n) === secondRunsBeforeDecision + 1, 'explicit fresh request completed');
    await until(() => Number(secondDatabase.prepare('SELECT COUNT(*) AS n FROM assistant_drafts').get().n) > draftsBeforeDecision, 'fresh request creates additional drafts');
    assert.deepEqual(secondDatabase.prepare('SELECT id,working_version,body_hash FROM documents WHERE id=?').get(originalNote.id), noteBeforeDecision);
    assert(Number(secondDatabase.prepare("SELECT COUNT(*) AS n FROM assistant_drafts WHERE disposition='pending'").get().n) > 0);
    assert.deepEqual(ordinary(), firstProjectOrdinary);
    assert.equal(count('SELECT COUNT(*) AS n FROM discussion_runs'), firstProjectRuns);
    assert.equal(count('SELECT COUNT(*) AS n FROM assistant_drafts'), firstProjectDrafts);
    recordCheck(checks, 'native-chat-smoke:14', 'Draft rejection and request-scoped Not now persist without changing the ordinary note; an unrelated idea sends only after explicit author action and creates fresh isolated drafts while the other project stays unchanged');
  } finally { secondDatabase.close(); }
  await composer.fill('An independent project with an independent composer.');
  await page.locator('.recent-project-picker > summary').click();
  await until(async () => /\d+ drafts? to review/.test(await page.locator('.project-picker-current').textContent()), 'current project badge reports its retained pending drafts');
  const activity = await invoke('project_activity');
  assert.equal(activity.find(item => item.projectId === entry.projectId).pendingDrafts, 0);
  assert(activity.find(item => item.projectId === secondEntry.projectId).pendingDrafts > 0);
  await page.getByRole('navigation', { name: 'Recent projects', exact: true }).getByRole('button').filter({ hasText: title }).click();
  await until(async () => (await page.locator('.app-header .brand > strong').textContent()) === title, 'recent project navigation settled');
  await until(async () => (await composer.inputValue()) === 'Keep the ending hopeful. This is my next unsent request.', 'original project composer restored');
  assert.equal(await composer.inputValue(), 'Keep the ending hopeful. This is my next unsent request.');
  assert.equal(count('SELECT COUNT(*) AS n FROM discussion_runs'), 1);
  assert.equal(ordinary().length, 2);
  recordCheck(checks, 'native-chat-smoke:04', 'Switching projects and reopening preserves exact independent composers, conversation, adoption, and run counts');

  // A first-project adoption should be usable as the basis for a subsequent
  // explicit request. The source is attached at its current head, while the
  // linked request still produces isolated drafts and never writes an
  // ordinary document automatically.
  await page.getByRole('tab', { name: 'All documents', exact: true }).click();
  await page.getByRole('searchbox', { name: 'Find documents' }).fill('');
  const linkedSource = database.prepare("SELECT id,working_version,body_hash,title FROM documents WHERE kind IN ('world','character') AND role='ordinary' AND trashed=0 ORDER BY id LIMIT 1").get();
  assert(linkedSource, 'The adopted first-project world or character document is available as a linkable source');
  await page.locator(`[data-document-id="${linkedSource.id}"]`).getByRole('button', { name: `Use ${linkedSource.title} as a source`, exact: true }).click();
  await page.getByLabel('Attached context', { exact: true }).waitFor();
  await until(() => {
    const value = JSON.parse(database.prepare('SELECT composer_json FROM project_conversations').get().composer_json);
    return value.sourceRefs.some(head => head.documentId === linkedSource.id && head.version === String(linkedSource.working_version) && head.bodyHash === linkedSource.body_hash);
  }, 'first-project linked source head attached');
  const linkedOrdinaryBefore = ordinary();
  const linkedRunsBefore = count('SELECT COUNT(*) AS n FROM discussion_runs');
  const linkedDraftsBefore = count('SELECT COUNT(*) AS n FROM assistant_drafts');
  await composer.fill('Using this adopted world or character document, develop one linked direction without overwriting the source.');
  await composer.press('Control+Enter');
  await until(() => count('SELECT COUNT(*) AS n FROM discussion_runs') === linkedRunsBefore + 1, 'linked first-project request accepted');
  await until(() => count("SELECT COUNT(*) AS n FROM discussion_runs WHERE status='completed'") === linkedRunsBefore + 1, 'linked first-project request completed');
  await until(() => count('SELECT COUNT(*) AS n FROM assistant_drafts') === linkedDraftsBefore + 2, 'linked first-project drafts materialized');
  const linkedRun = database.prepare('SELECT id,packet_id,target_document_id FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get();
  const linkedPacket = JSON.parse(database.prepare('SELECT packet_json FROM context_packets WHERE id=?').get(linkedRun.packet_id).packet_json);
  const linkedEnvelope = linkedPacket.messages.map(message => {
    try { return JSON.parse(message.content); } catch { return null; }
  }).find(message => message?.projectChat);
  assert(linkedEnvelope?.projectChat, 'The linked request retains a project-chat packet envelope');
  const linkedPacketSource = linkedEnvelope.projectChat.sourceRefs.find(head => head.documentId === linkedSource.id);
  assert(linkedPacketSource, 'The linked request packet includes the attached source');
  assert.equal(linkedPacketSource.version, String(linkedSource.working_version));
  assert.equal(linkedPacketSource.bodyHash, linkedSource.body_hash);
  assert.equal(linkedRun.target_document_id, database.prepare('SELECT anchor_document_id FROM project_conversations').get().anchor_document_id);
  const linkedDraftRows = database.prepare('SELECT origin_run_id,target_json FROM assistant_drafts WHERE origin_run_id=?').all(linkedRun.id);
  assert.equal(linkedDraftRows.length, 2);
  assert(linkedDraftRows.every(row => row.target_json === null), 'Linked planning drafts have no automatic ordinary-document target');
  assert.deepEqual(ordinary(), linkedOrdinaryBefore);
  recordCheck(checks, 'native-chat-smoke:15', 'An adopted first-project document can be attached at its exact current head for an explicit linked request; the frozen packet records source and conversation target while new drafts remain isolated');

  await page.getByRole('tab', { name: 'All documents', exact: true }).click();
  await page.getByRole('button', { name: 'Create blank chapter', exact: true }).click();
  const manuscript = page.getByRole('textbox', { name: 'Manuscript', exact: true });
  await manuscript.waitFor();
  await manuscript.fill('The healer stood at the bridge. The city waited in silence.');
  await manuscript.press('Control+s');
  await until(() => count("SELECT COUNT(*) AS n FROM documents WHERE role='ordinary' AND kind='chapter' AND working_version>0") === 1, 'chapter body saved');
  await until(async () => await manuscript.getAttribute('contenteditable') === 'true', 'chapter checkpoint releases input');
  const chapterBefore = ordinary();
  const chapterRunsBefore = count('SELECT COUNT(*) AS n FROM discussion_runs');
  const chapterItemsBefore = count("SELECT COUNT(*) AS n FROM conversation_items WHERE kind='chapterRequest'");
  const chapterDraftsBefore = count('SELECT COUNT(*) AS n FROM assistant_drafts');
  // Select exact text rather than ProseMirror's whole-document AllSelection.
  // Physical pointer selection is a separate author/native input gate.
  await manuscript.evaluate(element => {
    const editor = element.editor;
    editor.commands.setTextSelection({ from: 1, to: editor.state.doc.content.size - 1 });
    editor.commands.focus();
  });
  await manuscript.press('Control+Shift+F');
  await page.getByRole('region', { name: 'Captured chapter task', exact: true }).waitFor();
  await composer.fill('What does this selected passage suggest about the healer?');
  await composer.press('Control+Enter');
  await until(() => count("SELECT COUNT(*) AS n FROM conversation_items WHERE kind='chapterRequest'") === chapterItemsBefore + 1, 'chapter timeline request');
  await until(() => count("SELECT COUNT(*) AS n FROM discussion_runs WHERE status='completed'") === chapterRunsBefore + 1, 'chapter discussion completion');
  assert.deepEqual(ordinary(), chapterBefore);
  const chapterPayload = JSON.parse(database.prepare("SELECT payload_json FROM conversation_items WHERE kind='chapterRequest' ORDER BY sequence DESC LIMIT 1").get().payload_json);
  assert.equal(chapterPayload.scope.kind, 'passage');
  assert.equal(chapterPayload.scope.quote, 'The healer stood at the bridge. The city waited in silence.');
  assert.equal(count("SELECT COUNT(*) AS n FROM assistant_drafts"), chapterDraftsBefore);
  await page.screenshot({ path: resolve(output, 'chapter-conversation.png') });
  recordCheck(checks, 'native-chat-smoke:05', 'Native selected-text feedback links the exact chapter scope into the same conversation without chapter mutation or material drafts');

  await qualifyChatFailures({ page, projectDatabasePath: resolve(projectPath, 'project.sqlite3'), outputDirectory: output, checks });

  const separator = page.getByRole('separator', { name: 'Resize conversation and document panels' });
  await page.evaluate(() => { window.chatQualificationEditor = document.querySelector('[aria-label="Manuscript"]')?.editor; });
  await separator.press('Home');
  assert.equal(await separator.getAttribute('aria-valuenow'), '30');
  await separator.press('ArrowRight');
  assert.equal(await separator.getAttribute('aria-valuenow'), '32');
  await separator.press('End');
  assert.equal(await separator.getAttribute('aria-valuenow'), '70');
  assert.equal(await page.evaluate(() => window.chatQualificationEditor === document.querySelector('[aria-label="Manuscript"]')?.editor), true);
  recordCheck(checks, 'native-chat-smoke:11', 'Keyboard panel resizing changes the split within limits without replacing the mounted chapter editor');
  await page.getByRole('button', { name: /^Review drafts/ }).click();

  const resized = await promisify(execFile)('powershell.exe', ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File',
    resolve(root, 'apps/desktop/scripts/native-chat-window.ps1'), '-OwnerPid', String(app.pid)],
  { cwd: data, windowsHide: true, timeout: 20_000 });
  await writeFile(resolve(output, 'native-resize.json'), resized.stdout);
  await page.waitForFunction(() => innerWidth <= 802 && innerHeight >= 580);
  const layout = await page.evaluate(() => {
    const rect = document.querySelector('textarea[aria-label="Message the project assistant"]').getBoundingClientRect();
    return { width: innerWidth, height: innerHeight, overflow: document.documentElement.scrollWidth - innerWidth,
      composer: { x: rect.x, right: rect.right, y: rect.y, bottom: rect.bottom } };
  });
  assert(layout.overflow <= 1, `Narrow native window must not create page overflow: ${JSON.stringify(layout)}`);
  assert(layout.composer.x >= 0 && layout.composer.right <= layout.width && layout.composer.bottom <= layout.height,
    'The narrow chat composer remains visible and reachable.');
  const surfaces = page.getByRole('navigation', { name: 'Project workspace surfaces' });
  await surfaces.getByRole('button', { name: 'Documents', exact: true }).click();
  assert(await page.locator('.chat-document-view').isVisible(), 'Documents must leave review mode after the native window becomes narrow');
  await page.getByRole('tab', { name: 'All documents', exact: true }).click();
  await page.getByRole('searchbox', { name: 'Find documents' }).fill('chapter');
  await surfaces.getByRole('button', { name: 'Chat', exact: true }).click();
  await composer.fill('An unsent keyboard navigation check.');
  await composer.press('Tab');
  assert.equal(await page.getByRole('button', { name: /^Send/ }).evaluate(element => element === document.activeElement), true);
  recordCheck(checks, 'native-chat-smoke:07', 'An actual 800×600 native window keeps the composer visible, document navigation usable, and keyboard Send reachable');
  await page.screenshot({ path: resolve(output, 'native-narrow.png') });

  const beforeClose = ordinary();
  const runsBeforeClose = count('SELECT COUNT(*) AS n FROM discussion_runs');
  await composer.fill('Remember this unsent request after a real native close.');
  await promisify(execFile)('powershell.exe', ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File',
    resolve(root, 'apps/desktop/scripts/native-app-close.ps1'), '-OwnerPid', String(app.pid)],
  { cwd: data, windowsHide: true, timeout: 20_000 });
  await until(() => app.exitCode !== null, 'guarded native close flushes the composer', 30_000);
  assert.equal(app.exitCode, 0);
  await browser.close().catch(() => {});
  await stopOwned(app);
  app = launchApp(2);
  await connectApp();
  await page.getByRole('button', { name: new RegExp(`^${title} Last opened`) }).click();
  const reopenedComposer = page.getByRole('textbox', { name: 'Message the project assistant', exact: true });
  await reopenedComposer.waitFor();
  await until(async () => await reopenedComposer.inputValue() === 'Remember this unsent request after a real native close.', 'saved conversation hydrates after reopening');
  assert.equal(await reopenedComposer.inputValue(), 'Remember this unsent request after a real native close.');
  assert.deepEqual(ordinary(), beforeClose);
  assert.equal(count('SELECT COUNT(*) AS n FROM discussion_runs'), runsBeforeClose);
  await page.locator('.chat-message-user').filter({ hasText: 'A healer sells memories' }).waitFor();
  recordCheck(checks, 'native-chat-smoke:06', 'Real guarded native close and process restart preserve unsent composer, conversation, exact adopted heads, and request count');
  await page.screenshot({ path: resolve(output, 'native-reopened.png') });

  const zoomResize = await promisify(execFile)('powershell.exe', ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File',
    resolve(root, 'apps/desktop/scripts/native-chat-window.ps1'), '-OwnerPid', String(app.pid)],
  { cwd: data, windowsHide: true, timeout: 20_000 });
  await writeFile(resolve(output, 'native-zoom-resize.json'), zoomResize.stdout);
  await page.waitForFunction(() => innerWidth >= 395 && innerWidth <= 402);
  // Capture the document extent explicitly: WebView2's viewport screenshot
  // metrics otherwise crop the image when native controller zoom is active.
  await page.screenshot({ path: resolve(output, 'native-zoom-200.png'), fullPage: true });
  const zoomLayout = await page.evaluate(() => {
    const composer = document.querySelector('textarea[aria-label="Message the project assistant"]').getBoundingClientRect();
    const status = document.querySelector('.chat-request-status').getBoundingClientRect();
    const transcript = document.querySelector('.chat-transcript-shell').getBoundingClientRect();
    const replyButton = document.querySelector('.chat-transcript-new-reply')?.getBoundingClientRect();
    return { width: innerWidth, height: innerHeight, pixelRatio: devicePixelRatio,
      overflow: document.documentElement.scrollWidth - innerWidth,
      composer: { x: composer.x, right: composer.right, y: composer.y, bottom: composer.bottom },
      status: { y: status.y, bottom: status.bottom },
      transcript: { y: transcript.y, bottom: transcript.bottom },
      newReply: replyButton ? { y: replyButton.y, bottom: replyButton.bottom } : null };
  });
  await writeFile(resolve(output, 'native-zoom-layout.json'), JSON.stringify(zoomLayout, null, 2));
  await promisify(execFile)('powershell.exe', ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File',
    resolve(root, 'apps/desktop/scripts/native-chat-window.ps1'), '-OwnerPid', String(app.pid), '-Capture'],
  { cwd: data, windowsHide: true, timeout: 20_000 });
  assert(zoomLayout.overflow <= 1, `200% native zoom must not create page overflow: ${JSON.stringify(zoomLayout)}`);
  assert(zoomLayout.composer.x >= 0 && zoomLayout.composer.right <= zoomLayout.width && zoomLayout.composer.y >= 0 && zoomLayout.composer.bottom <= zoomLayout.height,
    `The composer must remain reachable at 200% native zoom: ${JSON.stringify(zoomLayout)}`);
  assert(zoomLayout.status.y >= 0 && zoomLayout.status.bottom <= zoomLayout.height, 'The current request stays visible at 200% native zoom');
  if (zoomLayout.newReply) assert(zoomLayout.newReply.y >= zoomLayout.transcript.y && zoomLayout.newReply.bottom <= zoomLayout.transcript.bottom + 1 && zoomLayout.newReply.bottom <= zoomLayout.composer.y,
    `New reply must stay in the transcript and never cover the composer: ${JSON.stringify(zoomLayout)}`);
  await reopenedComposer.press('Tab');
  assert.equal(await page.getByRole('button', { name: /^Send/ }).evaluate(element => element === document.activeElement), true);
  recordCheck(checks, 'native-chat-smoke:08', 'Actual WebView2 200% zoom at an 800×600 native window preserves the request state, composer, and keyboard Send');

  const reopenedSurfaces = page.getByRole('navigation', { name: 'Project workspace surfaces' });
  await reopenedSurfaces.getByRole('button', { name: 'Documents', exact: true }).click();
  await page.getByRole('tab', { name: 'All documents', exact: true }).click();
  await page.getByRole('searchbox', { name: 'Find documents' }).fill('');
  const source = database.prepare("SELECT id, working_version, body_hash, title FROM documents WHERE kind='world' AND role='ordinary' AND trashed=0 ORDER BY id LIMIT 1").get();
  await page.locator(`[data-document-id="${source.id}"]`).getByRole('button', { name: `Use ${source.title} as a source`, exact: true }).click();
  await page.getByLabel('Attached context', { exact: true }).waitFor();
  await until(() => {
    const value = JSON.parse(database.prepare('SELECT composer_json FROM project_conversations').get().composer_json);
    return value.sourceRefs.some(head => head.documentId === source.id && head.version === String(source.working_version) && head.bodyHash === source.body_hash);
  }, 'direct source attachment durably captures its exact revision');
  assert.equal(await reopenedComposer.inputValue(), 'Remember this unsent request after a real native close.');
  assert.equal(count('SELECT COUNT(*) AS n FROM discussion_runs'), runsBeforeClose);
  recordCheck(checks, 'native-chat-smoke:09', 'Direct document attachment saves an exact source and preserves unsent typing without opening Writer or generating');

  const beforeHandoff = ordinary();
  await reopenedComposer.fill('Write the first chapter. Private planning detail: ZEPHYR_SECRET_51.');
  await reopenedComposer.press('Control+Enter');
  const stopAtZoom = page.locator('.chat-request-status').getByRole('button', { name: 'Stop', exact: true });
  await stopAtZoom.waitFor();
  const stopLayout = await stopAtZoom.evaluate(element => {
    const bounds = element.getBoundingClientRect();
    const status = element.closest('.chat-request-status').getBoundingClientRect();
    const atCenter = document.elementFromPoint(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
    return { top: bounds.top, bottom: bounds.bottom, statusTop: status.top, statusBottom: status.bottom,
      viewportHeight: innerHeight, directlyReachable: element === atCenter || element.contains(atCenter) };
  });
  await writeFile(resolve(output, 'native-zoom-active-stop.json'), JSON.stringify(stopLayout, null, 2));
  assert(stopLayout.top >= stopLayout.statusTop && stopLayout.bottom <= stopLayout.statusBottom + 1 && stopLayout.bottom <= stopLayout.viewportHeight && stopLayout.directlyReachable,
    `Stop must be visible without scrolling request details at 200% zoom: ${JSON.stringify(stopLayout)}`);
  const handoff = page.getByRole('region', { name: 'Proposed chapter writing task' });
  await handoff.waitFor();
  await until(() => count("SELECT COUNT(*) AS n FROM discussion_runs WHERE status='completed'") === runsBeforeClose + 1, 'handoff response terminal');
  assert.deepEqual(ordinary(), beforeHandoff);
  await reopenedComposer.fill('Use the approved brief and begin gently.');
  await handoff.getByRole('button', { name: 'Create blank chapter and prepare writing', exact: true }).click();
  await page.getByRole('region', { name: 'Writing brief editor', exact: true }).waitFor();
  assert.equal(count('SELECT COUNT(*) AS n FROM discussion_runs'), runsBeforeClose + 1);
  assert.equal(await reopenedComposer.inputValue(), 'Use the approved brief and begin gently.');
  const newChapter = ordinary().find(document => !beforeHandoff.some(before => before.id === document.id));
  assert(newChapter);
  assert.deepEqual(ordinary().filter(document => document.id !== newChapter.id), beforeHandoff);
  await page.getByRole('button', { name: 'Approve this brief', exact: true }).click();
  await until(async () => await page.getByRole('button', { name: /^Send/ }).isEnabled(), 'approved writing task ready');
  await reopenedComposer.press('Control+Enter');
  await until(() => count("SELECT COUNT(*) AS n FROM discussion_runs WHERE status='completed'") === runsBeforeClose + 2, 'explicit chapter continuation terminal');
  const chapterRun = database.prepare('SELECT id,packet_id FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get();
  await until(() => Number(database.prepare('SELECT COUNT(*) AS n FROM proposals WHERE run_id=?').get(chapterRun.id).n) === 1, 'chapter proposal retained');
  const packet = database.prepare('SELECT packet_json FROM context_packets WHERE id=?').get(chapterRun.packet_id).packet_json;
  assert(!packet.includes('ZEPHYR_SECRET_51'), 'The restricted chapter packet must exclude the original private planning request');
  assert(packet.includes('Keep the ending hopeful.'), 'The explicitly approved brief is included');
  assert.deepEqual(ordinary(), [...beforeHandoff, newChapter].sort((a, b) => a.id.localeCompare(b.id)));
  recordCheck(checks, 'native-chat-smoke:10', 'Author-room handoff requires target preparation, brief approval, and a separate Send; its chapter proposal excludes private chat and does not change prose');

  await page.getByRole('navigation', { name: 'Project workspace surfaces' }).getByRole('button', { name: 'Chapter', exact: true }).click();
  const rangeEditor = page.getByRole('textbox', { name: 'Manuscript', exact: true });
  await rangeEditor.fill('The courier approached the tower.');
  await rangeEditor.press('End'); await rangeEditor.press('Enter');
  await rangeEditor.pressSequentially('A confrontation waited inside.');
  await rangeEditor.press('Enter'); await rangeEditor.pressSequentially('Keep this ending unchanged.');
  await rangeEditor.press('Control+s');
  await until(async () => await rangeEditor.getAttribute('contenteditable') === 'true' && await page.locator('.writing .save-status').textContent() === 'Saved', 'three-paragraph chapter checkpoint');
  const beforeRange = ordinary();
  const beforeRangeRuns = count('SELECT COUNT(*) AS n FROM discussion_runs');
  if (await page.getByRole('button', { name: 'Return to project conversation', exact: true }).count()) {
    await page.getByRole('button', { name: 'Return to project conversation', exact: true }).click();
  }
  await page.getByRole('button', { name: 'Discuss in project chat', exact: true }).click();
  await reopenedComposer.fill('Give feedback on the opening and propose the passage to strengthen.');
  await until(async () => await page.getByRole('button', { name: /^Send/ }).isEnabled(), 'unselected chapter send ready');
  await page.getByRole('button', { name: /^Send/ }).click();
  await until(() => count('SELECT COUNT(*) AS n FROM discussion_runs') === beforeRangeRuns + 1 && database.prepare('SELECT status FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get().status === 'completed', 'unselected chapter feedback completes');
  const rangeTurn = page.locator('.chat-turn').filter({ has: page.locator('.chat-message-user').filter({ hasText: 'Give feedback on the opening and propose the passage to strengthen.' }) });
  await rangeTurn.getByRole('button', { name: 'Open chapter result', exact: true }).click();
  await page.getByRole('button', { name: 'Use this passage for an edit', exact: true }).waitFor();
  assert.equal((await page.locator('[aria-label="Complete proposed passage"]').textContent()).trim(), 'The courier approached the tower.');
  await page.getByRole('button', { name: 'Use this passage for an edit', exact: true }).click();
  await until(() => {
    const value = JSON.parse(database.prepare('SELECT composer_json FROM project_conversations').get().composer_json);
    return value.chapter?.intent === 'proposeEdits' && value.chapter?.scope?.kind === 'blocks';
  }, 'author-confirmed range prepares an unsent scoped edit');
  const confirmedRange = JSON.parse(database.prepare('SELECT composer_json FROM project_conversations').get().composer_json).chapter.scope;
  assert.equal(confirmedRange.quote, 'The courier approached the tower.');
  assert.equal(confirmedRange.start.blockId, confirmedRange.end.blockId);
  assert.deepEqual(ordinary(), beforeRange);
  assert.equal(count('SELECT COUNT(*) AS n FROM discussion_runs'), beforeRangeRuns + 1);
  recordCheck(checks, 'native-chat-smoke:13', 'Unselected chapter feedback proposes exact paragraphs; author confirmation stages a separate edit request without dispatch or manuscript changes');

  async function sendSelectedChapterScope(expectedQuote, instruction, checkId, description, select) {
    if (await page.getByRole('button', { name: 'Return to project conversation', exact: true }).count()) {
      await page.getByRole('button', { name: 'Return to project conversation', exact: true }).click();
    }
    await rangeEditor.evaluate(select);
    await rangeEditor.press('Control+Shift+F');
    await page.getByRole('region', { name: 'Captured chapter task', exact: true }).waitFor();
    const itemsBefore = count("SELECT COUNT(*) AS n FROM conversation_items WHERE kind='chapterRequest'");
    const completedBefore = count("SELECT COUNT(*) AS n FROM discussion_runs WHERE status='completed'");
    await reopenedComposer.fill(instruction);
    await until(async () => await page.getByRole('button', { name: /^Send/ }).isEnabled(), `${checkId} send ready`);
    await page.getByRole('button', { name: /^Send/ }).click();
    await until(() => count("SELECT COUNT(*) AS n FROM conversation_items WHERE kind='chapterRequest'") === itemsBefore + 1, `${checkId} chapter request recorded`);
    await until(() => count("SELECT COUNT(*) AS n FROM discussion_runs WHERE status='completed'") === completedBefore + 1, `${checkId} selected response completed`);
    const payload = JSON.parse(database.prepare("SELECT payload_json FROM conversation_items WHERE kind='chapterRequest' ORDER BY sequence DESC LIMIT 1").get().payload_json);
    assert.equal(payload.scope.kind, 'passage');
    assert.equal(payload.scope.quote, expectedQuote, `${checkId} captures the exact selected text`);
    recordCheck(checks, checkId, description);
  }

  await sendSelectedChapterScope(
    'courier',
    'Discuss this selected word without changing the chapter.',
    'native-chat-smoke:18',
    'Native word selection becomes an exact chapter passage request and leaves the manuscript unchanged.',
    element => {
      const editor = element.editor;
      const text = editor.state.doc.firstChild.textContent;
      const start = text.indexOf('courier');
      editor.commands.setTextSelection({ from: 1 + start, to: 1 + start + 'courier'.length });
      editor.commands.focus();
    },
  );
  await sendSelectedChapterScope(
    'The courier approached the tower.',
    'Discuss this selected sentence without changing the chapter.',
    'native-chat-smoke:19',
    'Native sentence selection captures the complete sentence as the chapter request scope without broadening it.',
    element => {
      const editor = element.editor;
      const text = editor.state.doc.firstChild.textContent;
      editor.commands.setTextSelection({ from: 1, to: 1 + text.length });
      editor.commands.focus();
    },
  );

  // Recreate the proposed scoped edit after the selection-only requests. The
  // local fixture returns one structured candidate, so this exercises the
  // real preview/Apply path while the final paragraph is protected by being
  // outside the captured block scope.
  if (await page.getByRole('button', { name: 'Return to project conversation', exact: true }).count()) {
    await page.getByRole('button', { name: 'Return to project conversation', exact: true }).click();
  }
  await page.getByRole('button', { name: 'Discuss in project chat', exact: true }).click();
  const applyRangeInstruction = 'Prepare a scoped edit for the opening paragraph and leave the protected ending unchanged.';
  const applyRangeItemsBefore = count("SELECT COUNT(*) AS n FROM conversation_items WHERE kind='chapterRequest'");
  const applyRangeRunsBefore = count('SELECT COUNT(*) AS n FROM discussion_runs');
  await reopenedComposer.fill(applyRangeInstruction);
  await until(async () => await page.getByRole('button', { name: /^Send/ }).isEnabled(), 'protected-ending range send ready');
  await page.getByRole('button', { name: /^Send/ }).click();
  await until(() => count("SELECT COUNT(*) AS n FROM conversation_items WHERE kind='chapterRequest'") === applyRangeItemsBefore + 1, 'protected-ending range request recorded');
  await until(() => count("SELECT COUNT(*) AS n FROM discussion_runs WHERE status='completed'") === applyRangeRunsBefore + 1, 'protected-ending range response completed');
  const applyRangeTurn = page.locator('.chat-turn').filter({ has: page.locator('.chat-message-user').filter({ hasText: applyRangeInstruction }) });
  await applyRangeTurn.getByRole('button', { name: 'Open chapter result', exact: true }).click();
  await page.getByRole('button', { name: 'Use this passage for an edit', exact: true }).waitFor();
  await page.getByRole('button', { name: 'Use this passage for an edit', exact: true }).click();
  await until(() => {
    const value = JSON.parse(database.prepare('SELECT composer_json FROM project_conversations').get().composer_json);
    return value.chapter?.intent === 'proposeEdits' && value.chapter?.scope?.kind === 'blocks';
  }, 'protected-ending edit scope prepared');
  await reopenedComposer.fill('Replace only the captured opening with the reviewed local alternative. Preserve the protected ending exactly.');
  await until(async () => await page.getByRole('button', { name: /^Send/ }).isEnabled(), 'protected-ending edit send ready');
  await page.getByRole('button', { name: /^Send/ }).click();
  const applyRunCount = applyRangeRunsBefore + 2;
  await until(() => count("SELECT COUNT(*) AS n FROM discussion_runs WHERE status='completed'") === applyRunCount, 'protected-ending edit response completed');
  const applyRun = database.prepare('SELECT id FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get();
  await until(() => Number(database.prepare('SELECT COUNT(*) AS n FROM proposals WHERE run_id=?').get(applyRun.id).n) === 1, 'structured protected-ending proposal retained');
  const applyTurn = page.locator('.chat-turn').filter({ has: page.locator('.chat-message-user').filter({ hasText: 'Replace only the captured opening' }) });
  await applyTurn.getByRole('button', { name: 'Open chapter result', exact: true }).click();
  const proposalCard = page.locator('.proposal-card').first();
  await proposalCard.waitFor();
  await proposalCard.getByRole('button', { name: 'Preview', exact: true }).click();
  await proposalCard.locator('.proposal-preview').waitFor();
  const applyButton = proposalCard.getByRole('button', { name: 'Apply', exact: true });
  await until(() => applyButton.isEnabled(), 'protected-ending exact preview becomes applicable');
  await applyButton.click();
  await proposalCard.locator('.proposal-status').filter({ hasText: /^Applied$/ }).waitFor();
  const protectedBody = await rangeEditor.innerText();
  assert(protectedBody.includes('Keep this ending unchanged.'), 'Applying the opening proposal preserves the protected ending');
  recordCheck(checks, 'native-chat-smoke:20', 'A scoped chapter proposal requires an exact preview and explicit Apply; applying the opening leaves the protected ending unchanged');

  // Start another chapter request, then edit the target while its frozen
  // request is still in flight. The saved target head must remain the older
  // one so a later Apply cannot silently use the changed source.
  if (await page.getByRole('button', { name: 'Return to project conversation', exact: true }).count()) {
    await page.getByRole('button', { name: 'Return to project conversation', exact: true }).click();
  }
  await page.getByRole('button', { name: 'Discuss in project chat', exact: true }).click();
  const pendingInstruction = 'Discuss the opening while preserving the author-controlled ending.';
  await reopenedComposer.fill(pendingInstruction);
  await until(async () => await page.getByRole('button', { name: /^Send/ }).isEnabled(), 'pending chapter send ready');
  await page.getByRole('button', { name: /^Send/ }).click();
  await until(() => count('SELECT COUNT(*) AS n FROM discussion_runs') === applyRunCount + 1, 'pending chapter run accepted');
  const pendingRun = database.prepare('SELECT id,target_version,target_body_hash,status FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get();
  assert(['queued', 'running', 'stopping'].includes(pendingRun.status), `Pending chapter request must be in flight, got ${pendingRun.status}`);
  await rangeEditor.evaluate(element => {
    const editor = element.editor;
    let start = -1; let end = -1;
    editor.state.doc.descendants((node, position) => {
      if (!node.isText) return;
      start = position;
      end = position + node.nodeSize;
    });
    if (start < 0 || end < start) throw new Error('Could not locate the protected ending text node');
    editor.commands.insertContentAt({ from: start, to: end }, 'The author changed this ending while the assistant was working.');
    editor.commands.focus();
  });
  await rangeEditor.press('Control+s');
  await until(() => Number(database.prepare('SELECT working_version FROM documents WHERE id=?').get(newChapter.id).working_version) > Number(pendingRun.target_version), 'stale chapter edit checkpoint');
  await until(() => database.prepare('SELECT status FROM discussion_runs WHERE id=?').get(pendingRun.id).status === 'completed', 'pending chapter response completed after source edit');
  const staleHead = database.prepare('SELECT working_version,body_hash FROM documents WHERE id=?').get(newChapter.id);
  assert(Number(staleHead.working_version) > Number(pendingRun.target_version));
  assert.notEqual(staleHead.body_hash, pendingRun.target_body_hash);
  assert((await rangeEditor.innerText()).includes('The author changed this ending while the assistant was working.'));
  recordCheck(checks, 'native-chat-smoke:21', 'Editing the chapter while a request is pending produces a newer target head; the retained request stays bound to its older source and does not auto-apply');

  await exerciseBlankEntryPath(
    'World-first chat entry story',
    'World first: establish one setting rule and its human consequence. Do not require a protagonist, plot, chapter, or setup form.',
    'native-chat-smoke:16',
    'A blank project accepts a world-first author request directly in chat, without a setup form or automatic ordinary write.',
  );
  await exerciseBlankEntryPath(
    'Character-first chat entry story',
    'Character first: develop one motivation and conflict. Do not require a world template, plot, chapter, or setup form.',
    'native-chat-smoke:17',
    'A separate blank project accepts a character-first author request directly in chat, without a setup form or automatic ordinary write.',
  );

  assert.deepEqual(pageErrors, []);
  await writeFile(resolve(output, 'report.json'), JSON.stringify({ status: 'passed', checks, runtime, executable, dataDirectory: data, pageErrors,
    limitations: ['Synthetic mock evidence only. Live providers, human formative study, screen readers, physical keyboard behavior, and installed packaging are separate qualification gates.'] }, null, 2));
  console.log(JSON.stringify({ status: 'passed', checks, output }, null, 2));
} catch (error) {
  await writeFile(resolve(output, 'report.json'), JSON.stringify({ status: 'failed', checks, runtime, executable, dataDirectory: data, error: String(error) }, null, 2));
  await page?.screenshot({ path: resolve(output, 'failure.png') }).catch(() => {});
  const visible = await page?.locator('body').innerText().catch(() => '(unavailable)');
  await writeFile(resolve(output, 'failure.txt'), `${error.stack ?? error}\n${appLog}\n${visible}`);
  throw error;
} finally {
  database?.close();
  await browser?.close().catch(() => {});
  await stopOwned(app);
}
