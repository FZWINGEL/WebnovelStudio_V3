// Synthetic fixture builder for the reviewed-memory lookup qualification.
// This module only uses the application's native IPC surface. It does not
// launch a desktop process, select a provider, or dispatch a model request.
import assert from 'node:assert/strict';
import { realpath } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { isAbsolute, relative, sep, toNamespacedPath } from 'node:path';

function invoke(page, command, args) {
  return page.evaluate(([name, input]) => input === undefined
    ? window.__TAURI_INTERNALS__.invoke(name)
    : window.__TAURI_INTERNALS__.invoke(name, input), [command, args]);
}

function bodyWithParagraphs(documentId, paragraphs) {
  return {
    schemaVersion: 1,
    body: {
      type: 'doc',
      content: paragraphs.map((text, index) => ({
        type: 'paragraph',
        attrs: { id: `${documentId}-paragraph-${index + 1}` },
        content: [{ type: 'text', text }],
      })),
    },
  };
}

function sha256(value) {
  return createHash('sha256').update(value, 'utf8').digest('hex');
}

function evidence(quote, blockId, quoteHash = sha256(quote)) {
  return { blockId, fromUtf16: 0, toUtf16: quote.length, quote, quoteHash };
}

function projectInside(data, projectPath) {
  const child = relative(toNamespacedPath(data), toNamespacedPath(projectPath));
  assert(child && !isAbsolute(child) && child !== '..' && !child.startsWith(`..${sep}`),
    'Memory lookup fixture must stay inside the supplied native test data directory.');
}

/**
 * Create one older reviewed chapter with explicit knowledge, promise, and
 * possession identities, plus a current chapter for the lookup journey.
 *
 * The caller must supply a page attached to an already-running native app and
 * the app's temporary test-data root. The helper returns durable access/head
 * metadata and the exact IDs needed to drive model lookup requests later.
 */
export async function setupMemoryLookupFixture({ page, data, title = 'Memory lookup story', prefix = 'memory-lookup' }) {
  assert(page, 'setupMemoryLookupFixture requires a native Playwright page.');
  assert(data, 'setupMemoryLookupFixture requires the owned native test-data root.');
  const opened = await invoke(page, 'library_create', {
    operationId: `${prefix}-project`,
    title,
    session: `${prefix}-session`,
  });
  const access = opened.access;
  const olderBody = bodyWithParagraphs('memory-older-chapter', [
    'Mei carries the silver key into the watched gate.',
    'Mei promises to return the silver key before dusk.',
    'Mei believes the gate is watched.',
  ]);
  const currentBody = bodyWithParagraphs('memory-current-chapter', [
    'Beyond the gate, Mei listens for footsteps.',
  ]);
  const create = (operationId, documentId, documentTitle, body) => invoke(page, 'create_document', { request: {
    access, operationId, documentId, title: documentTitle, kind: 'chapter', body,
  } });
  const older = await create(`${prefix}-create-older`, 'memory-older-chapter', 'The Watched Gate', olderBody);

  const character = { id: 'memory-character-mei', label: 'Mei' };
  const topic = { id: 'memory-topic-watched-gate', label: 'The watched gate' };
  const object = { id: 'memory-object-silver-key', label: 'The silver key' };
  const promise = { id: 'memory-promise-return-key', label: 'Return the silver key' };
  const records = [{
    id: 'memory-possession-key', object, holder: character, timing: 'atPassage', audience: 'reader',
    evidence: evidence(olderBody.body.content[0].content[0].text, 'memory-older-chapter-paragraph-1'),
  }];
  const promises = [{
    id: 'memory-promise-return', promise, phase: 'setup', timing: 'atPassage', audience: 'reader',
    note: 'Mei promises to return the silver key before dusk.',
    evidence: evidence(olderBody.body.content[1].content[0].text, 'memory-older-chapter-paragraph-2'),
  }];
  const knowledge = [{
    id: 'memory-knowledge-gate', character, topic, attitude: 'believes', timing: 'atPassage', audience: 'reader',
    statement: 'Mei believes the gate is watched.',
    evidence: evidence(olderBody.body.content[2].content[0].text, 'memory-older-chapter-paragraph-3'),
  }];
  const stage = await invoke(page, 'stage_author_review', { request: {
    access,
    operationId: `${prefix}-stage-older`,
    expected: older.head,
    records,
    promises,
    knowledge,
  } });
  const bundle = await invoke(page, 'mark_ready', { request: {
    access, operationId: `${prefix}-ready-older`, stageId: stage.id,
  } });
  const current = await create(`${prefix}-create-current`, 'memory-current-chapter', 'Beyond the Watched Gate', currentBody);

  const library = await invoke(page, 'library_snapshot');
  const entry = library.entries.find(item => item.projectId === opened.project.projectId);
  assert(entry, 'The memory lookup fixture project must be registered in the native library.');
  const projectPath = await realpath(entry.path);
  projectInside(await realpath(data), projectPath);

  return {
    title,
    targetTitle: current.title,
    olderTitle: older.title,
    originalBody: olderBody,
    project: opened.project,
    access,
    projectPath,
    olderHead: older.head,
    targetHead: current.head,
    older: { document: older, head: older.head, body: olderBody, stage, bundle },
    current: { document: current, head: current.head, body: currentBody },
    ids: {
      characterId: character.id,
      topicId: topic.id,
      objectId: object.id,
      promiseId: promise.id,
      possessionRecordId: records[0].id,
      promiseRecordId: promises[0].id,
      knowledgeRecordId: knowledge[0].id,
    },
    records: { character, topic, object, promise, possession: records[0], promiseObservation: promises[0], knowledge: knowledge[0] },
  };
}
