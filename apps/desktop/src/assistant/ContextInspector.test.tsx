// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ContextInspector } from './ContextInspector';
import * as context from '../ipc/context';
import type { ProjectAccess } from '../ipc/projects';
import type { KnowledgeRecord, PossessionRecord } from '../ipc/reviews';

vi.mock('../ipc/context', () => ({
  preparedStoryContext: vi.fn(), preparedStoryContextIsCurrent: vi.fn(), storyContextSnapshot: vi.fn(),
  readStoryContextSource: vi.fn(), reviewedEvidenceHistory: vi.fn(), reviewedPromiseHistory: vi.fn(), reviewedKnowledgeHistory: vi.fn(), searchStoryContext: vi.fn(),
}));
const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const descriptor = (handle: string, title: string): context.SourceDescriptor => ({ handle, displayName: title, source: { projectId: 'project', documentId: handle, revisionId: handle, bodyHash: 'hash' }, kind: 'currentDraft', current: true, coverage: 'verbatim', disclosure: { readerPosition: '1', visibleToCharacters: [], authorOnly: false, futurePrivate: false }, storyTime: null, dependencies: [] });
const first = descriptor('first', 'The promise'); const second = descriptor('second', 'The separation');
const packet: context.CompiledPacket = { messages: [], options: { modelId: 'mock-story-context', maxOutputTokens: '100', tokenAccountingMethod: 'mock' }, receipt: { packetId: 'packet', sessionId: 'context', snapshotId: 'snapshot', invocationOrdinal: '0', sourceHandles: ['first'], coverage: [{ handle: 'first', label: 'fullText', detail: 'verbatim' }], omissions: ['handle:second;reason:optional source omitted by input budget;blocks:2'], inputHash: 'hash', inputTokens: '100', tokenAccountingMethod: 'mock' } };
const frozen: context.FrozenContext = { snapshot: { snapshotId: 'snapshot', projectId: 'project', basis: 'working', target: first.source, contextSourceEpoch: '1', orderingEpoch: '1', disclosurePolicyVersion: '0', sources: [first, second] }, policy: { version: '0', audience: 'authorRoom', readerFrontier: null, characterId: null, characterGrants: [], allowAlternatives: false, allowHistorical: false }, purpose: 'discuss', aliases: {}, excludedSourceCount: 0 };
const navigation: context.FrozenNavigationView = {
  reference: { viewId: 'memory-view', projectId: 'project', operationNamespace: 'namespace', contentHash: 'digest-hash' },
  sourceContextEpoch: '1', disclosurePolicyVersion: '0', dependencies: [second.source],
  candidate: { schemaVersion: 'navigation-digest.v1', source: second.source, items: [{
    text: 'Ren promised to return the key.', uncertainty: 'No later transfer was established.',
    evidence: [{ blockId: 'old-block', fromUtf16: 0, toUtf16: 28, quote: 'Ren promised to return it.' }],
  }] },
};
const reviewedRecord = (id: string, audience: PossessionRecord['audience'], objectLabel: string): PossessionRecord => ({
  id, object: { id: `${id}-object`, label: objectLabel }, holder: { id: `${id}-holder`, label: 'Mei' }, timing: 'atPassage', audience,
  evidence: { blockId: 'first-block', fromUtf16: 0, toUtf16: 17, quote: `${objectLabel} was in Mei's hand.`, quoteHash: 'a'.repeat(64) },
});
let host: HTMLDivElement; let root: Root;
async function render(packetId = 'packet', refreshKey = '1', delivered = true) {
  await act(async () => root.render(<ContextInspector access={access} packetId={packetId} delivered={delivered} refreshKey={refreshKey} />));
}
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); vi.resetAllMocks();
  vi.mocked(context.preparedStoryContext).mockResolvedValue(packet);
  vi.mocked(context.storyContextSnapshot).mockResolvedValue(frozen);
  vi.mocked(context.preparedStoryContextIsCurrent).mockResolvedValue(true);
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('historical context inspection', () => {
  const acceptedSummary: context.ReviewedSummarySet = {
    projectId: 'project', operationNamespace: 'namespace', bundleId: 'bundle', sourceHandle: 'second', summaryHash: 'summary-hash',
    summary: { id: 'accepted-summary', text: 'Mei keeps the promise despite their separation.', audience: 'reader', source: second.source, dependencies: [] },
  };
  it('shows accepted narrative summaries separately and opens their frozen chapter', async () => {
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, reviewedSummaries: [acceptedSummary], navigationViews: [navigation] });
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt,
      reviewedSummaries: [{ sourceHandle: 'second', bundleId: 'bundle', summaryId: 'accepted-summary', summaryHash: 'summary-hash' }],
      navigationOmissions: [{ viewId: 'memory-view', reason: 'acceptedSummaryIncluded' }],
    } });
    vi.mocked(context.readStoryContextSource).mockResolvedValue({ descriptor: second, usedValidatedProjection: false, body: { schemaVersion: 1, body: { type: 'doc', content: [] } }, passages: [] });
    await render();
    expect(host.querySelector('.context-accepted-summary')?.textContent).toContain('Reviewed by you for this saved chapter version');
    expect(host.textContent).toContain('1 accepted summary');
    expect(host.textContent).toContain('an accepted narrative summary was supplied. The original text was not included');
    expect(host.textContent).toContain('an accepted narrative summary was supplied instead');
    const button = host.querySelector('.context-accepted-summary button') as HTMLButtonElement;
    await act(async () => button.click());
    expect(context.readStoryContextSource).toHaveBeenCalledWith(access, 'snapshot', 'second');
  });
  it('withholds private accepted summary text from restricted context inspection', async () => {
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, policy: { ...frozen.policy, audience: 'restrictedWriting' }, reviewedSummaries: [{ ...acceptedSummary, summary: { ...acceptedSummary.summary, audience: 'authorRoom', text: 'Private mentor revelation.' } }] });
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt, reviewedSummaryOmissions: [{ sourceHandle: 'second', reason: 'disclosure' }] } });
    await render();
    expect(host.textContent).not.toContain('Private mentor revelation');
    expect(host.querySelector('.context-accepted-summary')).toBeNull();
    expect(host.textContent).toContain('accepted summary: it is private to the author room');
  });
  it('does not count a different summary revision or hash as delivered', async () => {
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, reviewedSummaries: [acceptedSummary] });
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt, reviewedSummaries: [{ sourceHandle: 'second', bundleId: 'bundle', summaryId: 'wrong-revision', summaryHash: 'summary-hash' }] } });
    await render();
    expect(host.querySelectorAll('.context-accepted-summary')).toHaveLength(1);
    expect(host.querySelector('details[open] > ul .context-accepted-summary')).toBeNull();
  });
  it('separates delivered summaries from original text and opens the exact frozen evidence', async () => {
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, navigationViews: [navigation] });
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt, navigationViews: [navigation.reference] } });
    vi.mocked(context.readStoryContextSource).mockResolvedValue({ descriptor: second, usedValidatedProjection: false, body: { schemaVersion: 1, body: { type: 'doc', content: [] } }, passages: [{ handle: second.handle, source: second.source, blockId: 'old-block', blockOrder: 0, text: 'Exact evidence from the saved chapter.' }] });
    await render();
    expect(host.textContent).toContain('Used · 1 source · 1 generated summary');
    expect(host.textContent).toContain('Available · 2 sources · 1 generated summary');
    expect(host.textContent).toContain('The separation: a generated summary was supplied. The original text was not included.');
    expect(host.querySelector('.context-navigation')?.textContent).toContain('Unreviewed chapter memory');
    expect(host.querySelector('.context-navigation')?.textContent).toContain('Uncertainty: No later transfer was established.');
    expect(host.querySelector('.context-navigation blockquote')?.textContent).toBe('Ren promised to return it.');
    const button = host.querySelector('.context-navigation button') as HTMLButtonElement;
    await act(async () => button.click());
    expect(context.readStoryContextSource).toHaveBeenCalledWith(access, 'snapshot', 'second');
    expect(host.querySelector('[aria-label="Saved story source"]')?.textContent).toContain('Exact evidence from the saved chapter.');
    vi.mocked(context.preparedStoryContextIsCurrent).mockResolvedValue(false);
    await render('packet', 'changed');
    expect(host.textContent).toContain('Needs refresh');
    expect(host.querySelector('.context-navigation')?.textContent).toContain('Ren promised to return the key.');
  });
  it.each([
    ['originalTextIncluded', 'the original text was included instead'],
    ['budget', 'it did not fit within the request budget'],
    ['notSmaller', 'it would not reduce the size of the supplied context'],
  ] as const)('reports an available but undelivered summary: %s', async (reason, explanation) => {
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, navigationViews: [navigation] });
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt, navigationOmissions: [{ viewId: navigation.reference.viewId, reason }] } });
    await render('packet', '1', false);
    expect(host.textContent).toContain('Prepared · 1 source · 0 generated summaries');
    expect(host.querySelector('.context-inspector details[open] .context-navigation')).toBeNull();
    expect(host.textContent).toContain(`The separation summary: ${explanation}.`);
    expect(host.textContent).toContain('Delivery has not been confirmed');
  });
  it('never reads a current chapter as a fallback for a missing exact dependency', async () => {
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, navigationViews: [{ ...navigation, dependencies: [{ ...second.source, revisionId: 'older-revision' }] }] });
    await render();
    const button = host.querySelector('.context-navigation button') as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    await act(async () => button.click());
    expect(context.readStoryContextSource).not.toHaveBeenCalled();
    expect(host.textContent).toContain('Source unavailable');
  });
  it('clears generated text and discards a late evidence failure after project replacement', async () => {
    let reject!: (reason: unknown) => void;
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, navigationViews: [navigation] });
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt, navigationViews: [navigation.reference] } });
    vi.mocked(context.readStoryContextSource).mockReturnValue(new Promise((_resolve, fail) => { reject = fail; }));
    await render();
    await act(async () => (host.querySelector('.context-navigation button') as HTMLButtonElement).click());
    vi.mocked(context.preparedStoryContext).mockRejectedValue({ detail: 'This request belongs to a different project.' });
    await act(async () => root.render(<ContextInspector access={{ ...access, projectId: 'other-project', operationNamespace: 'other-namespace' }} packetId="packet" delivered refreshKey="1" />));
    await act(async () => reject({ detail: 'Old source error including old private text.' }));
    expect(host.textContent).not.toContain('Ren promised');
    expect(host.textContent).not.toContain('Old source error');
    expect(host.textContent).toContain('This request belongs to a different project.');
  });
  it.each(['evidence', 'search'])('clears the entire frozen context if policy is revoked during %s', async action => {
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, navigationViews: [navigation] });
    const failure = { code: 'ContextPolicyChanged', detail: 'Source permissions changed.' };
    vi.mocked(context.readStoryContextSource).mockRejectedValue(failure);
    vi.mocked(context.searchStoryContext).mockRejectedValue(failure);
    await render();
    expect(host.textContent).toContain('Ren promised to return the key.');
    if (action === 'evidence') {
      await act(async () => (host.querySelector('.context-navigation button') as HTMLButtonElement).click());
      expect(context.readStoryContextSource).toHaveBeenCalledOnce();
    } else {
      const input = host.querySelector('.context-search input') as HTMLInputElement;
      await act(async () => {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, 'key');
        input.dispatchEvent(new Event('input', { bubbles: true }));
      });
      await act(async () => host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
      expect(context.searchStoryContext).toHaveBeenCalledOnce();
    }
    expect(host.textContent).toContain('Source permissions changed.');
    expect(host.textContent).not.toContain('Ren promised');
    expect(host.textContent).not.toContain('The promise');
    expect(host.querySelector('.context-search')).toBeNull();
  });
  it('distinguishes author-reviewed earlier prose from the unfinished target without claiming delivery', async () => {
    const reviewedEarlier = { ...second, kind: 'reviewedAuthority' as const };
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen,
      snapshot: { ...frozen.snapshot, basis: 'reviewed', sources: [first, reviewedEarlier], reviewedBasis: {
        projectId: access.projectId, operationNamespace: access.operationNamespace,
        prefix: [{ documentId: second.source.documentId, bundleId: 'bundle', revisionId: second.source.revisionId,
          version: '1', bodyHash: second.source.bodyHash }],
      } },
      purpose: 'continue', policy: { ...frozen.policy, audience: 'restrictedWriting', readerFrontier: '1' },
    });
    await render('packet', '1', false);
    expect(host.querySelector('.context-reviewed-basis')?.textContent).toContain('current chapter is still a working draft');
    expect(host.textContent).toContain('These reviews did not run AI checks');
    expect(host.textContent).toContain('Author-reviewed chapter');
    expect(host.textContent).toContain('Working chapter');
    expect(host.textContent).toContain('Delivery has not been confirmed');
    vi.mocked(context.preparedStoryContextIsCurrent).mockResolvedValue(false);
    await render('packet', 'source-changed', false);
    expect(host.textContent).toContain('Needs refresh');
    expect(host.textContent).toContain('The separation');
    expect(context.readStoryContextSource).not.toHaveBeenCalled();
  });
  it('distinguishes Codex stdin delivery and byte allowances from model understanding', async () => {
    const providerBinding = { providerId:'codex',modelId:'gpt-5.6-luna',reasoning:'max',serviceTier:'priority',profileVersion:'0.153.3',inputLimitBytes:'24576',reservedOutputBytes:'0',reservedProtocolBytes:'0',outputLimitBytes:'65536',accountingMethod:'utf8-byte-count/codex-stdin-application-cap-v1' };
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet,options:{ ...packet.options,modelId:providerBinding.modelId,providerBinding } });
    await render(); expect(host.textContent).toContain('confirms local delivery, not that the model understood every source');
    expect(host.textContent).toContain('not a model token count');
  });
  it('labels reviewed-memory lookup delivery per selected invocation and opens its frozen source', async () => {
    const memoryHistory: context.KnowledgeHistory = {
      characterId: 'character-id', topicId: 'topic-id', labelVariants: ['Mei'], observations: [{
        recordId: 'observation-id', sourceHandle: first.handle, source: first.source, sourceDisplayName: first.displayName, sourceOrder: 0,
        character: { id: 'character-id', label: 'Mei' }, topic: { id: 'topic-id', label: 'the pendant' }, attitude: 'knows', statement: 'Mei knows the pendant is a warning.', timing: 'atPassage', audience: 'reader',
        evidence: { blockId: 'first-block', fromUtf16: 0, toUtf16: 20, quote: 'The pendant is a warning.', quoteHash: 'q'.repeat(64) },
      }], uncertainty: [], incomplete: true,
    };
    const lookup: context.LookupPacketInput = {
      allowance: { maxAdditionalInvocations: 2, totalInputBytes: '73728', totalOutputBytes: '196608' }, completedInvocations: 1, reviewedMemory: 'reviewed-memory.v1',
      exchanges: [{ request: { kind: 'knowledgeHistory', id: 'history-1', characterId: 'character-id', topicId: 'topic-id', limit: 1 }, result: { kind: 'knowledgeHistory', history: memoryHistory, offset: 0, totalObservations: 1, nextOffset: null } }],
    };
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt, lookup } });
    vi.mocked(context.readStoryContextSource).mockResolvedValue({ descriptor: first, usedValidatedProjection: true, body: { schemaVersion: 1, body: { type: 'doc', content: [] } }, passages: [] });
    await act(async () => root.render(<ContextInspector access={access} packetId="packet" delivered={false} lookupDelivery="unconfirmed" refreshKey="1" />));
    expect(host.textContent).toContain('Delivery not confirmed');
    expect(host.textContent).toContain('Mei knows the pendant is a warning.');
    const button = [...host.querySelectorAll('button')].find(item => item.textContent === 'Open exact source') as HTMLButtonElement;
    await act(async () => button.click());
    expect(context.readStoryContextSource).toHaveBeenCalledWith(access, 'snapshot', 'first');
  });
  it('describes an HTTP response as upstream delivery rather than Codex local delivery', async () => {
    const providerBinding: context.ProviderBinding = { providerId: 'openai-compatible:test-endpoint', modelId: 'fiction-v1', reasoning: null, serviceTier: null,
      profileVersion: 'openai-chat-completions.v1', inputLimitBytes: '24576', reservedOutputBytes: '4096', reservedProtocolBytes: '1024', outputLimitBytes: '65536',
      accountingMethod: 'utf8-byte-count/http-request-v1', http: { baseUrl: 'https://example.test/v1', configRevision: '4', stream: true, responseFormat: 'text' } };
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, options: { ...packet.options, modelId: providerBinding.modelId, providerBinding } });
    await render('packet', '1', true);
    expect(host.textContent).toContain('Response headers were received for the prepared request. The saved result may be partial');
    expect(host.textContent).not.toContain('The complete prepared packet was written to Codex');
    await render('packet', '2', false);
    expect(host.textContent).toContain('Prepared sources. Delivery has not been confirmed.');
  });
  it('describes app-server acknowledgment without claiming the entire internal prompt was understood', async () => {
    const appServerDelivery: context.AppServerDelivery = {
      dispatch: { serverGeneration: 'server-1', threadId: 'thread-1', rpcId: 'rpc-1', packetHash: 'a'.repeat(64), requestHash: 'b'.repeat(64) },
      submission: 'acknowledged', turnId: 'turn-1', terminal: 'completed', requestSettled: true, connection: 'reusable',
    };
    await act(async () => root.render(<ContextInspector access={access} packetId="packet" delivered appServerDelivery={appServerDelivery} refreshKey="1" />));
    expect(host.textContent).toContain('The Codex app-server acknowledged the owned turn and received the prepared packet.');
    expect(host.textContent).toContain('This confirms delivery, not that the model understood every source.');
    expect(host.textContent).not.toContain('The complete prepared packet was written to Codex');
  });
  it('shows the exact approved brief separately without retrieving its private origin', async () => {
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt, safeBrief: { text: 'Mei reads the pause as grief.', textHash: 'hash', originMessageId: 'private-origin' } } });
    await render();
    expect(host.querySelector('.context-safe-brief p')?.textContent).toBe('Mei reads the pause as grief.');
    expect(host.textContent).not.toContain('private-origin');
    expect(context.readStoryContextSource).not.toHaveBeenCalled();
    vi.mocked(context.preparedStoryContextIsCurrent).mockResolvedValue(false); await render('packet', 'new-head');
    expect(host.querySelector('.context-safe-brief p')?.textContent).toBe('Mei reads the pause as grief.');
  });
  it('labels required sources from the receipt and offers a separate persistent-source action', async () => {
    const keep = vi.fn(); const next = vi.fn();
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt, mandatorySourceHandles: ['first'] } });
    await act(async () => root.render(<ContextInspector access={access} packetId="packet" delivered refreshKey="1" onPin={next} onKeepSource={keep} />));
    expect(host.querySelector('.context-inspector details[open] li')?.textContent).toContain('Required source');
    const keepButton = host.querySelector('[aria-label="Keep The promise for future discussions"]') as HTMLButtonElement;
    await act(async () => keepButton.click());
    expect(keep).toHaveBeenCalledWith('first'); expect(next).not.toHaveBeenCalled();
    await act(async () => root.render(<ContextInspector access={access} packetId="packet" delivered refreshKey="1" onPin={next} onKeepSource={keep} pinDisabled />));
    await act(async () => keepButton.click()); expect(keep).toHaveBeenCalledOnce(); expect(keepButton.disabled).toBe(true);
  });
  it('shows exact prior exchanges separately from guidance and reports history omissions', async () => {
    const conversation: context.FrozenConversation = { projectId: access.projectId, operationNamespace: access.operationNamespace, documentId: first.source.documentId, threadId: 'thread', omittedTurns: 2, turns: [{ runId: 'run', packetId: 'prior-packet', sourceSnapshotId: 'prior-snapshot', policyVersion: '0', user: { id: 'user-message', content: 'Keep the question open for now.', scope: null }, assistant: { id: 'assistant-message', content: 'An unadopted possibility.\nThe pendant could be a clue.', scope: null } }] };
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt, conversationMessageIds: ['user-message', 'assistant-message'], omittedDiscussionTurns: 2 } });
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, conversation });
    await render();
    expect(host.textContent).toContain('Used · 1 source · 1 earlier exchange');
    expect(host.querySelector('.context-turn')?.textContent).toContain('Keep the question open for now.');
    expect(host.querySelector('.context-turn')?.textContent).toContain('An unadopted possibility.\nThe pendant could be a clue.');
    expect(host.textContent).toContain('2 earlier complete exchanges were not included');
    expect(host.textContent).toContain('not saved guidance or established story facts');
    expect(context.readStoryContextSource).not.toHaveBeenCalled();
  });
  it('shows delivered reviewed details as passage evidence and hides private records in restricted packets', async () => {
    const authorOnly = reviewedRecord('author-detail', 'authorRoom', 'Private seal');
    const reader = reviewedRecord('reader-detail', 'reader', 'Silver key');
    const set: context.ReviewedEvidenceSet = { projectId: access.projectId, operationNamespace: access.operationNamespace, bundleId: 'bundle', recordsHash: 'b'.repeat(64), sourceHandle: first.handle, source: first.source, records: [authorOnly, reader] };
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, policy: { ...frozen.policy, audience: 'restrictedWriting' }, reviewedEvidence: [set] });
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt,
      reviewedEvidence: [{ sourceHandle: first.handle, bundleId: set.bundleId, recordsHash: set.recordsHash, projectionHash: 'p'.repeat(64), completeRecordSet: false, recordIds: [reader.id] }],
      reviewedEvidenceOmissions: [{ sourceHandle: first.handle, bundleId: set.bundleId, recordsHash: set.recordsHash, reason: 'disclosure', count: 1 }, { sourceHandle: first.handle, bundleId: set.bundleId, recordsHash: set.recordsHash, reason: 'budget', count: 2 }],
    } });
    await render();
    expect(host.textContent).toContain('Silver key');
    expect(host.textContent).toContain("Silver key was in Mei's hand.");
    expect(host.textContent).not.toContain('Private seal');
    expect(host.textContent).not.toContain("Private seal was in Mei's hand.");
    expect(host.querySelector('.context-reviewed-evidence')?.textContent).toContain('Silver key');
    expect(host.textContent).toContain('1 private detail from this chapter is excluded from this writing request');
    expect(host.textContent).toContain('1 author-only reviewed detail was withheld');
    expect(host.textContent).toContain('2 reviewed details were withheld');
  });
  it('keeps the exact adopted instruction visible when its packet becomes historical', async () => {
    const record = { handle: 'guidance-v1', projectId: access.projectId, version: { guidanceId: 'guidance', versionId: 'v1', version: '1', scope: 'document' as const, documentId: first.source.documentId, text: 'Keep the ending.\nHer sister survives.', textHash: 'hash', active: true, originMessageId: null, createdAt: 'then' } };
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt, guidanceHandles: [record.handle] } });
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, guidance: [record] });
    await render();
    expect(host.textContent).toContain('Used · 1 source · 1 instruction');
    expect(host.querySelector('.context-guidance p')?.textContent).toBe(record.version.text);
    expect(host.textContent).toContain('version 1');
    vi.mocked(context.preparedStoryContextIsCurrent).mockResolvedValue(false);
    await render('packet', 'guidance-edited');
    expect(host.textContent).toContain('Needs refresh');
    expect(host.querySelector('.context-guidance p')?.textContent).toBe(record.version.text);
    expect(context.readStoryContextSource).not.toHaveBeenCalled();
  });

  it('distinguishes prepared input from delivered sources and renders omitted source titles', async () => {
    await render('packet', '1', false);
    expect(host.textContent).toContain('Prepared · 1 source');
    expect(host.textContent).toContain('Available · 2 sources');
    expect(host.textContent).toContain('Delivery has not been confirmed.');
    expect(host.textContent).toContain('The separation: 2 blocks were not included');
    expect(host.textContent).not.toContain('handle:second');
    await render('packet', '1', true);
    expect(host.textContent).toContain('Used · 1 source');
  });

  it('clears loaded evidence when changed policy denies the historical receipt', async () => {
    await render();
    expect(host.textContent).toContain('The promise');
    vi.mocked(context.preparedStoryContext).mockRejectedValue({ code: 'ContextPolicyChanged', detail: 'Source permissions changed.' });
    await render('packet', '2');
    expect(host.textContent).toContain('Source permissions changed.');
    expect(host.textContent).not.toContain('The promise');
  });

  it('does not display a late source read after switching to another packet', async () => {
    let resolve!: (value: context.SourceRead) => void;
    vi.mocked(context.readStoryContextSource).mockReturnValue(new Promise(accept => { resolve = accept; }));
    await render();
    const button = Array.from(host.querySelectorAll('button')).find(item => item.textContent === 'The promise')!;
    await act(async () => button.click());
    await render('different-packet');
    await act(async () => resolve({ descriptor: first, usedValidatedProjection: false, body: { schemaVersion: 1, body: { type: 'doc', content: [] } }, passages: [{ handle: 'first', source: first.source, blockId: 'block', blockOrder: 0, text: 'Evidence from the old request.' }] }));
    expect(host.textContent).not.toContain('Evidence from the old request.');
  });
});


