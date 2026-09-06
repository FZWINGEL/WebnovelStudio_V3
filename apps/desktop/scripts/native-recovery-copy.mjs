import assert from 'node:assert/strict';
import { readFile, realpath, writeFile } from 'node:fs/promises';
import { DatabaseSync } from 'node:sqlite';
import { isAbsolute, relative, resolve, sep, toNamespacedPath } from 'node:path';

const RECOVERY_DIALOG_TITLE = 'Save recovery copy as a new file';
const RECOVERY_TRIGGER = 'synthetic_recovery_save_failure';

const recoveryBody = {
  type: 'doc',
  content: [
    { type: 'heading', attrs: { id: 'recovery-heading', level: 2 }, content: [{ type: 'text', text: 'Unsaved chapter' }] },
    { type: 'paragraph', attrs: { id: 'recovery-prose' }, content: [
      { type: 'text', text: 'Mara’s vow 🌙', marks: [{ type: 'bold' }] },
      { type: 'hardBreak' },
      { type: 'text', text: 'Still here.' },
    ] },
    { type: 'sceneBreak', attrs: { id: 'recovery-break' } },
    { type: 'paragraph', attrs: { id: 'recovery-end' } },
  ],
};

const exactMarkdown = '## Unsaved chapter\n\n**Mara’s vow 🌙**  \nStill here.\n\n---\n\n';

function assertInside(root, child, message) {
  const relativePath = relative(toNamespacedPath(root), toNamespacedPath(child));
  assert(relativePath && !isAbsolute(relativePath) && relativePath !== '..' && !relativePath.startsWith(`..${sep}`), message);
}

function count(db, table) {
  return Number(db.prepare(`SELECT count(*) AS count FROM ${table}`).get().count);
}

function documentHead(db, documentId) {
  const row = db.prepare('SELECT working_version, body_json, body_hash FROM documents WHERE id=?').get(documentId);
  assert(row, `Recovery fixture document ${documentId} must remain in SQLite`);
  return {
    workingVersion: Number(row.working_version),
    bodyJson: String(row.body_json),
    bodyHash: String(row.body_hash),
  };
}

/**
 * Qualifies the recovery-copy boundary against a real Tauri WebView.
 * `createWritingProject` and `operateSaveDialog` are supplied by native-smoke
 * so the fixture uses the same product and PID-bound native UI helpers.
 */
