// Bounded Story Workshop qualification against the actual Tauri/WebView2 app.
// This harness deliberately uses the built-in local mock and synthetic data;
// it never sends a provider request to a live service.
import assert from 'node:assert/strict';
import { chromium } from 'playwright-core';
import { spawn } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { createServer } from 'node:net';
import { fileURLToPath } from 'node:url';
import { mkdtemp, mkdir, realpath, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { isAbsolute, relative, resolve, sep, toNamespacedPath } from 'node:path';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const executable = process.env.WNS_V3_NATIVE_EXE
  ? resolve(process.env.WNS_V3_NATIVE_EXE)
  : resolve(root, 'target/debug/webnovel-desktop.exe');
const output = resolve(root, '.local/native-results/workshop');
await mkdir(output, { recursive: true });
// Resolve Windows short temp names/junctions before comparing them with the
// canonical project paths returned by the native library.
const data = await realpath(await mkdtemp(resolve(tmpdir(), 'wns-v3-workshop-native-')));

async function reservePort() {
  const server = createServer();
  await new Promise(resolvePromise => server.listen(0, '127.0.0.1', resolvePromise));
  const port = server.address().port;
  await new Promise(resolvePromise => server.close(resolvePromise));
  return port;
}

let port = await reservePort();
let appLog = '';
let spawnError;
function launch() {
  const child = spawn(executable, [], {
    cwd: data,
    windowsHide: true,
    stdio: 'pipe',
    env: {
      ...globalThis.process.env,
      WNS_V3_NATIVE_CDP_PORT: String(port),
      WNS_V3_TRIAL_WEBVIEW_DIR: resolve(data, 'webview'),
      WNS_V3_TEST_DATA_DIR: resolve(data, 'library'),
    },
  });
  child.stdout.on('data', chunk => { appLog += chunk; });
  child.stderr.on('data', chunk => { appLog += chunk; });
  child.on('error', error => { spawnError = error; appLog += `\n${error.stack}`; });
  return child;
}

const app = launch();
let browser;
let page;
let database;
let runtime;
const checks = [];
const pageErrors = [];

function invoke(command, args) {
  return page.evaluate(([name, value]) => window.__TAURI_INTERNALS__.invoke(name, value), [command, args]);
}

async function sleep(milliseconds) {
  await new Promise(resolvePromise => setTimeout(resolvePromise, milliseconds));
}

async function waitForDatabase(predicate, label, timeout = 15_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await sleep(50);
  }
  throw new Error(`Timed out waiting for database condition: ${label}`);
}

function documents() {
  // SQLite rows have null prototypes; structuredClone snapshots do not.
  // Compare every persisted field while keeping the same plain record shape.
  return database.prepare('SELECT id,kind,title,body_json FROM documents WHERE trashed=0 ORDER BY position').all().map(row => ({ ...row }));
}

function documentText(document) {
  const snapshot = JSON.parse(document.body_json);
  return (snapshot.body.content ?? []).map(block => (block.content ?? []).map(inline => inline.type === 'text' ? inline.text : '\n').join('')).join('\n');
}

function workshopState() {
  const row = database.prepare('SELECT version,state_json FROM workshop_state WHERE singleton=1').get();
  return row ? { version: String(row.version), state: JSON.parse(row.state_json) } : null;
}

function contextSourceEpoch() {
  return Number(database.prepare('SELECT context_source_epoch AS value FROM project WHERE singleton=1').get().value);
}

function documentAliases(documentId) {
  return database.prepare('SELECT alias FROM document_aliases WHERE document_id=? ORDER BY alias').all(documentId).map(row => row.alias);
}

function projectInside(projectPath) {
  const child = relative(toNamespacedPath(data), toNamespacedPath(projectPath));
  assert(child && !isAbsolute(child) && child !== '..' && !child.startsWith(`..${sep}`),
    `Workshop fixture must stay inside this synthetic run: ${projectPath} under ${data}`);
}

async function chooseLocalMock() {
  let state = await invoke('provider_state');
  if (state.settings.active.providerId !== 'mock' || state.settings.active.modelId !== 'mock-story-context') {
    await page.getByRole('button', { name: /^Choose model:/ }).click();
    await page.locator('.model-choice').filter({ hasText: 'Local test model' }).click();
    await page.getByRole('button', { name: 'Choose model: Local test model', exact: true }).waitFor();
  }
  state = await invoke('provider_state');
  assert.deepEqual(state.settings.active, { providerId: 'mock', modelId: 'mock-story-context', reasoning: null, serviceTier: null });
  assert.equal(state.dispatch.kind, 'localMock');
  assert.equal(state.catalog.models.find(model => model.key.providerId === 'mock' && model.key.modelId === 'mock-story-context')?.ready, true);
  checks.push('The active author model is the ready local mock; dispatch is localMock before any Workshop generation');
}

async function chooseStartWritingWhenPresented() {
  const modeChoice = page.getByRole('heading', { name: 'How do you want to begin?', exact: true });
  const chapterAction = page.getByRole('button', { name: 'Create a chapter', exact: true });
  const mode = await Promise.race([
    modeChoice.waitFor({ state: 'visible', timeout: 10_000 }).then(() => true),
    chapterAction.waitFor({ state: 'visible', timeout: 10_000 }).then(() => false),
  ]);
  if (mode) {
    await page.getByRole('button', { name: 'Start writing', exact: true }).click();
    await page.getByRole('tab', { name: /^Chapters/ }).waitFor();
  }
}

async function ensureDevelopMode() {
  const develop = page.getByRole('tab', { name: 'Develop', exact: true });
  if (await develop.getAttribute('aria-selected') !== 'true') await develop.click();
  await page.getByRole('main', { name: 'Current exploration', exact: true }).waitFor();
}

