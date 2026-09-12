import { recordCheck } from './native-evidence.mjs';
// Runs inside the owned native WebView2 qualification, against synthetic data.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, realpath } from 'node:fs/promises';
import { isAbsolute, relative, resolve, sep, toNamespacedPath } from 'node:path';
import { DatabaseSync } from 'node:sqlite';

export async function qualifyReviewedExport({ page, data, output, operateSaveDialog, createWritingProject, checks }) {
  const prose = 'Mei left the silver key beside the gate.';
  await createWritingProject('Reviewed export story', 'chapter', 'A reviewed promise', prose);
  await page.getByRole('button', { name: 'Story review', exact: true }).click();
  await page.getByRole('button', { name: 'Review saved chapter', exact: true }).click();
  await page.getByLabel('Chapter under review', { exact: true }).getByText(prose, { exact: true }).waitFor();
  await page.getByRole('button', { name: 'Mark this version reviewed', exact: true }).click();
  await page.getByRole('heading', { name: 'Reviewed version is current', exact: true }).waitFor();
  await page.getByRole('button', { name: 'Back to writing', exact: true }).click();
  const source = await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());
  const library = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('library_snapshot'));
  const path = await realpath(library.entries.find(entry => entry.title === 'Reviewed export story').path);
  const child = relative(toNamespacedPath(await realpath(data)), toNamespacedPath(path));
  assert(child && !isAbsolute(child) && child !== '..' && !child.startsWith(`..${sep}`), 'Export fixture must stay inside this synthetic run');
  const db = new DatabaseSync(resolve(path, 'project.sqlite3'));
  const count = table => db.prepare(`SELECT count(*) AS count FROM ${table}`).get().count;
  const capturePrepare = async () => page.evaluate(() => {
    const fetch = window.fetch;
    window.reviewedExportFetch = fetch;
    window.fetch = async (...args) => {
      const response = await fetch.apply(window, args);
      if (String(args[0]).endsWith('/prepare_reviewed_draft_export') && response.headers.get('Tauri-Response') === 'ok') {
        window.reviewedExportRequest = JSON.parse(args[1].body);
        window.reviewedExportPreview = await response.clone().json();
      }
      return response;
    };
  });
  const openPreview = async (format = 'markdown') => {
    if (await page.locator('.project-tools:not(.recent-project-picker)').getAttribute('open') === null) await page.locator('.project-tools:not(.recent-project-picker) > summary').click();
    await page.getByRole('button', { name: 'Export draft', exact: true }).click();
    const initial = page.getByRole('dialog', { name: 'Export draft', exact: true });
    await page.waitForFunction(() => !!document.querySelector('.export-preview') && document.querySelector('.export-actions .primary-button')?.disabled === false);
    const revisionCount = count('revisions');
    await page.evaluate(() => { window.reviewedExportPreview = null; });
    await initial.getByRole('radio', { name: 'Author-reviewed snapshot', exact: true }).check();
    const dialog = page.getByRole('dialog', { name: 'Export author-reviewed chapter', exact: true });
    await page.waitForFunction(() => !!window.reviewedExportPreview?.reviewBundleId && document.querySelector('.export-actions .primary-button')?.disabled === false);
    if (format !== 'markdown') {
      await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption(format);
      await page.waitForFunction(format => window.reviewedExportPreview?.format === format, format);
      await page.waitForFunction(() => document.querySelector('.export-actions .primary-button')?.disabled === false);
    }
    await page.waitForFunction(() => !!window.reviewedExportPreview?.reviewBundleId);
    assert.equal(count('revisions'), revisionCount, 'Reviewed preparation must not create a checkpoint');
    return dialog;
  };
  try {
    await capturePrepare();
    const exports = [];
    for (const [format, extension] of [['markdown', 'md'], ['plainText', 'txt']]) {
      const dialog = await openPreview(format);
      const preview = await page.evaluate(() => window.reviewedExportPreview);
      const text = await dialog.getByLabel('Exported file preview', { exact: true }).textContent();
      const destination = resolve(data, `author-reviewed.${extension}`);
      await page.screenshot({ path: resolve(output, `reviewed-export-${extension}.png`) });
      await dialog.getByRole('button', { name: 'Choose destination…', exact: true }).click();
      await operateSaveDialog('Save', destination, true);
      await dialog.getByRole('button', { name: 'Done', exact: true }).waitFor();
      const bytes = await readFile(destination);
      assert.equal(bytes.toString('utf8'), text);
      assert.equal(bytes.length, preview.utf8Bytes);
      assert.equal(createHash('sha256').update(bytes).digest('hex'), preview.sha256);
      const record = db.prepare('SELECT * FROM export_records WHERE id=?').get(preview.id);
      assert.equal(record.working_draft, 0);
      assert.equal(record.review_bundle_id, preview.reviewBundleId);
      assert.equal(record.revision_id, preview.revisionId);
      exports.push(record);
      await dialog.getByRole('button', { name: 'Done', exact: true }).click();
      assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), source);
      assert.equal(await page.evaluate(() => document.activeElement.textContent), 'Export draft');
    }
    // Exercise the file/receipt boundary without pretending SQLite can undo a file.
    const failedDialog = await openPreview();
    const failedPreview = await page.evaluate(() => window.reviewedExportPreview);
    const failedDestination = resolve(data, 'reviewed-file-without-record.md');
    const recordsBefore = count('export_records');
    db.exec("CREATE TRIGGER native_reviewed_export_record_failure BEFORE INSERT ON export_records BEGIN SELECT RAISE(ABORT,'synthetic export record fault'); END;");
    await failedDialog.getByRole('button', { name: 'Choose destination…', exact: true }).click();
    await operateSaveDialog('Save', failedDestination, true);
    await failedDialog.getByRole('alert').filter({ hasText: 'durable record is unavailable' }).waitFor();
    assert.equal(await readFile(failedDestination, 'utf8'), failedPreview.previewText);
    assert.equal(count('export_records'), recordsBefore);
    assert(await failedDialog.getByRole('button', { name: 'Choose destination…', exact: true }).isDisabled());
    db.exec('DROP TRIGGER native_reviewed_export_record_failure;');
    await page.screenshot({ path: resolve(output, 'reviewed-export-record-failure.png') });
    await failedDialog.getByRole('button', { name: 'Close export', exact: true }).click();
    await page.evaluate(() => { window.fetch = window.reviewedExportFetch; });

    // A duplicate retains verifiable old records but cannot adopt review authority.
    if (await page.locator('.project-tools:not(.recent-project-picker)').getAttribute('open') === null) await page.locator('.project-tools:not(.recent-project-picker) > summary').click();
    await page.getByRole('button', { name: 'Duplicate', exact: true }).click();
    await page.locator('.brand > strong').filter({ hasText: 'Reviewed export story copy' }).waitFor();
    if (await page.locator('.project-tools:not(.recent-project-picker)').getAttribute('open') === null) await page.locator('.project-tools:not(.recent-project-picker) > summary').click();
    await page.getByRole('button', { name: 'Export draft', exact: true }).click();
    await page.getByRole('radio', { name: 'Author-reviewed snapshot', exact: true }).check();
    const copiedDialog = page.getByRole('dialog', { name: 'Export author-reviewed chapter', exact: true });
    await copiedDialog.getByRole('alert').waitFor();
    assert(await copiedDialog.getByRole('button', { name: 'Choose destination…', exact: true }).isDisabled());
    await copiedDialog.getByRole('button', { name: 'Close export', exact: true }).click();
    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
    await page.reload();
    await page.getByRole('button', { name: /^Reviewed export story Last opened/ }).click();
    assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), prose);
    for (const record of exports) assert.deepEqual(db.prepare('SELECT * FROM export_records WHERE id=?').get(record.id), record);
    recordCheck(checks, 'native-reviewed-export:01', 'Native reviewed export marks a first chapter explicitly, previews and saves exact Markdown/TXT with bound review records and no reviewed checkpoint, reopens history, refuses copied authority, and preserves an installed file when recording fails');

    await capturePrepare();
    const staleDialog = await openPreview();
    await staleDialog.getByRole('button', { name: 'Choose destination…', exact: true }).click();
    await operateSaveDialog('Wait', '', true);
    // Real background IPC mutation after the verified native dialog opens.
    const saved = await page.evaluate(async source => {
      const { access, expected } = window.reviewedExportRequest;
      const body = { schemaVersion: 1, body: structuredClone(source) };
      body.body.content[0].content = [{ type: 'text', text: 'The key is gone before the author selects a destination.' }];
      return window.__TAURI_INTERNALS__.invoke('save_snapshot', { request: {
        access, expected, operationId: 'native-export-stale-while-dialog-open', localGeneration: '99', cause: 'typing', body,
      } });
    }, source);
    assert.notEqual(saved.head.bodyHash, exports[0].source_body_hash);
    const staleDestination = resolve(data, 'stale-reviewed-export.md');
    await operateSaveDialog('Save', staleDestination, true, true);
    await staleDialog.getByRole('alert').waitFor();
    assert(await staleDialog.getByRole('button', { name: 'Choose destination…', exact: true }).isDisabled());
    assert(!(await staleDialog.innerText()).includes('may already exist'));
    await assert.rejects(readFile(staleDestination), { code: 'ENOENT' });
    assert.equal(count('export_records'), recordsBefore);
    await page.screenshot({ path: resolve(output, 'reviewed-export-stale.png') });
    await staleDialog.getByRole('button', { name: 'Close export', exact: true }).click();
    await page.evaluate(() => { window.fetch = window.reviewedExportFetch; });
    // Reload reconciles the synthetic background writer before UI navigation.
    await page.reload();
    await page.getByRole('button', { name: /^Reviewed export story Last opened/ }).click();
    assert.equal(await page.getByRole('textbox', { name: 'Manuscript', exact: true }).innerText(), 'The key is gone before the author selects a destination.');
    for (const record of exports) assert.deepEqual(db.prepare('SELECT * FROM export_records WHERE id=?').get(record.id), record);
    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
    recordCheck(checks, 'native-reviewed-export:02', 'Native reviewed export rechecks after the real Save dialog opens, refuses intervening prose edits before file creation, and retains historical export records after reopening the changed chapter');
  } finally {
    db.exec('DROP TRIGGER IF EXISTS native_reviewed_export_record_failure;');
    db.close();
  }
}
