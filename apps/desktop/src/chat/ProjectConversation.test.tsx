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
const setChatDisposition = vi.hoisted(() => vi.fn());

// The chat renders the editor itself; this suite is about the chat around it.
vi.mock('../editor/Writer', () => ({ Writer: () => <div data-testid="mounted-editor">Working text stays mounted.</div> }));
vi.mock('../ipc/projectChat', async () => {
  const actual = await vi.importActual<typeof import('../ipc/projectChat')>('../ipc/projectChat');
  return { ...actual, readProjectConversation, saveProjectComposer, startProjectChapter, setChatDisposition };
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
  setChatDisposition.mockResolvedValue({ id: 'decision-new', sequence: '3', kind: 'chatDisposition', referenceId: 'run-1:q1', payload: { referenceId: 'run-1:q1', version: '5', disposition: 'reconsider' }, createdAt: '2026-09-09T00:02:00.000Z' });
  readProjectConversation.mockResolvedValue({
    id: 'conversation-1', composer: { conversationId: 'conversation-1', version: '0', body: emptyProjectComposer() },
    items: [{ id: 'item-1', sequence: '1', kind: 'request', referenceId: 'run-1', createdAt: '2026-09-09T00:00:00.000Z', payload: { instruction: 'Explain this scene.', userMessageId: 'user-message-1', assistantMessageId: 'assistant-message-1', run: completedRun() } }],
    olderBefore: null, activeRun: null, drafts: [], sourceEpoch: '1', policyEpoch: '1', earlierWorkshop: false, workerIssues: [],
  } satisfies ProjectConversationView);
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});

afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks(); vi.clearAllMocks(); });

