// Bounded native project-chat failure qualification.
//
// The caller owns the Tauri process and an already-open synthetic project. This
// module only exercises renderer acknowledgement loss and local materialization
// recovery on that page; it never starts an app or contacts a live provider.
import assert from 'node:assert/strict';
import { DatabaseSync } from 'node:sqlite';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { recordCheck } from './native-evidence.mjs';

function databaseAt(path) {
  const database = new DatabaseSync(path);
  database.exec('PRAGMA busy_timeout=5000');
  return database;
}

function runRows(database) {
  return database.prepare(
    'SELECT id,operation_id,status,dispatch_state FROM discussion_runs ORDER BY rowid',
  ).all();
}

function draftRows(database) {
  return database.prepare(
    'SELECT document_id,origin_run_id,output_ordinal FROM assistant_drafts ORDER BY rowid',
  ).all();
}

async function waitUntil(predicate, label, timeout = 30_000) {
  const deadline = Date.now() + timeout;
  let lastError;
  while (Date.now() < deadline) {
    try {
      if (await predicate()) return;
    } catch (error) {
      lastError = error;
    }
    await new Promise(resolvePromise => setTimeout(resolvePromise, 80));
  }
  throw new Error(`Timed out: ${label}${lastError ? ` (${lastError.message ?? lastError})` : ''}`);
}

async function waitForVisibleReconcile(page) {
  await page.getByRole('button', { name: 'Reconcile', exact: true }).waitFor({ timeout: 30_000 });
}

async function waitForReconcileToSettle(page) {
  await page.waitForFunction(() => ![...document.querySelectorAll('button')]
    .some(button => button.textContent?.trim() === 'Reconcile'), null, { timeout: 30_000 });
}

