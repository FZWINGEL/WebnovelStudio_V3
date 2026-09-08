// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FeedbackPanel, finalContextPacketIdForRun } from './FeedbackPanel';
import * as discussions from '../ipc/discussions';
import * as proposals from '../ipc/proposals';
import { bodyHash, canonicalJson, type WnsDocument } from '../editor/document';
import { DocumentSession } from '../editor/session';
import { Editor } from '@tiptap/core';
import { editorExtensions } from '../editor/schema';
import { captureSelection } from '../editor/selection';
import type { ProviderBinding } from '../ipc/context';
import type { DocumentRecord, Head, ProjectAccess, ProjectTransport } from '../ipc/projects';
import * as providerIpc from '../ipc/providers';
import { ProviderSettingsProvider, useProviders } from '../providers/ProviderContext';
vi.mock('../ipc/providers', async original => ({ ...await original<typeof import('../ipc/providers')>(), readProviderState: vi.fn(), saveModelSettings: vi.fn() }));

vi.mock('../ipc/discussions', () => ({
  readDiscussion: vi.fn(),
  saveDiscussionDraft: vi.fn(),
  startDiscussion: vi.fn(),
  stopDiscussion: vi.fn(),
  discussionRetry: vi.fn(),
  retryDiscussionSave: vi.fn(),
}));
vi.mock('../ipc/proposals', () => ({
  readProposals: vi.fn(),
  prepareProposal: vi.fn(),
  applyProposal: vi.fn(),
  rejectProposal: vi.fn(),
}));
vi.mock('./ContextInspector', () => ({ ContextInspector: (props: { packetId: string; delivered: boolean; lookupDelivery?: string; onPin?: unknown }) => <div data-testid="context-inspector" data-packet-id={props.packetId} data-delivered={String(props.delivered)} data-lookup-delivery={props.lookupDelivery ?? ''} data-can-pin={String(!!props.onPin)} /> }));
vi.mock('./GuidancePanel', () => ({ GuidancePanel: () => <div data-testid="guidance-panel" /> }));
vi.mock('./SourcePinsPanel', () => ({ SourcePinsPanel: () => <div data-testid="source-pins-panel" /> }));

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
  await act(async () => root.render(<FeedbackPanel session={session} state={session.state} title="Chapter" documentKind="chapter" selection={null} visible onClose={() => {}} registerSaver={() => {}} />));
  await waitFor(() => expect(host.querySelector('#discussion-composer')).not.toBeNull());
}

