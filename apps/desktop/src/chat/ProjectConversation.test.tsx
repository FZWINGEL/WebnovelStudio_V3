// @vitest-environment jsdom
import { act, createRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DiscussionRun } from '../ipc/discussions';
import { emptyProjectComposer, type ProjectConversationView } from '../ipc/projectChat';
import type { DocumentRecord, OpenedProject, ProjectAccess } from '../ipc/projects';

const readProjectConversation = vi.hoisted(() => vi.fn());
const saveProjectComposer = vi.hoisted(() => vi.fn());
const startProjectChapter = vi.hoisted(() => vi.fn());

vi.mock('../ipc/projectChat', async () => {
  const actual = await vi.importActual<typeof import('../ipc/projectChat')>('../ipc/projectChat');
  return { ...actual, readProjectConversation, saveProjectComposer, startProjectChapter };
});

vi.mock('../assistant/ContextInspector', () => ({
  ContextInspector: ({ packetId }: { packetId: string }) => <div data-testid="context-inspector">{packetId}</div>,
}));

vi.mock('../providers/ProviderContext', () => ({
  useProviders: () => ({
    state: {
      settings: { revision: '1', active: { providerId: 'mock', modelId: 'mock-story-context', reasoning: null, serviceTier: null }, favorites: [] },
      dispatch: { kind: 'localMock', detail: '' },
      catalog: { models: [] },
    },
    busy: false,
  }),
}));

import { ProjectConversation, type ProjectConversationHandle } from './ProjectConversation';
import * as briefContract from './brief';

const access: ProjectAccess = { projectId: 'project-1', operationNamespace: 'namespace-1', session: 'session-1', writerLease: 'lease-1' };
const head = { documentId: 'chapter-1', version: '2', bodyHash: 'a'.repeat(64) };
const chapterDocument: DocumentRecord = {
  head,
  title: 'Chapter One',
  kind: 'chapter',
  metadataVersion: '1',
  body: { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'p1' }, content: [{ type: 'text', text: 'A chapter.' }] }] } },
  lastCheckpointId: null,
  role: 'ordinary',
};
const project: OpenedProject = {
  project: { projectId: access.projectId, operationNamespace: access.operationNamespace, title: 'Test project', formatVersion: 1 },
  access,
  documents: [chapterDocument],
  metadataVersion: '1',
  viewState: null,
  libraryWarning: null,
};

function completedRun(): DiscussionRun {
  return {
    id: 'run-1', threadId: 'thread-1', owner: { projectId: access.projectId, operationNamespace: access.operationNamespace, runId: 'run-1' },
    operationId: 'operation-1', intent: 'discuss', basis: null, payloadHash: 'b'.repeat(64), target: head,
    packetId: 'packet-exact', previousRunId: null, status: 'completed', dispatchState: 'delivered', sequence: '1',
    outputText: JSON.stringify({ schemaVersion: 'project-assistant-output.v1', answer: 'A readable answer.', questions: [], assumptions: [] }),
    stopReason: null, createdAt: '2026-09-09T00:00:00.000Z', updatedAt: '2026-09-09T00:00:01.000Z',
  };
}

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  localStorage.clear();
  saveProjectComposer.mockImplementation(async request => ({ conversationId: request.conversationId, version: String(BigInt(request.expectedVersion) + 1n), body: request.body }));
  readProjectConversation.mockResolvedValue({
    id: 'conversation-1', composer: { conversationId: 'conversation-1', version: '0', body: emptyProjectComposer() },
    items: [{ id: 'item-1', sequence: '1', kind: 'request', referenceId: 'run-1', createdAt: '2026-09-09T00:00:00.000Z', payload: { instruction: 'Explain this scene.', userMessageId: 'user-message-1', assistantMessageId: 'assistant-message-1', run: completedRun() } }],
    olderBefore: null, activeRun: null, drafts: [], sourceEpoch: '1', policyEpoch: '1', earlierWorkshop: false, workerIssues: [],
  } satisfies ProjectConversationView);
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});

afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks(); vi.clearAllMocks(); });

