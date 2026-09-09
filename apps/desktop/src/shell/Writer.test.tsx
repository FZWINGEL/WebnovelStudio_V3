// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { bodyHash, canonicalJson, type WnsDocument } from '../editor/document';
import { DocumentSession } from '../editor/session';
import type { DocumentRecord, Head, ProjectAccess, ProjectTransport, SaveAck, SaveSnapshot, ViewState } from '../ipc/projects';
import { Writer } from './Writer';
import type { ProjectChapterComposer } from '../ipc/projectChat';

const mocks = vi.hoisted(() => ({
  readDocumentAliases: vi.fn(),
  setDocumentAliases: vi.fn(),
  saveViewState: vi.fn(),
  readProjectChapterFeedback: vi.fn(),
}));

vi.mock('../ipc/context', async importOriginal => {
  const original = await importOriginal<typeof import('../ipc/context')>();
  return { ...original, readDocumentAliases: mocks.readDocumentAliases, setDocumentAliases: mocks.setDocumentAliases };
});
vi.mock('../ipc/projects', () => ({ saveViewState: mocks.saveViewState }));
vi.mock('../ipc/proposals', () => ({ prepareContinuationProposal: vi.fn(), prepareProposal: vi.fn(), prepareStructuredProposal: vi.fn(), readProposals: vi.fn(async () => []) }));
vi.mock('../ipc/projectChat', () => ({ readProjectChapterFeedback: mocks.readProjectChapterFeedback }));
vi.mock('../assistant/FeedbackPanel', () => ({ FeedbackPanel: () => null }));
vi.mock('./HistoryPanel', () => ({ HistoryPanel: () => null }));
vi.mock('./ReviewPanel', () => ({ ReviewPanel: () => null }));
vi.mock('./ChapterMemory', () => ({ ChapterMemory: () => null }));
vi.mock('./RecoveryCopy', () => ({ RecoveryCopy: () => null }));

const access: ProjectAccess = { projectId: 'project-1', session: 'session-1', writerLease: 'lease-1', operationNamespace: 'writer-test' };

function body(text: string, id = 'paragraph-1'): WnsDocument {
  return { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id }, content: text ? [{ type: 'text', text }] : undefined }] } };
}
function textOf(document: WnsDocument): string {
  return document.body.content.flatMap(block => block.type === 'sceneBreak' ? [] : block.content?.flatMap(inline => inline.type === 'hardBreak' ? ['\n'] : [inline.text]) ?? []).join('');
}
function record(kind: string, title: string): DocumentRecord {
  return { head: { documentId: `${kind}-1`, version: '0', bodyHash: '' }, title, kind, metadataVersion: '0', body: body('initial manuscript'), lastCheckpointId: null };
}
function button(label: string): HTMLButtonElement {
  const result = [...host.querySelectorAll<HTMLButtonElement>('button')].find(item => item.textContent?.trim() === label);
  if (!result) throw new Error(`Missing button ${label}`);
  return result;
}
async function waitFor(check: () => void): Promise<void> {
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try { check(); return; } catch { await act(async () => new Promise(resolve => setTimeout(resolve, 5))); }
  }
  check();
}
function setTextValue(element: HTMLTextAreaElement, value: string): void {
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(element, value);
  element.dispatchEvent(new Event('input', { bubbles: true }));
}

async function makeHarness(kind: string): Promise<{ session: DocumentSession; record: DocumentRecord; saves: SaveSnapshot[]; transport: ProjectTransport }> {
  const initial = body('initial manuscript');
  const initialHash = await bodyHash(canonicalJson(initial));
  const current: DocumentRecord = { ...record(kind, kind === 'chapter' ? 'Chapter one' : 'Mira'), head: { documentId: `${kind}-1`, version: '0', bodyHash: initialHash }, body: initial };
  const saves: SaveSnapshot[] = [];
  const transport: ProjectTransport = {
    validate: vi.fn(async () => {}),
    save: vi.fn(async request => {
      saves.push(structuredClone(request));
      const hash = await bodyHash(canonicalJson(request.body));
      const changed = hash === request.expected.bodyHash ? 0n : 1n;
      current.head = { ...request.expected, version: (BigInt(request.expected.version) + changed).toString(), bodyHash: hash };
      current.body = structuredClone(request.body);
      const ack: SaveAck = { projectId: request.access.projectId, documentId: request.expected.documentId, session: request.access.session,
        operationNamespace: request.access.operationNamespace, operationId: request.operationId, head: { ...current.head }, savedGeneration: request.localGeneration };
      return ack;
    }),
    reconcile: vi.fn(async () => { throw new Error('Unexpected reconciliation in Writer test.'); }),
    checkpoint: vi.fn(async request => ({ id: 'checkpoint-1', head: { ...request.expected }, body: structuredClone(current.body), reason: request.reason, parentId: null })),
  };
  return { session: new DocumentSession(access, current, transport, { autosave: false, newId: () => `save-${saves.length + 1}` }), record: current, saves, transport };
}