describe('recorded object history', () => {
  const detail = reviewedRecord('evidence', 'reader', 'Key');
  const setupHistory = () => {
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, reviewedEvidence: [{ projectId: 'project', operationNamespace: 'namespace', bundleId: 'bundle', recordsHash: 'hash', sourceHandle: first.handle, source: first.source, records: [detail] }] });
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt, reviewedEvidence: [{ bundleId: 'bundle', recordsHash: 'hash', projectionHash: 'hash', sourceHandle: first.handle, recordIds: [detail.id], completeRecordSet: true }] } });
    return { snapshotId: 'snapshot', current: true, history: { objectId: detail.object.id, labelVariants: ['Key'], observations: [{ recordId: detail.id, sourceHandle: first.handle, source: first.source, sourceDisplayName: first.displayName, sourceOrder: 0, ...detail }], uncertainty: [], incomplete: true } } satisfies context.ReviewedHistoryResult;
  };
  const historyButton = () => host.querySelector('button[aria-label="Find recorded history for Key"]') as HTMLButtonElement;
  it('queries the exact object identity and opens its original retained chapter', async () => {
    const result = setupHistory();
    vi.mocked(context.reviewedEvidenceHistory).mockResolvedValue(result);
    vi.mocked(context.readStoryContextSource).mockResolvedValue({ descriptor: first, body: { schemaVersion: 1, body: { type: 'doc', content: [] } }, passages: [], usedValidatedProjection: false });
    await render(); await act(async () => historyButton().click());
    expect(context.reviewedEvidenceHistory).toHaveBeenCalledExactlyOnceWith(access, 'snapshot', detail.object.id);
    const history = host.querySelector('[aria-label="Recorded object history"]')!;
    expect(history.textContent).toContain('this does not establish the current holder');
    expect(history.textContent).toContain(detail.evidence.quote);
    await act(async () => (history.querySelector('button[aria-label^="Read"]') as HTMLButtonElement).click());
    expect(context.readStoryContextSource).toHaveBeenCalledExactlyOnceWith(access, 'snapshot', 'first');
  });
  it('discards late history after a request refresh', async () => {
    const result = setupHistory(); let resolve!: (value: context.ReviewedHistoryResult) => void;
    vi.mocked(context.reviewedEvidenceHistory).mockImplementation(() => new Promise(accept => { resolve = accept; }));
    await render(); await act(async () => historyButton().click());
    await render('next-packet', '2');
    await act(async () => resolve(result));
    expect(host.querySelector('[aria-label="Recorded object history"]')).toBeNull();
    expect(historyButton().disabled).toBe(false);
  });
  it('clears retained evidence when the history read reports revoked permissions', async () => {
    setupHistory(); vi.mocked(context.reviewedEvidenceHistory).mockRejectedValue({ code: 'ContextPolicyChanged', detail: 'Source permissions changed.' });
    await render(); await act(async () => historyButton().click());
    expect(host.querySelector('.context-reviewed-evidence')).toBeNull();
    expect(host.textContent).toContain('Source permissions changed.');
  });
});

