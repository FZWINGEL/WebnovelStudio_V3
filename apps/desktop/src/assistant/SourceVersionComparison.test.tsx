// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import { SourceVersionComparison } from './SourceVersionComparison';
import { readStoryContextSource, type SourceDescriptor } from '../ipc/context';
import { readDocument, type DocumentRecord, type ProjectAccess } from '../ipc/projects';

vi.mock('../ipc/context', () => ({ readStoryContextSource: vi.fn() }));
vi.mock('../ipc/projects', () => ({ readDocument: vi.fn() }));
const access: ProjectAccess = { projectId: 'p', operationNamespace: 'n', session: 's', writerLease: 'l' };
const source: SourceDescriptor = { handle: 'source-1', displayName: 'Harbor', source: { projectId: 'p', documentId: 'world', revisionId: 'revision-1', bodyHash: 'old-hash' }, kind: 'currentDraft', current: true, coverage: 'verbatim', disclosure: { readerPosition: '1', visibleToCharacters: [], authorOnly: false, futurePrivate: false }, storyTime: null, dependencies: [] };
function body(text: string): DocumentRecord['body'] { return { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'b' }, content: [{ type: 'text', text }] }] } }; }
const current: DocumentRecord = { head: { documentId: 'world', version: '7', bodyHash: 'new-hash' }, body: body('The new harbor.'), title: 'Harbor', kind: 'world', role: 'ordinary', metadataVersion: '1', lastCheckpointId: null };
let host: HTMLDivElement; let root: Root;
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); vi.resetAllMocks();
  vi.mocked(readDocument).mockResolvedValue(current);
  vi.mocked(readStoryContextSource).mockResolvedValue({ descriptor: source, body: body('The original harbor.'), passages: [], usedValidatedProjection: false });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
async function render(snapshotId = 'snapshot-1') { await act(async () => root.render(<SourceVersionComparison access={access} snapshotId={snapshotId} source={source} />)); }
async function click(label: string) { await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === label)!.click()); }

it('resolves discussed text from its frozen snapshot and current text independently', async () => {
  await render();
  expect(readDocument).not.toHaveBeenCalled(); expect(readStoryContextSource).not.toHaveBeenCalled();
  await click('Version discussed');
  expect(readStoryContextSource).toHaveBeenCalledWith(access, 'snapshot-1', 'source-1');
  expect(host.textContent).toContain('The original harbor.'); expect(readDocument).not.toHaveBeenCalled();
  await click('Current version');
  expect(readDocument).toHaveBeenCalledWith(access, 'world');
  expect(host.textContent).toContain('Current saved version · v7'); expect(host.textContent).toContain('The new harbor.');
  expect(host.textContent).not.toContain('The original harbor.');
  await click('Version discussed'); expect(host.textContent).toContain('revision-1'); expect(host.textContent).toContain('The original harbor.');
});

it('ignores a late current read after choosing the discussed version', async () => {
  let finish!: (value: DocumentRecord) => void;
  vi.mocked(readDocument).mockReturnValue(new Promise(resolve => { finish = resolve; }));
  await render(); await click('Current version'); await click('Version discussed');
  await act(async () => finish(current));
  expect(host.textContent).toContain('The original harbor.'); expect(host.textContent).not.toContain('The new harbor.');
});

it('fences a late result after the snapshot changes', async () => {
  let finish!: (value: DocumentRecord) => void;
  vi.mocked(readDocument).mockReturnValue(new Promise(resolve => { finish = resolve; }));
  await render(); await click('Current version'); await render('snapshot-2');
  await act(async () => finish(current));
  expect(host.textContent).not.toContain('The new harbor.'); expect(host.querySelector('[role="status"]')).toBeNull();
});

it('reports an unavailable current document without substituting old prose', async () => {
  vi.mocked(readDocument).mockRejectedValue({ code: 'DocumentNotFound', detail: 'The current document is unavailable.' });
  await render(); await click('Version discussed'); await click('Current version');
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('current document is unavailable');
  expect(host.textContent).not.toContain('The original harbor.'); expect(readStoryContextSource).toHaveBeenCalledTimes(1);
});