try {
  const started = Date.now();
  let ready = false;
  let readinessError = '';
  console.log(`Starting native Story Workshop qualification (PID ${app.pid}); waiting up to 90 seconds for CDP.`);
  while (Date.now() - started < 90_000) {
    if (spawnError || app.exitCode !== null) throw new Error(`Native app did not start (exit ${app.exitCode}): ${appLog}`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(2_000) });
      if (response.ok && (await response.json()).webSocketDebuggerUrl) {
        browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 10_000 });
        ready = true;
        break;
      }
      readinessError = `Unexpected CDP status ${response.status}`;
    } catch (error) {
      readinessError = String(error);
    }
    await sleep(500);
  }
  if (!ready) throw new Error(`WebView2 CDP was not ready after 90 seconds: ${readinessError}\n${appLog}`);

  const context = browser.contexts()[0];
  page = context.pages()[0] ?? await context.waitForEvent('page', { timeout: 10_000 });
  page.on('pageerror', error => pageErrors.push(error.message));
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor({ timeout: 30_000 });
  await chooseLocalMock();
  runtime = await invoke('runtime_info');
  assert.equal(runtime.host, 'Tauri');
  assert.equal(runtime.persistence, true);
  assert.equal(new URL(page.url()).hostname, 'tauri.localhost');
  checks.push(`Actual Tauri/WebView2 runtime attached over CDP (${runtime.webviewVersion})`);

  const title = 'Native Story Workshop fixture';
  const seed = 'A quiet archive preserves memories that their owners are not ready to face.';
  await page.getByRole('button', { name: 'New project', exact: true }).click();
  await page.getByRole('textbox', { name: 'Project title', exact: true }).fill(title);
  await page.getByRole('button', { name: 'Create project', exact: true }).click();
  await page.getByRole('heading', { name: 'How do you want to begin?', exact: true }).waitFor();
  await page.getByRole('button', { name: 'Develop a story', exact: true }).click();
  await page.getByRole('main', { name: 'Current exploration', exact: true }).waitFor();

  const library = await invoke('library_snapshot');
  const entry = library.entries.find(item => item.title === title);
  assert(entry, 'The synthetic Workshop project must be indexed');
  const projectPath = await realpath(entry.path);
  projectInside(projectPath);
  database = new DatabaseSync(resolve(projectPath, 'project.sqlite3'), { readOnly: true });
  assert.equal(documents().length, 0, 'Develop blank/noChapter must create no story documents');
  assert.equal(database.prepare("SELECT count(*) AS n FROM documents WHERE kind='chapter' AND trashed=0").get().n, 0);

  const seedInput = page.getByRole('textbox', { name: 'Your idea, image, dialogue, or attraction', exact: true });
  await seedInput.fill(seed);
  await page.locator('.workshop-save-status').filter({ hasText: 'Saved on this computer' }).waitFor();
  await waitForDatabase(() => workshopState()?.state.sessions.some(session => session.brief === seed), 'seed save');
  checks.push('Develop blank/noChapter opens a fresh Workshop and saves the seed without creating a chapter');

  await page.getByRole('button', { name: 'World', exact: true }).click();
  await page.getByRole('heading', { name: 'World', exact: true }).waitFor();
  await page.getByRole('heading', { name: 'Compare possible directions', exact: true }).waitFor();
  assert.equal(await page.locator('.candidate-card').count(), 0);
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, 0);
  await page.screenshot({ path: resolve(output, 'seed-world-no-generation.png') });
  checks.push('World lens navigation is a no-generation action: the empty comparison board and discussion table remain empty');

  await page.getByRole('button', { name: 'Explore', exact: true }).click();
  await page.getByText('Generation complete', { exact: true }).waitFor({ timeout: 30_000 });
  await page.locator('.candidate-card').first().waitFor();
  assert.equal(await page.locator('.candidate-card').count(), 3, 'Directions generation must return exactly three candidates');
  assert.equal(await database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, 1);
  assert.equal(database.prepare('SELECT status FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get().status, 'completed');
  assert.equal(await page.locator('.candidate-card').filter({ hasText: 'Local workshop direction 1' }).count(), 1);
  assert.equal(await page.locator('.candidate-card').filter({ hasText: 'deterministic workshop alternative' }).count(), 3);
  await page.screenshot({ path: resolve(output, 'three-mock-directions.png') });
  checks.push('One UI Explore request completes on the deterministic mock and renders exactly three alternatives');

  const interpretation = page.locator('.workshop-interpretation');
  assert.equal(await interpretation.getAttribute('open'), '', 'A new result should reveal its editable interpretation');
  const interpretationBefore = workshopState().state.sessions.find(session => session.id === workshopState().state.currentSessionId);
  const interpretationFields = ['You said', 'Possible direction', 'Still open'];
  const correctedInterpretation = ['A neighborhood archive, with no chosen savior.', 'Shared care for difficult memories.', 'Who pays the cost remains open.'];
  const savedInterpretation = [interpretationBefore.brief, interpretationBefore.direction, interpretationBefore.stillOpen];
  for (const [index, field] of interpretationFields.entries()) {
    await interpretation.getByRole('textbox', { name: `Current interpretation · ${field}`, exact: true }).fill(correctedInterpretation[index]);
  }
  await waitForDatabase(() => {
    const current = workshopState().state.sessions.find(session => session.id === interpretationBefore.id);
    return JSON.stringify([current.brief, current.direction, current.stillOpen]) === JSON.stringify(correctedInterpretation);
  }, 'corrected interpretation save');
  const interpretationAfter = workshopState().state.sessions.find(session => session.id === interpretationBefore.id);
  assert.equal(interpretationAfter.workingGeneration, interpretationBefore.workingGeneration);
  assert.equal(interpretationAfter.workingText, interpretationBefore.workingText);
  assert.equal(interpretationAfter.originalNotes, interpretationBefore.originalNotes);
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, 1);
  await page.screenshot({ path: resolve(output, 'editable-interpretation.png') });
  for (const [index, field] of interpretationFields.entries()) {
    await interpretation.getByRole('textbox', { name: `Current interpretation · ${field}`, exact: true }).fill(savedInterpretation[index]);
  }
  await waitForDatabase(() => {
    const current = workshopState().state.sessions.find(session => session.id === interpretationBefore.id);
    return JSON.stringify([current.brief, current.direction, current.stillOpen]) === JSON.stringify(savedInterpretation);
  }, 'interpretation baseline restored, including cleared fields');
  checks.push('The new result reveals three editable interpretation fields; changes and clears persist without replacing prose, original notes, or starting generation');

  const firstRun = database.prepare('SELECT packet_id,dispatch_state FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get();
  assert.equal(firstRun.dispatch_state, 'delivered');
  const firstPacket = JSON.parse(database.prepare('SELECT packet_json FROM context_packets WHERE id=?').get(firstRun.packet_id).packet_json);
  assert.equal(JSON.parse(firstPacket.messages.at(-1).content).authorBrief, seed, 'The final request must freeze the author brief separately');
  const firstEnvelope = firstPacket.messages.flatMap(message => {
    try { return JSON.parse(message.content).workshop ?? []; } catch { return []; }
  })[0];
  assert(firstEnvelope?.currentElement.includes(seed), 'The delivered Workshop envelope must retain the actual seed');
  assert.deepEqual(firstEnvelope.preferences, [], 'Without selected preferences, the request must not infer Want or Avoid choices');
  const revealContext = page.getByRole('button', { name: 'Working story & context', exact: true });
  if (await revealContext.isVisible()) await revealContext.click();
  const frozenCreativeContext = page.locator('.workshop-frozen-context');
  await frozenCreativeContext.locator('summary').click();
  const inspectedCurrentElement = frozenCreativeContext.locator('h4').filter({ hasText: /^Current element$/ }).locator('+ p');
  await inspectedCurrentElement.waitFor();
  assert.equal(await inspectedCurrentElement.innerText(), firstEnvelope.currentElement);
  const sourceContext = page.locator('.context-inspector');
  await sourceContext.locator(':scope > summary').click();
  await sourceContext.getByText('Sources supplied for this response. Opening a source reads its saved version.', { exact: true }).waitFor();
  assert.equal(await sourceContext.getByText(/Delivery has not been confirmed/).count(), 0);
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, 1, 'Inspecting delivered context must not generate');
  await page.screenshot({ path: resolve(output, 'delivered-workshop-context.png') });
  const closeContext = page.getByRole('button', { name: 'Close working story', exact: true });
  if (await closeContext.isVisible()) await closeContext.click();
  else await page.getByRole('button', { name: 'Hide working story', exact: true }).click();
  checks.push('Saved context inspection matches the exact delivered Workshop envelope and reports confirmed mock delivery without another generation');

  const firstCard = page.locator('.candidate-card').filter({ hasText: 'Local workshop direction 1' }).first();
  await firstCard.getByRole('button', { name: 'Select details', exact: true }).click();
  const beforeConsequence = workshopState().state.sessions.find(session => session.id === workshopState().state.currentSessionId);
  const implicationActions = firstCard.locator('.candidate-implication-actions').first();
  await implicationActions.getByRole('button', { name: 'Reject this assumption', exact: true }).click();
  await waitForDatabase(() => workshopState().state.sessions.some(session => session.composer.includes('Reject this assumption')), 'local assumption rejection');
  await implicationActions.getByRole('button', { name: 'Prepare a contrasting implication', exact: true }).click();
  await waitForDatabase(() => workshopState().state.sessions.some(session => session.composer.includes('Prepare a contrasting implication')), 'local consequence contrast');
  const afterConsequence = workshopState().state.sessions.find(session => session.id === beforeConsequence.id);
  assert.equal(afterConsequence.workingText, beforeConsequence.workingText);
  assert.deepEqual(afterConsequence.choices, beforeConsequence.choices);
  assert.deepEqual(afterConsequence.selectedDetails, beforeConsequence.selectedDetails);
  assert(afterConsequence.composer.includes('the selected workshop material'));
  assert(afterConsequence.composer.includes('the author wants the change explored'));
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, 1, 'Preparing a contrast must not generate');
  await page.getByRole('textbox', { name: 'Your direction', exact: true }).fill(beforeConsequence.composer);
  await waitForDatabase(() => workshopState().state.sessions.some(session => session.id === beforeConsequence.id && session.composer === beforeConsequence.composer), 'restore author direction');
  checks.push('Rejecting an implication assumption and preparing a contrast preserve candidate choices, selected details, and the working body while saving exact provisional evidence without generation');
  await firstCard.getByRole('button', { name: 'Select full direction', exact: true }).click();
  await page.locator('.workshop-tray textarea').first().waitFor();
  assert.match(await page.locator('.workshop-tray textarea').first().inputValue(), /deterministic workshop alternative/);
  const detailReview = firstCard.getByRole('button', { name: 'Review against current work', exact: true });
  if (await detailReview.count()) await detailReview.click();
  await firstCard.getByRole('button', { name: 'Develop this', exact: true }).click();
  const working = page.getByRole('textbox', { name: 'Develop or edit directly', exact: true });
  await working.waitFor();
  const generatedWorking = await working.inputValue();
  assert(generatedWorking.includes('deterministic workshop alternative'));
  const authorEdit = `${generatedWorking}\n\nAuthor edit: keep the archive ordinary and the uncertainty deliberate.`;
  await working.fill(authorEdit);
  await page.locator('.workshop-save-status').filter({ hasText: 'Saved on this computer' }).waitFor();
  await waitForDatabase(() => workshopState()?.state.sessions.some(session => session.workingText === authorEdit), 'working version save');
  await page.screenshot({ path: resolve(output, 'selected-detail-working-edit.png') });
  checks.push('A full candidate detail is selected into the tray, developed locally, and edited in the author-only working version');

  const documentsBeforeAdoption = documents();
  assert.equal(
    documentsBeforeAdoption.filter(document => document.kind === 'chapter').length,
    0,
    'The working generation must not create a chapter before adoption',
  );

  await page.getByRole('button', { name: 'Use this version', exact: true }).click();
  const adoption = page.getByRole('region', { name: 'Adoption preview', exact: true });
  await adoption.getByRole('heading', { name: 'Where should this version go?', exact: true }).waitFor();
  assert((await adoption.innerText()).includes('Chapters and character knowledge are not changed.'));
  await adoption.getByRole('textbox', { name: 'Title', exact: true }).fill('The Ember Archive');
  await adoption.getByRole('combobox', { name: 'Kind', exact: true }).selectOption('world');
  await adoption.getByRole('textbox', { name: 'Content to choose', exact: true }).fill(authorEdit);
  await adoption.getByRole('textbox', { name: 'Why this version?', exact: true }).fill('Keep the archive ordinary and the uncertainty deliberate.');
  assert.deepEqual(
    documents(),
    documentsBeforeAdoption,
    'Opening adoption must not change saved documents',
  );
  assert.equal(
    documents().filter(document => document.kind === 'chapter').length,
    0,
    'Opening adoption must not create a chapter',
  );
  await adoption.getByRole('button', { name: 'Preview all changes', exact: true }).click();
  await adoption.getByRole('heading', { name: 'Choose this version for your story', exact: true }).waitFor();
  assert((await adoption.innerText()).includes('The Ember Archive'));
  assert((await adoption.innerText()).includes('New document'));
  assert((await adoption.innerText()).includes('Author edit: keep the archive ordinary'));
  assert.deepEqual(
    documents(),
    documentsBeforeAdoption,
    'Adoption preview must not change saved documents',
  );
  assert.equal(
    documents().filter(document => document.kind === 'chapter').length,
    0,
    'Adoption preview must not create a chapter',
  );
  await page.screenshot({ path: resolve(output, 'adoption-preview.png') });
  const receiptsBeforeStalePreview = database.prepare('SELECT count(*) AS n FROM workshop_receipts WHERE operation_kind=\'adoptWorkshop\'').get().n;
  const decisionsBeforeStalePreview = workshopState().state.decisions;
  const runsBeforeStalePreview = database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n;
  const stalePreviewDraft = `${authorEdit}\n\nA later author note: the public archive closes at dusk.`;
  await working.fill(stalePreviewDraft);
  await waitForDatabase(() => workshopState()?.state.sessions.some(session => session.workingText === stalePreviewDraft), 'working edit after adoption preview');
  await adoption.getByRole('button', { name: 'Confirm Use this version', exact: true }).click();
  await page.getByRole('status').filter({ hasText: 'Your exploration changed after preview. Prepare the adoption again.' }).waitFor();
  assert.deepEqual(documents(), documentsBeforeAdoption, 'A stale exploration preview must not write any document or create a chapter');
  assert.deepEqual(workshopState().state.decisions, decisionsBeforeStalePreview, 'A stale preview must not adopt a decision');
  assert.equal(database.prepare('SELECT count(*) AS n FROM workshop_receipts WHERE operation_kind=\'adoptWorkshop\'').get().n, receiptsBeforeStalePreview, 'A stale preview must not produce an adoption receipt');
  assert.equal(await working.inputValue(), stalePreviewDraft, 'Refusal must preserve the later author draft');
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, runsBeforeStalePreview, 'Stale preview refusal must not generate');
  await page.screenshot({ path: resolve(output, 'stale-exploration-preview-refused.png') });
  checks.push('A working edit after preview visibly refuses adoption while preserving the later draft, all documents, decisions, and receipts without generation');

  await adoption.getByRole('button', { name: 'Keep exploring', exact: true }).click();
  await working.fill(authorEdit);
  await waitForDatabase(() => workshopState()?.state.sessions.some(session => session.workingText === authorEdit), 'intentional working version restored');
  await page.getByRole('button', { name: 'Use this version', exact: true }).click();
  await adoption.getByRole('heading', { name: 'Where should this version go?', exact: true }).waitFor();
  await adoption.getByRole('textbox', { name: 'Title', exact: true }).fill('The Ember Archive');
  await adoption.getByRole('combobox', { name: 'Kind', exact: true }).selectOption('world');
  await adoption.getByRole('textbox', { name: 'Content to choose', exact: true }).fill(authorEdit);
  await adoption.getByRole('textbox', { name: 'Why this version?', exact: true }).fill('Keep the archive ordinary and the uncertainty deliberate.');
  await adoption.getByRole('button', { name: 'Preview all changes', exact: true }).click();
  await adoption.getByRole('heading', { name: 'Choose this version for your story', exact: true }).waitFor();
  await adoption.getByRole('button', { name: 'Confirm Use this version', exact: true }).click();
  await page.getByText('Version chosen. Its source and rationale are saved; writing access remains author only.', { exact: true }).waitFor();
  await waitForDatabase(() => documents().filter(document => document.kind === 'world').length === 1, 'world adoption');
  const adoptedDocuments = documents();
  assert.equal(adoptedDocuments.filter(document => document.kind === 'chapter').length, 0);
  assert.equal(adoptedDocuments.filter(document => document.kind === 'world').length, 1);
  const adoptedWorld = adoptedDocuments.find(document => document.kind === 'world');
  assert.equal(adoptedWorld.title, 'The Ember Archive');
  const adoptedState = workshopState();
  const chosen = adoptedState.state.decisions.find(decision => decision.status === 'chosen');
  assert(chosen, 'Explicit adoption must create a chosen decision');
  assert.equal(chosen.access, 'authorRoom');
  assert.equal(chosen.documentId, adoptedWorld.id);
  assert.equal(database.prepare('SELECT count(*) AS n FROM workshop_receipts WHERE operation_kind=\'adoptWorkshop\'').get().n, 1);
  await page.screenshot({ path: resolve(output, 'adoption-committed-author-room.png') });
  checks.push('Preview leaves documents unchanged; explicit adoption creates one world document, zero chapters, and one author-room decision');

  await page.getByRole('tab', { name: 'Write', exact: true }).click();
  await page.getByRole('tab', { name: /^Chapters/ }).waitFor();
  assert.equal(await page.getByRole('tab', { name: /^Chapters/ }).getAttribute('aria-selected'), 'true');
  assert.equal(documents().filter(document => document.kind === 'chapter').length, 0);
  await page.screenshot({ path: resolve(output, 'write-barrier-no-chapters.png') });
  checks.push('Switching from Develop to Write crosses the save barrier and still exposes zero chapters');

  await page.getByRole('tab', { name: /^Characters/ }).click();
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await page.getByRole('combobox', { name: 'Start with', exact: true }).selectOption('character');
  await page.getByRole('textbox', { name: 'Title', exact: true }).fill('The Archive Keeper');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await page.getByRole('heading', { name: 'The Archive Keeper', exact: true }).waitFor();
  await page.getByRole('textbox', { name: 'Manuscript', exact: true }).fill('The keeper protects the archive while doubting who should inherit its memories.');
  await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  await waitForDatabase(() => documents().filter(document => document.kind === 'character').length === 1, 'character fixture');
  const participantIds = documents().filter(document => ['world', 'character'].includes(document.kind)).map(document => document.id);
  assert.equal(participantIds.length, 2);

  await page.getByRole('tab', { name: 'Develop', exact: true }).click();
  await page.getByRole('main', { name: 'Current exploration', exact: true }).waitFor();
  await page.getByRole('button', { name: 'People', exact: true }).click();
  await page.getByRole('heading', { name: 'People', exact: true }).waitFor();
  const relationships = page.getByRole('region', { name: 'Local relationships', exact: true });
  await relationships.getByRole('button', { name: 'Connect people or groups', exact: true }).click();
  const from = relationships.getByRole('combobox', { name: 'From', exact: true });
  const to = relationships.getByRole('combobox', { name: 'To', exact: true });
  const values = await from.locator('option').evaluateAll(options => options.map(option => option.value));
  assert.equal(values.length, 2);
  await from.selectOption(values[0]);
  await to.selectOption(values[1]);
  await relationships.getByRole('textbox', { name: 'Relationship type', exact: true }).fill('trusts');
  await relationships.getByRole('textbox', { name: 'What this person wants, misunderstands, or values', exact: true }).fill('The archive keeper trusts the archive to preserve what people cannot yet say.');
  await relationships.getByRole('textbox', { name: 'What remains uncertain', exact: true }).fill('Whether the archive chooses what to remember.');
  await relationships.getByRole('combobox', { name: 'Decision status', exact: true }).selectOption('chosen');
  await relationships.getByRole('button', { name: 'Save relationship', exact: true }).click();
  await relationships.locator('article').filter({ hasText: 'trusts' }).waitFor();
  await page.locator('.workshop-save-status').filter({ hasText: 'Saved on this computer' }).waitFor();
  await waitForDatabase(() => workshopState()?.state.relationships.length === 1, 'directional relationship save');
  const relationship = workshopState().state.relationships[0];
  assert.equal(relationship.type, 'trusts');
  assert.equal(relationship.status, 'chosen');
  assert.deepEqual(relationship.sourceHeads.map(head => head.documentId).sort(), participantIds.sort());
  await page.screenshot({ path: resolve(output, 'directional-relationship.png') });
  checks.push('People lens records one directional author-room relationship with both participant source heads');

  // A saved relationship prepares a separate scoped exploration.  Opening the
  // scope must not call a provider; only the explicit Explore action may do so.
  const relationshipParentId = workshopState().state.currentSessionId;
  const relationshipRunsBeforeExplore = database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n;
  const relationshipContextClose = page.getByRole('button', { name: 'Hide working story', exact: true });
  if (await relationshipContextClose.isVisible()) await relationshipContextClose.click();
  await relationships.getByRole('button', { name: 'Explore this relationship', exact: true }).click();
  await waitForDatabase(() => {
    const current = workshopState();
    return current?.state.currentSessionId !== relationshipParentId
      && current?.state.sessions.some(session => session.id === current.state.currentSessionId && session.relationshipId === relationship.id);
  }, 'relationship exploration scope');
  const relationshipScopeState = workshopState();
  const relationshipScopeId = relationshipScopeState.state.currentSessionId;
  const relationshipScope = relationshipScopeState.state.sessions.find(session => session.id === relationshipScopeId);
  assert(relationshipScope, 'Relationship exploration must create a saved session');
  assert.equal(relationshipScope.relationshipId, relationship.id);
  assert.deepEqual(new Set(relationshipScope.includedDocumentIds), new Set([relationship.fromDocumentId, relationship.toDocumentId]));
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, relationshipRunsBeforeExplore, 'Opening a relationship scope must not call a provider');
  assert.equal(relationshipScope.workingText, relationship.description);
  assert((await page.getByRole('region', { name: 'Relationship being explored', exact: true }).innerText()).includes('Only this direction is being explored.'));

  await page.getByRole('button', { name: 'Explore', exact: true }).click();
  await page.getByText('Generation complete', { exact: true }).waitFor({ timeout: 30_000 });
  await waitForDatabase(() => database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n === relationshipRunsBeforeExplore + 1, 'relationship packet generation');
  const relationshipRun = database.prepare('SELECT packet_id,dispatch_state FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get();
  assert.equal(relationshipRun.dispatch_state, 'delivered');
  const relationshipPacket = JSON.parse(database.prepare('SELECT packet_json FROM context_packets WHERE id=?').get(relationshipRun.packet_id).packet_json);
  const relationshipEnvelope = relationshipPacket.messages.flatMap(message => {
    try { return JSON.parse(message.content).workshop ?? []; } catch { return []; }
  })[0];
  assert.equal(relationshipEnvelope?.relationship?.id, relationship.id, 'The frozen packet must retain the chosen relationship identity');
  assert.equal(relationshipEnvelope?.relationship?.fromDocumentId, relationship.fromDocumentId);
  assert.equal(relationshipEnvelope?.relationship?.toDocumentId, relationship.toDocumentId);
  assert.equal(relationshipEnvelope?.relationship?.type, relationship.type);
  assert.equal(relationshipEnvelope?.relationship?.description, relationship.description);
  assert.equal(relationshipEnvelope?.relationship?.uncertainty, relationship.uncertainty);
  assert.equal(relationshipEnvelope?.relationship?.status, 'chosen');
  const mandatoryHandles = relationshipPacket.receipt?.mandatorySourceHandles ?? [];
  assert(mandatoryHandles.length >= 2, 'The relationship packet must retain both mandatory endpoint source handles');
  const pinnedDocumentIds = mandatoryHandles.map(handle => database.prepare('SELECT document_id FROM snapshot_sources WHERE snapshot_id=? AND handle=?').get(relationshipPacket.receipt.snapshotId, handle)?.document_id);
  assert(pinnedDocumentIds.includes(relationship.fromDocumentId), 'The frozen source pins must include the relationship source endpoint');
  assert(pinnedDocumentIds.includes(relationship.toDocumentId), 'The frozen source pins must include the relationship target endpoint');
  await page.screenshot({ path: resolve(output, 'relationship-exploration-packet.png') });

  await page.getByRole('button', { name: 'Use this version', exact: true }).click();
  const relationshipAdoption = page.getByRole('region', { name: 'Adoption preview', exact: true });
  await relationshipAdoption.getByRole('heading', { name: 'Where should this version go?', exact: true }).waitFor();
  assert.equal(await relationshipAdoption.locator('fieldset').count(), 0, 'Relationship adoption must not preselect a character destination');
  const chooseDestination = relationshipAdoption.getByRole('button', { name: 'Choose a destination', exact: true });
  await chooseDestination.waitFor();
  await chooseDestination.click();
  const relationshipTarget = relationshipAdoption.locator('fieldset').first();
  const relationshipDestination = relationshipTarget.locator('label').filter({ hasText: /^Destination/ }).locator('select');
  assert.equal(await relationshipDestination.inputValue(), '', 'Relationship adoption must require an explicit destination choice');
  await relationshipAdoption.getByRole('button', { name: 'Cancel', exact: true }).click();

  const savedExplorationsAfterRelationship = page.getByRole('navigation', { name: 'Saved explorations', exact: true });
  const relationshipParentIndex = workshopState().state.sessions.findIndex(session => session.id === relationshipParentId);
  await savedExplorationsAfterRelationship.getByRole('button').nth(relationshipParentIndex).click();
  await waitForDatabase(() => workshopState()?.state.currentSessionId === relationshipParentId, 'relationship parent restore');
  checks.push('Exploring a chosen relationship creates an independent pinned scope without auto-generation, freezes its exact direction and uncertainty in the packet, and requires an explicit adoption destination');

  const recap = page.locator('details.workshop-recap');
  if (await recap.getAttribute('open') === null) await recap.locator('summary').click();
  await recap.getByRole('button', { name: 'Saved exploration versions', exact: true }).click();
  await recap.getByText(/^Workshop version \d+$/, { exact: true }).first().waitFor();
  assert((await recap.locator('ul > li').count()) > 0);
  await page.screenshot({ path: resolve(output, 'workshop-history.png') });
  checks.push('Saved exploration history reopens from the UI after adoption and relationship edits');

  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.getByRole('button', { name: new RegExp(`^${title} Last opened`) }).click();
  await page.getByRole('tab', { name: 'Develop', exact: true }).waitFor();
  await ensureDevelopMode();
  await page.getByRole('heading', { name: 'People', exact: true }).waitFor();
  const reopenedIdea = page.locator('details.workshop-brief');
  if (await reopenedIdea.getAttribute('open') === null) await reopenedIdea.locator('summary').click();
  const reopenedBrief = reopenedIdea.getByRole('textbox', { name: 'What you want to explore', exact: true });
  await reopenedBrief.waitFor();
  assert.equal(await reopenedBrief.inputValue(), seed);
  assert.equal(await page.getByRole('textbox', { name: 'Develop or edit directly', exact: true }).inputValue(), authorEdit);
  const reopenedState = workshopState();
  assert.equal(reopenedState.state.sessions.find(session => session.brief === seed)?.workingText, authorEdit);
  const reopenedRecap = page.locator('details.workshop-recap');
  if (await reopenedRecap.getAttribute('open') === null) await reopenedRecap.locator('summary').click();
  await reopenedRecap.getByRole('button', { name: 'Saved exploration versions', exact: true }).click();
  await reopenedRecap.getByText(/^Workshop version \d+$/, { exact: true }).first().waitFor();
  assert((await reopenedRecap.locator('ul > li').count()) > 0);
  await page.screenshot({ path: resolve(output, 'reopened-workshop-history.png') });
  checks.push('Returning through the library restores the seed, author working text, chosen decision, relationship, and history without another generation');

  await page.getByRole('tab', { name: 'Write', exact: true }).click();
  await page.getByRole('tab', { name: /^Chapters/ }).waitFor();
  assert.equal(documents().filter(document => document.kind === 'chapter').length, 0);
  assert.equal(documents().filter(document => document.kind === 'world').length, 1);
  assert.equal(documents().filter(document => document.kind === 'character').length, 1);
  checks.push('The post-reopen Develop-to-Write barrier preserves author material while chapters remain unchanged');

  // W23: one atomic adoption can update an existing world, create a Unicode
  // character, and record a directional relationship.  The prior relationship
  // deliberately uses the world as one endpoint so its changed-source impact
  // can be checked after the commit.
  await page.getByRole('tab', { name: 'Develop', exact: true }).click();
  await ensureDevelopMode();
  const staleCandidateTray = page.getByRole('region', { name: 'Selected details', exact: true });
  while (await staleCandidateTray.getByRole('button', { name: 'Remove', exact: true }).count()) {
    await staleCandidateTray.getByRole('button', { name: 'Remove', exact: true }).first().click();
  }
  await page.locator('.workshop-save-status').filter({ hasText: 'Saved on this computer' }).waitFor();
  await waitForDatabase(() => {
    const sessions = workshopState()?.state.sessions;
    return sessions?.length > 0 && sessions.every(session => session.selectedDetails.length === 0);
  }, 'stale selected-detail removal');
  const secondAdoptionDocumentsBefore = documents();
  const existingWorld = secondAdoptionDocumentsBefore.find(document => document.id === adoptedWorld.id);
  const existingCharacter = secondAdoptionDocumentsBefore.find(document => document.kind === 'character');
  assert(existingWorld && existingCharacter, 'The second adoption requires the existing world and character fixtures');
  assert(relationship.sourceHeads.some(head => head.documentId === existingWorld.id), 'The prior relationship must include the world endpoint');
  const updatedWorldText = 'The archive now records each memory before its owner is ready to face it.';
  const newCharacterTitle = 'Érin — Qiao';
  const newCharacterText = 'Érin — Qiao keeps a careful index of the memories the archive refuses to name.';
  const adoptionRelationshipDescription = 'The archive trusts Érin — Qiao with the memories it cannot name.';
  await page.getByRole('button', { name: 'Use this version', exact: true }).click();
  const secondAdoption = page.getByRole('region', { name: 'Adoption preview', exact: true });
  await secondAdoption.getByRole('heading', { name: 'Where should this version go?', exact: true }).waitFor();
  const firstTarget = secondAdoption.locator('fieldset').first();
  await firstTarget.getByRole('combobox', { name: 'Destination', exact: true }).selectOption(existingWorld.id);
  await firstTarget.getByRole('combobox', { name: 'Change', exact: true }).selectOption('add');
  await firstTarget.getByRole('textbox', { name: 'Content to choose', exact: true }).fill(updatedWorldText);
  await secondAdoption.getByRole('button', { name: 'Include related material in this decision', exact: true }).click();
  const secondTarget = secondAdoption.locator('fieldset').nth(1);
  await secondTarget.getByRole('textbox', { name: 'Title', exact: true }).fill(newCharacterTitle);
  await secondTarget.getByRole('combobox', { name: 'Kind', exact: true }).selectOption('character');
  await secondTarget.getByRole('textbox', { name: 'Content to choose', exact: true }).fill(newCharacterText);
  const adoptionRelationships = secondAdoption.getByRole('region', { name: 'Relationships in this adoption', exact: true });
  await adoptionRelationships.getByRole('button', { name: 'Include a relationship', exact: true }).click();
  const newParticipant = adoptionRelationships.locator('option').filter({ hasText: 'New in this decision' }).first();
  const newCharacterTargetId = await newParticipant.getAttribute('value');
  assert(newCharacterTargetId, 'The new character must be available as a stable relationship participant');
  await adoptionRelationships.getByRole('combobox', { name: 'From', exact: true }).selectOption(existingWorld.id);
  await adoptionRelationships.getByRole('combobox', { name: 'To', exact: true }).selectOption(newCharacterTargetId);
  await adoptionRelationships.getByRole('textbox', { name: 'Relationship type', exact: true }).fill('trusts');
  await adoptionRelationships.getByRole('textbox', { name: 'What this relationship means', exact: true }).fill(adoptionRelationshipDescription);
  await adoptionRelationships.getByRole('textbox', { name: 'What remains uncertain', exact: true }).fill('Whether the archive will accept the index as a true account.');
  await secondAdoption.getByRole('textbox', { name: 'Why this version?', exact: true }).fill('Update the archive and give its keeper a bounded responsibility.');
  const secondPreviewState = workshopState();
  const secondPreviewDocuments = documents();
  const secondPreviewChapters = secondPreviewDocuments.filter(document => document.kind === 'chapter').length;
  await secondAdoption.getByRole('button', { name: 'Preview all changes', exact: true }).click();
  await secondAdoption.getByRole('heading', { name: 'Choose this version for your story', exact: true }).waitFor();
  assert.deepEqual(documents(), secondPreviewDocuments, 'Multi-target adoption preview must not write story documents');
  assert.equal(documents().filter(document => document.kind === 'chapter').length, secondPreviewChapters, 'Multi-target preview must not write chapters');
  assert.deepEqual(workshopState(), secondPreviewState, 'Multi-target adoption preview must not change workshop state');
  const storedPreviewRow = database.prepare('SELECT preview_json FROM workshop_adoption_previews ORDER BY rowid DESC LIMIT 1').get();
  assert(storedPreviewRow?.preview_json, 'The multi-target adoption preview must be durable');
  const storedPreview = JSON.parse(storedPreviewRow.preview_json);
  assert.equal(storedPreview.targets.length, 2);
  assert.equal(storedPreview.relationships.length, 1);
  assert.deepEqual(storedPreview.candidateIds, [], 'The manual multi-target adoption must not reuse stale generated candidates');
  assert.equal(storedPreview.relationships[0].fromDocumentId, existingWorld.id);
  assert.equal(storedPreview.relationships[0].toDocumentId, newCharacterTargetId);
  const secondPreviewText = await secondAdoption.innerText();
  assert(secondPreviewText.includes(newCharacterTitle));
  assert(secondPreviewText.includes(adoptionRelationshipDescription));
  assert(secondPreviewText.includes('Relationships to choose'));
  await page.screenshot({ path: resolve(output, 'multi-target-adoption-preview.png') });
  await secondAdoption.getByRole('button', { name: 'Confirm Use this version', exact: true }).click();
  await page.getByText('Version chosen. Its source and rationale are saved; writing access remains author only.', { exact: true }).waitFor();
  await waitForDatabase(() => documents().filter(document => document.kind === 'character').length === 2, 'multi-target character adoption');
  const secondAdoptionDocuments = documents();
  const updatedWorld = secondAdoptionDocuments.find(document => document.id === existingWorld.id);
  const newCharacter = secondAdoptionDocuments.find(document => document.kind === 'character' && document.id !== existingCharacter.id);
  assert(updatedWorld && newCharacter, 'The multi-target adoption must save both target documents');
  assert.notEqual(updatedWorld.body_json, existingWorld.body_json);
  assert(documentText(updatedWorld).includes(updatedWorldText), 'The existing world update must preserve the chosen appended text');
  assert.equal(newCharacter.title, newCharacterTitle);
  assert.equal(documentText(newCharacter), newCharacterText);
  assert.equal(secondAdoptionDocuments.filter(document => document.kind === 'chapter').length, 0, 'Multi-target adoption must not write chapters');
  const secondState = workshopState().state;
  const chosenWorld = secondState.decisions.find(decision => decision.documentId === existingWorld.id && decision.status === 'chosen');
  const chosenCharacter = secondState.decisions.find(decision => decision.documentId === newCharacter.id && decision.status === 'chosen');
  assert(chosenWorld && chosenCharacter, 'Both multi-target decisions must remain chosen');
  const committedRelationship = secondState.relationships.find(item => item.description === adoptionRelationshipDescription);
  assert(committedRelationship, 'The new directional relationship must be committed');
  assert.deepEqual(committedRelationship.sourceHeads, [chosenWorld.head, chosenCharacter.head]);
  const changedRelationshipImpact = secondState.impacts.find(impact => impact.relationshipId === relationship.id && impact.documentId === existingWorld.id);
  assert(changedRelationshipImpact, 'The changed world endpoint must receive a relationship review flag');
  assert.equal(changedRelationshipImpact.decisionId, chosenWorld.id);
  assert.equal(changedRelationshipImpact.candidateId ?? null, null);
  assert.equal(changedRelationshipImpact.status, 'needsReview');
  const committedNewCharacterBody = JSON.parse(newCharacter.body_json);
  checks.push('One atomic adoption updates the existing world, creates Unicode character material, commits a directional relationship with exact source heads, records the changed endpoint provenance, and leaves chapters untouched');

  // The new character identity and body must survive a real project reopen,
  // before the next Workshop request is prepared.
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.getByRole('button', { name: new RegExp(`^${title} Last opened`) }).click();
  await page.getByRole('tab', { name: 'Develop', exact: true }).waitFor();
  await ensureDevelopMode();
  const reopenedNewCharacter = documents().find(document => document.id === newCharacter.id);
  assert(reopenedNewCharacter, 'The Unicode character must be present after reopening');
  assert.equal(reopenedNewCharacter.title, newCharacterTitle);
  assert.deepEqual(JSON.parse(reopenedNewCharacter.body_json), committedNewCharacterBody);
  assert.equal(documents().filter(document => document.kind === 'chapter').length, 0);
  checks.push('The Unicode character title and exact body survive reopening the project');

  // W23 names are explicit author metadata. Exercise both the Workshop lens
  // picker and the Writer action, then verify the atomic source-epoch CAS
  // without creating another provider run or changing the document itself.
  const namesRunsBefore = database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n;
  const namesDocumentBefore = documents().find(document => document.id === newCharacter.id);
  const namesEpochBefore = contextSourceEpoch();
  const namesContextClose = page.getByRole('button', { name: 'Close working story', exact: true });
  if (await namesContextClose.isVisible()) await namesContextClose.click();
  await page.getByRole('button', { name: 'People', exact: true }).click();
  await page.getByRole('heading', { name: 'People', exact: true }).waitFor();
  const workshopNamesSection = page.getByRole('region', { name: 'Names for saved people and places', exact: true });
  const workshopNamesSelect = workshopNamesSection.getByRole('combobox', { name: 'Saved person, place, or group', exact: true });
  await workshopNamesSelect.selectOption(newCharacter.id);
  const workshopNames = page.getByRole('region', { name: 'Names and aliases', exact: true });
  await workshopNames.waitFor();
  await workshopNames.locator('textarea:not([disabled])').waitFor();
  assert.equal(await workshopNames.getByRole('textbox', { name: 'Alternate names and transliterations', exact: true }).inputValue(), '');
  await workshopNames.getByRole('button', { name: 'Close names', exact: true }).click();

  await page.getByRole('tab', { name: 'Write', exact: true }).click();
  await page.getByRole('tab', { name: /^Characters/ }).click();
  await page.getByRole('navigation', { name: 'Documents', exact: true }).getByRole('button').filter({ has: page.getByText(newCharacterTitle, { exact: true }) }).click();
  const writer = page.getByRole('main', { name: 'Writing desk', exact: true });
  await writer.waitFor();
  await writer.getByRole('button', { name: 'Names & aliases', exact: true }).click();
  const writerNames = page.getByRole('region', { name: 'Names and aliases', exact: true });
  await writerNames.waitFor();
  const namesInput = writerNames.getByRole('textbox', { name: 'Alternate names and transliterations', exact: true });
  const savedNames = '林乔\nLin Qiao\nAsh Wren';
  const savedNamesCanonical = 'Ash Wren\nLin Qiao\n林乔';
  await namesInput.fill(savedNames);
  await writerNames.getByRole('button', { name: 'Save names', exact: true }).click();
  await waitForDatabase(() => contextSourceEpoch() === namesEpochBefore + 1 && JSON.stringify(documentAliases(newCharacter.id)) === JSON.stringify(['Ash Wren', 'Lin Qiao', '林乔']), 'saved aliases');
  assert.deepEqual(documentAliases(newCharacter.id), ['Ash Wren', 'Lin Qiao', '林乔']);
  assert.equal(contextSourceEpoch(), namesEpochBefore + 1);
  const namesDocumentAfter = documents().find(document => document.id === newCharacter.id);
  assert.equal(namesDocumentAfter.title, namesDocumentBefore.title);
  assert.equal(namesDocumentAfter.body_json, namesDocumentBefore.body_json);
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, namesRunsBefore, 'Saving names must not generate');
  await writerNames.getByRole('button', { name: 'Close names', exact: true }).click();
  await writer.getByRole('button', { name: 'Names & aliases', exact: true }).click();
  const reopenedWriterNames = page.getByRole('region', { name: 'Names and aliases', exact: true });
  await reopenedWriterNames.getByRole('textbox', { name: 'Alternate names and transliterations', exact: true }).waitFor();
  await reopenedWriterNames.locator('textarea:not([disabled])').waitFor();
  assert.equal(await reopenedWriterNames.getByRole('textbox', { name: 'Alternate names and transliterations', exact: true }).inputValue(), savedNamesCanonical);
  await reopenedWriterNames.getByRole('button', { name: 'Close names', exact: true }).click();
  checks.push('Writer Names & aliases saves Unicode names and transliterations atomically while preserving the character and avoiding generation');

  // A project resume must reload the saved aliases, still without another run.
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.getByRole('button', { name: new RegExp(`^${title} Last opened`) }).click();
  await page.getByRole('tab', { name: 'Develop', exact: true }).waitFor();
  await ensureDevelopMode();
  const resumedNamesContextClose = page.getByRole('button', { name: 'Close working story', exact: true });
  if (await resumedNamesContextClose.isVisible()) await resumedNamesContextClose.click();
  await page.getByRole('button', { name: 'People', exact: true }).click();
  await page.getByRole('heading', { name: 'People', exact: true }).waitFor();
  const resumedNamesSection = page.getByRole('region', { name: 'Names for saved people and places', exact: true });
  await resumedNamesSection.getByRole('combobox', { name: 'Saved person, place, or group', exact: true }).selectOption(newCharacter.id);
  const resumedNames = page.getByRole('region', { name: 'Names and aliases', exact: true });
  await resumedNames.getByRole('textbox', { name: 'Alternate names and transliterations', exact: true }).waitFor();
  // The textbox mounts before the asynchronous saved-alias read finishes.
  // Waiting for the field to be editable observes that read, not just its DOM.
  await resumedNames.locator('textarea:not([disabled])').waitFor();
  assert.equal(await resumedNames.getByRole('textbox', { name: 'Alternate names and transliterations', exact: true }).inputValue(), savedNamesCanonical);
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, namesRunsBefore, 'Reopening names must not generate');

  // Unsaved names block both workspace navigation and project switching. The
  // author can then save deliberately and continue to Write.
  const unsavedNames = `${savedNamesCanonical}\nUnpersisted draft`;
  await resumedNames.getByRole('textbox', { name: 'Alternate names and transliterations', exact: true }).fill(unsavedNames);
  await page.getByRole('tab', { name: 'Write', exact: true }).click();
  await sleep(250);
  assert.equal(await page.getByRole('tab', { name: 'Develop', exact: true }).getAttribute('aria-selected'), 'true', 'Unsaved names must refuse Develop to Write');
  assert.equal(await page.getByRole('main', { name: 'Current exploration', exact: true }).count(), 1);
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await sleep(250);
  assert.equal(await page.getByRole('heading', { name: 'Your stories', exact: true }).count(), 0, 'Unsaved names must refuse project switching');
  const namesAfterRefusal = page.getByRole('region', { name: 'Names and aliases', exact: true });
  const changedNames = `${savedNamesCanonical}\nThe Archive Keeper`;
  await namesAfterRefusal.getByRole('textbox', { name: 'Alternate names and transliterations', exact: true }).fill(changedNames);
  await namesAfterRefusal.getByRole('button', { name: 'Save names', exact: true }).click();
  const changedNamesCanonical = ['Ash Wren', 'Lin Qiao', 'The Archive Keeper', '林乔'];
  await waitForDatabase(() => contextSourceEpoch() === namesEpochBefore + 2 && JSON.stringify(documentAliases(newCharacter.id)) === JSON.stringify(changedNamesCanonical), 'names save after navigation refusal');
  assert.deepEqual(documentAliases(newCharacter.id), changedNamesCanonical);
  assert.equal(contextSourceEpoch(), namesEpochBefore + 2);
  await namesAfterRefusal.getByRole('button', { name: 'Close names', exact: true }).click();
  await page.getByRole('tab', { name: 'Write', exact: true }).click();
  await page.getByRole('tab', { name: /^Characters/ }).click();
  await page.getByRole('navigation', { name: 'Documents', exact: true }).getByRole('button').filter({ has: page.getByText(newCharacterTitle, { exact: true }) }).click();
  await page.getByRole('main', { name: 'Writing desk', exact: true }).waitFor();
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, namesRunsBefore, 'Saved names flow must still avoid generation');
  checks.push('Saved names reopen through project resume; unsaved aliases refuse Write and project switching until explicitly saved');

  // W23 voice guidance is an explicit second local-mock dispatch. It only
  // changes the saved Workshop request/result; it must not adopt the sample.
  await page.getByRole('tab', { name: 'Develop', exact: true }).click();
  await ensureDevelopMode();
  await page.getByRole('button', { name: 'Themes & tone', exact: true }).click();
  await page.getByRole('heading', { name: 'Themes & tone', exact: true }).waitFor();
  const voiceSample = 'Rain ticked against the workshop glass while she counted each drop.';
  const voiceWorking = page.getByRole('textbox', { name: 'Develop or edit directly', exact: true });
  await voiceWorking.fill(voiceSample);
  await page.locator('.workshop-save-status').filter({ hasText: 'Saved on this computer' }).waitFor();
  const voiceSessionId = workshopState().state.currentSessionId;
  await waitForDatabase(() => workshopState()?.state.sessions.find(session => session.id === voiceSessionId)?.workingText === voiceSample, 'voice sample save');
  const voiceDocumentsBefore = documents();
  const voiceDecisionsBefore = workshopState().state.decisions.length;
  const voiceRunsBefore = database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n;
  const adoptionReceiptsBeforeVoice = database.prepare("SELECT count(*) AS n FROM workshop_receipts WHERE operation_kind='adoptWorkshop'").get().n;
  await page.getByRole('button', { name: 'Propose voice guidance from this sample', exact: true }).click();
  const voiceAction = page.getByRole('combobox', { name: 'Next action', exact: true });
  assert.equal(await voiceAction.inputValue(), 'voiceGuidance');
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, voiceRunsBefore, 'Preparing voice guidance must not dispatch');
  await page.getByRole('textbox', { name: 'Your direction', exact: true }).fill('Use measured warmth and varied sentence density; keep the sample events outside the story.');
  await page.locator('.workshop-save-status').filter({ hasText: 'Saved on this computer' }).waitFor();
  await page.getByRole('button', { name: 'Explore', exact: true }).click();
  await page.getByText('Generation complete', { exact: true }).waitFor({ timeout: 30_000 });
  await page.getByRole('heading', { name: 'Compare voice guidance', exact: true }).waitFor();
  await waitForDatabase(() => {
    const latest = database.prepare('SELECT status,output_text FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get();
    return database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n === voiceRunsBefore + 1
      && latest?.status === 'completed' && latest.output_text.includes('STYLE guidance');
  }, 'voice guidance mock run');
  const voiceCards = page.locator('.candidate-card');
  assert.equal(await voiceCards.count(), 3, 'Voice guidance must produce three alternatives');
  const voiceCardTexts = await voiceCards.allTextContents();
  assert(voiceCardTexts.every(text => text.includes('STYLE guidance') && text.includes('Sentence density') && text.includes('Viewpoint distance') && text.includes('Humor') && text.includes('Exposition') && text.includes('Dialogue rhythm')));
  assert.equal(new Set(['restrained', 'brisk', 'lyrical'].filter(dimension => voiceCardTexts.some(text => text.includes(dimension)))).size, 3);
  assert.equal(await voiceWorking.inputValue(), voiceSample, 'Voice generation must not replace the author sample');
  assert.deepEqual(documents(), voiceDocumentsBefore, 'Voice guidance must not mutate story documents');
  assert.equal(workshopState().state.decisions.length, voiceDecisionsBefore, 'Voice guidance must not create a decision automatically');
  assert.equal(database.prepare("SELECT count(*) AS n FROM workshop_receipts WHERE operation_kind='adoptWorkshop'").get().n, adoptionReceiptsBeforeVoice, 'Voice guidance must not adopt automatically');
  await voiceCards.first().getByRole('button', { name: 'Develop this', exact: true }).click();
  await waitForDatabase(() => {
    const savedVoice = workshopState()?.state.sessions.find(session => session.id === voiceSessionId)?.workingText;
    return typeof savedVoice === 'string' && savedVoice !== voiceSample;
  }, 'author voice guidance development');
  assert.notEqual(await voiceWorking.inputValue(), voiceSample, 'The sample may change only after the author develops a guidance alternative');
  assert.deepEqual(documents(), voiceDocumentsBefore, 'Developing voice guidance must remain author-only material');
  assert.equal(workshopState().state.decisions.length, voiceDecisionsBefore, 'Developing voice guidance must not create a decision automatically');
  assert.equal(database.prepare("SELECT count(*) AS n FROM workshop_receipts WHERE operation_kind='adoptWorkshop'").get().n, adoptionReceiptsBeforeVoice, 'Developing voice guidance must not adopt automatically');
  await page.screenshot({ path: resolve(output, 'voice-guidance-alternatives.png') });
  checks.push('Themes & tone sends a second explicit local-mock voice-guidance request, renders three STYLE alternatives, preserves the sample until author development, and keeps guidance out of adoption');

  // Noncanon feel tests have their own sample route; their event text must
  // never enter the working version just to ask for voice guidance.
  const momentWorkingBefore = workshopState().state.sessions.find(session => session.id === voiceSessionId).workingText;
  const momentDetailsBefore = structuredClone(workshopState().state.sessions.find(session => session.id === voiceSessionId).selectedDetails);
  const momentRunsBefore = database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n;
  await voiceAction.selectOption('moment');
  await page.getByRole('button', { name: 'Explore', exact: true }).click();
  await waitForDatabase(() => database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n === momentRunsBefore + 1
    && database.prepare('SELECT status FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get()?.status === 'completed', 'noncanon moment mock run');
  await page.getByText('Noncanon experiment', { exact: true }).waitFor();
  const momentCards = page.locator('.candidate-card');
  assert([2, 3].includes(await momentCards.count()), 'A moment must provide two or three treatments');
  assert.equal(await momentCards.getByRole('button', { name: 'Develop this', exact: true }).count(), 0);
  const momentCard = momentCards.first();
  await momentCard.getByRole('button', { name: 'Save for later', exact: true }).click();
  await momentCard.getByRole('button', { name: 'Select a sample passage', exact: true }).click();
  assert.equal(await momentCard.locator('.candidate-include').count(), 0);
  assert.equal(await momentCard.getByRole('button', { name: 'Select full direction', exact: true }).count(), 0);
  const momentSample = await momentCard.getByRole('textbox', { name: 'Noncanon sample', exact: true }).inputValue();
  await page.screenshot({ path: resolve(output, 'noncanon-sample-actions.png') });
  await momentCard.getByRole('button', { name: 'Propose voice guidance', exact: true }).click();
  await waitForDatabase(() => database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n === momentRunsBefore + 2
    && database.prepare('SELECT status FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get()?.status === 'completed', 'explicit moment-to-voice-guidance request');
  await page.getByRole('heading', { name: 'Compare voice guidance', exact: true }).waitFor();
  const momentVoiceRun = database.prepare('SELECT packet_id FROM discussion_runs ORDER BY rowid DESC LIMIT 1').get();
  const momentVoiceMessages = JSON.parse(database.prepare('SELECT packet_json FROM context_packets WHERE id=?').get(momentVoiceRun.packet_id).packet_json).messages;
  const momentVoiceEnvelope = momentVoiceMessages.map(message => { try { return JSON.parse(message.content).workshop; } catch { return null; } }).find(Boolean);
  assert.equal(momentVoiceEnvelope.voiceGuidance.sample, momentSample);
  assert.equal(momentVoiceEnvelope.voiceGuidance.adoptEvents, false);
  const momentSessionAfter = workshopState().state.sessions.find(session => session.id === voiceSessionId);
  assert.equal(momentSessionAfter.workingText, momentWorkingBefore);
  assert.deepEqual(momentSessionAfter.selectedDetails, momentDetailsBefore);
  assert.deepEqual(documents(), voiceDocumentsBefore);
  assert.equal(workshopState().state.decisions.length, voiceDecisionsBefore);
  assert.equal(database.prepare("SELECT count(*) AS n FROM workshop_receipts WHERE operation_kind='adoptWorkshop'").get().n, adoptionReceiptsBeforeVoice);
  checks.push('Noncanon moments save as samples, expose no Develop/tray/context-include shortcut, and send the exact chosen sample only on an explicit voice-guidance request without changing the working version, selected details, documents, or decisions');

  const runsBeforeBible = database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n;
  await page.getByRole('button', { name: 'Story Bible', exact: true }).click();
  const bible = page.getByRole('dialog', { name: 'Story Bible', exact: true });
  const characterChoice = bible.locator('article').filter({ has: page.getByRole('heading', { name: newCharacterTitle, exact: true }) });
  await characterChoice.locator('.workshop-prose').waitFor();
  assert.equal(await characterChoice.locator('.workshop-prose').innerText(), newCharacterText);
  const worldChoice = bible.locator('article').filter({ has: page.getByRole('heading', { name: 'The Ember Archive', exact: true }) });
  assert.equal(await worldChoice.count(), 1, 'Superseded world choices must not compete with the current choice');
  await characterChoice.getByRole('button', { name: `Open source: ${newCharacterTitle}`, exact: true }).click();
  await page.getByRole('heading', { name: newCharacterTitle, exact: true }).waitFor();
  const newerCharacterText = 'Later author edit: the keeper has left the archive for the harbor.';
  await page.getByRole('textbox', { name: 'Manuscript', exact: true }).fill(newerCharacterText);
  await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  await waitForDatabase(() => documentText(documents().find(document => document.id === newCharacter.id)) === newerCharacterText, 'changed chosen source save');
  await page.getByRole('button', { name: 'Story Bible', exact: true }).click();
  await characterChoice.getByText(/Source changed since this choice/).waitFor();
  assert.equal(await characterChoice.locator('.workshop-prose').innerText(), newCharacterText, 'The Bible must show the exact chosen historical version after its source changes');
  assert.equal(await worldChoice.count(), 1, 'The other chosen material remains available');
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, runsBeforeBible, 'Reading chosen history and manual edits must not generate');
  await page.screenshot({ path: resolve(output, 'story-bible-exact-history.png') });
  await bible.getByRole('button', { name: 'Close Story Bible', exact: true }).click();
  await page.getByRole('button', { name: 'Story Bible', exact: true }).evaluate(button => {
    if (document.activeElement !== button) return new Promise(resolvePromise => requestAnimationFrame(resolvePromise));
  });
  assert.equal(await page.getByRole('button', { name: 'Story Bible', exact: true }).evaluate(button => document.activeElement === button), true, 'Closing the Bible restores its launch button focus');
  checks.push('Story Bible opens exact chosen sources, excludes superseded choices, preserves historical text after a source edit, and restores focus without generation');

  await page.getByRole('tab', { name: 'Develop', exact: true }).click();
  await ensureDevelopMode();
  const navigationDocuments = documents();
  const navigationRuns = database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n;
  for (const [label, lens] of [['Overview', 'overview'], ['World', 'world'], ['People', 'people'], ['Themes & tone', 'themes'], ['Story possibilities', 'possibilities'], ['Notebook', 'notebook']]) {
    await page.getByRole('button', { name: label, exact: true }).click();
    await page.getByRole('heading', { name: label, exact: true }).waitFor();
    await waitForDatabase(() => {
      const current = workshopState()?.state;
      return current?.sessions.find(session => session.id === current.currentSessionId)?.lens === lens;
    }, `${label} saved navigation`);
  }
  assert.deepEqual(documents(), navigationDocuments, 'Lens navigation must leave all documents untouched');
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, navigationRuns);
  checks.push('All six development lenses are reachable and save their position without changing documents or generating');

  const recapSessionId = workshopState().state.currentSessionId;
  const recapQuestion = workshopState().state.sessions.find(session => session.id === recapSessionId).focusQuestion;
  await page.getByRole('button', { name: 'Keep mysterious', exact: true }).click();
  await waitForDatabase(() => workshopState()?.state.sessions.find(session => session.id === recapSessionId)?.questions.some(question => question.text === recapQuestion && question.status === 'keepMysterious'), 'intentional mystery save');
  const stoppingRecap = page.locator('.workshop-recap');
  if (!await stoppingRecap.evaluate(node => node.open)) await stoppingRecap.locator(':scope > summary').click();
  const recapNextTime = stoppingRecap.locator('dt:has-text("Next time") + dd');
  assert(!(await recapNextTime.innerText()).includes(recapQuestion), 'A stopping point must not recommend answering an intentional mystery');
  assert((await stoppingRecap.innerText()).includes('Intentionally mysterious'), 'The mystery remains visible with its deliberate status');
  await page.screenshot({ path: resolve(output, 'recap-preserves-intentional-mystery.png') });
  await page.getByText('Open questions and intentional unknowns', { exact: true }).click();
  const recapQuestionRecord = page.locator('.workshop-question-record').filter({ has: page.getByText(recapQuestion, { exact: true }) });
  await recapQuestionRecord.getByRole('combobox', { name: /^State/ }).selectOption('open');
  await waitForDatabase(() => workshopState()?.state.sessions.find(session => session.id === recapSessionId)?.questions.some(question => question.text === recapQuestion && question.status === 'open'), 'deliberate question reopening');
  assert((await recapNextTime.innerText()).includes(recapQuestion), 'Explicit reopening makes the question available for the next session');
  assert.deepEqual(documents(), navigationDocuments, 'Recap inspection and question dispositions must leave documents untouched');
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, navigationRuns, 'Recaps and question reopening must not generate');
  checks.push('The saved stopping point preserves an intentional mystery without recommending it, then restores the next question only after explicit reopening, with no generation or document writes');

  async function openPreferenceShelf() {
    const reveal = page.getByRole('button', { name: 'Working story & context', exact: true });
    if (await reveal.isVisible()) await reveal.click();
    const preferences = page.getByRole('region', { name: 'Creative preferences', exact: true });
    const summary = preferences.locator(':scope > details > summary').filter({ hasText: 'Find a preference or preset' });
    if (!await summary.evaluate(node => node.closest('details').open)) await summary.click();
    return preferences;
  }
  let preferences = await openPreferenceShelf();
  const preferencesBefore = workshopState().state.preferences;
  const presetsBefore = workshopState().state.presets.length;
  const presetRunsBefore = database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n;
  const preset = { schemaVersion: 'workshop-preset.v1', name: 'Archive boundaries', preferences: [{
    label: 'Inherited exceptionalism', family: 'Story ingredients', meaning: 'No inherited gift solves the central conflict.',
    examples: 'No bloodline unlock', timing: 'Throughout this project', polarity: 'avoid', strength: 'hard',
  }] };
  await preferences.getByRole('button', { name: 'Export or import preferences', exact: true }).click();
  const presetReview = page.getByRole('region', { name: 'Review preference preset', exact: true });
  await presetReview.getByRole('textbox', { name: 'Preset text', exact: true }).fill(JSON.stringify(preset, null, 2));
  assert.equal(await presetReview.getByRole('textbox', { name: 'Preset name', exact: true }).inputValue(), preset.name);
  assert.deepEqual(workshopState().state.preferences, preferencesBefore, 'Reviewing imported text must not adopt it');
  assert.equal(workshopState().state.presets.length, presetsBefore, 'Draft preset review must not save a definition');
  await presetReview.getByRole('button', { name: 'Add these project preferences', exact: true }).click();
  await waitForDatabase(() => workshopState()?.state.presets.some(item => item.name === preset.name), 'explicit preset adoption');
  const savedPreset = workshopState().state.presets.find(item => item.name === preset.name);
  const adoptedPreferences = workshopState().state.preferences;
  assert.equal(adoptedPreferences.length, preferencesBefore.length + 1);
  assert(adoptedPreferences.some(item => item.scope === 'project' && item.confirmed && item.polarity === 'avoid' && item.strength === 'hard' && item.label === preset.preferences[0].label));

  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.getByRole('button', { name: new RegExp(`^${title} Last opened`) }).click();
  await ensureDevelopMode();
  await page.getByRole('heading', { name: 'Notebook', exact: true }).waitFor();
  preferences = await openPreferenceShelf();
  await preferences.getByText('Saved project presets', { exact: true }).click();
  const savedPresetCard = preferences.locator('.workshop-saved-preset').filter({ hasText: preset.name });
  await savedPresetCard.getByRole('button', { name: 'Review saved preset', exact: true }).click();
  assert.equal(JSON.parse(await presetReview.getByRole('textbox', { name: 'Preset text', exact: true }).inputValue()).name, preset.name);
  assert.deepEqual(workshopState().state.preferences, adoptedPreferences, 'Reopening a definition must not adopt it again');
  await presetReview.getByRole('textbox', { name: 'Preset name', exact: true }).fill('Archive boundaries revised');
  const revisedPreset = JSON.parse(await presetReview.getByRole('textbox', { name: 'Preset text', exact: true }).inputValue());
  assert.equal(revisedPreset.name, 'Archive boundaries revised', 'Name field edits must update the review text');
  revisedPreset.preferences[0].label = 'Unrevealed ancestry';
  revisedPreset.preferences[0].meaning = 'Keep ancestry outside the central explanation.';
  await presetReview.getByRole('textbox', { name: 'Preset text', exact: true }).fill(JSON.stringify(revisedPreset, null, 2));
  await presetReview.getByRole('button', { name: 'Save preset definition', exact: true }).click();
  await waitForDatabase(() => workshopState()?.state.presets.some(item => item.id === savedPreset.id && item.name === revisedPreset.name), 'edited preset definition');
  assert.equal(workshopState().state.presets.length, presetsBefore + 1, 'Definition edits retain the saved identity');
  assert.deepEqual(workshopState().state.preferences, adoptedPreferences, 'Saving a definition must not change active preferences');
  await preferences.locator('.workshop-saved-preset').filter({ hasText: revisedPreset.name }).getByRole('button', { name: 'Review saved preset', exact: true }).click();
  await presetReview.getByRole('button', { name: 'Add these project preferences', exact: true }).click();
  await waitForDatabase(() => workshopState()?.state.preferences.length === adoptedPreferences.length + 1, 'explicit saved preset reuse');
  await preferences.getByText('1 project preference added.', { exact: true }).waitFor();
  assert.equal(await preferences.getByText('Saved preset opened for review. No preferences have been added yet.', { exact: true }).count(), 0);
  assert.equal(workshopState().state.presets.length, presetsBefore + 1, 'Reusing a saved preset must not duplicate its definition');
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, presetRunsBefore, 'Review, save, reuse, and reopen do not generate');
  assert.deepEqual(documents(), navigationDocuments, 'Preset operations do not alter story documents');
  await page.screenshot({ path: resolve(output, 'saved-preset-reuse.png') });
  checks.push('Preset names follow JSON and field edits, explicit adoption persists across Library reopen, and saved definitions can be edited and reused without duplicate definitions or automatic preference adoption');

  const beforeConflict = workshopState().state.preferences;
  await preferences.getByRole('button', { name: 'Add preference', exact: true }).click();
  const preferenceForm = preferences.locator('.workshop-preference-form');
  await preferenceForm.getByRole('textbox', { name: 'Name', exact: true }).fill('Inherited exceptionalism');
  await preferenceForm.getByRole('textbox', { name: 'What it means to you', exact: true }).fill('An inherited gift unlocks the archive.');
  await preferenceForm.getByRole('combobox', { name: /^Direction/ }).selectOption('want');
  await preferenceForm.getByRole('combobox', { name: /^Applies to/ }).selectOption('exploration');
  await preferenceForm.getByRole('button', { name: 'Save preference', exact: true }).click();
  await preferences.getByRole('alert').filter({ hasText: 'conflicts with the project’s Never preference' }).waitFor();
  assert.deepEqual(workshopState().state.preferences, beforeConflict, 'A local Want must not silently replace the hard project exclusion');
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, presetRunsBefore, 'Resolving a preference conflict remains local');
  await page.screenshot({ path: resolve(output, 'hard-project-preference-conflict.png') });
  await preferenceForm.getByRole('button', { name: 'Cancel', exact: true }).click();
  checks.push('A conflicting local Want is visibly refused without changing a hard project exclusion or generating');

  const firstProjectState = workshopState().state;
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.getByRole('button', { name: 'New project', exact: true }).click();
  await page.getByRole('textbox', { name: 'Project title', exact: true }).fill('');
  await page.getByRole('button', { name: 'Create project', exact: true }).click();
  await page.getByRole('button', { name: 'Develop a story', exact: true }).click();
  await page.getByRole('heading', { name: 'What are you excited about?', exact: true, level: 1 }).waitFor();
  const secondLibrary = await invoke('library_snapshot');
  const untitled = secondLibrary.entries.find(item => item.title === 'Untitled project' && item.path !== entry.path);
  assert(untitled, 'A project title must be optional');
  const untitledPath = await realpath(untitled.path);
  projectInside(untitledPath);
  const secondDatabase = new DatabaseSync(resolve(untitledPath, 'project.sqlite3'), { readOnly: true });
  try {
    assert.equal(secondDatabase.prepare('SELECT count(*) AS n FROM documents').get().n, 0, 'Blank development requires no chapter, character, or world document');
    assert.equal(secondDatabase.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, 0, 'Project creation must not generate');
  } finally { secondDatabase.close(); }
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.getByRole('button', { name: new RegExp(`^${title} Last opened`) }).click();
  await ensureDevelopMode();
  await page.getByRole('heading', { name: 'Notebook', exact: true }).waitFor();
  assert.deepEqual(workshopState().state, firstProjectState, 'A separate blank project must not alter the original Workshop state');
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, presetRunsBefore);
  checks.push('An untitled second project can begin development without story prerequisites or generation; returning restores the first project’s Workshop and preferences');

  // W30: create a real parent exploration, fork it through the visible UI,
  // save and reopen the alternate, then prove that only an explicit adoption
  // preview/confirmation can change an existing story document.
  const w30ExistingWorld = documents().find(document => document.kind === 'world');
  assert(w30ExistingWorld, 'W30 comparison requires an existing chosen world source');
  const w30PreviousSessionId = workshopState().state.currentSessionId;
  await page.getByRole('button', { name: 'New', exact: true }).click();
  const w30Brief = 'A what-if archive makes repair knowledge public.';
  const w30ParentText = 'The archive keeps repair knowledge restricted to apprentices.';
  const w30AlternateText = 'What if repair knowledge becomes public, while the archive still records who can safely use it?';
  const w30BriefDetails = page.locator('details.workshop-brief');
  if (await w30BriefDetails.getAttribute('open') === null) await w30BriefDetails.locator(':scope > summary').click();
  await w30BriefDetails.getByRole('button', { name: 'Bring existing notes', exact: true }).click();
  await w30BriefDetails.getByRole('combobox', { name: 'Saved material', exact: true }).selectOption(w30ExistingWorld.id);
  await waitForDatabase(() => {
    const current = workshopState()?.state.currentSessionId;
    return current && current !== w30PreviousSessionId;
  }, 'W30 new exploration');
  const w30NewSessionId = workshopState().state.currentSessionId;
  await waitForDatabase(() => workshopState()?.state.sessions.find(session => session.id === w30NewSessionId)?.focusDocumentId === w30ExistingWorld.id, 'W30 focus chosen world');
  await w30BriefDetails.getByRole('textbox', { name: 'What you want to explore', exact: true }).fill(w30Brief);
  await page.getByRole('textbox', { name: 'Working title', exact: true }).fill('Repair knowledge baseline');
  await page.getByRole('textbox', { name: 'Develop or edit directly', exact: true }).fill(w30ParentText);
  await page.locator('.workshop-save-status').filter({ hasText: 'Saved on this computer' }).waitFor();
  await waitForDatabase(() => workshopState()?.state.sessions.some(session => session.brief === w30Brief && session.workingText === w30ParentText), 'W30 parent baseline save');

  let w30Preferences = await openPreferenceShelf();
  await w30Preferences.getByRole('button', { name: 'Add preference', exact: true }).click();
  await w30Preferences.getByRole('textbox', { name: 'Name', exact: true }).fill('Public repair knowledge');
  await w30Preferences.getByRole('textbox', { name: 'What it means to you', exact: true }).fill('Explore who benefits when useful repair knowledge is available outside the archive.');
  await w30Preferences.getByRole('button', { name: 'Save preference', exact: true }).click();
  await page.locator('.workshop-save-status').filter({ hasText: 'Saved on this computer' }).waitFor();
  const w30ParentSnapshot = workshopState();
  const w30ParentId = w30ParentSnapshot.state.currentSessionId;
  const w30Parent = w30ParentSnapshot.state.sessions.find(session => session.id === w30ParentId);
  assert(w30Parent, 'W30 parent exploration must be saved');
  const w30ParentPreference = w30ParentSnapshot.state.preferences.find(preference => preference.targetId === w30ParentId && preference.label === 'Public repair knowledge');
  assert(w30ParentPreference, 'W30 parent exploration preference must be saved');
  const w30ParentRecordBeforeFork = structuredClone(w30Parent);
  const w30ParentAnchor = w30Parent.anchorDocumentId;
  const w30DocumentsBeforeFork = structuredClone(documents());
  const w30ChaptersBeforeFork = w30DocumentsBeforeFork.filter(document => document.kind === 'chapter').map(document => ({ id: document.id, body_json: document.body_json }));
  const w30RunsBeforeFork = database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n;

  // Fork only through the product action.  The child must carry an isolated
  // identity and exploration-scoped preference while leaving the parent row
  // and story documents unchanged.
  const w30ContextClose = page.getByRole('button', { name: 'Close working story', exact: true });
  if (await w30ContextClose.isVisible()) await w30ContextClose.click();
  await page.getByRole('button', { name: 'Explore a what-if', exact: true }).click();
  await waitForDatabase(() => {
    const current = workshopState();
    return current?.state.currentSessionId !== w30ParentId && current?.state.sessions.some(session => session.parentSessionId === w30ParentId && session.branchKind === 'whatIf');
  }, 'W30 what-if fork');
  let w30ForkState = workshopState();
  const w30ChildId = w30ForkState.state.currentSessionId;
  const w30Child = w30ForkState.state.sessions.find(session => session.id === w30ChildId);
  assert(w30Child && w30Child.parentSessionId === w30ParentId && w30Child.branchKind === 'whatIf');
  assert.notEqual(w30Child.anchorDocumentId, w30ParentAnchor, 'What-if must have an independent workshop anchor');
  assert.equal(w30Child.workingText, w30ParentText, 'What-if starts from the parent working version');
  const w30ChildPreference = w30ForkState.state.preferences.find(preference => preference.targetId === w30ChildId && preference.label === w30ParentPreference.label);
  assert(w30ChildPreference, 'What-if must carry an exploration preference targeted to the child');
  assert.notEqual(w30ChildPreference.id, w30ParentPreference.id, 'What-if preference must have an independent identity');
  assert.deepEqual(w30ForkState.state.sessions.find(session => session.id === w30ParentId), w30ParentRecordBeforeFork, 'Forking must preserve the complete parent exploration record');
  assert.deepEqual(documents(), w30DocumentsBeforeFork, 'Forking must not write story documents');
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, w30RunsBeforeFork, 'Forking must not call a provider');

  await page.getByRole('textbox', { name: 'Develop or edit directly', exact: true }).fill(w30AlternateText);
  await page.locator('.workshop-save-status').filter({ hasText: 'Saved on this computer' }).waitFor();
  await waitForDatabase(() => workshopState()?.state.sessions.find(session => session.id === w30ChildId)?.workingText === w30AlternateText, 'W30 alternate save');
  w30ForkState = workshopState();
  assert.deepEqual(w30ForkState.state.sessions.find(session => session.id === w30ParentId), w30ParentRecordBeforeFork, 'Editing what-if must preserve the complete parent exploration record');
  const w30SessionsBeforeNavigation = structuredClone(w30ForkState.state.sessions);

  const w30Compare = page.locator('.workshop-branch-comparison');
  const w30CompareSummary = w30Compare.locator(':scope > summary');
  if (await w30CompareSummary.count() && await w30Compare.getAttribute('open') === null) await w30CompareSummary.click();
  await w30Compare.getByText(w30ParentText, { exact: true }).waitFor();
  await w30Compare.getByText(w30AlternateText, { exact: true }).waitFor();
  await w30Compare.getByRole('heading', { name: 'Chosen source revisions', exact: true }).waitFor();
  await w30Compare.getByRole('region', { name: 'Chosen source revisions', exact: true }).getByText(w30ExistingWorld.title, { exact: true }).waitFor();
  await w30Compare.getByText('The archive keeper trusts the archive to preserve what people cannot yet say.', { exact: true }).waitFor();
  await w30Compare.getByRole('heading', { name: 'Likely affected material', exact: true }).waitFor();
  assert((await w30Compare.innerText()).includes('review whether it still holds.'), 'Comparison must surface the saved relationship impact reason');
  await page.screenshot({ path: resolve(output, 'what-if-comparison.png') });

  // Reopen the saved parent, then switch back to the saved child through the
  // exploration navigation.  This exercises persistence without app state
  // injection and verifies neither session overwrites the other.
  const w30SavedExplorations = page.getByRole('navigation', { name: 'Saved explorations', exact: true });
  await w30SavedExplorations.getByRole('button').nth((await w30SavedExplorations.getByRole('button').count()) - 2).click();
  await waitForDatabase(() => workshopState()?.state.currentSessionId === w30ParentId, 'W30 switch back to parent');
  assert.equal(await page.getByRole('textbox', { name: 'Develop or edit directly', exact: true }).inputValue(), w30ParentText);
  assert.deepEqual(workshopState().state.sessions, w30SessionsBeforeNavigation, 'Switching explorations must change only currentSessionId');
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.getByRole('button', { name: new RegExp(`^${title} Last opened`) }).click();
  await ensureDevelopMode();
  await waitForDatabase(() => workshopState()?.state.currentSessionId === w30ParentId, 'W30 parent reopen');
  assert.deepEqual(workshopState().state.sessions, w30SessionsBeforeNavigation, 'Reopening the parent must preserve every saved exploration record');
  await page.getByRole('navigation', { name: 'Saved explorations', exact: true }).getByRole('button').last().click();
  await waitForDatabase(() => workshopState()?.state.currentSessionId === w30ChildId, 'W30 child reopen');
  assert.equal(await page.getByRole('textbox', { name: 'Develop or edit directly', exact: true }).inputValue(), w30AlternateText, 'Reopening must retain the alternate');
  assert.deepEqual(workshopState().state.sessions.find(session => session.id === w30ParentId), w30ParentRecordBeforeFork, 'Reopening must retain the complete parent exploration record');

  const w30DocumentsBeforeAdoption = structuredClone(documents());
  const w30StateBeforeAdoption = structuredClone(workshopState());
  const w30ChaptersBeforeAdoption = w30DocumentsBeforeAdoption.filter(document => document.kind === 'chapter').map(document => ({ id: document.id, body_json: document.body_json }));
  const w30RunsBeforeAdoption = database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n;
  await page.getByRole('button', { name: 'Use this version', exact: true }).click();
  const w30Adoption = page.getByRole('region', { name: 'Adoption preview', exact: true });
  await w30Adoption.getByRole('heading', { name: 'Where should this version go?', exact: true }).waitFor();
  await w30Adoption.getByRole('combobox', { name: 'Destination', exact: true }).selectOption(w30ExistingWorld.id);
  await w30Adoption.getByRole('combobox', { name: 'Change', exact: true }).selectOption('add');
  await w30Adoption.getByRole('textbox', { name: 'Content to choose', exact: true }).fill(w30AlternateText);
  await w30Adoption.getByRole('textbox', { name: 'Why this version?', exact: true }).fill('Test the public repair knowledge what-if explicitly.');
  assert.deepEqual(documents(), w30DocumentsBeforeAdoption, 'Opening what-if adoption must not write documents');
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, w30RunsBeforeAdoption, 'Opening what-if adoption must not call a provider');
  await w30Adoption.getByRole('button', { name: 'Preview all changes', exact: true }).click();
  await w30Adoption.getByRole('heading', { name: 'Choose this version for your story', exact: true }).waitFor();
  assert.deepEqual(documents(), w30DocumentsBeforeAdoption, 'What-if preview must not write documents');
  assert.deepEqual(workshopState(), w30StateBeforeAdoption, 'What-if preview must not change the saved Workshop state');
  assert.deepEqual(w30ChaptersBeforeAdoption, documents().filter(document => document.kind === 'chapter').map(document => ({ id: document.id, body_json: document.body_json })), 'What-if preview must not write chapters');
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, w30RunsBeforeAdoption, 'What-if preview must not call a provider');
  const w30PreviewRow = database.prepare('SELECT preview_json FROM workshop_adoption_previews ORDER BY rowid DESC LIMIT 1').get();
  assert(w30PreviewRow?.preview_json, 'What-if adoption preview must be durable');
  const w30Preview = JSON.parse(w30PreviewRow.preview_json);
  assert.equal(w30Preview.sessionId, w30ChildId, 'Preview must identify the what-if session');
  assert.equal(w30Preview.targets[0].documentId, w30ExistingWorld.id);
  await page.screenshot({ path: resolve(output, 'what-if-adoption-preview.png') });

  await w30Adoption.getByRole('button', { name: 'Confirm Use this version', exact: true }).click();
  await page.getByText('Version chosen. Its source and rationale are saved; writing access remains author only.', { exact: true }).waitFor();
  await waitForDatabase(() => {
    const updated = documents().find(document => document.id === w30ExistingWorld.id);
    return updated && updated.body_json !== w30ExistingWorld.body_json;
  }, 'W30 explicit what-if adoption');
  const w30AfterAdoptionDocuments = documents();
  const w30AfterParent = workshopState().state.sessions.find(session => session.id === w30ParentId);
  assert.deepEqual(w30AfterParent, w30ParentRecordBeforeFork, 'What-if adoption must not rewrite the complete parent exploration record');
  assert.deepEqual(
    w30AfterAdoptionDocuments.filter(document => document.id !== w30ExistingWorld.id),
    w30DocumentsBeforeAdoption.filter(document => document.id !== w30ExistingWorld.id),
    'What-if adoption must not rewrite non-target story documents',
  );
  assert.deepEqual(w30ChaptersBeforeAdoption, w30AfterAdoptionDocuments.filter(document => document.kind === 'chapter').map(document => ({ id: document.id, body_json: document.body_json })), 'What-if adoption must not write chapters');
  assert.equal(database.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, w30RunsBeforeAdoption, 'What-if adoption must not call a provider');
  const w30Decision = workshopState().state.decisions.find(decision => decision.sessionId === w30ChildId && decision.documentId === w30ExistingWorld.id && decision.status === 'chosen');
  assert(w30Decision && w30Decision.access === 'authorRoom', 'Explicit what-if adoption must create an author-room decision');
  await page.screenshot({ path: resolve(output, 'what-if-adoption-committed.png') });
  checks.push('W30 forks an independent what-if with its own anchor and exploration preference, preserves the parent across save/reopen and comparison, leaves story documents and providers untouched through preview, and changes only the existing world after explicit adoption while chapters remain unchanged');

  assert.deepEqual(pageErrors, [], `Native Workshop page errors: ${pageErrors.join('; ')}`);
  await writeFile(resolve(output, 'report.json'), JSON.stringify({
    status: 'passed', passed: checks.length, strictChecks: checks.length, checks, runtime, executable, dataDirectory: data,
    pageErrors, limitations: [
      'Qualification uses the built-in deterministic local mock and synthetic temporary projects only.',
      'This bounded flow does not claim live provider, physical keyboard, screen-reader, minimum-window, or multi-DPI coverage.',
      'Late, stale, and partial provider recovery are covered only where existing dedicated native harnesses provide synthetic controls.',
    ],
  }, null, 2));
  console.log(JSON.stringify({ status: 'passed', passed: checks.length, strictChecks: checks.length, checks, output }, null, 2));
} catch (error) {
  if (page && !page.isClosed()) {
    await page.screenshot({ path: resolve(output, 'failure.png') }).catch(() => {});
    appLog += `\nVisible native state:\n${await page.locator('body').innerText().catch(() => '(unavailable)')}`;
  }
  await writeFile(resolve(output, 'failure.txt'), `${error.stack ?? error}\n${appLog}`);
  await writeFile(resolve(output, 'failure.json'), JSON.stringify({
    status: 'failed', passed: checks.length, strictChecks: checks.length, checks, executable, dataDirectory: data,
    pageErrors, failure: String(error),
  }, null, 2));
  throw error;
} finally {
  database?.close();
  await browser?.close().catch(() => {});
  if (app.exitCode === null) {
    app.kill();
    await new Promise(resolvePromise => {
      const timeout = setTimeout(resolvePromise, 5_000);
      app.once('exit', () => { clearTimeout(timeout); resolvePromise(); });
    });
  }
}
