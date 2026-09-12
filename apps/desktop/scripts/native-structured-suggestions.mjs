import { recordCheck } from './native-evidence.mjs';
import assert from 'node:assert/strict';
import { realpath } from 'node:fs/promises';
import { isAbsolute, relative, resolve, sep, toNamespacedPath } from 'node:path';
import { DatabaseSync } from 'node:sqlite';

export async function qualifyStructuredSuggestions({ page, data, output, createWritingProject, checks }) {
  await createWritingProject('Structured suggestions story', 'chapter', 'The river gate', 'Mei waited at the gate.');
  const original = { type: 'doc', content: [
    { type: 'paragraph', attrs: { id: 'protected-opening' }, content: [{ type: 'text', text: 'Mei waited at the gate.', marks: [{ type: 'bold' }] }] },
    { type: 'heading', attrs: { id: 'selected-heading', level: 2 }, content: [{ type: 'text', text: 'The crossing' }] },
    { type: 'paragraph', attrs: { id: 'selected-first' }, content: [{ type: 'text', text: 'The river carried a warning.' }] },
    { type: 'sceneBreak', attrs: { id: 'selected-break' } },
    { type: 'paragraph', attrs: { id: 'selected-last' }, content: [{ type: 'text', text: 'A lantern turned towards the shore.' }] },
    { type: 'paragraph', attrs: { id: 'protected-ending' }, content: [{ type: 'text', text: 'The promise would hold.', marks: [{ type: 'italic' }] }] },
  ] };
  await page.evaluate(body => {
    const editor = document.querySelector('[aria-label="Manuscript"]').editor;
    editor.commands.setContent(body); window.structuredManuscript = editor;
  }, original);
  await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  const library = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('library_snapshot'));
  const projectPath = await realpath(library.entries.find(entry => entry.title === 'Structured suggestions story').path);
  const child = relative(toNamespacedPath(await realpath(data)), toNamespacedPath(projectPath));
  assert(child && !isAbsolute(child) && child !== '..' && !child.startsWith(`..${sep}`));
  const db = new DatabaseSync(resolve(projectPath, 'project.sqlite3'));
  const manuscript = () => page.evaluate(() => document.querySelector('[aria-label="Manuscript"]').editor.getJSON());
  const reopen = async () => {
    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
    await page.reload();
    await page.getByRole('button', { name: /^Structured suggestions story Last opened/ }).click();
    await page.getByRole('heading', { name: 'The river gate', exact: true }).waitFor();
  };
  try {
    // Capture only partial endpoint paragraphs, then explicitly widen them.
    await page.evaluate(() => {
      const editor = window.structuredManuscript;
      let offset = 0; const positions = [];
      editor.state.doc.forEach(node => { positions.push(offset); offset += node.nodeSize; });
      editor.commands.setTextSelection({ from: positions[1] + 3, to: positions[4] + 9 });
    });
    await page.getByRole('button', { name: 'Discuss selection', exact: true }).click();
    await page.getByRole('button', { name: 'Suggest edits', exact: true }).click();
    await page.getByRole('button', { name: 'Selected paragraphs', exact: true }).click();
    await page.getByRole('textbox', { name: 'Request edits for these paragraphs', exact: true }).fill('Make the crossing more urgent. Preserve everything outside these paragraphs.');
    const captured = await page.locator('.quoted-scope blockquote').innerText();
    assert(captured.startsWith('The crossing') && captured.endsWith('A lantern turned towards the shore.'));
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    let card = page.locator('.proposal-card').last();
    await card.getByText('Before · selected paragraphs', { exact: true }).waitFor();
    assert.deepEqual(await manuscript(), original);
    const mini = card.getByRole('textbox', { name: /^Replacement prose:/ });
    await mini.fill('The crossing changed everything.');
    await mini.press('End'); await mini.press('Enter');
    await page.keyboard.type('Mei heard the warning.');
    await mini.press('Shift+Enter'); await page.keyboard.type('She answered with the lantern.');
    await card.getByRole('button', { name: 'Suggestion bold', exact: true }).click();
    await page.keyboard.type(' Hold fast.');
    const runsBeforePreview = db.prepare('SELECT count(*) AS n FROM discussion_runs').get().n;
    await page.evaluate(() => {
      const fetch = window.fetch;
      window.structuredPrepareRequests = []; window.structuredAckLost = false; window.structuredReadHidden = false;
      window.fetch = async (...args) => {
        const url = String(args[0]);
        if (url.endsWith('/prepare_structured')) {
          const parsed = JSON.parse(args[1].body);
          window.structuredPrepareRequests.push(structuredClone(parsed.request ?? parsed));
          const response = await fetch.apply(window, args);
          if (!window.structuredAckLost && response.headers.get('Tauri-Response') === 'ok') {
            window.structuredAckLost = true;
            return new Response(JSON.stringify({ code: 'UncertainOutcome', detail: 'Synthetic lost structured preview acknowledgment' }), { headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' } });
          }
          window.fetch = fetch;
          return response;
        }
        const response = await fetch.apply(window, args);
        if (url.endsWith('/proposals') && window.structuredAckLost && !window.structuredReadHidden && response.headers.get('Tauri-Response') === 'ok') {
          const rows = await response.clone().json(); window.structuredReadHidden = true;
          return new Response(JSON.stringify(rows.map(row => row.id === window.structuredPrepareRequests[0].proposalId ? { ...row, prepared: null } : row)), { headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'ok' } });
        }
        return response;
      };
    });
    await card.getByRole('button', { name: 'Preview', exact: true }).click();
    await card.getByRole('button', { name: 'Check preview', exact: true }).click();
    await card.locator('.structured-prose strong').filter({ hasText: 'Hold fast.' }).waitFor();
    const requests = await page.evaluate(() => window.structuredPrepareRequests);
    assert.equal(requests.length, 2);
    assert.deepEqual(requests[0], requests[1], 'Lost preview acknowledgment must retain operation, blocks, IDs and exact body');
    assert.equal(db.prepare('SELECT count(*) AS n FROM proposal_versions WHERE proposal_id=?').get(requests[0].proposalId).n, 1);
    assert.equal(db.prepare('SELECT count(*) AS n FROM discussion_runs').get().n, runsBeforePreview);
    assert.deepEqual(await manuscript(), original);
    await page.screenshot({ path: resolve(output, 'structured-paragraph-preview.png') });
    await card.getByRole('button', { name: 'Apply', exact: true }).click();
    await card.locator('.proposal-status').filter({ hasText: /^Applied$/ }).waitFor();
    const after = await manuscript();
    assert.deepEqual(after, requests[0].body.body);
    assert.deepEqual(after.content[0], original.content[0]);
    assert.deepEqual(after.content.at(-1), original.content.at(-1));
    assert(await page.evaluate(() => document.querySelector('[aria-label="Manuscript"]').editor === window.structuredManuscript));
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    assert.deepEqual(await manuscript(), original);
    await page.getByRole('button', { name: 'Redo', exact: true }).click();
    await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
    assert.deepEqual(await manuscript(), after);
    await reopen();
    assert.deepEqual(await manuscript(), after);
    await page.locator('.proposal-card').last().locator('.proposal-status').filter({ hasText: /^Applied$/ }).waitFor();
    recordCheck(checks, 'native-structured-suggestions:01', 'Native explicit paragraph scope expands partial endpoints visibly; rich preview retains hard breaks and formatting without generation, retries one lost acknowledgment with identical IDs/body, protects outside blocks on Apply, and survives undo/redo/reload');

    await page.getByRole('button', { name: 'Suggest edits', exact: true }).click();
    await page.getByRole('textbox', { name: 'Request edits for this passage', exact: true }).fill('Rewrite this chapter with a quieter ending.');
    assert(await page.getByRole('button', { name: 'Send', exact: true }).isDisabled(), 'No selection must not authorize a whole chapter');
    await page.getByRole('button', { name: 'Whole chapter', exact: true }).click();
    await page.getByRole('textbox', { name: 'Request edits for this chapter', exact: true }).waitFor();
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    card = page.locator('.proposal-card').last();
    await card.getByText('Before · whole chapter', { exact: true }).waitFor();
    await card.getByRole('button', { name: 'Preview', exact: true }).click();
    await card.locator('.structured-prose').waitFor();
    assert.deepEqual(await manuscript(), after);
    // Formatting is part of the preview identity, even when text is unchanged.
    await card.getByRole('textbox', { name: /^Replacement prose:/ }).click();
    await page.keyboard.press('Control+a');
    await card.getByRole('button', { name: 'Suggestion italic', exact: true }).click();
    assert(await card.getByRole('button', { name: 'Apply', exact: true }).isDisabled());
    await card.getByRole('button', { name: 'Preview', exact: true }).click();
    await card.locator('.structured-prose em').first().waitFor();
    await card.getByRole('button', { name: 'Apply', exact: true }).click();
    await card.locator('.proposal-status').filter({ hasText: /^Applied$/ }).waitFor();
    const wholeAfter = await manuscript();
    assert(wholeAfter.content.length >= 2);
    assert(wholeAfter.content.every(block => !after.content.some(old => old.attrs.id === block.attrs.id)));
    await page.screenshot({ path: resolve(output, 'structured-whole-chapter-applied.png') });
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    assert.deepEqual(await manuscript(), after);
    await page.getByRole('button', { name: 'Redo', exact: true }).click();
    await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
    await reopen();
    assert.deepEqual(await manuscript(), wholeAfter);
    recordCheck(checks, 'native-structured-suggestions:02', 'Native whole-chapter suggestions require explicit scope; formatting edits invalidate preview, complete structured Apply replaces only its authorized chapter, and exact whole-chapter undo/redo and saved decisions survive reload');
    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  } finally { db.close(); }
}
