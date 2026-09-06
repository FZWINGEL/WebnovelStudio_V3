// Native qualification for passage-backed character knowledge records.
//
// This helper is intentionally independent from native-smoke.mjs. The parent
// harness supplies its synthetic project creator and can invoke this flow only
// after the rebuilt desktop binary is ready. All story text is synthetic and
// the database is opened read-only for durable assertions.
import assert from 'node:assert/strict';
import { realpath } from 'node:fs/promises';
import { isAbsolute, relative, resolve, sep, toNamespacedPath } from 'node:path';
import { DatabaseSync } from 'node:sqlite';

function projectInside(data, projectPath) {
  const child = relative(toNamespacedPath(data), toNamespacedPath(projectPath));
  assert(child && !isAbsolute(child) && child !== '..' && !child.startsWith(`..${sep}`),
    'Knowledge fixture must stay inside this synthetic run');
}

function bodyWithParagraphs(paragraphs) {
  return {
    type: 'doc',
    content: paragraphs.map((text, index) => ({
      type: 'paragraph',
      attrs: { id: `knowledge-paragraph-${index + 1}` },
      content: [{ type: 'text', text }],
    })),
  };
}

/**
 * Runs the character-knowledge review, author-room inspection/history, and
 * restricted-writing journeys against a real Tauri WebView.
 *
 * The fixture deliberately keeps the model interaction on the harness's local
 * deterministic provider. Review and history are author-controlled local
 * actions; they do not trigger a paid model request.
 */