describe('recorded promise context and history', () => {
  const promise = (id: string, audience: 'reader' | 'authorRoom' = 'reader') => ({ id, promise: { id: 'return-key', label: 'Return the key' }, phase: 'setup' as const, timing: 'unknown' as const, note: 'Ren promises to return the key.', audience, evidence: reviewedRecord(id, audience, 'Key').evidence });
  const setup = (restricted = false) => {
    const reader = promise('setup'); const hidden = { ...promise('private', 'authorRoom'), promise: { id: 'secret', label: 'Private betrayal promise' }, note: 'Private plan that must stay hidden.' };
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, policy: { ...frozen.policy, audience: restricted ? 'restrictedWriting' : 'authorRoom' }, reviewedPromises: [{ projectId: 'project', operationNamespace: 'namespace', bundleId: 'promise-bundle', recordsHash: 'promise-hash', sourceHandle: first.handle, source: first.source, records: [reader, hidden] }] });
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt, reviewedPromises: [{ bundleId: 'promise-bundle', recordsHash: 'promise-hash', projectionHash: 'projection', sourceHandle: first.handle, recordIds: [reader.id], completeRecordSet: false }], reviewedPromiseOmissions: [{ sourceHandle: first.handle, bundleId: 'promise-bundle', recordsHash: 'promise-hash', reason: restricted ? 'disclosure' : 'budget', count: 1 }] } });
    return { snapshotId: 'snapshot', current: true, history: { promiseId: reader.promise.id, labelVariants: [reader.promise.label], observations: [{ ...reader, recordId: reader.id, sourceHandle: first.handle, source: first.source, sourceDisplayName: first.displayName, sourceOrder: 0 }], uncertainty: ['unknownTiming'], incomplete: true, hasRecordedPayoff: false } } satisfies context.ReviewedPromiseHistoryResult;
  };
  const historyButton = () => host.querySelector('button[aria-label="Find promise history for Return the key"]') as HTMLButtonElement;
  it('separates delivered promise records from available records and explains budget omissions', async () => {
    setup(); await render();
    const used = [...host.querySelectorAll('details')].find(item => item.querySelector(':scope > summary')?.textContent?.startsWith('Used'))!;
    expect(used.textContent).toContain('Ren promises'); expect(used.textContent).not.toContain('Private betrayal promise');
    expect(host.textContent).toContain('Private betrayal promise'); expect(host.textContent).toContain('promise detail was withheld');
  });
  it('hides private labels and notes even when they exist in the authenticated frozen set', async () => {
    setup(true); await render(); expect(host.textContent).toContain('Ren promises');
    expect(host.textContent).not.toContain('Private betrayal promise'); expect(host.textContent).not.toContain('Private plan that must stay hidden');
    expect(host.textContent).toContain('reader disclosure policy');
  });
  it('queries the exact promise and opens the retained source without asserting resolution', async () => {
    const result = setup(); vi.mocked(context.reviewedPromiseHistory).mockResolvedValue({ ...result, current: false });
    vi.mocked(context.readStoryContextSource).mockResolvedValue({ descriptor: first, body: { schemaVersion: 1, body: { type: 'doc', content: [] } }, passages: [], usedValidatedProjection: false });
    await render(); await act(async () => historyButton().click());
    expect(context.reviewedPromiseHistory).toHaveBeenCalledExactlyOnceWith(access, 'snapshot', 'return-key');
    const history = host.querySelector('[aria-label="Recorded promise history"]')!;
    expect(history.textContent).toContain('does not prove the promise remains unresolved'); expect(history.textContent).toContain('Earlier story version');
    await act(async () => (history.querySelector('button[aria-label^="Read"]') as HTMLButtonElement).click());
    expect(context.readStoryContextSource).toHaveBeenCalledExactlyOnceWith(access, 'snapshot', 'first');
  });
  it('drops a late or misbound promise-history reply', async () => {
    const result = setup(); let finish!: (value: context.ReviewedPromiseHistoryResult) => void;
    vi.mocked(context.reviewedPromiseHistory).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    await render(); await act(async () => historyButton().click()); await render('other-packet', '2'); await act(async () => finish(result));
    expect(host.querySelector('[aria-label="Recorded promise history"]')).toBeNull();
    vi.mocked(context.reviewedPromiseHistory).mockResolvedValue({ ...result, history: { ...result.history, promiseId: 'other-promise' } });
    await act(async () => historyButton().click()); expect(host.textContent).toContain('promise history did not match');
    expect(host.querySelector('[aria-label="Recorded promise history"]')).toBeNull();
  });
  it('distinguishes reader-disclosed and author-only observations in promise history', async () => {
    const result = setup();
    result.history.observations.push({ ...result.history.observations[0], recordId: 'private-payoff', audience: 'authorRoom', note: 'Private payoff plan.' });
    vi.mocked(context.reviewedPromiseHistory).mockResolvedValue(result);
    await render(); await act(async () => historyButton().click());
    const rows = host.querySelectorAll('[aria-label="Recorded promise history"] ol > li');
    expect(rows[0].textContent).toContain('Explicitly reader-disclosed');
    expect(rows[1].textContent).toContain('Author room only');
  });
  it('removes loaded promise history and source evidence after policy revocation', async () => {
    const result = setup(); vi.mocked(context.reviewedPromiseHistory).mockResolvedValueOnce(result).mockRejectedValueOnce({ code: 'ContextPolicyChanged', detail: 'Promise permissions changed.' });
    await render(); await act(async () => historyButton().click()); expect(host.querySelector('[aria-label="Recorded promise history"]')).not.toBeNull();
    await act(async () => historyButton().click()); expect(host.querySelector('[aria-label="Recorded promise history"]')).toBeNull();
    expect(host.textContent).not.toContain('Ren promises'); expect(host.textContent).toContain('Promise permissions changed.');
  });
});

