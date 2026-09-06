// Native qualification for passage-backed promise records and frozen history.
// This helper is intentionally independent from native-smoke.mjs so its
// project and its assertions can be run from a focused native harness.
import assert from 'node:assert/strict';
import { realpath } from 'node:fs/promises';
import { isAbsolute, relative, resolve, sep, toNamespacedPath } from 'node:path';
import { DatabaseSync } from 'node:sqlite';

function projectInside(data, projectPath) {
  const child = relative(toNamespacedPath(data), toNamespacedPath(projectPath));
  assert(child && !isAbsolute(child) && child !== '..' && !child.startsWith(`..${sep}`),
    'Promise fixture must stay inside this synthetic run');
}

function bodyWithParagraphs(paragraphs) {
  return {
    type: 'doc',
    content: paragraphs.map((text, index) => ({
      type: 'paragraph',
      attrs: { id: `promise-paragraph-${index + 1}` },
      content: [{ type: 'text', text }],
    })),
  };
}

/**
 * Runs the promise review/history journeys against a real Tauri WebView.
 * `createWritingProject` is supplied by native-smoke and is deliberately the
 * only project-creation abstraction used here; all review, context, reload,
 * and duplicate actions below go through the product UI and native IPC.
 */
export async function runPromiseHistoryFlow({ page, data, output, createWritingProject, checks }) {
  const title = 'Promise history story';
  const firstTitle = 'The vow';
  const setup = 'Mei promised to return the silver key before dawn.';
  const privateSetup = 'Only Mei knew the promise was a lie.';
  const payoff = 'Ren received the silver key before dawn.';
  const thirdText = 'Ren waited beneath the bridge.';
  await createWritingProject(title, 'chapter', firstTitle, setup);

  const library = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('library_snapshot'));
  const entry = library.entries.find(item => item.title === title);
  assert(entry, 'Promise fixture project must be visible in the native library');
  const projectPath = await realpath(entry.path);
  projectInside(await realpath(data), projectPath);
  const db = new DatabaseSync(resolve(projectPath, 'project.sqlite3'), { readOnly: true });
  const latestPacket = () => db.prepare('SELECT * FROM context_packets ORDER BY rowid DESC LIMIT 1').get();
  const editorJson = () => page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());

  async function saveBody(body) {
    await page.evaluate(value => {
      const editor = document.querySelector('.tiptap').editor;
      editor.commands.setContent(value);
    }, body);
    await page.waitForFunction(expected => document.querySelector('.tiptap')?.editor?.getJSON()?.content?.length === expected, body.content.length);
    await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  }

  await saveBody(bodyWithParagraphs([setup, privateSetup]));
  const firstOriginal = await editorJson();

  async function selectBlock(quote) {
    await page.evaluate(text => {
      const editor = document.querySelector('.tiptap').editor;
      let found = null;
      editor.state.doc.descendants((node, position) => {
        if (!found && node.type.name === 'paragraph' && node.textContent === text) found = { from: position + 1, to: position + 1 + text.length };
      });
      if (!found) throw new Error(`Synthetic promise passage not found: ${text}`);
      editor.commands.setTextSelection(found);
    }, quote);
  }

  async function openReview() {
    await page.getByRole('button', { name: 'Story review', exact: true }).click();
  }

  async function reviewSaved() {
    await page.getByRole('button', { name: 'Review saved chapter', exact: true }).click();
    await page.getByLabel('Chapter under review', { exact: true }).waitFor();
  }

  async function openPromises() {
    await page.getByRole('button', { name: /^Promises \(/ }).click();
  }

  async function keepPromise({ quote, phase, timing, note, label, promiseId, reader }) {
    await selectBlock(quote);
    await page.getByRole('button', { name: 'Add promise detail', exact: true }).click();
    const form = page.getByLabel('Add promise detail', { exact: true });
    await form.getByLabel('Promise', { exact: true }).selectOption(promiseId ?? '__new__');
    if (!promiseId) await form.getByRole('textbox', { name: 'Promise name', exact: true }).fill(label);
    await form.getByLabel('What this passage records', { exact: true }).selectOption(phase);
    await form.getByRole('textbox', { name: 'Promise note', exact: true }).fill(note);
    await form.getByLabel('Promise timing', { exact: true }).selectOption(timing);
    const disclosure = form.getByRole('checkbox', { name: 'Explicitly disclosed to the reader', exact: true });
    if (reader) await disclosure.check();
    await form.getByRole('button', { name: 'Keep promise', exact: true }).click();
    await page.locator('.review-detail-card').filter({ hasText: label }).waitFor();
  }

  async function markReviewed() {
    await page.getByRole('button', { name: 'Mark this version reviewed', exact: true }).click();
    await page.getByRole('button', { name: 'Update reviewed details', exact: true }).waitFor();
    await page.getByRole('heading', { name: 'Reviewed version is current', exact: true }).waitFor();
  }

  async function back() {
    await page.getByRole('button', { name: 'Back to writing', exact: true }).click();
  }

  async function openInspector() {
    const inspector = page.locator('.context-inspector');
    if (await inspector.getAttribute('open') === null) await inspector.locator(':scope > summary').click();
    await inspector.locator(':scope > details[open] > summary').filter({ hasText: /^Used/ }).waitFor();
  }

  async function createChapter(documentTitle, text) {
    await page.getByRole('button', { name: 'Add', exact: true }).click();
    await page.getByLabel('Start with', { exact: true }).selectOption('chapter');
    await page.getByRole('textbox', { name: 'Title', exact: true }).fill(documentTitle);
    await page.getByRole('button', { name: 'Create', exact: true }).click();
    await page.getByRole('heading', { name: documentTitle, exact: true }).waitFor();
    await page.getByRole('textbox', { name: 'Manuscript', exact: true }).fill(text);
    await page.waitForFunction(expected => document.querySelector('.tiptap')?.editor?.getText() === expected, text);
    await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  }

  try {
    // First journey: two distinct reader/private observations and a lost stage
    // acknowledgement. The retry must be the same logical request and Rust
    // must retain one committed stage/bundle for its operation id.
    await openReview();
    await reviewSaved();
    await openPromises();
    await keepPromise({ quote: setup, phase: 'setup', timing: 'atPassage', label: 'Return the silver key', note: 'Mei promises to return the silver key before dawn.', reader: true });
    await keepPromise({ quote: privateSetup, phase: 'setup', timing: 'unknown', label: 'Hidden return', note: 'The private plan is known only in the author room.', reader: false });
    const setupDraft = await editorJson();
    assert.deepEqual(setupDraft, firstOriginal, 'Recording promise observations must not edit the manuscript');

    await page.evaluate(() => {
      const fetch = window.fetch;
      window.promiseFetch = fetch;
      window.promiseStageRequests = [];
      window.promiseLostAck = false;
      window.fetch = async (...args) => {
        const response = await fetch.apply(window, args);
        if (String(args[0]).endsWith('/stage_author_review')) {
          const payload = JSON.parse(args[1].body);
          const request = payload.request ?? payload;
          if (request.promises?.length) {
            window.promiseStageRequests.push(structuredClone(request));
            if (!window.promiseLostAck && response.headers.get('Tauri-Response') === 'ok') {
              window.promiseLostAck = true;
              return new Response(JSON.stringify({ code: 'UncertainOutcome', detail: 'Synthetic lost promise-stage acknowledgment after commit' }), {
                headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' },
              });
            }
          }
        }
        return response;
      };
    });
    await page.getByRole('button', { name: 'Save reviewed details', exact: true }).click();
    await page.getByRole('button', { name: 'Check review save', exact: true }).click();
    await page.getByRole('button', { name: 'Mark this version reviewed', exact: true }).waitFor();
    const retries = await page.evaluate(() => window.promiseStageRequests);
    assert.equal(retries.length, 2, 'Lost promise-stage acknowledgement must reconcile with one retry');
    const withoutAccess = request => { const value = structuredClone(request); delete value.access; return value; };
    assert.deepEqual(withoutAccess(retries[0]), withoutAccess(retries[1]), 'Promise retry must retain exact records and operation payload');
    assert.equal(db.prepare('SELECT count(*) AS n FROM review_stages WHERE operation_id=?').get(retries[0].operationId).n, 1);
    await markReviewed();
    await page.evaluate(() => { window.fetch = window.promiseFetch; });
    assert.deepEqual(await editorJson(), firstOriginal);

    const setupBundle = db.prepare('SELECT b.* FROM ready_bundles b JOIN ready_heads h ON h.bundle_id=b.id WHERE b.document_id=(SELECT id FROM documents WHERE title=?)').get(firstTitle);
    assert(setupBundle?.promises_json, 'The reviewed setup bundle must persist promise observations');
    const setupPromises = JSON.parse(setupBundle.promises_json);
    assert.equal(setupPromises.length, 2);
    const publicPromise = setupPromises.find(record => record.audience === 'reader');
    const privatePromise = setupPromises.find(record => record.audience === 'authorRoom');
    assert(publicPromise && privatePromise);

    await back();
    await createChapter('The return', payoff);
    const payoffOriginal = await editorJson();
    await openReview();
    await reviewSaved();
    await openPromises();
    await keepPromise({ quote: payoff, phase: 'payoff', timing: 'atPassage', note: 'Ren receives the key before dawn, recording the payoff.', label: publicPromise.promise.label, promiseId: publicPromise.promise.id, reader: true });
    assert.deepEqual(await editorJson(), payoffOriginal, 'Recording a payoff must not edit the second chapter');
    await page.getByRole('button', { name: 'Save reviewed details', exact: true }).click();
    await markReviewed();
    const payoffBundle = db.prepare('SELECT b.* FROM ready_bundles b JOIN ready_heads h ON h.bundle_id=b.id WHERE b.document_id=(SELECT id FROM documents WHERE title=?)').get('The return');
    assert(payoffBundle?.promises_json);
    const payoffPromises = JSON.parse(payoffBundle.promises_json);
    assert.equal(payoffPromises[0].promise.id, publicPromise.promise.id, 'Payoff must reuse the explicit promise identity');
    await back();
    await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).fill('What promise evidence supports this chapter?');
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    await page.locator('.persistent-feedback article').filter({ hasText: 'This test confirms discussion and context handling' }).waitFor();
    const discussionPacket = JSON.parse(latestPacket().packet_json);
    const packetText = JSON.stringify(discussionPacket);
    assert(packetText.includes(publicPromise.promise.id), 'Discussion packet must expose the reviewed promise identity');
    assert(packetText.includes('Return the silver key'), 'Discussion packet must expose the public promise label');
    await openInspector();
    const deliveredRows = page.locator('.context-inspector > details[open] .context-reviewed-evidence').filter({ hasText: 'Return the silver key' });
    await deliveredRows.first().waitFor();
    assert.equal(await deliveredRows.count(), 2, 'Setup and payoff must both be delivered');
    await page.locator('.context-inspector > details[open]').getByRole('button', { name: 'Find promise history for Return the silver key', exact: true }).first().click();
    const history = page.getByRole('region', { name: 'Recorded promise history', exact: true });
    await history.waitFor();
    assert.equal(await history.locator('ol > li').count(), 2);
    assert((await history.innerText()).includes('Mei promises to return the silver key before dawn.'));
    assert((await history.innerText()).includes('Ren receives the key before dawn, recording the payoff.'));
    assert((await history.innerText()).includes('Other payoffs or changes may be missing.'));
    await page.screenshot({ path: resolve(output, 'promise-history.png') });
    const beforeSource = latestPacket();
    await history.getByRole('button', { name: /Read The vow for promise observation 1/ }).click();
    await page.getByRole('region', { name: 'Saved story source', exact: true }).waitFor();
    assert.deepEqual(latestPacket(), beforeSource, 'Opening promise evidence must not create a model packet');
    checks.push('Native promise review records reader and author-room observations, retries a lost stage acknowledgment with one exact immutable set, reuses an explicit identity for a payoff, and shows complete evidence history without a model call');

    // Second journey: restricted writing receives the public promise only.
    await page.getByRole('button', { name: 'Close source', exact: true }).click();
    await page.getByRole('button', { name: 'Close promise history', exact: true }).click();
    await createChapter('After the bridge', thirdText);
    await page.getByRole('button', { name: 'Continue chapter', exact: true }).click();
    await page.getByLabel('Story basis', { exact: true }).selectOption('reviewed');
    await page.getByRole('textbox', { name: 'What should happen next?', exact: true }).fill('Continue from reviewed, reader-visible promise evidence only.');
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    await page.locator('.proposal-card').filter({ hasText: 'Local test continuation' }).waitFor();
    const restrictedPacketObject = JSON.parse(latestPacket().packet_json);
    const restrictedPacket = JSON.stringify(restrictedPacketObject);
    const restrictedEnvelope = JSON.parse(restrictedPacketObject.messages[1].content);
    const deliveredPromises = restrictedEnvelope.reviewedPromises?.sets?.flatMap(set => set.records) ?? [];
    assert(deliveredPromises.some(record => record.promise.id === publicPromise.promise.id), 'Restricted continuation must deliver the reader-visible promise');
    assert(!deliveredPromises.some(record => record.promise.id === privatePromise.promise.id), 'Restricted continuation must omit private promise identity from the reviewed envelope');
    assert(!restrictedPacket.includes(privatePromise.promise.id), 'Restricted continuation must omit private promise identity');
    assert(!restrictedPacket.includes(privatePromise.note), 'Restricted continuation must omit private promise note');
    await openInspector();
    const usedText = await page.locator('.context-inspector').innerText();
    assert(usedText.includes('Return the silver key'));
    assert(!usedText.includes('Hidden return'));
    await page.screenshot({ path: resolve(output, 'promise-restricted-context.png') });
    const candidate = page.locator('.proposal-card').filter({ hasText: 'Local test continuation' });
    await candidate.getByRole('button', { name: 'Reject', exact: true }).click();
    await candidate.getByText('Rejected', { exact: true }).waitFor();

    // A record-only re-review advances the review fence while preserving every
    // saved manuscript and already committed bundle. An explicit empty promise
    // selection is stored as a nullable empty pair; older bundles remain history.
    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
    await page.getByRole('button', { name: new RegExp(`${title} Last opened`) }).click();
    await page.locator('.document-sidebar nav button').filter({ hasText: new RegExp(`^${firstTitle}`) }).click();
    await page.getByRole('heading', { name: firstTitle, exact: true }).waitFor();
    await openReview();
    await page.getByRole('button', { name: 'Update reviewed details', exact: true }).click();
    const beforeClearEpoch = db.prepare('SELECT context_source_epoch AS epoch FROM project').get().epoch;
    const beforeClearFences = db.prepare('SELECT count(*) AS n FROM review_fences WHERE affected_bundle_id=?').get(payoffBundle.id).n;
    await openPromises();
    for (const label of ['Return the silver key', 'Hidden return']) await page.locator('.review-detail-card').filter({ hasText: label }).getByRole('button', { name: 'Remove promise', exact: true }).click();
    await page.getByRole('button', { name: 'Save reviewed details', exact: true }).click();
    await markReviewed();
    await back();
    assert.deepEqual(await editorJson(), firstOriginal);
    const cleared = db.prepare('SELECT b.promises_json,b.promises_hash FROM ready_bundles b JOIN ready_heads h ON h.bundle_id=b.id WHERE b.document_id=(SELECT id FROM documents WHERE title=?)').get(firstTitle);
    assert.equal(cleared.promises_json, null, 'Explicit promise clearing canonicalizes to an empty nullable set');
    assert.equal(cleared.promises_hash, null, 'Explicit promise clearing canonicalizes to an empty nullable hash');
    assert(db.prepare('SELECT context_source_epoch AS epoch FROM project').get().epoch > beforeClearEpoch);
    assert(db.prepare('SELECT count(*) AS n FROM review_fences WHERE affected_bundle_id=?').get(payoffBundle.id).n > beforeClearFences);
    checks.push('Restricted reviewed continuation excludes author-room promise identity and note from its reviewed envelope; a record-only re-review fences later work, explicit clearing survives reload, and prior promise history remains immutable');

    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
    await page.reload();
    await page.getByRole('button', { name: new RegExp(`${title} Last opened`) }).click();
    await page.locator('.document-sidebar nav button').filter({ hasText: new RegExp(`^${firstTitle}`) }).click();
    await page.getByRole('heading', { name: firstTitle, exact: true }).waitFor();
    assert.deepEqual(await editorJson(), firstOriginal);
    await page.getByRole('button', { name: 'Story review', exact: true }).click();
    await openPromises();
    await page.getByText('No promises recorded for this review. Adding one is optional.', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Back to writing', exact: true }).click();
    await page.getByRole('button', { name: 'Duplicate', exact: true }).click();
    await page.locator('.trial-label').filter({ hasText: `${title} copy` }).waitFor();
    const copiedLibrary = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('library_snapshot'));
    const copiedEntry = copiedLibrary.entries.find(item => item.title === `${title} copy`);
    assert(copiedEntry);
    const copiedPath = await realpath(copiedEntry.path);
    projectInside(await realpath(data), copiedPath);
    const copiedDb = new DatabaseSync(resolve(copiedPath, 'project.sqlite3'), { readOnly: true });
    try {
      assert.equal(copiedDb.prepare('SELECT count(*) AS n FROM ready_heads').get().n, 0, 'Copied promise history must not gain copied authority');
      assert(copiedDb.prepare('SELECT count(*) AS n FROM ready_bundles WHERE promises_json IS NOT NULL').get().n > 0, 'Copied history must retain immutable promise bundles');
      assert.notEqual(copiedDb.prepare('SELECT operation_namespace FROM project').get().operation_namespace, db.prepare('SELECT operation_namespace FROM project').get().operation_namespace);
    } finally { copiedDb.close(); }
    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  } finally {
    db.close();
  }
}
