// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FeedbackPanel } from './FeedbackPanel';
import * as discussions from '../ipc/discussions';
import { bodyHash, canonicalJson, type WnsDocument } from '../editor/document';
import { DocumentSession } from '../editor/session';
import type { DocumentRecord, Head, ProjectAccess, ProjectTransport } from '../ipc/projects';

vi.mock('../ipc/discussions', () => ({
  readDiscussion: vi.fn(),
  saveDiscussionDraft: vi.fn(),
  startDiscussion: vi.fn(),
  stopDiscussion: vi.fn(),
}));
vi.mock('./ContextInspector', () => ({ ContextInspector: () => <div data-testid="context-inspector" /> }));
vi.mock('./GuidancePanel', () => ({ GuidancePanel: () => <div data-testid="guidance-panel" /> }));

const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const emptyBody: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'paragraph-1' }, content: [{ type: 'text', text: 'A quiet chapter.' }] }] } };

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((accept, fail) => { resolve = accept; reject = fail; });
  return { promise, resolve, reject };
}

async function makeSession(documentId = 'document') {
  const hash = await bodyHash(canonicalJson(emptyBody));
  const head: Head = { documentId, version: '0', bodyHash: hash };
  const document: DocumentRecord = { head, title: 'Chapter', kind: 'chapter', metadataVersion: '0', body: emptyBody, lastCheckpointId: null };
  const transport: ProjectTransport = {
    validate: async () => {},
    save: async request => ({ projectId: request.access.projectId, documentId, session: request.access.session, operationNamespace: request.access.operationNamespace, operationId: request.operationId, head: request.expected, savedGeneration: request.localGeneration }),
    reconcile: async request => ({ access: { ...access, projectId: request.projectId, operationNamespace: request.operationNamespace, writerLease: 'fresh-lease' }, document, receipts: [] }),
    checkpoint: async request => ({ id: 'checkpoint', head: request.expected, body: emptyBody, reason: request.reason, parentId: null }),
  };
  return new DocumentSession({ ...access, projectId: documentId === 'document-b' ? 'project-b' : access.projectId }, document, transport, { autosave: false });
}

function emptyView(documentId: string): discussions.DiscussionView {
  return { documentId, threadId: null, messages: [], runs: [], draft: null };
}

function startResult(session: DocumentSession, runId = 'run-1', operationId = 'operation-from-request'): discussions.DiscussionStart {
  const requestHead = session.state.head;
  const owner = { projectId: session.projectAccess.projectId, operationNamespace: session.projectAccess.operationNamespace, runId };
  const packetId = `packet-${runId}`;
  return {
    threadId: `thread-${runId}`,
    userMessage: { id: `message-${runId}`, threadId: `thread-${runId}`, runId, role: 'user', content: 'Make this moment more emotional.', scope: null, packetId, createdAt: 'now' },
    run: { id: runId, threadId: `thread-${runId}`, owner, operationId, payloadHash: 'payload', target: requestHead, packetId, previousRunId: null, status: 'queued', dispatchState: 'pending', sequence: '0', outputText: '', stopReason: null, createdAt: 'now', updatedAt: 'now' },
    packet: { messages: [], options: { modelId: 'mock-story-context', maxOutputTokens: '100', tokenAccountingMethod: 'mock' }, receipt: { packetId, sessionId: 'context', snapshotId: 'snapshot', invocationOrdinal: '0', sourceHandles: [], coverage: [], omissions: [], inputHash: 'input', inputTokens: '1', tokenAccountingMethod: 'mock' } },
  };
}

let host: HTMLDivElement;
let root: Root;

async function renderPanel(session: DocumentSession) {
  await act(async () => root.render(<FeedbackPanel session={session} state={session.state} title="Chapter" selection={null} visible onClose={() => {}} registerSaver={() => {}} />));
  await waitFor(() => expect(host.querySelector('#discussion-composer')).not.toBeNull());
}

async function waitFor<T>(assertion: () => T) {
  await act(async () => { await vi.waitFor(assertion); });
}

