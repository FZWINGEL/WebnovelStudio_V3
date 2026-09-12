// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { GuidancePanel } from './GuidancePanel';
import * as guidance from '../ipc/guidance';
import { bodyHash, canonicalJson, type WnsDocument } from '../kernel';
import { DocumentSession } from '../editor/session';
import type { DocumentRecord, Head, ProjectAccess, ProjectTransport } from '../ipc/projects';

vi.mock('../ipc/guidance', () => ({
  readGuidance: vi.fn(),
  saveGuidance: vi.fn(),
}));

const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const body: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'paragraph-1' }, content: [{ type: 'text', text: 'A quiet chapter.' }] }] } };

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((accept, fail) => { resolve = accept; reject = fail; });
  return { promise, resolve, reject };
}

async function makeSession(documentId = 'document', projectId = 'project') {
  const hash = await bodyHash(canonicalJson(body));
  const head: Head = { documentId, version: '0', bodyHash: hash };
  const document: DocumentRecord = { head, title: 'Chapter', kind: 'chapter', metadataVersion: '0', body, lastCheckpointId: null };
  const transport: ProjectTransport = {
    validate: async () => {},
    save: async request => ({ projectId: request.access.projectId, documentId, session: request.access.session, operationNamespace: request.access.operationNamespace, operationId: request.operationId, head: request.expected, savedGeneration: request.localGeneration }),
    reconcile: async request => ({ access: { ...access, projectId: request.projectId, writerLease: 'fresh-lease' }, document, receipts: [] }),
    checkpoint: async request => ({ id: 'checkpoint', head: request.expected, body, reason: request.reason, parentId: null }),
  };
  return new DocumentSession({ ...access, projectId }, document, transport, { autosave: false });
}

function emptyGuidance(): guidance.GuidanceVersion[] { return []; }

function savedGuidance(request: guidance.SaveGuidance): guidance.GuidanceVersion {
  return {
    guidanceId: request.guidanceId || 'guidance-1', versionId: 'version-1', version: request.expectedVersion === '0' ? '1' : (BigInt(request.expectedVersion) + 1n).toString(),
    scope: request.scope, documentId: request.documentId, text: request.text, textHash: 'hash', active: request.active,
    originMessageId: request.originMessageId, createdAt: 'now',
  };
}

let host: HTMLDivElement;
let root: Root;

async function renderPanel(session: DocumentSession, adoption: { text: string; originMessageId: string; nonce: number } | null = null) {
  await act(async () => root.render(<GuidancePanel session={session} documentId={session.state.head.documentId} adoption={adoption} refreshKey="0" onChanged={() => {}} />));
  await waitFor(() => expect(host.querySelector('.writing-guidance')).not.toBeNull());
}

async function waitFor<T>(assertion: () => T) {
  await act(async () => { await vi.waitFor(assertion); });
}

async function typeGuidance(text: string) {
  const textarea = host.querySelector('#writing-guidance-text') as HTMLTextAreaElement;
  const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!;
  await act(async () => {
    setter.call(textarea, text);
    textarea.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await waitFor(() => expect((host.querySelector('#writing-guidance-text') as HTMLTextAreaElement).value).toBe(text));
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.resetAllMocks();
  vi.mocked(guidance.readGuidance).mockResolvedValue(emptyGuidance());
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe('GuidancePanel', () => {
  it('retains and retries the exact pending request after a lost acknowledgment', async () => {
    const session = await makeSession();
    vi.mocked(guidance.saveGuidance)
      .mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'The guidance response was lost.' })
      .mockImplementation(async request => savedGuidance(request));
    await renderPanel(session);
    await act(async () => (host.querySelector('button') as HTMLButtonElement).click());
    await typeGuidance('Keep the ending intact.');
    const save = Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Save guidance') as HTMLButtonElement;
    await act(async () => save.click());
    await waitFor(() => expect(host.textContent).toContain('Check guidance'));
    const first = vi.mocked(guidance.saveGuidance).mock.calls[0][0];
    expect((host.querySelector('#writing-guidance-text') as HTMLTextAreaElement).value).toBe('Keep the ending intact.');
    const check = Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Check guidance') as HTMLButtonElement;
    await act(async () => check.click());
    await waitFor(() => expect(guidance.saveGuidance).toHaveBeenCalledTimes(2));
    const retry = vi.mocked(guidance.saveGuidance).mock.calls[1][0];
    expect({ ...retry, access: first.access }).toEqual(first);
    expect(retry.access.writerLease).toBe('fresh-lease');
    await waitFor(() => expect(host.textContent).toContain('Writing guidance saved.'));
  });

  it('opens adoption without saving it or mutating the manuscript', async () => {
    const session = await makeSession();
    const original = structuredClone(session.body);
    await renderPanel(session, { text: 'Keep the sister alive in this arc.', originMessageId: 'assistant-message', nonce: 1 });
    await waitFor(() => expect((host.querySelector('#writing-guidance-text') as HTMLTextAreaElement).value).toBe('Keep the sister alive in this arc.'));
    expect(guidance.saveGuidance).not.toHaveBeenCalled();
    expect(session.body).toEqual(original);
  });

  it('preserves the pending direction when another message is offered for adoption', async () => {
    const session = await makeSession();
    const gate = deferred<guidance.GuidanceVersion>();
    vi.mocked(guidance.saveGuidance).mockReturnValue(gate.promise);
    await renderPanel(session, { text: 'Keep the ending.', originMessageId: 'first-message', nonce: 1 });
    await act(async () => (Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Save guidance') as HTMLButtonElement).click());
    await waitFor(() => expect(guidance.saveGuidance).toHaveBeenCalledTimes(1));
    await renderPanel(session, { text: 'A different direction.', originMessageId: 'second-message', nonce: 2 });
    expect((host.querySelector('#writing-guidance-text') as HTMLTextAreaElement).value).toBe('Keep the ending.');
    expect(host.textContent).toContain('Finish saving this direction');
    const request = vi.mocked(guidance.saveGuidance).mock.calls[0][0];
    expect(request.originMessageId).toBe('first-message');
    await act(async () => gate.resolve(savedGuidance(request)));
    expect(guidance.saveGuidance).toHaveBeenCalledTimes(1);
  });

  it('ignores a late response after the project and document context changes', async () => {
    const source = await makeSession('document-a', 'project-a');
    const destination = await makeSession('document-b', 'project-b');
    const gate = deferred<guidance.GuidanceVersion>();
    vi.mocked(guidance.saveGuidance).mockReturnValue(gate.promise);
    await renderPanel(source);
    await act(async () => (host.querySelector('button') as HTMLButtonElement).click());
    await typeGuidance('Source-only instruction.');
    await act(async () => (Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Save guidance') as HTMLButtonElement).click());
    await waitFor(() => expect(guidance.saveGuidance).toHaveBeenCalledTimes(1));
    await act(async () => root.render(<GuidancePanel session={destination} documentId="document-b" adoption={null} refreshKey="0" onChanged={() => {}} />));
    await waitFor(() => expect(host.querySelector('#writing-guidance-text')).toBeNull());
    gate.resolve(savedGuidance(vi.mocked(guidance.saveGuidance).mock.calls[0][0]));
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });
    expect(host.textContent).not.toContain('Writing guidance saved.');
    expect(host.textContent).not.toContain('Source-only instruction.');
    expect(guidance.saveGuidance).toHaveBeenCalledTimes(1);
  });
});