export async function qualifyRecoveryCopy({ page, data, output, operateSaveDialog, createWritingProject, checks, errors = [] }) {
  const projectTitle = 'Recovery copy story';
  const documentTitle = 'The unsaved chapter';
  await createWritingProject(projectTitle, 'chapter', documentTitle, 'The durable starting point.');

  const library = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('library_snapshot'));
  const entry = library.entries.find(item => item.title === projectTitle);
  assert(entry, 'Recovery fixture project must be visible in the native library');
  const ownedRoot = await realpath(data);
  const projectPath = await realpath(entry.path);
  assertInside(ownedRoot, projectPath, 'Recovery fixture project must stay inside this synthetic run');
  const databasePath = resolve(projectPath, 'project.sqlite3');
  const db = new DatabaseSync(databasePath);
  db.exec('PRAGMA busy_timeout=5000');

  // Read the active document identity from the owned SQLite fixture. The
  // renderer's access object stays private to the mounted session.
  const documentRows = db.prepare('SELECT id FROM documents ORDER BY position LIMIT 1').all();
  assert.equal(documentRows.length, 1, 'Recovery fixture must contain one document');
  const fixtureDocumentId = String(documentRows[0].id);
  const baseline = documentHead(db, fixtureDocumentId);
  const baselineDocumentCount = count(db, 'documents');
  const baselineProviderResults = count(db, 'provider_results');
  const baselineDiscussionRuns = count(db, 'discussion_runs');
  let triggerInstalled = false;
  const manuscript = () => page.evaluate(() => document.querySelector('[aria-label="Manuscript"]').editor.getJSON());

  try {
    db.exec(`CREATE TRIGGER ${RECOVERY_TRIGGER} BEFORE UPDATE OF working_version,body_json,body_hash ON documents BEGIN SELECT RAISE(ABORT,'synthetic recovery save failure'); END;`);
    triggerInstalled = true;
    await page.evaluate(body => {
      const editor = document.querySelector('[aria-label="Manuscript"]').editor;
      editor.commands.setContent(body);
    }, recoveryBody);
    await page.waitForFunction(() => {
      const text = document.querySelector('[aria-label="Manuscript"]')?.editor?.getText() ?? '';
      return text.includes('Unsaved chapter') && text.includes('Mara’s vow 🌙') && text.includes('Still here.');
    });
    await page.getByRole('status').filter({ hasText: /Couldn't save/u }).waitFor();
    const liveBeforeCopy = await manuscript();
    try {
      assert.deepEqual(liveBeforeCopy, recoveryBody);
    } catch (error) {
      await writeFile(resolve(output, 'recovery-copy-live-json.json'), JSON.stringify({ expected: recoveryBody, actual: liveBeforeCopy }, null, 2));
      throw error;
    }
    assert.equal(documentHead(db, fixtureDocumentId).bodyJson, baseline.bodyJson, 'Save failure must leave the durable body unchanged');
    assert.equal(documentHead(db, fixtureDocumentId).bodyHash, baseline.bodyHash, 'Save failure must leave the durable hash unchanged');
    assert.equal(documentHead(db, fixtureDocumentId).workingVersion, baseline.workingVersion, 'Save failure must leave the durable version unchanged');
    assert.equal(count(db, 'documents'), baselineDocumentCount, 'Save failure must not change document count');
    await page.locator('.save-error').scrollIntoViewIfNeeded();
    await page.screenshot({ path: resolve(output, 'recovery-copy-save-error.png') });

    const destination = resolve(data, 'recovery-copy.md');
    await page.getByRole('button', { name: 'Save recovery copy…', exact: true }).click();
    await operateSaveDialog('Save', destination, false, false, true);
    await page.getByRole('status').filter({ hasText: 'Recovery copy saved:', exact: false }).waitFor();
    const copiedMarkdown = await readFile(destination, 'utf8');
    assert.equal(copiedMarkdown, exactMarkdown, 'Recovery copy must contain the exact Markdown projection of the captured buffer');
    assert.match(await page.getByRole('status').filter({ hasText: 'Recovery copy saved:', exact: false }).innerText(), /This does not save the project/u);
    assert.deepEqual(await manuscript(), liveBeforeCopy, 'Recovery copy must not replace the live editor');
    assert.deepEqual(documentHead(db, fixtureDocumentId), baseline, 'Recovery copy must not advance the saved head or body');
    await page.getByRole('status').filter({ hasText: /Couldn't save/u }).waitFor();

    const liveBeforeCancel = await manuscript();
    await page.getByRole('button', { name: 'Save recovery copy…', exact: true }).click();
    await operateSaveDialog('Cancel', '', false, false, true);
    await page.getByRole('status').filter({ hasText: 'Recovery copy cancelled', exact: false }).waitFor();
    assert.deepEqual(await manuscript(), liveBeforeCancel, 'Cancelling a recovery copy must retain the editor buffer');
    assert.deepEqual(documentHead(db, fixtureDocumentId), baseline, 'Cancelling recovery copy must not advance the saved head or body');
    await page.getByRole('status').filter({ hasText: /Couldn't save/u }).waitFor();
    assert.equal(await page.getByRole('status').filter({ hasText: 'Recovery copy cancelled', exact: false }).count(), 1);
    assert.equal(count(db, 'documents'), baselineDocumentCount, 'Save and cancel must not create another document');

    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await new Promise(resolve => setTimeout(resolve, 1000));
    assert.equal(await page.getByRole('heading', { name: documentTitle, exact: true }).count(), 1, 'Navigation must remain on the document while its save is faulted');
    assert.deepEqual(await manuscript(), liveBeforeCancel, 'Navigation failure must retain the live editor buffer');

    db.exec(`DROP TRIGGER ${RECOVERY_TRIGGER}`);
    triggerInstalled = false;
    await page.getByRole('button', { name: 'Retry save', exact: true }).click();
    await page.getByRole('status').filter({ hasText: /^Saved$/u }).waitFor();
    const liveAfterRetry = await manuscript();
    const savedAfterRetry = documentHead(db, fixtureDocumentId);
    assert.deepEqual(JSON.parse(savedAfterRetry.bodyJson), { schemaVersion: 1, body: liveAfterRetry }, 'Retry must persist the exact live editor body');
    assert.notEqual(savedAfterRetry.bodyHash, baseline.bodyHash, 'Retry must advance the durable body hash');
    assert(savedAfterRetry.workingVersion > baseline.workingVersion, 'Retry must advance the durable working version');
    assert.equal(count(db, 'documents'), baselineDocumentCount, 'Retry must preserve the document count');
    assert.equal(count(db, 'provider_results'), baselineProviderResults, 'Recovery qualification must make no provider calls');
    assert.equal(count(db, 'discussion_runs'), baselineDiscussionRuns, 'Recovery qualification must make no discussion runs');
    assert.deepEqual(errors, [], 'Recovery qualification must produce no page errors');
    checks.push('Native recovery copy captures rich click-time editor JSON after a synthetic save failure, writes exact Markdown through the owned Save dialog, supports Save/Cancel, leaves head/body/counts and Saved watermark unchanged until Retry, keeps navigation on the faulted document, then retries successfully with zero provider calls and zero page errors');
  } finally {
    if (triggerInstalled) db.exec(`DROP TRIGGER ${RECOVERY_TRIGGER}`);
    db.close();
  }
}

export { RECOVERY_DIALOG_TITLE };
