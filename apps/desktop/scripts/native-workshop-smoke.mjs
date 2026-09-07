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
  return database.prepare('SELECT id,kind,title,body_json FROM documents WHERE trashed=0 ORDER BY position').all();
}

function documentText(document) {
  const snapshot = JSON.parse(document.body_json);
  return (snapshot.body.content ?? []).map(block => (block.content ?? []).map(inline => inline.type === 'text' ? inline.text : '\n').join('')).join('\n');
}

function workshopState() {
  const row = database.prepare('SELECT version,state_json FROM workshop_state WHERE singleton=1').get();
  return row ? { version: String(row.version), state: JSON.parse(row.state_json) } : null;
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

  const firstCard = page.locator('.candidate-card').filter({ hasText: 'Local workshop direction 1' }).first();
  await firstCard.getByRole('button', { name: 'Select details', exact: true }).click();
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
  const reopenedBrief = page.locator('.workshop-brief textarea');
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
  await waitForDatabase(() => workshopState()?.state.sessions.every(session => session.selectedDetails.length === 0), 'stale selected-detail removal');
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

  // W23 voice guidance is an explicit second local-mock dispatch. It only
  // changes the saved Workshop request/result; it must not adopt the sample.
  await page.getByRole('button', { name: 'Themes & tone', exact: true }).click();
  await page.getByRole('heading', { name: 'Themes & tone', exact: true }).waitFor();
  const voiceSample = 'Rain ticked against the workshop glass while she counted each drop.';
  const voiceWorking = page.getByRole('textbox', { name: 'Develop or edit directly', exact: true });
  await voiceWorking.fill(voiceSample);
  await page.locator('.workshop-save-status').filter({ hasText: 'Saved on this computer' }).waitFor();
  await waitForDatabase(() => workshopState()?.state.sessions.some(session => session.workingText === voiceSample), 'voice sample save');
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
  await waitForDatabase(() => workshopState()?.state.sessions.some(session => session.workingText !== voiceSample), 'author voice guidance development');
  assert.notEqual(await voiceWorking.inputValue(), voiceSample, 'The sample may change only after the author develops a guidance alternative');
  assert.deepEqual(documents(), voiceDocumentsBefore, 'Developing voice guidance must remain author-only material');
  assert.equal(workshopState().state.decisions.length, voiceDecisionsBefore, 'Developing voice guidance must not create a decision automatically');
  assert.equal(database.prepare("SELECT count(*) AS n FROM workshop_receipts WHERE operation_kind='adoptWorkshop'").get().n, adoptionReceiptsBeforeVoice, 'Developing voice guidance must not adopt automatically');
  await page.screenshot({ path: resolve(output, 'voice-guidance-alternatives.png') });
  checks.push('Themes & tone sends a second explicit local-mock voice-guidance request, renders three STYLE alternatives, preserves the sample until author development, and keeps guidance out of adoption');

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