async function waitFor<T>(assertion: () => T, timeout = 1000) {
  // Let each asynchronous receipt/save finish an act boundary before checking
  // the DOM. Holding one act open around polling can defer the very render it
  // waits for, especially while native crypto hashing is still in flight.
  await vi.waitFor(async () => {
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 0)); });
    return assertion();
  }, { timeout });
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
  vi.mocked(proposals.readProposals).mockResolvedValue([]);
  vi.mocked(discussions.saveDiscussionDraft).mockImplementation(async request => ({ documentId: request.documentId, version: (BigInt(request.expectedVersion) + 1n).toString(), text: request.text, intent: request.intent, basis: request.basis, scope: request.scope, pinnedDocumentIds: request.pinnedDocumentIds, previousRunId: request.previousRunId, safeBrief: request.safeBrief, updatedAt: 'now' }));
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe('persistent FeedbackPanel safeguards', () => {
  it('uses the final assistant packet for context inspection after a lookup', () => {
    const run = { id: 'run', packetId: 'initial' } as discussions.DiscussionRun;
    const view = { messages: [
      { id: 'user', threadId: 'thread', runId: 'run', role: 'user' as const, content: 'Find it.', scope: null, packetId: 'initial', createdAt: 'now' },
      { id: 'assistant', threadId: 'thread', runId: 'run', role: 'assistant' as const, content: 'Found it.', scope: null, packetId: 'final', createdAt: 'now' },
    ] } as discussions.DiscussionView;
    expect(finalContextPacketIdForRun(view, run)).toBe('final');
    expect(finalContextPacketIdForRun({ messages: [view.messages[0]] }, run)).toBe('initial');
  });

  it('lets the author inspect each lookup packet and does not mark an unclaimed call delivered', async () => {
    const session = await makeSession();
    const response = startResult(session, 'lookup-run', 'lookup-operation');
    response.run = {
      ...response.run,
      status: 'completed',
      dispatchState: 'delivered',
      lookup: {
        allowance: { maxAdditionalInvocations: 2, totalInputBytes: '73728', totalOutputBytes: '196608' },
        invocations: [
          { ordinal: '0', packetId: 'initial-packet', state: 'needsContext', inputDelivered: false, response: null, error: null },
          { ordinal: '1', packetId: 'prepared-packet', state: 'prepared', inputDelivered: false, response: null, error: null },
          { ordinal: '2', packetId: 'final-packet', state: 'completed', inputDelivered: true, response: null, error: null },
          { ordinal: '3', packetId: 'failed-packet', state: 'failed', inputDelivered: true, response: null, error: 'Malformed response' },
          { ordinal: '4', packetId: 'stopped-packet', state: 'stopped', inputDelivered: true, response: null, error: null },
        ],
      },
    };
    response.run.packetId = 'initial-packet';
    response.userMessage.packetId = 'initial-packet';
    vi.mocked(discussions.readDiscussion).mockResolvedValue({
      ...emptyView('document'),
      threadId: response.threadId,
      runs: [response.run],
      messages: [response.userMessage, { ...response.userMessage, id: 'answer', role: 'assistant', content: 'Final answer.', packetId: 'final-packet' }],
    });
    await renderPanel(session);
    const selector = host.querySelector('#discussion-context-call') as HTMLSelectElement;
    expect(selector).not.toBeNull();
    expect(selector.value).toBe('final-packet');
    expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-packet-id')).toBe('final-packet');
    expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-delivered')).toBe('true');
    expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-lookup-delivery')).toBe('delivered');
    await act(async () => { selector.value = 'prepared-packet'; selector.dispatchEvent(new Event('change', { bubbles: true })); });
    await waitFor(() => expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-packet-id')).toBe('prepared-packet'));
    expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-delivered')).toBe('false');
    expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-lookup-delivery')).toBe('prepared');
    expect(host.textContent).toContain('Call 2 · prepared · not sent');
    await act(async () => { selector.value = 'failed-packet'; selector.dispatchEvent(new Event('change', { bubbles: true })); });
    await waitFor(() => expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-packet-id')).toBe('failed-packet'));
    expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-delivered')).toBe('true');
    expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-lookup-delivery')).toBe('delivered');
    await act(async () => { selector.value = 'stopped-packet'; selector.dispatchEvent(new Event('change', { bubbles: true })); });
    await waitFor(() => expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-packet-id')).toBe('stopped-packet'));
    expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-delivered')).toBe('true');
    expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-lookup-delivery')).toBe('delivered');
    await act(async () => { selector.value = 'initial-packet'; selector.dispatchEvent(new Event('change', { bubbles: true })); });
    await waitFor(() => expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-packet-id')).toBe('initial-packet'));
    expect(selector.value).toBe('initial-packet');
    expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-delivered')).toBe('false');
    expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-lookup-delivery')).toBe('unconfirmed');
  });

  it('sends continuation with its explicit basis and no passage scope, preserving a refused reviewed request', async () => {
    const session = await makeSession();
    vi.mocked(providerIpc.readProviderState).mockResolvedValue(providerState(false));
    await renderWithProvider(session); await click('Continue chapter');
    const basis = host.querySelector('#continuation-basis') as HTMLSelectElement;
    expect(basis.value).toBe('working');
    await act(async () => { basis.value = 'reviewed'; basis.dispatchEvent(new Event('change', { bubbles: true })); });
    await typeInstruction('Let her ask about the letter.');
    vi.mocked(discussions.startDiscussion).mockRejectedValueOnce({ code: 'ReviewedBasisUnavailable', detail: 'Review the earlier chapter first.' });
    await click('Send');
    await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledOnce());
    const refused = vi.mocked(discussions.startDiscussion).mock.calls[0][0];
    expect(refused).toMatchObject({ intent: 'continue', basis: 'reviewed', scope: null });
    await waitFor(() => expect(host.textContent).toContain('Review the earlier chapter first.'));
    expect((host.querySelector('#continuation-basis') as HTMLSelectElement).value).toBe('reviewed');
    expect((host.querySelector('#discussion-composer') as HTMLTextAreaElement).value).toBe(refused.instruction);
    expect(host.textContent).not.toContain('Check request');
    await act(async () => { basis.value = 'working'; basis.dispatchEvent(new Event('change', { bubbles: true })); });
    vi.mocked(discussions.startDiscussion).mockImplementation(async request => ({ ...startResult(session, 'continuation', request.operationId), run: { ...startResult(session, 'continuation', request.operationId).run, intent: 'continue', basis: request.basis } }));
    await click('Send');
    await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledTimes(2));
    const working = vi.mocked(discussions.startDiscussion).mock.calls[1][0];
    expect(working.basis).toBe('working'); expect(working.operationId).not.toBe(refused.operationId);
    expect(session.body).toEqual(emptyBody); expect(proposals.applyProposal).not.toHaveBeenCalled();
  });

  it('adding an approved brief retains continuation intent and switching to Discuss clears its basis', async () => {
    const session = await makeSession(); await renderPanel(session); await click('Continue chapter');
    await click('Add writing brief');
    expect((host.querySelector('#continuation-basis') as HTMLSelectElement).value).toBe('working');
    await typeBrief('She hesitates before answering.'); await click('Approve this brief');
    await click('Discuss'); await typeInstruction('Talk about the ending.');
    await waitFor(() => expect(discussions.saveDiscussionDraft).toHaveBeenCalled());
    const last = vi.mocked(discussions.saveDiscussionDraft).mock.calls.at(-1)![0];
    expect(last.intent).toBe('discuss'); expect(last.basis).toBeUndefined(); expect(last.safeBrief).toBeUndefined();
  });
  function providerState(blocked: boolean): providerIpc.ProviderState {
    const active = blocked ? { providerId: 'codex', modelId: 'gpt-5.6-luna', reasoning: 'max', serviceTier: 'priority' } : providerIpc.localModel;
    return { settings: { revision: blocked ? '1' : '0', active, favorites: [] }, dispatch: { kind: blocked ? 'blocked' : 'localMock', detail: '' }, catalog: { models: [{ key: active, label: blocked ? 'GPT-5.6-Luna' : 'Local test model', providerLabel: 'Test catalog', reasoningLevels: [], serviceTiers: [], origin: 'builtIn', ready: !blocked, statusDetail: '', contextWindowTokens: null, maxOutputTokens: null }] } };
  }
  const httpSelection: providerIpc.ModelSelection = { providerId: 'openai-compatible:test-endpoint', modelId: 'fiction-v1', reasoning: null, serviceTier: null };
  const httpBinding: ProviderBinding = { ...httpSelection, profileVersion: 'openai-chat-completions.v1', inputLimitBytes: '24576', reservedOutputBytes: '4096', reservedProtocolBytes: '1024', outputLimitBytes: '65536', accountingMethod: 'utf8-byte-count/http-request-v1', http: { baseUrl: 'https://example.test/v1', configRevision: '4', stream: true, responseFormat: 'text' } };
  const appServerBinding: ProviderBinding = { providerId: 'codex', modelId: 'gpt-6-astra', reasoning: 'low', serviceTier: null, profileVersion: 'codex-app-server.author.v1', inputLimitBytes: '24576', reservedOutputBytes: '0', reservedProtocolBytes: '1024', outputLimitBytes: '65536', accountingMethod: 'utf8-byte-count/codex-app-server-application-cap-v1' };
  const appServerDispatch = { serverGeneration: 'server-1', threadId: 'thread-1', rpcId: 'rpc-1', packetHash: 'a'.repeat(64), requestHash: 'b'.repeat(64) };
  function httpProviderState(): providerIpc.ProviderState {
    return { settings: { revision: '1', active: httpSelection, favorites: [] }, dispatch: { kind: 'openAiCompatible', detail: '' }, catalog: { models: [{ key: httpSelection, label: 'Fiction V1', providerLabel: 'Synthetic endpoint', reasoningLevels: [], serviceTiers: [], origin: 'openAiCompatible', ready: true, statusDetail: '', contextWindowTokens: null, maxOutputTokens: null }] } };
  }
  function RefreshModel() { const providers = useProviders(); return <button onClick={() => void providers.refresh()}>Reload model choice</button>; }
  it.each([null, 'high'] as const)('retains an author-selected Codex response with %s requested reasoning and frozen defaults', async reasoning => {
    const session = await makeSession(); const connected = providerState(true);
    connected.settings.active = { providerId: 'codex', modelId: 'gpt-5.4-mini', reasoning, serviceTier: null };
    connected.dispatch.kind = 'codexCli'; connected.catalog.models[0] = { ...connected.catalog.models[0], key: connected.settings.active, label: 'GPT-5.4-Mini', ready: true };
    vi.mocked(providerIpc.readProviderState).mockResolvedValue(connected);
    const binding: ProviderBinding = { providerId: 'codex', modelId: 'gpt-5.4-mini', reasoning: reasoning ?? 'medium', serviceTier: null, profileVersion: 'codex-stdin.author.v1', inputLimitBytes: '24576', reservedOutputBytes: '0', reservedProtocolBytes: '0', outputLimitBytes: '65536', accountingMethod: 'utf8-byte-count/codex-stdin-application-cap-v1' };
    vi.mocked(discussions.startDiscussion).mockImplementation(async request => {
      const response = startResult(session, 'author-model-run', request.operationId);
      response.packet.options = { ...response.packet.options, modelId: binding.modelId, providerBinding: binding };
      response.run = { ...response.run, providerBinding: binding, status: 'completed' };
      vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'), runs: [response.run], messages: [response.userMessage, { ...response.userMessage, id: 'author-model-answer', role: 'assistant', content: 'Keep the quiet ending.' }] });
      return response;
    });
    await renderWithProvider(session); await typeInstruction('Discuss this ending.'); await click('Send');
    await waitFor(() => expect(host.textContent).toContain('Keep the quiet ending.'));
    expect(host.textContent).not.toContain('did not confirm the model');
    expect(vi.mocked(discussions.startDiscussion).mock.calls[0][0].modelSelection).toEqual(connected.settings.active);
    expect(session.body).toEqual(emptyBody);
  });
  async function renderWithProvider(session: DocumentSession) {
    await act(async () => root.render(<ProviderSettingsProvider><RefreshModel /><FeedbackPanel session={session} state={session.state} title="Chapter" documentKind="chapter" selection={null} visible onClose={() => {}} registerSaver={() => {}} /></ProviderSettingsProvider>));
    await waitFor(() => expect(host.querySelector('#discussion-composer')).not.toBeNull());
  }
  it('keeps feedback editable while an unavailable selected provider blocks Send', async () => {
    const session = await makeSession(); vi.mocked(providerIpc.readProviderState).mockResolvedValue(providerState(true));
    await renderWithProvider(session); await typeInstruction('Keep the ending.');
    expect([...host.querySelectorAll('button')].find(button => button.textContent === 'Send')!.disabled).toBe(true);
    await act(async () => host.querySelector('form.feedback-form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
    expect(discussions.startDiscussion).not.toHaveBeenCalled(); expect(session.body).toEqual(emptyBody);
    expect((host.querySelector('#discussion-composer') as HTMLTextAreaElement).disabled).toBe(false);
  });
  it('keeps story lookup unavailable for a connected Claude author choice', async () => {
    const session = await makeSession();
    const claude: providerIpc.ModelSelection = { providerId: 'claude', modelId: 'claude-opus-5', reasoning: 'high', serviceTier: null };
    vi.mocked(providerIpc.readProviderState).mockResolvedValue({
      settings: { revision: '1', active: claude, favorites: [] },
      dispatch: { kind: 'claudeCli', detail: 'Claude Code is connected.' },
      claudeConnection: { ready: true, detail: 'Claude Code is connected.' },
      catalog: { models: [{ key: claude, label: 'Claude Opus 5', providerLabel: 'Claude Code', reasoningLevels: ['low', 'medium', 'high', 'xhigh', 'max'], serviceTiers: [], contextWindowTokens: null, maxOutputTokens: null, origin: 'reference', ready: true, statusDetail: 'Connected' }] },
    });
    await renderWithProvider(session);
    const lookup = host.querySelector('#discussion-lookup') as HTMLInputElement;
    expect(lookup.disabled).toBe(true);
    expect(host.textContent).toContain('Story lookups are available through Codex.');
  });
  it('sends the connected choice and keeps the saved response identity after model changes', async () => {
    const session = await makeSession(); const connected = providerState(true); connected.dispatch.kind = 'codexCli';
    connected.catalog.models[0].ready = true; vi.mocked(providerIpc.readProviderState).mockResolvedValue(connected);
    const binding = { providerId:'codex',modelId:'gpt-5.6-luna',reasoning:'max',serviceTier:'priority',profileVersion:'0.153.3',inputLimitBytes:'24576',reservedOutputBytes:'0',reservedProtocolBytes:'0',outputLimitBytes:'65536',accountingMethod:'utf8-byte-count/codex-stdin-application-cap-v1' };
    vi.mocked(discussions.startDiscussion).mockImplementation(async request => {
      const response = startResult(session,'live-run',request.operationId);
      response.packet.options = { ...response.packet.options, modelId:binding.modelId,providerBinding:binding };
      response.run = { ...response.run,providerBinding:binding,status:'completed',providerResult:{binding,status:'completed',confirmedStdinBytes:'100',usage:null,cleanup:'settled',error:null,effectiveIdentity:null} };
      vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'),runs:[response.run],messages:[response.userMessage,{ ...response.userMessage,id:'live-answer',role:'assistant',content:'The promise can deepen this scene.' }] });
      return response;
    });
    await renderWithProvider(session); await typeInstruction('Discuss the promise.'); await click('Send');
    await waitFor(() => expect(host.textContent).toContain('The promise can deepen this scene.'));
    expect(vi.mocked(discussions.startDiscussion).mock.calls[0][0].modelSelection).toEqual(connected.settings.active);
    vi.mocked(providerIpc.readProviderState).mockResolvedValue(providerState(false)); await click('Reload model choice');
    await waitFor(() => expect(host.querySelector('.scope-controls')?.textContent).toContain('Local test model'));
    expect(host.querySelectorAll('.feedback-note')[1].textContent).toContain('GPT-5.6-Luna');
    expect(host.textContent).toContain('Usage is unavailable for this response.'); expect(session.body).toEqual(emptyBody);
  });
  it('accepts an HTTP response only when the selected model, packet binding, run binding, and profile match', async () => {
    const session = await makeSession(); vi.mocked(providerIpc.readProviderState).mockResolvedValue(httpProviderState());
    vi.mocked(discussions.startDiscussion).mockImplementation(async request => {
      const response = startResult(session, 'http-run', request.operationId);
      response.packet.options = { ...response.packet.options, modelId: httpBinding.modelId, providerBinding: httpBinding };
      response.run = { ...response.run, providerBinding: httpBinding, status: 'completed', providerResult: { binding: httpBinding, status: 'completed', confirmedStdinBytes: '0', usage: null, cleanup: 'settled', error: null, effectiveIdentity: null, delivery: { bodyHash: 'body-hash', bodyBytes: '100', submission: 'responseReceived' } } };
      vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'), threadId: response.threadId, runs: [response.run], messages: [response.userMessage, { ...response.userMessage, id: 'http-answer', role: 'assistant', content: 'The endpoint returned a response.' }] });
      return response;
    });
    await renderWithProvider(session); await typeInstruction('Discuss the endpoint response.'); await click('Send');
    await waitFor(() => expect(host.textContent).toContain('The endpoint returned a response.'));
    expect(vi.mocked(discussions.startDiscussion).mock.calls[0][0].modelSelection).toEqual(httpSelection);
    expect(host.textContent).not.toContain('unexpectedly returned a live provider binding');
    expect(host.querySelector('.discussion-error[role="alert"]')).toBeNull();
  });
  it.each([
    ['a packet model mismatch', (binding: ProviderBinding) => ({ packet: { ...binding, modelId: 'other-model' }, run: binding }), 'model settings'],
    ['a run binding mismatch', (binding: ProviderBinding) => ({ packet: binding, run: { ...binding, modelId: 'other-model' } }), 'model settings'],
    ['a missing HTTP profile', (binding: ProviderBinding) => ({ packet: { ...binding, http: undefined }, run: { ...binding, http: undefined } }), 'API connection'],
  ] as const)('rejects an HTTP response with %s', async (_caseName, bindingsFor, expectedError) => {
    const session = await makeSession(); vi.mocked(providerIpc.readProviderState).mockResolvedValue(httpProviderState());
    vi.mocked(discussions.startDiscussion).mockImplementation(async request => {
      const response = startResult(session, 'invalid-http-run', request.operationId);
      const bindings = bindingsFor(httpBinding);
      response.packet.options = { ...response.packet.options, modelId: bindings.packet.modelId, providerBinding: bindings.packet };
      response.run = { ...response.run, providerBinding: bindings.run, status: 'completed', providerResult: { binding: bindings.run, status: 'completed', confirmedStdinBytes: '0', usage: null, cleanup: 'settled', error: null, effectiveIdentity: null, delivery: { bodyHash: 'body-hash', bodyBytes: '100', submission: 'responseReceived' } } };
      return response;
    });
    await renderWithProvider(session); await typeInstruction('Discuss the endpoint response.'); await click('Send');
    await waitFor(() => expect(host.textContent).toContain(`The response did not confirm the ${expectedError}`));
    expect(host.textContent).not.toContain('unexpectedly returned a live provider binding');
  });
  it.each([
    ['partial', 'failed' as const, 'responseReceived' as const, 'Response headers were received; the saved result may be partial.'],
    ['uncertain', 'failed' as const, 'uncertain' as const, 'The request may have reached the API, but delivery could not be confirmed. It was not automatically retried.'],
  ])('labels %s HTTP delivery separately from local cleanup and reports unknown usage', async (_label, status, submission, deliveryLabel) => {
    const session = await makeSession(); const started = startResult(session, `http-${_label}`);
    const run = { ...started.run, status, providerBinding: httpBinding, providerResult: { binding: httpBinding, status, confirmedStdinBytes: '0', usage: null, cleanup: 'settled' as const, error: null, effectiveIdentity: null, delivery: { bodyHash: 'body-hash', bodyBytes: '100', submission } } };
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'), threadId: started.threadId, runs: [run], messages: [started.userMessage, { ...started.userMessage, id: `http-${_label}-answer`, role: 'assistant', content: 'The endpoint response was saved.' }] });
    await renderPanel(session);
    expect(host.textContent).toContain('Usage is unavailable for this response.');
    expect(host.textContent).toContain(deliveryLabel);
    expect(host.textContent).toContain('The local HTTP request has finished.');
    expect(host.textContent).toContain('Stopping locally does not confirm that the upstream service stopped processing or charging.');
    expect(host.textContent).not.toContain('The local provider process has finished.');
  });
  it('describes a completed reusable app-server turn without claiming a local process finished', async () => {
    const session = await makeSession(); const started = startResult(session, 'app-server-complete');
    const run = { ...started.run, status: 'completed' as const, providerBinding: appServerBinding, providerResult: {
      binding: appServerBinding, status: 'completed' as const, confirmedStdinBytes: '0', usage: null, cleanup: 'settled' as const,
      error: null, effectiveIdentity: null, appServer: { dispatch: appServerDispatch, submission: 'acknowledged' as const, turnId: 'turn-1', terminal: 'completed' as const, requestSettled: true, connection: 'reusable' as const },
    } };
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'), threadId: started.threadId, runs: [run], messages: [started.userMessage, { ...started.userMessage, id: 'app-server-answer', role: 'assistant', content: 'The turn was saved.' }] });
    await renderPanel(session);
    expect(host.textContent).toContain('App-server acknowledgment: the owned turn was acknowledged.');
    expect(host.textContent).toContain('App-server terminal: completed.');
    expect(host.textContent).toContain('Request settlement: settled; the shared app-server is reusable for another request.');
    expect(host.textContent).not.toContain('The local provider process has finished.');
    expect(host.textContent).not.toContain('Stopping locally does not confirm');
  });
  it('keeps uncertain app-server start delivery unresolved and does not imply an automatic retry', async () => {
    const session = await makeSession(); const started = startResult(session, 'app-server-uncertain');
    const run = { ...started.run, status: 'failed' as const, providerBinding: appServerBinding, providerResult: {
      binding: appServerBinding, status: 'failed' as const, confirmedStdinBytes: '0', usage: null, cleanup: 'unresolved' as const,
      error: null, effectiveIdentity: null, appServer: { dispatch: appServerDispatch, submission: 'uncertain' as const, turnId: null, terminal: null, requestSettled: false, connection: 'unresolved' as const },
    } };
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'), threadId: started.threadId, runs: [run], messages: [started.userMessage, { ...started.userMessage, id: 'app-server-uncertain-answer', role: 'assistant', content: 'The outcome is unresolved.' }] });
    await renderPanel(session);
    expect(host.textContent).toContain('App-server acknowledgment: uncertain; it may have accepted this turn. It was not automatically retried.');
    expect(host.textContent).toContain('App-server terminal: not confirmed.');
    expect(host.textContent).toContain('Request settlement: unresolved. It was not automatically retried.');
    expect(host.textContent).not.toContain('The local provider process has finished.');
  });
  it('keeps an uncertain request bound to its original model after the active choice changes', async () => {
    const session = await makeSession(); vi.mocked(providerIpc.readProviderState).mockResolvedValue(providerState(false));
    vi.mocked(discussions.startDiscussion).mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'The acknowledgment was lost.' }).mockImplementationOnce(async request => startResult(session, 'run-1', request.operationId));
    await renderWithProvider(session); await typeInstruction('Keep the ending.'); await click('Send');
    await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledOnce());
    const original = vi.mocked(discussions.startDiscussion).mock.calls[0][0]; expect(original.modelSelection).toEqual(providerIpc.localModel);
    vi.mocked(providerIpc.readProviderState).mockResolvedValue(providerState(true)); await click('Reload model choice');
    await waitFor(() => expect(host.textContent).toContain('GPT-5.6-Luna')); await click('Check request');
    await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledTimes(2));
    expect(vi.mocked(discussions.startDiscussion).mock.calls[1][0]).toEqual({ ...original, access: { ...original.access, writerLease: 'fresh-lease' } });
  });
  function stoppedView(session: DocumentSession) {
    const started = startResult(session);
    return { ...emptyView(session.state.head.documentId), threadId: started.threadId, messages: [started.userMessage], runs: [{ ...started.run, status: 'stopped' as const }] };
  }

  function click(text: string) {
    return act(async () => (Array.from(host.querySelectorAll('button')).find(button => button.textContent === text) as HTMLButtonElement).click());
  }

  async function briefView(session: DocumentSession, role: 'user' | 'assistant' = 'user') {
    const original = startResult(session);
    const scope: discussions.DiscussionScope = { kind: 'passage', start: { blockId: 'paragraph-1', utf16Offset: 0 }, end: { blockId: 'paragraph-1', utf16Offset: 1 }, quote: 'A', sourceBodyHash: session.state.head.bodyHash };
    const view: discussions.DiscussionView = { ...emptyView('document'), threadId: original.threadId,
      runs: [{ ...original.run, status: 'completed' }], messages: [{ ...original.userMessage, role, packetId: role === 'assistant' ? null : original.packet.receipt.packetId, content: 'The mentor killed her father. Keep it secret.' }],
      draft: { documentId: 'document', version: '0', text: 'Hint at his recognition.', scope, pinnedDocumentIds: [], updatedAt: 'now' } };
    vi.mocked(discussions.readDiscussion).mockResolvedValue(view);
    await renderPanel(session); return view;
  }

  async function typeBrief(text: string) {
    await act(async () => {
      const textarea = host.querySelector('.safe-brief-editor textarea') as HTMLTextAreaElement;
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(textarea, text);
      textarea.dispatchEvent(new Event('input', { bubbles: true }));
    });
  }

  it.each(['user', 'assistant'] as const)('requires exact brief approval when adapting a %s message and sends only the adopted directions', async role => {
    const session = await makeSession(); const view = await briefView(session, role);
    await click('Adapt as writing brief');
    expect((host.querySelector('.safe-brief-editor textarea') as HTMLTextAreaElement).value).toBe(view.messages[0].content);
    expect(discussions.startDiscussion).not.toHaveBeenCalled();
    const sendButton = () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Send')!;
    expect(sendButton().disabled).toBe(true);
    await typeBrief('He notices the pendant, then changes the subject.'); await click('Approve this brief');
    await waitFor(() => expect(sendButton().disabled).toBe(false));
    await typeBrief('He notices the pendant. Mei reads the pause as grief.'); expect(sendButton().disabled).toBe(true);
    await click('Approve this brief');
    vi.mocked(discussions.startDiscussion).mockImplementation(async request => {
      const result = startResult(session, 'brief-run', request.operationId);
      result.packet.receipt.safeBrief = { text: request.safeBrief!.text, textHash: await bodyHash(request.safeBrief!.text), originMessageId: request.safeBrief!.originMessageId };
      return result;
    });
    await click('Send'); await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledOnce());
    const request = vi.mocked(discussions.startDiscussion).mock.calls[0][0];
    expect(request.safeBrief).toEqual({ text: 'He notices the pendant. Mei reads the pause as grief.', originMessageId: view.messages[0].id, confirmed: true });
    expect(request.instruction).not.toContain('killed her father'); expect(request.intent).toBe('proposeEdits');
    // Observing dispatch is earlier than async receipt hashing and draft save.
    // Wait for the completed author action on slower hosted runners.
    await waitFor(() => {
      expect(host.querySelector('.safe-brief-editor')).toBeNull();
      expect((host.querySelector('#discussion-composer') as HTMLTextAreaElement).disabled).toBe(false);
    }, 4000);
    expect(session.body).toEqual(emptyBody);
  });

  it('revokes approval when selecting another passage or switching to the whole document', async () => {
    const session = await makeSession(); await briefView(session);
    await click('Adapt as writing brief'); await typeBrief('Keep the pause.'); await click('Approve this brief');
    const editor = new Editor({ extensions: editorExtensions, content: emptyBody.body });
    try {
      editor.commands.setTextSelection({ from: 3, to: 8 });
      const scope = captureSelection(editor)!;
      await act(async () => root.render(<FeedbackPanel session={session} state={session.state} title="Chapter" documentKind="chapter" selection={{ scope, nonce: 1 }} visible onClose={() => {}} registerSaver={() => {}} />));
      await waitFor(() => expect(host.querySelector('.quoted-scope blockquote')?.textContent).toBe('quiet'));
      expect(host.textContent).toContain('Writing brief needs approval');
      expect([...host.querySelectorAll('button')].find(button => button.textContent === 'Send')!.disabled).toBe(true);
      await click('Approve this brief'); await click('Whole chapter');
      await waitFor(() => expect(host.textContent).toContain('Writing brief needs approval'));
      expect([...host.querySelectorAll('button')].find(button => button.textContent === 'Send')!.disabled).toBe(true);
      expect(discussions.startDiscussion).not.toHaveBeenCalled();
    } finally { editor.destroy(); }
  });

  it('preserves exact approved directions when preparing an unchanged linked edit retry', async () => {
    const session = await makeSession(); const view = await briefView(session);
    const safeBrief = { text: 'Keep the pause.', originMessageId: view.messages[0].id, confirmed: true };
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...view, runs: [{ ...view.runs[0], intent: 'proposeEdits', status: 'stopped' }] });
    const destination = await makeSession(); await renderPanel(destination);
    vi.mocked(discussions.discussionRetry).mockResolvedValue({ text: 'Hint at his recognition.', intent: 'proposeEdits', scope: view.draft!.scope, pinnedDocumentIds: [], previousRunId: 'run-1', safeBrief });
    await click('Prepare another attempt');
    await waitFor(() => expect(discussions.saveDiscussionDraft).toHaveBeenCalledWith(expect.objectContaining({ previousRunId: 'run-1', safeBrief })));
    expect(host.textContent).toContain('Writing brief approved');
    expect([...host.querySelectorAll('button')].find(button => button.textContent === 'Send')!.disabled).toBe(false);
  });

  it('drops the brief when returning to Discuss and never sends private planning implicitly', async () => {
    const session = await makeSession(); await briefView(session);
    await click('Adapt as writing brief'); await click('Discuss');
    expect(host.querySelector('.safe-brief-editor')).toBeNull();
    vi.mocked(discussions.startDiscussion).mockImplementation(async request => startResult(session, 'discussion-without-brief', request.operationId));
    await click('Send'); await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledOnce());
    expect(vi.mocked(discussions.startDiscussion).mock.calls[0][0].safeBrief).toBeUndefined();
  });

  it('keeps direct briefs optional, blocks empty or oversized directions, and returns focus after removal', async () => {
    const session = await makeSession(); await briefView(session);
    await click('Suggest edits'); await click('Add writing brief');
    const approve = () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Approve this brief')!;
    expect(approve().disabled).toBe(true);
    expect((host.querySelector('.safe-brief-editor textarea') as HTMLTextAreaElement).value).toBe('');
    await typeBrief('é'.repeat(9000)); expect(approve().disabled).toBe(true);
    expect(host.textContent).toContain('This brief is too long');
    await click('Remove brief');
    expect(host.querySelector('.safe-brief-editor')).toBeNull();
    expect(document.activeElement?.textContent).toBe('Add writing brief');
    await waitFor(() => expect([...host.querySelectorAll('button')].find(button => button.textContent === 'Send')!.disabled).toBe(false));
  });

  it('retains an approved brief and checks the same operation when its acknowledgment is malformed', async () => {
    const session = await makeSession(); await briefView(session);
    await click('Adapt as writing brief'); await typeBrief('Let the pendant catch his attention.'); await click('Approve this brief');
    vi.mocked(discussions.startDiscussion).mockImplementation(async request => startResult(session, 'missing-brief-receipt', request.operationId));
    await waitFor(() => expect([...host.querySelectorAll('button')].find(button => button.textContent === 'Send')!.disabled).toBe(false));
    await click('Send'); await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledOnce());
    await waitFor(() => expect(host.textContent).toContain('Check request'));
    const original = vi.mocked(discussions.startDiscussion).mock.calls[0][0];
    await click('Check request'); await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledTimes(2));
    expect(vi.mocked(discussions.startDiscussion).mock.calls[1][0]).toEqual({ ...original, access: { ...original.access, writerLease: 'fresh-lease' } });
    expect((host.querySelector('.safe-brief-editor textarea') as HTMLTextAreaElement).value).toBe(original.safeBrief!.text);
  });

  it('restores the exact retry pins, saves its link, and sends the linked request', async () => {
    const session = await makeSession();
    const view = stoppedView(session);
    vi.mocked(discussions.readDiscussion).mockResolvedValue(view);
    vi.mocked(discussions.discussionRetry).mockResolvedValue({ text: view.messages[0].content, scope: null, pinnedDocumentIds: ['old-promise'], previousRunId: 'run-1' });
    vi.mocked(discussions.startDiscussion).mockImplementation(async request => startResult(session, 'retry-run', request.operationId));
    await renderPanel(session);
    await click('Prepare another attempt');
    await waitFor(() => expect(discussions.saveDiscussionDraft).toHaveBeenCalledTimes(1));
    expect(discussions.saveDiscussionDraft).toHaveBeenLastCalledWith(expect.objectContaining({ previousRunId: 'run-1', pinnedDocumentIds: ['old-promise'] }));
    expect(host.textContent).toContain('Another attempt at the same feedback.');
    await click('Send');
    await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledTimes(1));
    expect(discussions.startDiscussion).toHaveBeenCalledWith(expect.objectContaining({ previousRunId: 'run-1', pinnedDocumentIds: ['old-promise'], instruction: view.messages[0].content }));
  });

  it('restores retry mode from a saved draft and makes edited feedback a new request', async () => {
    const session = await makeSession();
    const view = stoppedView(session);
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...view, draft: { documentId: 'document', version: '4', text: view.messages[0].content, scope: null, pinnedDocumentIds: ['old-promise'], previousRunId: 'run-1', updatedAt: 'now' } });
    vi.mocked(discussions.startDiscussion).mockImplementation(async request => startResult(session, 'new-run', request.operationId));
    await renderPanel(session);
    await waitFor(() => expect(host.textContent).toContain('Another attempt at the same feedback.'));
    await typeInstruction('Different feedback for a new discussion.');
    expect(host.textContent).not.toContain('Another attempt at the same feedback.');
    await click('Send');
    await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledTimes(1));
    expect(discussions.startDiscussion).toHaveBeenCalledWith(expect.objectContaining({ previousRunId: null, instruction: 'Different feedback for a new discussion.' }));
  });

  it('does not let a late retry preparation replace a destination composer', async () => {
    const source = await makeSession(); const destination = await makeSession('document-b');
    const gate = deferred<discussions.ComposerBody & { previousRunId: string }>();
    vi.mocked(discussions.readDiscussion).mockImplementation(async (_access, documentId) => documentId === 'document' ? stoppedView(source) : emptyView(documentId));
    vi.mocked(discussions.discussionRetry).mockReturnValue(gate.promise);
    await renderPanel(source); await click('Prepare another attempt');
    await waitFor(() => expect(discussions.discussionRetry).toHaveBeenCalledTimes(1));
    await renderPanel(destination); await typeInstruction('Keep this destination feedback.');
    await act(async () => gate.resolve({ text: 'Old retry text.', scope: null, pinnedDocumentIds: ['old-pin'], previousRunId: 'run-1' }));
    expect((host.querySelector('#discussion-composer') as HTMLTextAreaElement).value).toBe('Keep this destination feedback.');
    expect(host.textContent).not.toContain('Another attempt at the same feedback.');
    expect(discussions.saveDiscussionDraft).not.toHaveBeenCalled();
  });

  it('retains the composer when original retry guidance is no longer valid', async () => {
    const session = await makeSession();
    vi.mocked(discussions.readDiscussion).mockResolvedValue(stoppedView(session));
    vi.mocked(discussions.discussionRetry).mockRejectedValue({ code: 'RetryGuidanceChanged', detail: 'Guidance was retired. Start a new request.' });
    await renderPanel(session); await typeInstruction('My unsent thought.');
    await click('Prepare another attempt');
    await waitFor(() => expect(host.textContent).toContain('Guidance was retired. Start a new request.'));
    expect((host.querySelector('#discussion-composer') as HTMLTextAreaElement).value).toBe('My unsent thought.');
    expect(discussions.startDiscussion).not.toHaveBeenCalled();
  });

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

  it('persists the explicit Suggest edits intent and requires a chapter passage', async () => {
    const session = await makeSession();
    const hash = await bodyHash(canonicalJson(emptyBody));
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'), draft: {
      documentId: 'document', version: '1', text: 'Tighten this passage.', intent: 'discuss',
      scope: { kind: 'passage', start: { blockId: 'paragraph-1', utf16Offset: 0 }, end: { blockId: 'paragraph-1', utf16Offset: 16 }, quote: 'A quiet chapter.', sourceBodyHash: hash }, pinnedDocumentIds: [], previousRunId: null, updatedAt: 'now',
    } });
    vi.mocked(discussions.startDiscussion).mockImplementation(async request => startResult(session, 'proposal-run', request.operationId));
    await renderPanel(session);
    await waitFor(() => expect(host.textContent).toContain('Selected passage'));
    await click('Suggest edits');
    await waitFor(() => expect((Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Send') as HTMLButtonElement).disabled).toBe(false));
    await click('Send');
    await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledTimes(1));
    expect(discussions.startDiscussion).toHaveBeenCalledWith(expect.objectContaining({ intent: 'proposeEdits', scope: expect.objectContaining({ kind: 'passage' }) }));

    const noteSession = await makeSession('document-b');
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document-b'), draft: {
      documentId: 'document-b', version: '1', text: 'Try this.', intent: 'proposeEdits', scope: null, pinnedDocumentIds: [], previousRunId: null, updatedAt: 'now',
    } });
    await act(async () => root.render(<FeedbackPanel session={noteSession} state={noteSession.state} title="Note" documentKind="note" selection={null} visible onClose={() => {}} registerSaver={() => {}} />));
    await waitFor(() => expect(host.querySelector('#discussion-composer')).not.toBeNull());
    expect((Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Send') as HTMLButtonElement).disabled).toBe(true);
    expect(host.textContent).toContain('Choose Whole document to request a reviewable development draft.');
  });

  it('clears transient Discuss pins when switching a chapter into restricted editing', async () => {
    const session = await makeSession();
    const hash = await bodyHash(canonicalJson(emptyBody));
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'), draft: {
      documentId: 'document', version: '1', text: 'Keep the promise in view.', intent: 'discuss',
      scope: { kind: 'passage', start: { blockId: 'paragraph-1', utf16Offset: 0 }, end: { blockId: 'paragraph-1', utf16Offset: 16 }, quote: 'A quiet chapter.', sourceBodyHash: hash },
      pinnedDocumentIds: ['worldbuilding-1'], previousRunId: null, updatedAt: 'now',
    } });
    await renderPanel(session);
    await waitFor(() => expect(host.textContent).toContain('Selected passage'));
    await click('Suggest edits');
    await waitFor(() => expect(discussions.saveDiscussionDraft).toHaveBeenCalled());

    expect(vi.mocked(discussions.saveDiscussionDraft).mock.calls.at(-1)![0]).toEqual(expect.objectContaining({ intent: 'proposeEdits', pinnedDocumentIds: [] }));
    expect(host.textContent).toContain('Removed discussion-only source from this writing request.');
    expect(host.textContent).toContain('approved writing brief');
  });

  it('keeps transient pins when developing a non-chapter AuthorRoom document', async () => {
    const session = await makeSession('document-b');
    const hash = await bodyHash(canonicalJson(emptyBody));
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document-b'), draft: {
      documentId: 'document-b', version: '1', text: 'Keep this world detail.', intent: 'discuss',
      scope: { kind: 'wholeDocument', start: null, end: null, quote: 'A quiet chapter.', sourceBodyHash: hash },
      pinnedDocumentIds: ['chapter-promise'], previousRunId: null, updatedAt: 'now',
    } });
    await act(async () => root.render(<FeedbackPanel session={session} state={session.state} title="World" documentKind="world" selection={null} visible onClose={() => {}} registerSaver={() => {}} />));
    await waitFor(() => expect(host.querySelector('#discussion-composer')).not.toBeNull());
    await click('Suggest edits');
    await waitFor(() => expect(discussions.saveDiscussionDraft).toHaveBeenCalled());

    expect(vi.mocked(discussions.saveDiscussionDraft).mock.calls.at(-1)![0]).toEqual(expect.objectContaining({ intent: 'proposeEdits', pinnedDocumentIds: ['chapter-promise'] }));
    expect(host.textContent).not.toContain('Removed discussion-only source');
  });

  it('hides Include next time after a chapter leaves Discuss', async () => {
    const session = await makeSession();
    const started = startResult(session, 'completed-discussion');
    started.run.status = 'completed';
    started.run.dispatchState = 'delivered';
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'), threadId: started.threadId, messages: [started.userMessage], runs: [started.run] });
    await renderPanel(session);
    await waitFor(() => expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-can-pin')).toBe('true'));
    await click('Suggest edits');
    await waitFor(() => expect(host.querySelector('[data-testid="context-inspector"]')?.getAttribute('data-can-pin')).toBe('false'));
  });

  it('flags restored chapter pins and blocks sending until they are removed or Discuss is selected', async () => {
    const session = await makeSession();
    const hash = await bodyHash(canonicalJson(emptyBody));
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'), draft: {
      documentId: 'document', version: '1', text: 'Continue from this point.', intent: 'proposeEdits',
      scope: { kind: 'passage', start: { blockId: 'paragraph-1', utf16Offset: 0 }, end: { blockId: 'paragraph-1', utf16Offset: 16 }, quote: 'A quiet chapter.', sourceBodyHash: hash },
      pinnedDocumentIds: ['old-discussion-source'], previousRunId: null, updatedAt: 'now',
    } });
    await renderPanel(session);

    await waitFor(() => expect(host.textContent).toContain('Remove them below before sending, or switch to Discuss.'));
    expect((Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Send') as HTMLButtonElement).disabled).toBe(true);
    expect(host.textContent).toContain('Unavailable source · remove');
    expect(discussions.saveDiscussionDraft).not.toHaveBeenCalled();
  });

  it('activates an explicit assistant action without submitting or replacing the composer text', async () => {
    const session = await makeSession();
    await act(async () => root.render(<FeedbackPanel session={session} state={session.state} title="Chapter" documentKind="chapter" selection={null} visible onClose={() => {}} registerSaver={() => {}} assistantAction={{ kind: 'draft', nonce: 1 }} />));
    await waitFor(() => expect(host.querySelector('#discussion-composer')).not.toBeNull());
    await typeInstruction('Keep the quiet ending.');
    await act(async () => root.render(<FeedbackPanel session={session} state={session.state} title="Chapter" documentKind="chapter" selection={null} visible onClose={() => {}} registerSaver={() => {}} assistantAction={{ kind: 'draft', nonce: 2 }} />));
    await waitFor(() => expect(host.querySelector('#continuation-basis')).not.toBeNull());
    expect((host.querySelector('#discussion-composer') as HTMLTextAreaElement).value).toBe('Keep the quiet ending.');
    expect(discussions.startDiscussion).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(host.querySelector('#discussion-composer'));
  });

  it('activates non-chapter Develop as a reviewable whole-document proposal', async () => {
    const session = await makeSession('document-b');
    await act(async () => root.render(<FeedbackPanel session={session} state={session.state} title="World" documentKind="world" selection={null} visible onClose={() => {}} registerSaver={() => {}} assistantAction={{ kind: 'develop', nonce: 1 }} />));
    await waitFor(() => expect(host.querySelector('#discussion-composer')).not.toBeNull());
    await waitFor(() => expect(host.textContent).toContain('Whole document'));
    await typeInstruction('Develop the rules while preserving the existing ideas.');
    expect((host.querySelector('.quoted-scope blockquote') as HTMLElement).textContent).toBe('A quiet chapter.');
    expect(discussions.startDiscussion).not.toHaveBeenCalled();
  });

  it('keeps Send blocked without a scope and serializes Whole chapter only after the explicit action', async () => {
    const session = await makeSession();
    vi.mocked(discussions.startDiscussion).mockImplementation(async request => startResult(session, 'whole-chapter-run', request.operationId));
    await renderPanel(session);
    await click('Suggest edits');
    await typeInstruction('Tighten the chapter arc.');
    const send = () => Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Send') as HTMLButtonElement;
    expect(send().disabled).toBe(true);
    expect((Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Selected paragraphs') as HTMLButtonElement).disabled).toBe(true);
    expect((Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Whole chapter') as HTMLButtonElement).disabled).toBe(false);

    await click('Whole chapter');
    await waitFor(() => expect(host.textContent).toContain('Whole chapter'));
    await waitFor(() => expect(send().disabled).toBe(false));
    await click('Send');
    await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledTimes(1));
    const request = vi.mocked(discussions.startDiscussion).mock.calls[0][0];
    expect(request.scope).toEqual({ kind: 'wholeDocument', start: null, end: null, quote: 'A quiet chapter.', sourceBodyHash: session.state.head.bodyHash });
  });

  it('widens a passage to complete Selected paragraphs only when the author chooses that action', async () => {
    const session = await makeSession();
    const hash = session.state.head.bodyHash;
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'), draft: {
      documentId: 'document', version: '1', text: 'Tighten this paragraph.', intent: 'proposeEdits',
      scope: { kind: 'passage', start: { blockId: 'paragraph-1', utf16Offset: 2 }, end: { blockId: 'paragraph-1', utf16Offset: 7 }, quote: 'quiet', sourceBodyHash: hash }, pinnedDocumentIds: [], previousRunId: null, updatedAt: 'now',
    } });
    vi.mocked(discussions.startDiscussion).mockImplementation(async request => startResult(session, 'blocks-run', request.operationId));
    await renderPanel(session);
    await waitFor(() => expect(host.textContent).toContain('Selected passage'));
    await click('Suggest edits');
    await click('Selected paragraphs');
    await waitFor(() => expect(host.textContent).toContain('Selected paragraphs'));
    await waitFor(() => expect((Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Send') as HTMLButtonElement).disabled).toBe(false));
    await click('Send');
    await waitFor(() => expect(discussions.startDiscussion).toHaveBeenCalledTimes(1));
    const request = vi.mocked(discussions.startDiscussion).mock.calls[0][0];
    expect(request.scope).toEqual({ kind: 'blocks', start: { blockId: 'paragraph-1', utf16Offset: 0 }, end: { blockId: 'paragraph-1', utf16Offset: 16 }, quote: 'A quiet chapter.', sourceBodyHash: hash });
  });

  it('keeps malformed Suggest edits output out of the author conversation', async () => {
    const session = await makeSession();
    const started = startResult(session, 'malformed-run');
    const run = { ...started.run, intent: 'proposeEdits' as const, status: 'completed' as const, outputText: '{"suggestions": [{"title": "raw"}]}' };
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'), threadId: started.threadId, messages: [started.userMessage, { ...started.userMessage, id: 'assistant-message', role: 'assistant', content: run.outputText }], runs: [run] });
    await renderPanel(session);
    expect(host.textContent).not.toContain('{"suggestions"');
    expect(host.textContent).toContain('did not return a usable edit');
    expect(host.textContent).not.toContain('Suggestions are ready');
    const details = host.querySelector('.feedback-note details') as HTMLDetailsElement;
    await act(async () => { details.open = true; details.dispatchEvent(new Event('toggle')); });
    await waitFor(() => expect(details.textContent).toContain(run.outputText));
    expect(host.querySelector('.proposal-panel')).toBeNull();
    expect(session.body).toEqual(emptyBody);
  });

  it.each(['stopped', 'failed', 'interrupted'] as const)('keeps an incomplete %s suggestion response inspectable without claiming it is ready', async status => {
    const session = await makeSession(); const started = startResult(session);
    const run = { ...started.run, intent: 'proposeEdits' as const, status, outputText: 'Retained partial response.' };
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'), threadId: started.threadId, runs: [run], messages: [started.userMessage, { ...started.userMessage, id: 'assistant', role: 'assistant', content: 'Retained partial response. The response could not finish.' }] });
    await renderPanel(session);
    expect(host.textContent).toContain(`The suggestion response is ${status} and cannot be applied.`);
    expect(host.textContent).not.toContain('Suggestions are ready');
    const details = host.querySelector('.feedback-note details') as HTMLDetailsElement;
    await act(async () => { details.open = true; details.dispatchEvent(new Event('toggle')); });
    await waitFor(() => expect(details.textContent).toContain('The response could not finish.'));
    expect(proposals.applyProposal).not.toHaveBeenCalled();
    expect(session.body).toEqual(emptyBody);
  });

  it('shows Stopping until cleanup settles and keeps another attempt unavailable', async () => {
    const session = await makeSession(); const started = startResult(session);
    const run = { ...started.run, status: 'stopping' as const, outputText: 'A saved partial response.' };
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'), threadId: started.threadId, runs: [run], messages: [started.userMessage] });
    await renderPanel(session);
    expect(host.textContent).toContain('Stopping…');
    expect(host.textContent).toContain('A saved partial response.');
    expect([...host.querySelectorAll('button')].find(button => button.textContent === 'Stop response')?.disabled).toBe(true);
    expect([...host.querySelectorAll('button')].find(button => button.textContent === 'Send')?.disabled).toBe(true);
    expect(host.textContent).not.toContain('Prepare another attempt');
    expect(discussions.stopDiscussion).not.toHaveBeenCalled();
    expect(session.body).toEqual(emptyBody);
  });

  it('reconciles then retries only local response storage without sending another request', async () => {
    const session = await makeSession(); const started = startResult(session);
    const run = { ...started.run, status: 'stopping' as const, outputText: 'Saved partial text.' };
    const pendingView = { ...emptyView('document'), threadId: started.threadId, runs: [run], messages: [started.userMessage], workerIssues: [{ runId: run.id, detail: 'The final state could not be saved. Retry locally.' }] };
    const settledView = { ...pendingView, runs: [{ ...run, status: 'stopped' as const }], workerIssues: [] };
    vi.mocked(discussions.readDiscussion).mockResolvedValue(pendingView);
    vi.mocked(discussions.retryDiscussionSave).mockImplementation(async () => {
      vi.mocked(discussions.readDiscussion).mockResolvedValue(settledView);
      return settledView;
    });
    await renderPanel(session);
    expect(host.textContent).toContain('Response needs saving');
    expect(host.textContent).not.toContain('Stopping…');
    await click('Retry saving response');
    await waitFor(() => expect(discussions.retryDiscussionSave).toHaveBeenCalledWith(expect.objectContaining({ writerLease: 'fresh-lease' }), 'document', run.id));
    await waitFor(() => expect(host.textContent).toContain('This response is stopped.'));
    expect(discussions.startDiscussion).not.toHaveBeenCalled();
    expect(discussions.discussionRetry).not.toHaveBeenCalled();
    expect(proposals.applyProposal).not.toHaveBeenCalled();
    expect(session.body).toEqual(emptyBody);
  });

  it('retains the local retry action when saving the response still fails', async () => {
    const session = await makeSession(); const started = startResult(session);
    vi.mocked(discussions.readDiscussion).mockResolvedValue({ ...emptyView('document'), runs: [{ ...started.run, status: 'running' }], workerIssues: [{ runId: started.run.id, detail: 'Retry saving locally.' }] });
    vi.mocked(discussions.retryDiscussionSave).mockRejectedValue({ code: 'PersistenceUnavailable', detail: 'Storage is still unavailable.' });
    await renderPanel(session);
    await click('Retry saving response');
    await waitFor(() => expect(discussions.retryDiscussionSave).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(host.textContent).toContain('Storage is still unavailable.'));
    expect([...host.querySelectorAll('button')].find(button => button.textContent === 'Retry saving response')?.disabled).toBe(false);
    expect(discussions.startDiscussion).not.toHaveBeenCalled();
  });

  it('does not show a completed local retry in a different document after navigation', async () => {
    const source = await makeSession(); const destination = await makeSession('document-b');
    const started = startResult(source); const gate = deferred<discussions.DiscussionView>();
    const view = { ...emptyView('document'), runs: [{ ...started.run, status: 'stopping' as const }], workerIssues: [{ runId: started.run.id, detail: 'Only source document.' }] };
    vi.mocked(discussions.readDiscussion).mockImplementation(async (_access, documentId) => documentId === 'document' ? view : emptyView(documentId));
    vi.mocked(discussions.retryDiscussionSave).mockReturnValue(gate.promise);
    await renderPanel(source); await click('Retry saving response');
    await waitFor(() => expect(discussions.retryDiscussionSave).toHaveBeenCalledTimes(1));
    await renderPanel(destination);
    await act(async () => gate.resolve({ ...view, workerIssues: [] }));
    expect(host.textContent).not.toContain('Only source document.');
    expect(host.textContent).not.toContain('Prepare another attempt');
    expect(discussions.startDiscussion).not.toHaveBeenCalled();
  });
});
