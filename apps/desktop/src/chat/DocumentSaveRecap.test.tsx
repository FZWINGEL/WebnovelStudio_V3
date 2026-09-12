// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { DocumentSaveRecap } from './DocumentSaveRecap';
import type { ChatDocumentSave } from '../ipc/projectChat';
import type { ProjectAccess, Revision } from '../ipc/projects';
const read = vi.hoisted(() => vi.fn());
vi.mock('../ipc/history', async importOriginal => ({ ...await importOriginal<typeof import('../ipc/history')>(), readDocumentRevision: read }));
const access: ProjectAccess = { projectId: 'project', operationNamespace: 'space', session: 'session', writerLease: 'lease' };
const save: ChatDocumentSave = { operationId: 'save', head: { documentId: 'note', version: '5', bodyHash: 'hash' }, title: 'The captain', createdAt: '', revisionId: 'revision' };
const revision: Revision = { id: 'revision', head: save.head, body: { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'p' }, content: [{ type: 'text', text: 'Retained author wording' }] }] } }, reason: 'manual', parentId: null };
let host: HTMLDivElement;
let root: Root;
beforeEach(() => { Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); read.mockReset(); host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
async function render(current = access, saves = [save]) { await act(async () => root.render(<DocumentSaveRecap access={current} saves={saves} documents={[]} />)); }
async function inspect() { await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Inspect saved version 5')!.click()); }
it('shows receipt metadata without reading prose, then reads only the exact checkpoint on request', async () => {
  read.mockResolvedValue(revision);
  await render();
  expect(host.textContent).toContain('You saved The captain · Version 5');
  expect(read).not.toHaveBeenCalled();
  await inspect();
  expect(read).toHaveBeenCalledWith(access, 'note', 'revision');
  expect(host.textContent).toContain('Retained author wording');
});
it('does not display a mismatched revision or a delayed response after changing projects', async () => {
  read.mockResolvedValue({ ...revision, head: { ...save.head, version: '6' } });
  await render(); await inspect();
  expect(host.textContent).toContain('does not match this save');
  expect(host.textContent).not.toContain('Retained author wording');
  let resolve!: (value: Revision) => void;
  read.mockImplementation(() => new Promise<Revision>(done => { resolve = done; }));
  await inspect();
  await render({ ...access, projectId: 'other', operationNamespace: 'other-space' }, []);
  await act(async () => resolve(revision));
  expect(host.textContent).not.toContain('Retained author wording');
});
it('does not invent a retained body when a save has no checkpoint', async () => {
  await render(access, [{ ...save, revisionId: null }]);
  expect(host.textContent).toContain('no separate retained checkpoint');
  expect(host.querySelector('button')).toBeNull();
  expect(read).not.toHaveBeenCalled();
});