describe('ProjectConversation context inspection', () => {
  it('invalidates a pending brief approval when chapter basis changes even if it changes back', async () => {
    const view = await readProjectConversation();
    const brief = { text: 'Preserve the hopeful ending.', originMessageId: null, confirmed: false };
    view.composer.body.chapter = { target: head, intent: 'continue', basis: 'working', scope: null, safeBrief: brief };
    readProjectConversation.mockResolvedValue(view);
    let finish!: (value: typeof brief) => void;
    vi.spyOn(briefContract, 'approveChapterBrief').mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
    const ref = createRef<ProjectConversationHandle>();
    await act(async () => root.render(<ProjectConversation ref={ref} project={project} onOpenDocument={() => {}} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Edit brief')!.click());
    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Approve this brief')!.click());
    const basis = [...host.querySelectorAll('label')].find(label => label.textContent?.startsWith('Continuation basis'))!.querySelector('select')!;
    await act(async () => { basis.value = 'reviewed'; basis.dispatchEvent(new Event('change', { bubbles: true })); });
    await act(async () => { basis.value = 'working'; basis.dispatchEvent(new Event('change', { bubbles: true })); });
    await act(async () => finish({ ...brief, confirmed: true }));
    await act(async () => ref.current!.flush());
    expect(saveProjectComposer.mock.calls.at(-1)![0].body.chapter.safeBrief.confirmed).toBe(false);
    expect(host.textContent).toContain('Writing brief needs approval');
  });

  it('does not save a late brief approval after switching projects', async () => {
    const view = await readProjectConversation();
    const brief = { text: 'Preserve the hopeful ending.', originMessageId: null, confirmed: false };
    view.composer.body.chapter = { target: head, intent: 'continue', basis: 'working', scope: null, safeBrief: brief };
    readProjectConversation.mockResolvedValue(view);
    let finish!: (value: typeof brief) => void;
    vi.spyOn(briefContract, 'approveChapterBrief').mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
    await act(async () => root.render(<ProjectConversation key="first" project={project} onOpenDocument={() => {}} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Edit brief')!.click());
    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Approve this brief')!.click());
    const nextProject = { ...project, project: { ...project.project, projectId: 'project-2', title: 'Second project' }, access: { ...access, projectId: 'project-2', session: 'session-2' } };
    readProjectConversation.mockResolvedValue({ ...view, id: 'conversation-2', composer: { conversationId: 'conversation-2', version: '0', body: emptyProjectComposer() }, items: [] });
    const ref = createRef<ProjectConversationHandle>();
    await act(async () => root.render(<ProjectConversation key="second" ref={ref} project={nextProject} onOpenDocument={() => {}} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    await act(async () => finish({ ...brief, confirmed: true }));
    await act(async () => ref.current!.flush());
    expect(saveProjectComposer).not.toHaveBeenCalled();
    expect(host.textContent).not.toContain('Writing brief approved');
  });

  it('requires a chosen chapter and separate brief approval without sending or replacing newer typing', async () => {
    const view = await readProjectConversation();
    const run = completedRun();
    run.outputText = JSON.stringify({ schemaVersion: 'project-assistant-output.v1', answer: 'A chapter could begin here.', questions: [], assumptions: [], drafts: [], chapterHandoff: {
      targetHandle: 'a-frozen-chapter-handle', proposedTitle: 'Cloud bridge', instruction: 'Begin the journey.', brief: 'Keep the ending hopeful.',
    } });
    view.items[0].payload.run = run;
    view.composer.body.text = 'My newer unsent thought.';
    readProjectConversation.mockResolvedValue(view);
    const prepare = vi.fn(async () => chapterDocument);
    await act(async () => root.render(<ProjectConversation project={project} onOpenDocument={() => {}} onPrepareChapter={prepare} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    expect(prepare).not.toHaveBeenCalled();
    const button = [...host.querySelectorAll('button')].find(button => button.textContent === 'Prepare chapter continuation')!;
    expect(button.disabled).toBe(true);
    const destination = host.querySelector<HTMLSelectElement>('[aria-label="Chapter destination"]')!;
    await act(async () => { destination.value = head.documentId; destination.dispatchEvent(new Event('change', { bubbles: true })); });
    await act(async () => button.click());
    expect(prepare).toHaveBeenCalledWith(head.documentId, 'Cloud bridge');
    expect(startProjectChapter).not.toHaveBeenCalled();
    const saved = saveProjectComposer.mock.calls.at(-1)![0].body;
    expect(saved.text).toBe('My newer unsent thought.');
    expect(saved.chapter.target).toEqual(head);
    expect(saved.chapter.intent).toBe('continue');
    expect(saved.chapter.safeBrief.confirmed).toBe(false);
    expect(saved.chapter.safeBrief.projectOrigin.messageId).toBe('assistant-message-1');
    expect(host.textContent).toContain('Approve this brief');
  });

  it('opens a draft decision in isolated review instead of the ordinary Writer', async () => {
    const view = await readProjectConversation();
    const draft = {
      document: { ...chapterDocument, head: { ...head, documentId: 'draft-1' }, kind: 'world', role: 'assistantDraft' },
      conversationId: 'conversation-1', originRunId: 'run-1', packetId: 'packet-exact', initialRevisionId: 'revision-1',
      target: null, disposition: 'rejected', dispositionVersion: '1', stale: false,
    };
    readProjectConversation.mockResolvedValue({ ...view, drafts: [draft], items: [...view.items, {
      id: 'decision-1', sequence: '2', kind: 'chatDisposition', referenceId: 'draft-1',
      payload: { disposition: 'rejected' }, createdAt: '2026-09-09T00:01:00Z',
    }] });
    const openOrdinaryDocument = vi.fn();
    await act(async () => root.render(<ProjectConversation project={project} onOpenDocument={openOrdinaryDocument} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    const openDraft = [...host.querySelectorAll('button')].find(button => button.textContent === 'Open affected draft');
    expect(openDraft).toBeDefined();
    await act(async () => openDraft!.click());
    expect(openOrdinaryDocument).not.toHaveBeenCalled();
    expect(host.querySelector('.chat-review-view')?.classList.contains('is-hidden')).toBe(false);
    expect(host.querySelector('[data-draft-id="draft-1"]')).not.toBeNull();
    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Back to chat')!.click());
    const surfaces = host.querySelector('[aria-label="Project workspace surfaces"]')!;
    await act(async () => [...surfaces.querySelectorAll('button')].find(button => button.textContent === 'Documents')!.click());
    expect(host.querySelector('.chat-document-view')?.classList.contains('is-hidden')).toBe(false);
    expect(host.querySelector('.chat-review-view')?.classList.contains('is-hidden')).toBe(true);
    expect(host.querySelector('.chat-document-surface')?.classList.contains('chat-mobile-documents')).toBe(true);
    await act(async () => [...surfaces.querySelectorAll('button')].find(button => button.textContent === 'Chapter')!.click());
    expect(host.querySelector('.chat-document-view')?.classList.contains('is-hidden')).toBe(false);
    expect(host.querySelector('.chat-document-surface')?.classList.contains('chat-mobile-chapter')).toBe(true);
  });

  it('passes the immutable run packet id to the existing ContextInspector', async () => {
    await act(async () => root.render(<ProjectConversation project={project} onOpenDocument={() => {}} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    expect(host.querySelector('[data-testid="context-inspector"]')?.textContent).toBe('packet-exact');
  });
});
