import { expect, it, vi } from 'vitest';
import { bodyHash, canonicalJson, type WnsDocument } from '../editor/document';
import { DocumentSession } from '../editor/session';
import { captureRevisionScope } from '../editor/revisionScope';
import type { ProjectTransport } from '../ipc/projects';
import { confirmChapterRange } from './confirmChapterRange';
import type { SuggestedChapterRange } from './ChapterRangeReview';

async function fixture() {
  const access = { projectId: 'p', operationNamespace: 'n', session: 's', writerLease: 'l' };
  const body: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: ['Arrival.', 'The confrontation.', 'Keep this ending.'].map((text, i) => ({ type: 'paragraph', attrs: { id: `p${i}` }, content: [{ type: 'text', text }] })) } };
  const head = { documentId: 'chapter', version: '1', bodyHash: await bodyHash(canonicalJson(body)) };
  const save = vi.fn<ProjectTransport['save']>(async request => ({ ...access, documentId: head.documentId, operationId: request.operationId, head: { ...head, version: '2', bodyHash: await bodyHash(canonicalJson(request.body)) }, savedGeneration: request.localGeneration }));
  const transport: ProjectTransport = { save, validate: vi.fn(async () => {}), reconcile: vi.fn(), checkpoint: vi.fn() };
  const session = new DocumentSession(access, { head, title: 'Arrival', kind: 'chapter', metadataVersion: '1', body, lastCheckpointId: null }, transport, { autosave: false });
  const scope = captureRevisionScope(body, head.bodyHash, 'blocks', { kind: 'passage', start: { blockId: 'p1', utf16Offset: 0 }, end: { blockId: 'p1', utf16Offset: 18 }, quote: 'The confrontation.', sourceBodyHash: head.bodyHash });
  const range: SuggestedChapterRange = { target: head, scope, explanation: 'This confrontation can carry more emotion.' };
  return { body, session, range, save };
}

it('confirms exact paragraphs into a separate unsent edit request while preserving the ending and body', async () => {
  const { body, session, range, save } = await fixture();
  const stage = vi.fn(async () => { expect(session.state.editable).toBe(false); });
  await confirmChapterRange(session, range, async () => range, stage);
  expect(stage).toHaveBeenCalledWith({ target: range.target, intent: 'proposeEdits', basis: null, scope: range.scope });
  expect(stage.mock.calls).toHaveLength(1); expect(save).not.toHaveBeenCalled();
  expect(session.body).toEqual(body); expect(range.scope.quote).not.toContain('ending'); expect(session.state.editable).toBe(true);
});

it('saves newer typing and refuses the older range without replacing the author body', async () => {
  const { body, session, range, save } = await fixture();
  const changed = structuredClone(body);
  changed.body.content[1] = { type: 'paragraph', attrs: { id: 'p1' }, content: [{ type: 'text', text: 'A rewritten confrontation.' }] };
  session.update(changed);
  const stage = vi.fn();
  await expect(confirmChapterRange(session, range, async () => range, stage)).rejects.toThrow('chapter changed');
  expect(save).toHaveBeenCalledOnce(); expect(stage).not.toHaveBeenCalled(); expect(session.body).toEqual(changed); expect(session.state.editable).toBe(true);
});

it('refuses a revoked or altered range even when the chapter itself is unchanged', async () => {
  const { session, range } = await fixture();
  const stage = vi.fn();
  await expect(confirmChapterRange(session, range, async () => null, stage)).rejects.toThrow('no longer available');
  await expect(confirmChapterRange(session, range, async () => ({ ...range, scope: { ...range.scope, quote: 'Other text.' } }), stage)).rejects.toThrow('suggested passage changed');
  expect(stage).not.toHaveBeenCalled(); expect(session.state.editable).toBe(true);
});