let host: HTMLDivElement;
let root: Root;
let current: Awaited<ReturnType<typeof makeHarness>>;

async function render(): Promise<void> {
  await act(async () => root.render(<Writer active={{ record: current.record, session: current.session, viewState: null }} sources={[]} onError={vi.fn()} onRename={vi.fn()} />));
  await waitFor(() => expect(host.querySelector('[aria-label="Manuscript"]')).not.toBeNull());
}
async function editManuscript(value: string): Promise<void> {
  const prose = host.querySelector<HTMLElement>('.ProseMirror')!;
  prose.firstElementChild!.textContent = value;
  await act(async () => prose.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText' })));
  await waitFor(() => expect(textOf(current.session.body)).toBe(value));
}
async function openNames(): Promise<HTMLTextAreaElement> {
  await act(async () => button('Names & aliases').click());
  await waitFor(() => expect(host.querySelector('[aria-label="Names and aliases"] textarea')).not.toBeNull());
  return host.querySelector<HTMLTextAreaElement>('[aria-label="Names and aliases"] textarea')!;
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.clearAllMocks();
  mocks.readDocumentAliases.mockImplementation(async (_access: ProjectAccess, documentId: string) => ({ documentId, aliases: [], sourceEpoch: '7' }));
  mocks.setDocumentAliases.mockResolvedValue({ source: '8', policy: '0' });
  mocks.saveViewState.mockImplementation(async (_access: ProjectAccess, head: Head, anchor: ViewState['anchor'], focus: ViewState['focus']) => ({ documentId: head.documentId, head, anchor, focus }));
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe('Writer names and aliases lifecycle boundary', () => {
  it('blocks departure for an unfinished name draft without fencing the manuscript editor', async () => {
    current = await makeHarness('character');
    await render();
    await editManuscript('edited while names are open');
    const names = await openNames();
    await act(async () => setTextValue(names, 'Mira of the eastern gate'));

    const destination = vi.fn(async () => 'next-document');
    await act(async () => {
      await expect(current.session.detachAfter(destination)).rejects.toThrow(/name|alias/i);
    });

    expect(destination).not.toHaveBeenCalled();
    expect(current.session.state.phase).toBe('editing');
    expect(current.session.state.error).toBe(null);
    expect(current.session.state.dirty).toBe(false);
    expect(textOf(current.session.body)).toBe('edited while names are open');
    expect(host.querySelector<HTMLTextAreaElement>('[aria-label="Names and aliases"] textarea')?.value).toContain('Mira of the eastern gate');
    expect(current.saves).toHaveLength(1);
  });

  it('flushes the current manuscript before a successful name save and permits departure afterward', async () => {
    current = await makeHarness('world');
    await render();
    await editManuscript('current manuscript before names save');
    const names = await openNames();
    await act(async () => setTextValue(names, 'The eastern gate\nAsh Wren'));
    await act(async () => button('Save names').click());
    await waitFor(() => expect(mocks.setDocumentAliases).toHaveBeenCalledOnce());

    expect(mocks.setDocumentAliases.mock.calls[0].slice(0, 3)).toEqual([access, 'world-1', '7']);
    expect(new Set(mocks.setDocumentAliases.mock.calls[0][3])).toEqual(new Set(['The eastern gate', 'Ash Wren']));
    expect(current.saves).toHaveLength(1);
    expect(textOf(current.saves[0].body)).toBe('current manuscript before names save');
    expect(current.session.state.phase).toBe('editing');
    expect(current.session.state.error).toBe(null);
    expect(current.session.state.dirty).toBe(false);

    const destination = vi.fn(async () => 'next-document');
    await expect(current.session.detachAfter(destination)).resolves.toBe('next-document');
    expect(destination).toHaveBeenCalledOnce();
    expect(current.session.state.phase).toBe('disposed');
  });

  it('does not expose names and aliases for a chapter document', async () => {
    current = await makeHarness('chapter');
    await render();
    expect(host.textContent).not.toContain('Names & aliases');
    expect(mocks.readDocumentAliases).not.toHaveBeenCalled();
  });
});

describe('Writer project conversation bridge', () => {
  it('flushes the real editor and captures exact whole-chapter revision scope without sending a request', async () => {
    current = await makeHarness('chapter');
    const stageChapter = vi.fn(async (_task: ProjectChapterComposer) => {});
    const bridge = { stageChapter, attachSource: vi.fn(async () => {}), reviewRunId: null };
    const onError = vi.fn();
    const props = { active: { record: current.record, session: current.session, viewState: null }, sources: [], onError, onRename: vi.fn(), conversation: bridge };
    await act(async () => root.render(<Writer {...props} />));
    const mounted = host.querySelector('.ProseMirror');
    await editManuscript('The city waited for the healer.');
    await act(async () => button('Suggest chapter changes').click());
    await waitFor(() => expect(stageChapter).toHaveBeenCalledOnce());
    expect(stageChapter.mock.calls[0][0]).toMatchObject({ target: current.session.state.head, intent: 'proposeEdits', scope: { kind: 'wholeDocument', quote: 'The city waited for the healer.', sourceBodyHash: current.session.state.head.bodyHash } });
    expect(current.session.state.dirty).toBe(false);
    expect(onError).not.toHaveBeenCalled();
    await act(async () => root.render(<Writer {...props} conversation={{ ...bridge }} />));
    expect(host.querySelector('.ProseMirror')).toBe(mounted);
    expect(textOf(current.session.body)).toBe('The city waited for the healer.');
  });

  it('attaches the saved world head to chat while leaving the world directly editable', async () => {
    current = await makeHarness('world');
    const attachSource = vi.fn(async () => {});
    const stageChapter = vi.fn(async () => {});
    await act(async () => root.render(<Writer active={{ record: current.record, session: current.session, viewState: null }} sources={[]} onError={vi.fn()} onRename={vi.fn()} conversation={{ attachSource, stageChapter, reviewRunId: null }} />));
    await editManuscript('The harbor has seven bridges.');
    await act(async () => button('Discuss in project chat').click());
    await waitFor(() => expect(attachSource).toHaveBeenCalledOnce());
    expect(attachSource).toHaveBeenCalledWith(current.session.state.head);
    expect(stageChapter).not.toHaveBeenCalled();
    expect(current.session.state.editable).toBe(true);
  });

  it('reviews and confirms a suggested passage into the composer without changing the mounted editor', async () => {
    current = await makeHarness('chapter');
    const feedback = { runId: 'feedback-run', target: current.record.head, answer: 'Focus the tension here.', rangeProposal: { sourceHead: current.record.head, firstBlockId: 'paragraph-1', lastBlockId: 'paragraph-1', quote: 'initial manuscript' } };
    mocks.readProjectChapterFeedback.mockResolvedValue(feedback);
    const stageChapter = vi.fn(async (_task: ProjectChapterComposer) => {});
    await act(async () => root.render(<Writer active={{ record: current.record, session: current.session, viewState: null }} sources={[]} onError={vi.fn()} onRename={vi.fn()} conversation={{ attachSource: vi.fn(), stageChapter, reviewRunId: feedback.runId }} />));
    await waitFor(() => expect(host.textContent).toContain('Use this passage for an edit'));
    const editor = host.querySelector('.ProseMirror');
    expect(stageChapter).not.toHaveBeenCalled();
    await act(async () => button('Use this passage for an edit').click());
    await waitFor(() => expect(stageChapter).toHaveBeenCalledOnce());
    expect(mocks.readProjectChapterFeedback).toHaveBeenCalledTimes(2);
    expect(stageChapter.mock.calls[0][0]).toMatchObject({ target: current.record.head, intent: 'proposeEdits', scope: { kind: 'blocks', quote: 'initial manuscript' } });
    expect(current.saves).toHaveLength(0); expect(host.querySelector('.ProseMirror')).toBe(editor);
    expect(host.textContent).toContain('Passage confirmed in the composer');
  });
});