describe('ProjectConversation context inspection', () => {
  it('keeps the captured chapter task visible while sources and details collapse', async () => {
    const view = await readProjectConversation();
    view.composer.body.chapter = {
      target: head,
      intent: 'continue',
      basis: 'working',
      scope: { kind: 'passage', start: null, end: null, quote: 'the final exchange', sourceBodyHash: head.bodyHash },
      safeBrief: null,
    };
    readProjectConversation.mockResolvedValue(view);
    await act(async () => root.render(<ProjectConversation project={project} onOpenDocument={() => {}} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    const chapterContext = host.querySelector<HTMLElement>('.chat-chapter-context')!;
    const details = host.querySelector<HTMLDetailsElement>('.coauthor-composer-details')!;
    expect(chapterContext.closest('details')).toBeNull();
    expect(chapterContext.textContent).toContain('the final exchange');
    expect(details.open).toBe(false);
    const summary = details.querySelector('summary')!;
    await act(async () => summary.dispatchEvent(new MouseEvent('click', { bubbles: true })));
    expect(details.open).toBe(true);
    await act(async () => summary.dispatchEvent(new MouseEvent('click', { bubbles: true })));
    expect(details.open).toBe(false);
    expect(chapterContext.isConnected).toBe(true);
  });

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

  it('opens the latest pending drafts in review when no ordinary document is active', async () => {
    const view = await readProjectConversation();
    const draft = {
      document: { ...chapterDocument, head: { ...head, documentId: 'draft-latest' }, title: 'Latest world note', kind: 'world', role: 'assistantDraft' },
      conversationId: 'conversation-1', originRunId: 'run-1', packetId: 'packet-exact', initialRevisionId: 'revision-latest',
      target: null, disposition: 'pending', dispositionVersion: '1', stale: false,
    };
    readProjectConversation.mockResolvedValue({ ...view, drafts: [draft] });
    await act(async () => root.render(<ProjectConversation project={project} onOpenDocument={() => {}} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    expect(host.querySelector('.chat-review-view')?.classList.contains('is-hidden')).toBe(false);
    expect(host.textContent).toContain('Latest world note');
    expect(host.textContent).toContain('Drafts to review');
    const documentTab = [...host.querySelectorAll<HTMLButtonElement>('.chat-right-mode-tabs button')].find(button => button.textContent === 'Document')!;
    await act(async () => documentTab.click());
    expect(host.querySelector('.chat-document-view')?.classList.contains('is-hidden')).toBe(false);
    expect(host.querySelector('.chat-review-view')?.classList.contains('is-hidden')).toBe(true);
  });

  it('keeps document mode and the mounted editor when an ordinary document is active', async () => {
    const view = await readProjectConversation();
    const draft = {
      document: { ...chapterDocument, head: { ...head, documentId: 'draft-background' }, title: 'Background candidate', kind: 'world', role: 'assistantDraft' },
      conversationId: 'conversation-1', originRunId: 'run-1', packetId: 'packet-exact', initialRevisionId: 'revision-background',
      target: null, disposition: 'pending', dispositionVersion: '1', stale: false,
    };
    readProjectConversation.mockResolvedValue({ ...view, drafts: [draft] });
    await act(async () => root.render(<ProjectConversation project={project} activeDocument={chapterDocument} editor={{ active: { record: chapterDocument, session: null as never, viewState: null }, sources: [], onError: () => {}, onRename: () => {} }} onOpenDocument={() => {}} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    expect(host.querySelector('.chat-document-view')?.classList.contains('is-hidden')).toBe(false);
    expect(host.querySelector('.chat-review-view')?.classList.contains('is-hidden')).toBe(true);
    expect(host.querySelector('[data-testid="mounted-editor"]')?.textContent).toBe('Working text stays mounted.');
    const reviewTab = [...host.querySelectorAll<HTMLButtonElement>('.chat-right-mode-tabs button')].find(button => button.textContent?.startsWith('Review drafts'))!;
    await act(async () => reviewTab.click());
    expect(host.querySelector('.chat-review-view')?.classList.contains('is-hidden')).toBe(false);
    expect(host.querySelector('[data-testid="mounted-editor"]')).not.toBeNull();
  });

  it('links materialized drafts inline to the isolated review surface', async () => {
    const view = await readProjectConversation();
    const draft = {
      document: { ...chapterDocument, head: { ...head, documentId: 'draft-inline' }, title: 'Inline world draft', kind: 'world', role: 'assistantDraft' },
      conversationId: 'conversation-1', originRunId: 'run-1', packetId: 'packet-exact', initialRevisionId: 'revision-inline',
      target: null, disposition: 'pending', dispositionVersion: '1', stale: false,
    };
    const materialization = { id: 'materialized-1', sequence: '2', kind: 'materializeChatResult', referenceId: 'run-1', payload: { outputValid: true, draftRefs: [{ ordinal: 0, documentId: 'draft-inline' }] }, createdAt: '2026-09-09T00:01:00.000Z' };
    readProjectConversation.mockResolvedValue({ ...view, drafts: [draft], items: [...view.items, materialization] });
    const focus = vi.spyOn(HTMLElement.prototype, 'focus').mockImplementation(() => {});
    await act(async () => root.render(<ProjectConversation project={project} onOpenDocument={() => {}} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    const link = [...host.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent?.includes('Inline world draft'))!;
    expect(link).toBeDefined();
    await act(async () => link.click());
    expect(host.querySelector('.chat-review-view')?.classList.contains('is-hidden')).toBe(false);
    expect(host.querySelector('[data-draft-id="draft-inline"]')).not.toBeNull();
    focus.mockRestore();
  });

  it('passes the immutable run packet id to the existing ContextInspector', async () => {
    await act(async () => root.render(<ProjectConversation project={project} onOpenDocument={() => {}} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    expect(host.querySelector('[data-testid="context-inspector"]')?.textContent).toBe('packet-exact');
  });

  it('stages a direct assumption correction in the unsent composer without sending', async () => {
    const view = await readProjectConversation();
    const run = completedRun();
    run.outputText = JSON.stringify({ schemaVersion: 'project-assistant-output.v1', answer: 'A readable answer.', questions: [], assumptions: [{ key: 'voice', text: 'Use a restrained voice.' }] });
    view.items[0].payload.run = run;
    view.composer.body.text = 'Keep my existing direction.';
    readProjectConversation.mockResolvedValue(view);
    const ref = createRef<ProjectConversationHandle>();
    await act(async () => root.render(<ProjectConversation ref={ref} project={project} onOpenDocument={() => {}} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Edit assumption')!.click());
    const editor = host.querySelector<HTMLTextAreaElement>('[aria-label="Correct proposed assumption"]')!;
    expect(editor.value).toBe('Use a restrained voice.');
    Object.getOwnPropertyDescriptor(Object.getPrototypeOf(editor), 'value')!.set!.call(editor, 'Use a warm, direct voice.');
    await act(async () => editor.dispatchEvent(new Event('input', { bubbles: true })));
    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Use correction for next draft')!.click());
    await act(async () => ref.current!.flush());
    const saved = saveProjectComposer.mock.calls.at(-1)![0].body.text as string;
    expect(saved).toContain('Keep my existing direction.');
    expect(saved).toContain('Correction for the next draft.');
    expect(saved).toContain('Original assumption: “Use a restrained voice.”');
    expect(saved).toContain('Author correction: Use a warm, direct voice.');
    expect(setChatDisposition).not.toHaveBeenCalled();
  });

  it('cancels an assumption edit without changing the composer', async () => {
    const view = await readProjectConversation();
    const run = completedRun();
    run.outputText = JSON.stringify({ schemaVersion: 'project-assistant-output.v1', answer: 'A readable answer.', questions: [], assumptions: [{ key: 'voice', text: 'Use a restrained voice.' }] });
    view.items[0].payload.run = run;
    view.composer.body.text = 'Keep this exact text.';
    readProjectConversation.mockResolvedValue(view);
    const ref = createRef<ProjectConversationHandle>();
    await act(async () => root.render(<ProjectConversation ref={ref} project={project} onOpenDocument={() => {}} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Edit assumption')!.click());
    const editor = host.querySelector<HTMLTextAreaElement>('[aria-label="Correct proposed assumption"]')!;
    Object.getOwnPropertyDescriptor(Object.getPrototypeOf(editor), 'value')!.set!.call(editor, 'This should be discarded.');
    await act(async () => editor.dispatchEvent(new Event('input', { bubbles: true })));
    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Cancel')!.click());
    await act(async () => ref.current!.flush());
    expect(saveProjectComposer).not.toHaveBeenCalled();
    expect(setChatDisposition).not.toHaveBeenCalled();
  });

  it('refuses to stage an author-room correction while a chapter task is active', async () => {
    const view = await readProjectConversation();
    const run = completedRun();
    run.outputText = JSON.stringify({ schemaVersion: 'project-assistant-output.v1', answer: 'A readable answer.', questions: [], assumptions: [{ key: 'voice', text: 'Use a restrained voice.' }] });
    view.items[0].payload.run = run;
    view.composer.body.text = 'Keep the chapter task request.';
    view.composer.body.chapter = { target: head, intent: 'discuss', basis: 'working', scope: null, safeBrief: null };
    readProjectConversation.mockResolvedValue(view);
    const ref = createRef<ProjectConversationHandle>();
    await act(async () => root.render(<ProjectConversation ref={ref} project={project} onOpenDocument={() => {}} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Edit assumption')!.click());
    const editor = host.querySelector<HTMLTextAreaElement>('[aria-label="Correct proposed assumption"]')!;
    Object.getOwnPropertyDescriptor(Object.getPrototypeOf(editor), 'value')!.set!.call(editor, 'Do not change the chapter task.');
    await act(async () => editor.dispatchEvent(new Event('input', { bubbles: true })));
    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Use correction for next draft')!.click());
    await act(async () => ref.current!.flush());
    expect(saveProjectComposer).not.toHaveBeenCalled();
    expect(setChatDisposition).not.toHaveBeenCalled();
    expect(host.textContent).toContain('Return to the project conversation');
    expect(host.textContent).toContain('Keep the chapter task request.');
  });

  it('projects persisted decision scope, unknown audience, rationale, and reconsiders with the same scope/version', async () => {
    const view = await readProjectConversation();
    const run = completedRun();
    run.outputText = JSON.stringify({ schemaVersion: 'project-assistant-output.v1', answer: 'A readable answer.', questions: [{ key: 'q1', text: 'Should the hidden motive remain unrevealed?' }], assumptions: [] });
    view.items[0].payload.run = run;
    view.items.push({ id: 'decision-1', sequence: '2', kind: 'chatDisposition', referenceId: 'run-1:q1', payload: { referenceId: 'run-1:q1', version: '4', disposition: 'keepMysterious', rationale: 'Preserve uncertainty until the reveal.', scope: { kind: 'task', referenceId: 'run-1' }, unknownTo: 'both' }, createdAt: '2026-09-09T00:01:00.000Z' });
    readProjectConversation.mockResolvedValue(view);
    await act(async () => root.render(<ProjectConversation project={project} onOpenDocument={() => {}} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    const question = host.querySelector('.chat-question')!;
    expect(question.querySelector('select')?.value).toBe('task');
    expect(host.textContent).toContain('Response kept mysterious.');
    expect(host.textContent).toContain('Response version 4');
    expect(host.textContent).toContain('Scope: This request · run-1');
    expect(host.textContent).toContain('Unknown to: Author and reader');
    expect(host.textContent).toContain('Rationale: Preserve uncertainty until the reveal.');
    await act(async () => { [...host.querySelectorAll('button')].find(button => button.textContent === 'Reconsider this response')!.click(); await Promise.resolve(); });
    expect(setChatDisposition).toHaveBeenCalledWith(access, 'conversation-1', 'run-1:q1', '4', 'reconsider', '', expect.any(String), { scope: { kind: 'task', referenceId: 'run-1' } });
  });

  it('keeps each historical disposition tied to its own recorded scope and version', async () => {
    const view = await readProjectConversation();
    view.items.push(
      { id: 'decision-old', sequence: '2', kind: 'chatDisposition', referenceId: 'run-1:q1', payload: { version: '1', disposition: 'notRelevant', scope: { kind: 'project' }, rationale: 'Earlier project decision.' }, createdAt: '' },
      { id: 'decision-new', sequence: '3', kind: 'chatDisposition', referenceId: 'run-1:q1', payload: { version: '2', disposition: 'notNow', scope: { kind: 'task', referenceId: 'run-1' }, rationale: 'Later task decision.' }, createdAt: '' },
    );
    readProjectConversation.mockResolvedValue(view);
    await act(async () => root.render(<ProjectConversation project={project} onOpenDocument={() => {}} onDocumentsChanged={() => {}} onEarlierWorkshop={() => {}} />));
    const old = host.querySelector('[data-conversation-item-id="decision-old"]')!;
    const current = host.querySelector('[data-conversation-item-id="decision-new"]')!;
    expect(old.textContent).toContain('Response version 1');
    expect(old.textContent).toContain('Scope: Project');
    expect(old.textContent).not.toContain('Later task decision');
    expect(old.querySelector('button')).toBeNull();
    expect(current.textContent).toContain('Response version 2');
    expect(current.textContent).toContain('Scope: This request');
    expect(current.querySelector('button')?.textContent).toBe('Reconsider this response');
  });
});