describe('recorded character knowledge context and history', () => {
  const knowledge = (id: string, audience: KnowledgeRecord['audience'] = 'reader'): KnowledgeRecord => ({
    id, character: { id: 'mei', label: 'Mei' }, topic: { id: 'gate', label: 'The gate' }, attitude: 'believes', statement: id === 'private' ? 'Private plan.' : 'Mei believes the gate is watched.', timing: 'unknown', audience, evidence: reviewedRecord(id, audience, 'Gate').evidence,
  });
  const setup = (restricted = false) => {
    const reader = knowledge('reader'); const hidden = knowledge('private', 'authorRoom');
    vi.mocked(context.storyContextSnapshot).mockResolvedValue({ ...frozen, policy: { ...frozen.policy, audience: restricted ? 'restrictedWriting' : 'authorRoom' }, reviewedKnowledge: [{ projectId: 'project', operationNamespace: 'namespace', bundleId: 'knowledge-bundle', recordsHash: 'knowledge-hash', sourceHandle: first.handle, source: first.source, records: [reader, hidden] }] });
    vi.mocked(context.preparedStoryContext).mockResolvedValue({ ...packet, receipt: { ...packet.receipt, reviewedKnowledge: [{ bundleId: 'knowledge-bundle', recordsHash: 'knowledge-hash', projectionHash: 'projection', sourceHandle: first.handle, recordIds: [reader.id], completeRecordSet: false }], reviewedKnowledgeOmissions: [{ sourceHandle: first.handle, bundleId: 'knowledge-bundle', recordsHash: 'knowledge-hash', reason: restricted ? 'disclosure' : 'budget', count: 1 }] } });
    return { snapshotId: 'snapshot', current: true, history: { characterId: 'mei', topicId: 'gate', labelVariants: ['Mei'], observations: [{ ...reader, recordId: reader.id, sourceHandle: first.handle, source: first.source, sourceDisplayName: first.displayName, sourceOrder: 0 }], uncertainty: ['noEligibleObservations', 'multipleRecordedAttitudes'], incomplete: true } } satisfies context.ReviewedKnowledgeHistoryResult;
  };
  const historyButton = () => host.querySelector('button[aria-label="Find knowledge history for Mei about The gate"]') as HTMLButtonElement;
  it('separates delivered knowledge from available private records', async () => {
    setup(true); await render();
    const used = [...host.querySelectorAll('details')].find(item => item.querySelector(':scope > summary')?.textContent?.startsWith('Used'))!;
    expect(used.textContent).toContain('Mei believes the gate'); expect(used.textContent).not.toContain('Private plan'); expect(host.textContent).toContain('reader disclosure policy');
  });
  it('queries character and topic history and opens the exact frozen source', async () => {
    const result = setup(); vi.mocked(context.reviewedKnowledgeHistory).mockResolvedValue({ ...result, current: false });
    vi.mocked(context.readStoryContextSource).mockResolvedValue({ descriptor: first, body: { schemaVersion: 1, body: { type: 'doc', content: [] } }, passages: [], usedValidatedProjection: false });
    await render(); await act(async () => historyButton().click());
    expect(context.reviewedKnowledgeHistory).toHaveBeenCalledExactlyOnceWith(access, 'snapshot', 'mei', 'gate');
    const history = host.querySelector('[aria-label="Character knowledge history"]')!;
    expect(history.textContent).toContain('does not establish that the character is unaware'); expect(history.textContent).toContain('Earlier story version');
    await act(async () => (history.querySelector('button[aria-label^="Read"]') as HTMLButtonElement).click()); expect(context.readStoryContextSource).toHaveBeenCalledExactlyOnceWith(access, 'snapshot', 'first');
  });
});