async function typeInstruction(text: string) {
  const textarea = host.querySelector('#discussion-composer') as HTMLTextAreaElement;
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!;
    setter.call(textarea, text);
    textarea.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await waitFor(() => expect((host.querySelector('#discussion-composer') as HTMLTextAreaElement).value).toBe(text));
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.resetAllMocks();
  vi.mocked(discussions.readDiscussion).mockImplementation(async (_access, documentId) => emptyView(documentId));
  vi.mocked(discussions.saveDiscussionDraft).mockImplementation(async request => ({ documentId: request.documentId, version: (BigInt(request.expectedVersion) + 1n).toString(), text: request.text, scope: request.scope, pinnedDocumentIds: request.pinnedDocumentIds, updatedAt: 'now' }));
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe('persistent FeedbackPanel safeguards', () => {
  it('retries a lost start acknowledgment with the same operation and payload while retaining the composer', async () => {
    const session = await makeSession();
    vi.mocked(discussions.startDiscussion).mockImplementationOnce(async () => { throw { code: 'UncertainOutcome', detail: 'The start response was lost.' }; }).mockImplementationOnce(async request => startResult(session, 'run-1', request.operationId));
    await renderPanel(session);
    await typeInstruction('Make this moment more emotional.');
    await act(async () => (Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Send') as HTMLButtonElement).click());
    await waitFor(() => expect(host.textContent).toContain('Check the request before sending another.'));
    expect((host.querySelector('#discussion-composer') as HTMLTextAreaElement).value).toBe('Make this moment more emotional.');
    const originalRequest = vi.mocked(discussions.startDiscussion).mock.calls[0][0];
    await act(async () => (Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Check request') as HTMLButtonElement).click());
    await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledTimes(2));
    const retryRequest = vi.mocked(discussions.startDiscussion).mock.calls[1][0];
    expect({ ...retryRequest, access: originalRequest.access }).toEqual(originalRequest);
    expect(retryRequest.access.writerLease).toBe('fresh-lease');
  });

  it('does not let a late start response update a switched destination', async () => {
    const source = await makeSession('document-a');
    const destination = await makeSession('document-b');
    const gate = deferred<discussions.DiscussionStart>();
    const sourceRefresh = deferred<discussions.DiscussionView>();
    const destinationRefresh = deferred<discussions.DiscussionView>();
    const destinationInitial = emptyView('document-b');
    destinationInitial.messages = [{ id: 'destination-message', threadId: 'destination-thread', runId: null, role: 'user', content: 'Destination history is loaded.', scope: null, packetId: null, createdAt: 'now' }];
    let destinationReads = 0;
    let sourceReads = 0;
    vi.mocked(discussions.readDiscussion).mockImplementation(async (_access, documentId) => {
      if (documentId === 'document-b' && ++destinationReads > 1) return destinationRefresh.promise;
      if (documentId === 'document-a') sourceReads += 1;
      if (documentId === 'document-a' && sourceReads > 1) return sourceRefresh.promise;
      return documentId === 'document-b' ? destinationInitial : emptyView(documentId);
    });
    vi.mocked(discussions.startDiscussion).mockReturnValue(gate.promise);
    await renderPanel(source);
    await typeInstruction('Old destination instruction.');
    await act(async () => (Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Send') as HTMLButtonElement).click());
    await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledTimes(1));
    await act(async () => root.render(<FeedbackPanel session={destination} state={destination.state} title="Destination" selection={null} visible registerSaver={() => {}} onClose={() => {}} />));
    await waitFor(() => expect(destinationReads).toBe(1));
    await waitFor(() => expect(host.textContent).toContain('Destination history is loaded.'));
    const originalRequest = vi.mocked(discussions.startDiscussion).mock.calls[0][0];
    await act(async () => { gate.resolve(startResult(source, 'late-run', originalRequest.operationId)); await Promise.resolve(); await Promise.resolve(); });
    expect({
      sourceReads,
      destinationHistory: host.textContent.includes('Destination history is loaded.'),
      leakedSourceMessage: host.textContent.includes('Make this moment more emotional.'),
    }).toEqual({ sourceReads: 1, destinationHistory: true, leakedSourceMessage: false });
    await act(async () => { destinationRefresh.resolve(emptyView('document-b')); });
  });

  it('retains unsent text when saving the composer fails before a discussion starts', async () => {
    const session = await makeSession();
    vi.mocked(discussions.saveDiscussionDraft).mockRejectedValue({ code: 'PersistenceUnavailable', detail: 'Draft storage is unavailable.' });
    await renderPanel(session);
    await typeInstruction('Keep this unsent thought.');
    await act(async () => (Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Send') as HTMLButtonElement).click());
    await waitFor(() => expect(host.textContent).toContain('Draft storage is unavailable.'));
    expect((host.querySelector('#discussion-composer') as HTMLTextAreaElement).value).toBe('Keep this unsent thought.');
    expect(discussions.startDiscussion).not.toHaveBeenCalled();
  });
});