async function dropAckOnce(page, command, marker) {
  await page.evaluate(({ command: targetCommand, marker: targetMarker }) => {
    window[targetMarker] = false;
    delete window[`${targetMarker}Payload`];
    const originalFetch = window.fetch;
    window.fetch = async (...args) => {
      const response = await originalFetch.apply(window, args);
      if (window[targetMarker] || !String(args[0]).endsWith(`/${targetCommand}`)
          || response.headers.get('Tauri-Response') !== 'ok') {
        return response;
      }
      window[targetMarker] = true;
      // Capture proof that this is the response for the request which the
      // native side already accepted, then reject only the renderer promise.
      // The next fetch is restored immediately so Reconcile can read it.
      window[`${targetMarker}Payload`] = {
        request: JSON.parse(args[1].body).request,
        response: await response.clone().json(),
      };
      window.fetch = originalFetch;
      return new Response(JSON.stringify({ code: 'OutcomeUncertain', detail: 'Synthetic lost native acknowledgment.' }),
        { headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' } });
    };
  }, { command, marker });
}

async function waitForDroppedAck(page, marker) {
  await page.waitForFunction(name => !!window[`${name}Payload`], marker, { timeout: 30_000 });
}

function installMaterializationFailure(database) {
  database.exec(`
    DROP TRIGGER IF EXISTS native_chat_materialization_failure;
    CREATE TRIGGER native_chat_materialization_failure
    BEFORE INSERT ON assistant_drafts
    BEGIN
      SELECT RAISE(ABORT, 'synthetic native chat materialization failure');
    END
  `);
}

function removeMaterializationFailure(database) {
  database.exec('DROP TRIGGER IF EXISTS native_chat_materialization_failure');
}

/**
 * Qualify failure settlement on the caller's already-open native chat page.
 *
 * @param {{page: import('playwright-core').Page, projectDatabasePath: string,
 *   outputDirectory?: string, checks: string[]}} options
 */
export async function qualifyChatFailures({ page, projectDatabasePath, outputDirectory, checks }) {
  assert(page, 'A live native chat page is required.');
  assert(projectDatabasePath, 'The fenced synthetic project database path is required.');
  assert(Array.isArray(checks), 'The caller must provide the shared check collection.');

  if (outputDirectory) await mkdir(outputDirectory, { recursive: true });
  const database = databaseAt(projectDatabasePath);
  let triggerInstalled = false;
  try {
    // The smoke journey ends with selected chapter feedback. Clear that
    // captured task so this qualification intentionally exercises the root
    // project-chat command and its project-level composer.
    const returnToProject = page.getByRole('button', { name: 'Return to project conversation', exact: true });
    if (await returnToProject.count()) {
      await returnToProject.click();
      await page.getByRole('region', { name: 'Captured chapter task', exact: true }).waitFor({ state: 'detached', timeout: 30_000 });
    }
    const composer = page.getByRole('textbox', { name: 'Message the project assistant', exact: true });
    await composer.waitFor({ timeout: 30_000 });

    // A rejected renderer promise after a successful native response models a
    // lost acknowledgement. Reconciliation must observe the one accepted run,
    // rather than dispatching the request again.
    const runsBeforeLostAck = runRows(database);
    await dropAckOnce(page, 'start_project_chat', '__wnsDroppedChatAck');
    await composer.fill('A lost acknowledgement must reconcile to this accepted story-room response.');
    await waitUntil(async () => await page.getByRole('button', { name: /^Send/ }).isEnabled(), 'previous request UI settles');
    await composer.press('Control+Enter');
    await waitForDroppedAck(page, '__wnsDroppedChatAck');
    // Click while the store still exposes the uncertain request. The native
    // response has already arrived, but the renderer did not receive it.
    await waitForVisibleReconcile(page);
    await page.getByRole('button', { name: 'Reconcile', exact: true }).click();
    await waitUntil(() => runRows(database).length === runsBeforeLostAck.length + 1,
      'accepted project-chat run after the dropped acknowledgement');
    await waitUntil(() => runRows(database).at(-1)?.status === 'completed',
      'accepted project-chat run completion after the dropped acknowledgement');
    const acceptedRun = runRows(database).at(-1);
    assert(acceptedRun?.id, 'The accepted run must have a durable id.');
    await waitForReconcileToSettle(page);
    await waitUntil(() => draftRows(database).filter(draft => draft.origin_run_id === acceptedRun.id).length === 2,
      'the first recovered run finishes its own local materialization');
    const runsAfterReconcile = runRows(database);
    assert.equal(runsAfterReconcile.length, runsBeforeLostAck.length + 1,
      'Visible Reconcile must settle the accepted run without redispatching it.');
    assert.equal(runsAfterReconcile.at(-1).id, acceptedRun.id);
    assert.equal(new Set(runsAfterReconcile.map(run => run.operation_id)).size, runsAfterReconcile.length,
      'The lost acknowledgement must not create a second operation identity.');
    recordCheck(checks, 'native-chat-failures:01',
      `Dropped start_project_chat acknowledgment reconciled one accepted run (${runsBeforeLostAck.length} → ${runsAfterReconcile.length}) without redispatch`);
    if (outputDirectory) await page.screenshot({ path: resolve(outputDirectory, 'chat-lost-ack-reconciled.png') });

    // Force only local materialization to fail. The completed provider result
    // remains durable, so removing the trigger and using Retry local save must
    // materialize that same run and its drafts without another provider call.
    const runsBeforeMaterialization = runRows(database);
    const draftsBeforeMaterialization = draftRows(database);
    installMaterializationFailure(database);
    triggerInstalled = true;
    await composer.fill('A completed response should survive a temporary local materialization failure.');
    await waitUntil(async () => await page.getByRole('button', { name: /^Send/ }).isEnabled(), 'reconciled request UI settles');
    await composer.press('Control+Enter');
    await waitUntil(() => runRows(database).length === runsBeforeMaterialization.length + 1,
      'second accepted project-chat run');
    await waitUntil(() => runRows(database).at(-1)?.status === 'completed',
      'second project-chat run completion');
    const materializationRun = runRows(database).at(-1);
    assert(materializationRun?.id, 'The materialization run must have a durable id.');
    await page.getByRole('button', { name: 'Retry local save', exact: true }).waitFor({ timeout: 30_000 });
    removeMaterializationFailure(database);
    triggerInstalled = false;
    await page.getByRole('button', { name: 'Retry local save', exact: true }).click();
    await waitUntil(() => draftRows(database).length === draftsBeforeMaterialization.length + 2,
      'two drafts materialized by local retry');
    const draftsAfterRetry = draftRows(database);
    const retriedDrafts = draftsAfterRetry.slice(draftsBeforeMaterialization.length);
    assert.equal(retriedDrafts.length, 2);
    assert(retriedDrafts.every(draft => draft.origin_run_id === materializationRun.id),
      'Local retry must materialize drafts from the original completed run.');
    const runsAfterRetry = runRows(database);
    assert.equal(runsAfterRetry.length, runsBeforeMaterialization.length + 1,
      'Local materialization retry must not dispatch a second provider run.');
    assert.equal(runsAfterRetry.at(-1).id, materializationRun.id);
    await page.waitForFunction(() => ![...document.querySelectorAll('button')]
      .some(button => button.textContent?.trim() === 'Retry local save'), null, { timeout: 30_000 });
    recordCheck(checks, 'native-chat-failures:02',
      `Temporary materialization failure recovered by local retry for the same run (${draftsBeforeMaterialization.length} → ${draftsAfterRetry.length} drafts; ${runsAfterRetry.length} total runs)`);
    if (outputDirectory) await page.screenshot({ path: resolve(outputDirectory, 'chat-materialization-retried.png') });

    // Adoption has its own idempotent operation receipt. Drop the renderer
    // acknowledgement after the server commits a grouped two-document apply,
    // then retry the exact preview and operation identity through the UI.
    await page.getByRole('button', { name: /^Review drafts/ }).click();
    const latestDraftIds = new Set(draftRows(database)
      .filter(draft => draft.origin_run_id === materializationRun.id)
      .map(draft => draft.document_id));
    assert.equal(latestDraftIds.size, 2, 'The grouped adoption fixture must have exactly two latest-run drafts.');
    const cards = page.locator('.chat-draft-card');
    let selectedLatestDrafts = 0;
    for (const card of await cards.all()) {
      const provenance = card.locator('details.chat-draft-provenance');
      await provenance.locator(':scope > summary').click();
      const originRunText = await provenance.innerText();
      const checkbox = card.getByRole('checkbox', { name: 'Include in this adoption', exact: true });
      if (!(await checkbox.count())) continue;
      const isLatest = originRunText.includes(materializationRun.id);
      if (isLatest) {
        selectedLatestDrafts += 1;
        if (!(await checkbox.isChecked())) await checkbox.check();
      } else if (await checkbox.isChecked()) {
        await checkbox.uncheck();
      }
    }
    assert.equal(selectedLatestDrafts, 2, 'The grouped preview must select only the latest run drafts.');
    await page.getByRole('button', { name: 'Prepare grouped preview', exact: true }).click();
    const previewRegion = page.getByRole('region', { name: 'Exact adoption preview', exact: true });
    await previewRegion.waitFor({ timeout: 30_000 });
    const adoptionButton = previewRegion.getByRole('button', { name: /^Adopt all 2 documents:/ });
    await adoptionButton.waitFor({ timeout: 30_000 });
    const ordinaryBeforeAdoption = Number(database.prepare("SELECT COUNT(*) AS n FROM documents WHERE role='ordinary' AND trashed=0").get().n);
    const epochBeforeAdoption = Number(database.prepare('SELECT context_source_epoch AS n FROM project').get().n);
    const receiptsBeforeAdoption = Number(database.prepare("SELECT COUNT(*) AS n FROM command_receipts WHERE operation_kind='adoptChatPreview'").get().n);
    const runsBeforeAdoption = runRows(database).length;
    await dropAckOnce(page, 'adopt_chat_preview', '__wnsDroppedAdoptionAck');
    await adoptionButton.click();
    await waitForDroppedAck(page, '__wnsDroppedAdoptionAck');
    await waitUntil(() => Number(database.prepare("SELECT COUNT(*) AS n FROM documents WHERE role='ordinary' AND trashed=0").get().n) === ordinaryBeforeAdoption + 2,
      'both grouped adoption documents commit before lost acknowledgment');
    await waitUntil(() => Number(database.prepare("SELECT COUNT(*) AS n FROM command_receipts WHERE operation_kind='adoptChatPreview'").get().n) === receiptsBeforeAdoption + 1,
      'grouped adoption receipt persists before lost acknowledgment');
    assert.equal(Number(database.prepare('SELECT context_source_epoch AS n FROM project').get().n), epochBeforeAdoption + 1);
    assert.equal(runRows(database).length, runsBeforeAdoption, 'Adoption must not dispatch another model run.');
    await page.getByText('Adoption needs confirmation.', { exact: true }).waitFor({ timeout: 30_000 });
    const retryAdoptionButton = previewRegion.getByRole('button', { name: /^Adopt all 2 documents:/ });
    await retryAdoptionButton.click();
    await previewRegion.waitFor({ state: 'detached', timeout: 30_000 });
    assert.equal(Number(database.prepare("SELECT COUNT(*) AS n FROM documents WHERE role='ordinary' AND trashed=0").get().n), ordinaryBeforeAdoption + 2);
    assert.equal(Number(database.prepare('SELECT context_source_epoch AS n FROM project').get().n), epochBeforeAdoption + 1);
    assert.equal(Number(database.prepare("SELECT COUNT(*) AS n FROM command_receipts WHERE operation_kind='adoptChatPreview'").get().n), receiptsBeforeAdoption + 1);
    assert.equal(runRows(database).length, runsBeforeAdoption);
    recordCheck(checks, 'native-chat-failures:03',
      `Grouped adoption acknowledgment recovery committed exactly two documents once (ordinary ${ordinaryBeforeAdoption} → ${ordinaryBeforeAdoption + 2}; one receipt; no run increase)`);
    if (outputDirectory) await page.screenshot({ path: resolve(outputDirectory, 'chat-grouped-adoption-reconciled.png') });
  } finally {
    if (triggerInstalled) {
      try { removeMaterializationFailure(database); } catch { /* preserve the original failure */ }
    }
    database.close();
  }
}