export async function runKnowledgeFlow({ page, data, output, createWritingProject, checks }) {
  const title = 'Character knowledge story';
  const firstTitle = 'The watched gate';
  const publicPassage = 'Mei knows the gate is watched.';
  const privatePassage = 'Mei notices the mentor avoids the key.';
  const privateStatement = 'Mei suspects the mentor hid the key.';
  // The shared native project helper seeds one paragraph; saveBody below
  // installs the two explicit passages through the editor model.
  const initialText = publicPassage;
  let db;

  await createWritingProject(title, 'chapter', firstTitle, initialText);

  const library = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('library_snapshot'));
  const entry = library.entries.find(item => item.title === title);
  assert(entry, 'Knowledge fixture project must be visible in the native library');
  const projectPath = await realpath(entry.path);
  projectInside(await realpath(data), projectPath);
  db = new DatabaseSync(resolve(projectPath, 'project.sqlite3'), { readOnly: true });

  const latestPacket = () => db.prepare('SELECT * FROM context_packets ORDER BY rowid DESC LIMIT 1').get();
  const editorJson = () => page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());

  async function saveBody(body) {
    await page.evaluate(value => document.querySelector('.tiptap').editor.commands.setContent(value), body);
    await page.waitForFunction(expected => document.querySelector('.tiptap')?.editor?.getText() === expected,
      body.content.map(node => node.content?.[0]?.text ?? '').join('\n\n'));
    await page.getByRole('status').filter({ hasText: /^Saved$/ }).waitFor();
  }

  async function selectBlock(quote) {
    await page.evaluate(text => {
      const editor = document.querySelector('.tiptap').editor;
      let found = null;
      editor.state.doc.descendants((node, position) => {
        if (!found && node.type.name === 'paragraph' && node.textContent === text) {
          found = { from: position + 1, to: position + 1 + text.length };
        }
      });
      if (!found) throw new Error(`Synthetic knowledge passage not found: ${text}`);
      editor.commands.setTextSelection(found);
    }, quote);
  }

  async function openReview() {
    await page.getByRole('button', { name: 'Story review', exact: true }).click();
    await page.getByRole('button', { name: 'Review saved chapter', exact: true }).click();
    await page.getByLabel('Chapter under review', { exact: true }).waitFor();
  }

  async function openKnowledge() {
    await page.getByRole('button', { name: /^Knowledge \(/ }).click();
  }

  async function keepKnowledge({ quote, characterId, topicId, characterName, topicName, attitude, statement, reader }) {
    await selectBlock(quote);
    await page.getByRole('button', { name: 'Add knowledge observation', exact: true }).click();
    const form = page.getByLabel('Add character knowledge', { exact: true });
    if (characterId) await form.getByLabel('Character', { exact: true }).selectOption(characterId);
    else {
      await form.getByLabel('Character', { exact: true }).selectOption('__new_character__');
      await form.getByRole('textbox', { name: 'Character name', exact: true }).fill(characterName);
    }
    if (topicId) await form.getByLabel('Topic', { exact: true }).selectOption(topicId);
    else {
      await form.getByLabel('Topic', { exact: true }).selectOption('__new_topic__');
      await form.getByRole('textbox', { name: 'Topic name', exact: true }).fill(topicName);
    }
    await form.getByLabel('Recorded attitude', { exact: true }).selectOption(attitude);
    await form.getByRole('textbox', { name: 'Knowledge statement', exact: true }).fill(statement);
    await form.getByLabel('Knowledge timing', { exact: true }).selectOption('atPassage');
    const disclosure = form.getByRole('checkbox', { name: 'Explicitly disclosed to the reader', exact: true });
    if (reader) await disclosure.check();
    await form.getByRole('button', { name: 'Keep knowledge', exact: true }).click();
    await page.locator('.review-detail-card').filter({ hasText: statement }).waitFor();
  }

  async function markReviewed() {
    await page.getByRole('button', { name: 'Save reviewed details', exact: true }).click();
    await page.getByRole('button', { name: 'Mark this version reviewed', exact: true }).waitFor();
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
    await saveBody(bodyWithParagraphs([publicPassage, privatePassage]));
    const original = await editorJson();

    await openReview();
    await openKnowledge();
    await keepKnowledge({
      quote: publicPassage,
      characterName: 'Mei', topicName: 'The watched gate', attitude: 'knows',
      statement: 'Mei knows the gate is watched.', reader: true,
    });

    // The second observation selects the explicit entities from the first
    // record. This validates identity reuse instead of creating duplicate
    // characters/topics with the same display label.
    await selectBlock(privatePassage);
    await page.getByRole('button', { name: 'Add knowledge observation', exact: true }).click();
    const secondForm = page.getByLabel('Add character knowledge', { exact: true });
    const meiOption = secondForm.getByLabel('Character', { exact: true }).locator('option').filter({ hasText: /^Mei$/ }).first();
    const gateOption = secondForm.getByLabel('Topic', { exact: true }).locator('option').filter({ hasText: /^The watched gate$/ }).first();
    const meiId = await meiOption.getAttribute('value');
    const gateId = await gateOption.getAttribute('value');
    assert(meiId && gateId, 'The second observation must reuse the first character and topic identities');
    await secondForm.getByLabel('Character', { exact: true }).selectOption(meiId);
    await secondForm.getByLabel('Topic', { exact: true }).selectOption(gateId);
    await secondForm.getByLabel('Recorded attitude', { exact: true }).selectOption('suspects');
    await secondForm.getByRole('textbox', { name: 'Knowledge statement', exact: true }).fill(privateStatement);
    await secondForm.getByLabel('Knowledge timing', { exact: true }).selectOption('atPassage');
    await secondForm.getByRole('button', { name: 'Keep knowledge', exact: true }).click();
    await page.locator('.review-detail-card').filter({ hasText: privateStatement }).waitFor();
    assert.deepEqual(await editorJson(), original, 'Recording knowledge must not edit the manuscript');
    await page.screenshot({ path: resolve(output, 'knowledge-review-draft.png') });

    await markReviewed();
    const bundle = db.prepare(`
      SELECT b.* FROM ready_bundles b
      JOIN ready_heads h ON h.bundle_id=b.id
      WHERE b.document_id=(SELECT id FROM documents WHERE title=?)
    `).get(firstTitle);
    assert(bundle?.knowledge_json, 'The reviewed knowledge bundle must persist observations');
    const records = JSON.parse(bundle.knowledge_json);
    assert.equal(records.length, 2);
    assert.equal(new Set(records.map(record => record.id)).size, 2);
    const publicRecord = records.find(record => record.audience === 'reader');
    const privateRecord = records.find(record => record.audience === 'authorRoom');
    assert(publicRecord && privateRecord);
    assert.equal(publicRecord.evidence.quote, publicPassage);
    assert.equal(privateRecord.evidence.quote, privatePassage);
    assert.equal(privateRecord.statement, privateStatement);
    assert.equal(publicRecord.character.id, privateRecord.character.id, 'Records must reuse character identity');
    assert.equal(publicRecord.topic.id, privateRecord.topic.id, 'Records must reuse topic identity');

    await back();
    await page.getByRole('textbox', { name: 'Discuss this document', exact: true }).fill('What does Mei know about the watched gate?');
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    await page.locator('.persistent-feedback article').filter({ hasText: 'This test confirms discussion and context handling' }).waitFor();
    const workingPacket = JSON.parse(latestPacket().packet_json);
    const workingText = JSON.stringify(workingPacket);
    assert(workingText.includes(publicRecord.id));
    assert(workingText.includes(privateRecord.id));
    assert.deepEqual(workingPacket.receipt.reviewedKnowledge.flatMap(set => set.recordIds).sort(), records.map(record => record.id).sort());

    await openInspector();
    const usedRows = page.locator('.context-inspector > details[open] .context-reviewed-evidence');
    await usedRows.filter({ hasText: 'Mei knows the gate is watched.' }).waitFor();
    await usedRows.filter({ hasText: privateStatement }).waitFor();
    const historyButton = usedRows.getByRole('button', { name: 'Find knowledge history for Mei about The watched gate', exact: true }).first();
    await historyButton.click();
    const history = page.getByRole('region', { name: 'Character knowledge history', exact: true });
    await history.waitFor();
    assert.equal(await history.locator('ol > li').count(), 2);
    assert((await history.innerText()).includes(publicStatementFrom(publicRecord)));
    assert((await history.innerText()).includes(privateStatement));
    assert((await history.innerText()).includes(publicPassage));
    assert((await history.innerText()).includes(privatePassage));
    const historyPacket = latestPacket();
    await history.getByRole('button', { name: /Read .* for knowledge observation 1/ }).click();
    const source = page.getByRole('region', { name: 'Saved story source', exact: true });
    await source.waitFor();
    assert((await source.innerText()).includes(publicPassage));
    assert.deepEqual(latestPacket(), historyPacket, 'Knowledge history/source inspection must not create a model packet');
    await page.screenshot({ path: resolve(output, 'knowledge-history.png') });
    checks.push('Native review records reader-visible and author-room character knowledge with exact selected quotations, reuses entity identities, and exposes source-ordered knowledge history without another model request');

    await page.getByRole('button', { name: 'Close source', exact: true }).click();
    await page.getByRole('button', { name: 'Close knowledge history', exact: true }).click();
    await createChapter('Beyond the watched gate', 'The path beyond the gate was quiet.');
    await page.getByRole('button', { name: 'Continue chapter', exact: true }).click();
    await page.getByLabel('Story basis', { exact: true }).selectOption('reviewed');
    await page.getByRole('textbox', { name: 'What should happen next?', exact: true }).fill('Continue from reviewed, reader-visible character knowledge only.');
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    const candidate = page.locator('.proposal-card').filter({ hasText: 'Local test continuation' });
    await candidate.waitFor();
    const restrictedPacketObject = JSON.parse(latestPacket().packet_json);
    const restrictedPacket = JSON.stringify(restrictedPacketObject);
    const restrictedEnvelope = JSON.parse(restrictedPacketObject.messages[1].content);
    const deliveredKnowledge = restrictedEnvelope.reviewedKnowledge?.sets?.flatMap(set => set.records) ?? [];
    assert(deliveredKnowledge.some(record => record.id === publicRecord.id), 'Restricted continuation must deliver reader-visible knowledge');
    assert(!deliveredKnowledge.some(record => record.id === privateRecord.id), 'Restricted continuation must omit private knowledge identity');
    assert(!restrictedPacket.includes(privateRecord.id), 'Restricted continuation must omit private knowledge identity');
    assert(!restrictedPacket.includes(privateStatement), 'Restricted continuation must omit private knowledge statement');
    await openInspector();
    const restrictedText = await page.locator('.context-inspector').innerText();
    assert(restrictedText.includes('Mei knows the gate is watched.'));
    assert(!restrictedText.includes(privateStatement));
    await page.screenshot({ path: resolve(output, 'knowledge-restricted-context.png') });
    await candidate.getByRole('button', { name: 'Reject', exact: true }).click();
    await candidate.getByText('Rejected', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'All projects', exact: true }).click();
    await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
    checks.push('Native restricted continuation includes reader-visible character knowledge and excludes the private observation from the delivered envelope and context inspector');
  } finally {
    db?.close();
  }
}

function publicStatementFrom(record) {
  return record.statement;
}
