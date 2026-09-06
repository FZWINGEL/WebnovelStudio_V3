// Visible author actions and exact receipts in the owned synthetic native run.
import assert from 'node:assert/strict';
import { realpath } from 'node:fs/promises';
import { isAbsolute, relative, resolve, sep, toNamespacedPath } from 'node:path';
import { DatabaseSync } from 'node:sqlite';

export async function qualifyReviewedEvidence({ page, data, output, createWritingProject, checks }) {
  const keyPassage = 'Mei passed the silver key to Ren.';
  const letterPassage = 'Mei kept the sealed letter inside her coat.';
  const prose = `${keyPassage} ${letterPassage}`;
  await createWritingProject('Reviewed evidence story', 'chapter', 'The exchange', prose);
  const original = await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());
  const library = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('library_snapshot'));
  const projectPath = await realpath(library.entries.find(entry => entry.title === 'Reviewed evidence story').path);
  const child = relative(toNamespacedPath(await realpath(data)), toNamespacedPath(projectPath));
  assert(child && !isAbsolute(child) && child !== '..' && !child.startsWith(`..${sep}`), 'Evidence fixture must stay inside this synthetic run');
  const db = new DatabaseSync(resolve(projectPath, 'project.sqlite3'));
  const latestPacket = () => db.prepare('SELECT * FROM context_packets ORDER BY rowid DESC LIMIT 1').get();
  const select = async quote => page.evaluate(({ prose, quote }) => {
    const offset = prose.indexOf(quote);
    if (offset < 0) throw new Error('Unknown synthetic quotation');
    document.querySelector('.tiptap').editor.commands.setTextSelection({ from: offset + 1, to: offset + 1 + quote.length });
  }, { prose, quote });
  const mark = async () => {
    await page.getByRole('button', { name: 'Mark this version reviewed', exact: true }).click();
    await page.getByRole('button', { name: 'Update reviewed details', exact: true }).waitFor();
    await page.getByRole('heading', { name: 'Reviewed version is current', exact: true }).waitFor();
  };
  const back = async () => page.getByRole('button', { name: 'Back to writing', exact: true }).click();
  const inspector = async () => {
    if (await page.locator('.context-inspector').getAttribute('open') === null) await page.locator('.context-inspector>summary').click();
    await page.locator('.context-inspector > details[open] > summary').filter({ hasText: /^Used/ }).waitFor();
  };
  try {
    await page.getByRole('button', { name: 'Story review', exact: true }).click();
    await page.getByRole('button', { name: 'Review saved chapter', exact: true }).click();
    await page.getByLabel('Chapter under review', { exact: true }).getByText(prose, { exact: true }).waitFor();

    // The form helper uses only labelled visible controls. Selection is made in
    // the existing mounted editor; accepting a detail must not edit its body.
    async function addDetail(quote, object, holder, reader) {
      await select(quote);
      await page.getByRole('button', { name: 'Add possession detail', exact: true }).click();
      const form = page.getByLabel('Add possession detail', { exact: true });
      await form.getByLabel('Object', { exact: true }).selectOption('__new_object__');
      await form.getByRole('textbox', { name: 'New object name', exact: true }).fill(object);
      await form.getByLabel('Holder', { exact: true }).selectOption('__new_holder__');
      await form.getByRole('textbox', { name: 'New holder name', exact: true }).fill(holder);
      await form.getByLabel('Timing', { exact: true }).selectOption('atPassage');
      assert.equal(await form.getByRole('checkbox', { name: 'Explicitly disclosed to the reader', exact: true }).isChecked(), false);
      if (reader) await form.getByRole('checkbox', { name: 'Explicitly disclosed to the reader', exact: true }).check();
      await form.getByRole('button', { name: 'Keep detail', exact: true }).click();
      await page.locator('.review-detail-card').filter({ hasText: object }).waitFor();
    }
    await addDetail(letterPassage, 'Sealed letter', 'Mei', false);
    await addDetail(keyPassage, 'Silver key', 'Ren', true);
    await page.screenshot({ path: resolve(output, 'reviewed-evidence-draft.png') });
    await page.evaluate(() => {
      const fetch = window.fetch;
      window.evidenceFetch = fetch; window.evidenceStageRequests = [];
      window.evidenceLostAck = false;
      window.fetch = async (...args) => {
        const response = await fetch.apply(window, args);
        if (String(args[0]).endsWith('/stage_author_review')) {
          const payload = JSON.parse(args[1].body);
          const request = payload.request ?? payload;
          if (request.records?.length) {
            window.evidenceStageRequests.push(structuredClone(request));
            if (!window.evidenceLostAck && response.headers.get('Tauri-Response') === 'ok') {
              window.evidenceLostAck = true;
              return new Response(JSON.stringify({ code: 'UncertainOutcome', detail: 'Synthetic lost evidence-stage acknowledgment after commit' }),
                { headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' } });
            }
          }
        }
        return response;
      };
    });
    await page.getByRole('button', { name: 'Save reviewed details', exact: true }).click();
    await page.getByRole('button', { name: 'Check review save', exact: true }).click();
    await page.getByRole('button', { name: 'Mark this version reviewed', exact: true }).waitFor();
    const retries = await page.evaluate(() => window.evidenceStageRequests);
    assert.equal(retries.length, 2);
    const logical = request => { const value = structuredClone(request); delete value.access; return value; };
    assert.deepEqual(logical(retries[0]), logical(retries[1]), 'Retry must retain operation, full record array, IDs and evidence');
    assert.equal(db.prepare('SELECT count(*) AS n FROM review_stages WHERE operation_id=?').get(retries[0].operationId).n, 1);
    await mark();
    await page.evaluate(() => { window.fetch = window.evidenceFetch; });
    assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), original);
    const firstBundle = db.prepare('SELECT b.* FROM ready_bundles b JOIN ready_heads h ON h.bundle_id=b.id').get();
    const originalRecords = JSON.parse(firstBundle.records_json);
    assert.equal(originalRecords.length, 2);
    assert.equal(new Set(originalRecords.map(record => record.id)).size, 2);
    await back();
    await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).fill('Which possession details are supported by this scene?');
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    await page.locator('.persistent-feedback article').filter({ hasText: 'This test confirms discussion and context handling' }).waitFor();
    await inspector();
    await page.locator('.context-inspector > details[open] .context-reviewed-evidence').filter({ hasText: 'Silver key' }).waitFor();
    await page.locator('.context-inspector > details[open] .context-reviewed-evidence').filter({ hasText: 'Sealed letter' }).waitFor();
    const discussionRow = latestPacket();
    const discussion = JSON.parse(discussionRow.packet_json);
    assert.deepEqual(discussion.receipt.reviewedEvidence.flatMap(set => set.recordIds).sort(), originalRecords.map(record => record.id).sort());
    await page.locator('.context-inspector > details[open] .context-reviewed-evidence').filter({ hasText: 'Silver key' }).scrollIntoViewIfNeeded();
    await page.screenshot({ path: resolve(output, 'reviewed-evidence-discussion.png') });
    checks.push('Native chapter review adds passage-backed possession details with explicit reader disclosure, reconciles a lost stage acknowledgment with one exact complete record set, preserves prose, and shows delivered reviewed evidence separately in discussion context');

    await page.getByRole('button', { name: 'Add', exact: true }).click();
    await page.getByLabel('Start with', { exact: true }).selectOption('chapter');
    await page.getByRole('textbox', { name: 'Title', exact: true }).fill('At the archway');
    await page.getByRole('button', { name: 'Create', exact: true }).click();
    await page.getByRole('heading', { name: 'At the archway', exact: true }).waitFor();
    await page.getByRole('textbox', { name: 'Manuscript', exact: true }).fill('Ren left the key beneath the archway.');
    await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
    await page.getByRole('button', { name: 'Continue chapter', exact: true }).click();
    await page.getByLabel('Story basis', { exact: true }).selectOption('reviewed');
    await page.getByRole('textbox', { name: 'What should happen next?', exact: true }).fill('Continue quietly from the reviewed exchange.');
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    const candidate = page.locator('.proposal-card').filter({ hasText: 'Local test continuation' });
    await candidate.waitFor();
    await inspector();
    const continuationRow = latestPacket();
    const continuation = JSON.parse(continuationRow.packet_json);
    const readerRecord = originalRecords.find(record => record.audience === 'reader');
    const privateRecord = originalRecords.find(record => record.audience === 'authorRoom');
    assert.deepEqual(continuation.receipt.reviewedEvidence.flatMap(set => set.recordIds), [readerRecord.id]);
    // The exact chapter prose is public here; the private *record* identity and
    // label must still remain outside the restricted evidence projection.
    assert(!JSON.stringify(continuation.messages).includes(privateRecord.id));
    assert(!JSON.stringify(continuation.messages).includes(privateRecord.object.label));
    const restrictedEvidence = page.locator('.context-inspector > details[open] .context-reviewed-evidence');
    await restrictedEvidence.filter({ hasText: 'Silver key' }).scrollIntoViewIfNeeded();
    assert.equal(await restrictedEvidence.filter({ hasText: 'Sealed letter' }).count(), 0);
    await page.screenshot({ path: resolve(output, 'reviewed-evidence-restricted.png') });
    await candidate.getByRole('button', { name: 'Reject', exact: true }).click();
    await candidate.getByText('Rejected', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Story review', exact: true }).click();
    await page.getByRole('button', { name: 'Review saved chapter', exact: true }).click();
    await page.evaluate(() => {
      const editor = document.querySelector('.tiptap').editor;
      editor.commands.setTextSelection({ from: 1, to: 1 + 'Ren left the key beneath the archway.'.length });
    });
    await page.getByRole('button', { name: 'Add possession detail', exact: true }).click();
    await page.getByLabel('Object', { exact: true }).selectOption(readerRecord.object.id);
    assert.equal(await page.getByLabel('Object', { exact: true }).locator('option:checked').innerText(), 'Silver key · The exchange');
    await page.getByLabel('Timing', { exact: true }).selectOption('atPassage');
    await page.getByLabel('Explicitly disclosed to the reader', { exact: true }).check();
    await page.getByRole('button', { name: 'Keep detail', exact: true }).click();
    await page.getByRole('button', { name: 'Save reviewed details', exact: true }).click();
    await mark(); await back();
    await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).fill('What do the saved observations establish about the key?');
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    await page.locator('.persistent-feedback article').filter({ hasText: 'This test confirms discussion and context handling' }).waitFor();
    await inspector();
    await page.locator('.context-inspector > details[open]').getByRole('button', { name: 'Find recorded history for Silver key', exact: true }).first().click();
    const history = page.getByRole('region', { name: 'Recorded object history', exact: true });
    await history.waitFor();
    assert.equal(await history.locator('ol > li').count(), 2);
    assert.deepEqual(await history.locator('blockquote').allTextContents(), [keyPassage, 'Ren left the key beneath the archway.']);
    assert((await history.innerText()).includes('Holder unknown'));
    assert((await history.innerText()).includes('does not establish the current holder'));
    await history.scrollIntoViewIfNeeded();
    await page.screenshot({ path: resolve(output, 'evidence-history.png') });
    const historyPacket = latestPacket();
    await history.getByRole('button', { name: 'Read The exchange for observation 1', exact: true }).click();
    await page.getByRole('region', { name: 'Saved story source', exact: true }).waitFor();
    assert.deepEqual(latestPacket(), historyPacket, 'History/source inspection must not prepare another packet');
    checks.push('Native cross-chapter object selection reuses an explicit identity, preserves unknown holders, shows exact saved evidence in chapter order, and opens the original source without another model request');

    await page.locator('.document-sidebar nav button').filter({ hasText: /^The exchange/ }).click();
    await page.getByRole('heading', { name: 'The exchange', exact: true }).waitFor();
    await page.getByRole('button', { name: 'Story review', exact: true }).click();
    await page.getByRole('button', { name: 'Update reviewed details', exact: true }).click();
    const letter = page.locator('.review-detail-card').filter({ hasText: 'Sealed letter' });
    await letter.getByRole('button', { name: 'Edit', exact: true }).click();
    const editForm = page.getByLabel('Edit possession detail', { exact: true });
    await editForm.getByRole('checkbox', { name: 'Explicitly disclosed to the reader', exact: true }).check();
    await editForm.getByRole('button', { name: 'Keep detail', exact: true }).click();
    await page.getByRole('button', { name: 'Save reviewed details', exact: true }).click();
    await mark();
    assert.deepEqual(db.prepare('SELECT * FROM ready_bundles WHERE id=?').get(firstBundle.id), firstBundle);
    assert.deepEqual(db.prepare('SELECT * FROM context_packets WHERE id=?').get(discussionRow.id), discussionRow);
    assert.deepEqual(db.prepare('SELECT * FROM context_packets WHERE id=?').get(continuationRow.id), continuationRow);
    assert(db.prepare('SELECT count(*) AS n FROM review_fences').get().n > 0);
    assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), original);
    await back();
    await page.locator('.document-sidebar nav button').filter({ hasText: /^At the archway/ }).click();
    await page.getByRole('button', { name: 'Story review', exact: true }).click();
    await page.getByRole('heading', { name: 'Earlier story needs review', exact: true }).waitFor();
    assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), 'Ren left the key beneath the archway.');
    await back();
    await page.locator('.document-sidebar nav button').filter({ hasText: /^The exchange/ }).click();
    await page.getByRole('button', { name: 'Story review', exact: true }).click();
    await page.getByRole('button', { name: 'Update reviewed details', exact: true }).click();
    for (const label of ['Sealed letter', 'Silver key']) await page.locator('.review-detail-card').filter({ hasText: label }).getByRole('button', { name: 'Remove', exact: true }).click();
    await page.getByRole('button', { name: 'Save reviewed details', exact: true }).click();
    await mark(); await back();
    const selected = db.prepare('SELECT b.* FROM ready_bundles b JOIN ready_heads h ON h.bundle_id=b.id WHERE b.document_id=?').get(firstBundle.document_id);
    assert.equal(selected.records_json, null);
    assert.equal(selected.records_hash, null);
    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
    await page.reload();
    await page.getByRole('button', { name: /^Reviewed evidence story Last opened/ }).click();
    assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), prose);
    await page.getByRole('button', { name: 'Story review', exact: true }).click();
    await page.getByText('No reviewed story details are recorded yet.', { exact: true }).waitFor();
    assert.deepEqual(db.prepare('SELECT * FROM ready_bundles WHERE id=?').get(firstBundle.id), firstBundle);
    assert.deepEqual(db.prepare('SELECT * FROM context_packets WHERE id=?').get(continuationRow.id), continuationRow);
    await page.screenshot({ path: resolve(output, 'reviewed-evidence-cleared.png') });
    await back();
    await page.getByRole('button', { name: 'Duplicate', exact: true }).click();
    await page.locator('.trial-label').filter({ hasText: 'Reviewed evidence story copy' }).waitFor();
    const copiedLibrary = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('library_snapshot'));
    const copyPath = await realpath(copiedLibrary.entries.find(entry => entry.title === 'Reviewed evidence story copy').path);
    const copiedChild = relative(toNamespacedPath(await realpath(data)), toNamespacedPath(copyPath));
    assert(copiedChild && !isAbsolute(copiedChild) && copiedChild !== '..' && !copiedChild.startsWith(`..${sep}`));
    const copiedDb = new DatabaseSync(resolve(copyPath, 'project.sqlite3'));
    try {
      assert.equal(copiedDb.prepare('SELECT count(*) AS n FROM ready_heads').get().n, 0);
      assert.deepEqual(copiedDb.prepare('SELECT * FROM ready_bundles WHERE id=?').get(firstBundle.id), firstBundle);
      assert.deepEqual(copiedDb.prepare('SELECT * FROM context_packets WHERE id=?').get(continuationRow.id), continuationRow);
      assert.notEqual(copiedDb.prepare('SELECT operation_namespace FROM project').get().operation_namespace, firstBundle.operation_namespace);
    } finally { copiedDb.close(); }
    await page.getByRole('button', { name: 'Story review', exact: true }).click();
    await page.getByRole('heading', { name: 'Not reviewed yet', exact: true }).waitFor();
    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
    checks.push('Native reviewed continuation supplies only reader-approved records from a mixed bundle; a detail-only review change fences later review without changing prose, and explicit clearing/reopen/independent copy preserve old bundles and frozen packet bytes without copied authority');
  } finally { db.close(); }
}
