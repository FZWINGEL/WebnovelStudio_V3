import { errorTextFor } from '../kernel';
import { useEffect, useRef, useState } from 'react';
import {
  preparedStoryContext, preparedStoryContextIsCurrent, readStoryContextSource, reviewedEvidenceHistory, reviewedKnowledgeHistory, reviewedPromiseHistory, searchStoryContext, storyContextSnapshot,
  type AppServerDelivery, type CompiledPacket, type FrozenContext, type FrozenNavigationView, type ReviewedEvidenceSet, type SourceDescriptor, type SourceRead, type SourceRef,
} from '../ipc/context';
import type { ProjectAccess } from '../ipc/projects';
import { EvidenceHistoryView } from './EvidenceHistoryView';
import { PromiseHistoryView } from './PromiseHistoryView';
import { KnowledgeHistoryView } from './KnowledgeHistoryView';
import { permittedPromiseRows, PromiseContextRows } from './PromiseContextRows';
import { permittedKnowledgeRows, KnowledgeContextRows } from './KnowledgeContextRows';
import { LookupContextView, type LookupDeliveryState } from './LookupContextView';
import { SourceVersionComparison } from './SourceVersionComparison';

const message = errorTextFor('Could not read this story context. Try again.');
function sameSource(left: SourceRef, right: SourceRef): boolean {
  return left.projectId === right.projectId && left.documentId === right.documentId
    && left.revisionId === right.revisionId && left.bodyHash === right.bodyHash;
}

/** Reads the immutable receipt; never reconstructs a past request from current prose. */
export function ContextInspector({ access, packetId, delivered, appServerDelivery, lookupDelivery, refreshKey, onPin, onKeepSource, pinDisabled = false, showVersionLinks = false }: {
  access: ProjectAccess; packetId: string; delivered: boolean; refreshKey: string;
  appServerDelivery?: AppServerDelivery;
  lookupDelivery?: LookupDeliveryState;
  onPin?: (documentId: string, title: string) => void;
  onKeepSource?: (documentId: string) => void;
  pinDisabled?: boolean;
  showVersionLinks?: boolean;
}) {
  const [state, setState] = useState<{ packet: CompiledPacket; frozen: FrozenContext; current: boolean } | null>(null);
  const [source, setSource] = useState<SourceRead | null>(null);
  const [query, setQuery] = useState('');
  const [search, setSearch] = useState<Awaited<ReturnType<typeof searchStoryContext>> | null>(null);
  const [history, setHistory] = useState<Awaited<ReturnType<typeof reviewedEvidenceHistory>> | null>(null);
  const [promiseHistory, setPromiseHistory] = useState<Awaited<ReturnType<typeof reviewedPromiseHistory>> | null>(null);
  const [knowledgeHistory, setKnowledgeHistory] = useState<Awaited<ReturnType<typeof reviewedKnowledgeHistory>> | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [reload, setReload] = useState(0);
  const identity = `${access.projectId}/${access.operationNamespace}/${access.session}/${access.writerLease}/${packetId}`;
  const active = useRef(identity); active.current = identity;
  const sourceRequest = useRef(0);
  useEffect(() => {
    let cancelled = false;
    setError(''); setState(null); setSource(null); setSearch(null); setHistory(null); setPromiseHistory(null); setKnowledgeHistory(null); setBusy(false); sourceRequest.current += 1;
    void (async () => {
      try {
        const packet = await preparedStoryContext(access, packetId);
        const [frozen, current] = await Promise.all([
          storyContextSnapshot(access, packet.receipt.snapshotId), preparedStoryContextIsCurrent(access, packetId),
        ]);
        if (!cancelled) setState({ packet, frozen, current });
      } catch (reason) { if (!cancelled) setError(message(reason)); }
    })();
    return () => { cancelled = true; sourceRequest.current += 1; };
  }, [identity, refreshKey, reload]);
  function reportReadFailure(reason: unknown) {
    if (reason && typeof reason === 'object' && 'code' in reason && reason.code === 'ContextPolicyChanged') {
      setState(null); setSource(null); setSearch(null); setHistory(null); setPromiseHistory(null); setKnowledgeHistory(null); setBusy(false); sourceRequest.current += 1;
    }
    setError(message(reason));
  }
  async function read(item: SourceDescriptor) {
    if (!state) return;
    const captured = identity; const request = ++sourceRequest.current;
    setError(''); setSource(null); setBusy(true);
    try {
      const result = await readStoryContextSource(access, state.frozen.snapshot.snapshotId, item.handle);
      if (active.current === captured && sourceRequest.current === request) setSource(result);
    } catch (reason) { if (active.current === captured && sourceRequest.current === request) reportReadFailure(reason); }
    finally { if (active.current === captured && sourceRequest.current === request) setBusy(false); }
  }
  async function find() {
    if (!state || busy || !query.trim()) return;
    const captured = identity; const request = ++sourceRequest.current;
    setBusy(true); setError(''); setSearch(null);
    try {
      const result = await searchStoryContext({ access, snapshotId: state.frozen.snapshot.snapshotId, query, mode: 'literal', limit: 20 });
      if (active.current === captured && sourceRequest.current === request) setSearch(result);
    } catch (reason) { if (active.current === captured && sourceRequest.current === request) reportReadFailure(reason); }
    finally { if (active.current === captured && sourceRequest.current === request) setBusy(false); }
  }
  const items = state?.frozen.snapshot.sources ?? [];
  async function findHistory(objectId: string) {
    if (!state || busy) return;
    const captured = identity; const request = ++sourceRequest.current;
    setBusy(true); setError(''); setHistory(null);
    try {
      const result = await reviewedEvidenceHistory(access, state.frozen.snapshot.snapshotId, objectId);
      if (active.current !== captured || sourceRequest.current !== request) return;
      if (result.snapshotId !== state.frozen.snapshot.snapshotId || result.history.objectId !== objectId) throw new Error('The recorded history did not match this request. Try reading it again.');
      setHistory(result);
    } catch (reason) { if (active.current === captured && sourceRequest.current === request) reportReadFailure(reason); }
    finally { if (active.current === captured && sourceRequest.current === request) setBusy(false); }
  }
  async function findPromiseHistory(promiseId: string) {
    if (!state || busy) return;
    const captured = identity; const request = ++sourceRequest.current;
    setBusy(true); setError(''); setPromiseHistory(null);
    try {
      const result = await reviewedPromiseHistory(access, state.frozen.snapshot.snapshotId, promiseId);
      if (active.current !== captured || sourceRequest.current !== request) return;
      if (result.snapshotId !== state.frozen.snapshot.snapshotId || result.history.promiseId !== promiseId) throw new Error('The promise history did not match this request. Try reading it again.');
      setPromiseHistory(result);
    } catch (reason) { if (active.current === captured && sourceRequest.current === request) reportReadFailure(reason); }
    finally { if (active.current === captured && sourceRequest.current === request) setBusy(false); }
  }
  async function findKnowledgeHistory(characterId: string, topicId: string | null) {
    if (!state || busy) return;
    const captured = identity; const request = ++sourceRequest.current;
    setBusy(true); setError(''); setKnowledgeHistory(null);
    try {
      const result = await reviewedKnowledgeHistory(access, state.frozen.snapshot.snapshotId, characterId, topicId);
      if (active.current !== captured || sourceRequest.current !== request) return;
      if (result.snapshotId !== state.frozen.snapshot.snapshotId || result.history.characterId !== characterId || result.history.topicId !== topicId) throw new Error('The character knowledge history did not match this request. Try reading it again.');
      setKnowledgeHistory(result);
    } catch (reason) { if (active.current === captured && sourceRequest.current === request) reportReadFailure(reason); }
    finally { if (active.current === captured && sourceRequest.current === request) setBusy(false); }
  }
  const supplied = new Set(state?.packet.receipt.sourceHandles ?? []);
  const guidance = state?.frozen.guidance ?? [];
  const suppliedGuidance = new Set(state?.packet.receipt.guidanceHandles ?? []);
  const conversation = state?.frozen.conversation?.turns ?? [];
  const suppliedMessages = new Set(state?.packet.receipt.conversationMessageIds ?? []);
  const suppliedTurns = conversation.filter(turn => suppliedMessages.has(turn.user.id) && suppliedMessages.has(turn.assistant.id));
  const omittedTurns = state?.packet.receipt.omittedDiscussionTurns ?? 0;
  const navigation = state?.frozen.navigationViews ?? [];
  const suppliedNavigation = navigation.filter(view => state?.packet.receipt.navigationViews?.some(ref =>
    ref.viewId === view.reference.viewId && ref.contentHash === view.reference.contentHash
    && ref.projectId === view.reference.projectId && ref.operationNamespace === view.reference.operationNamespace));
  const navigationOmissions = state?.packet.receipt.navigationOmissions ?? [];
  const reviewedEvidence = state?.frozen.reviewedEvidence ?? [];
  const reviewedEvidenceCoverage = state?.packet.receipt.reviewedEvidence ?? [];
  const reviewedEvidenceOmissions = state?.packet.receipt.reviewedEvidenceOmissions ?? [];
  const restrictedAudience = state?.frozen.policy.audience === 'restrictedWriting';
  const acceptedSummaries = (state?.frozen.reviewedSummaries ?? []).filter(set =>
    !restrictedAudience || set.summary.audience === 'reader');
  const summaryCoverage = state?.packet.receipt.reviewedSummaries ?? [];
  const suppliedSummaries = acceptedSummaries.filter(set => summaryCoverage.some(receipt =>
    receipt.sourceHandle === set.sourceHandle && receipt.bundleId === set.bundleId
      && receipt.summaryId === set.summary.id && receipt.summaryHash === set.summaryHash));
  const summaryOmissions = state?.packet.receipt.reviewedSummaryOmissions ?? [];
  const reviewedPromises = state?.frozen.reviewedPromises ?? [];
  const promiseCoverage = state?.packet.receipt.reviewedPromises ?? [];
  const promiseOmissions = state?.packet.receipt.reviewedPromiseOmissions ?? [];
  const suppliedPromises = permittedPromiseRows(reviewedPromises, promiseCoverage, restrictedAudience, true);
  const availablePromises = permittedPromiseRows(reviewedPromises, promiseCoverage, restrictedAudience, false);
  const reviewedKnowledge = state?.frozen.reviewedKnowledge ?? [];
  const knowledgeCoverage = state?.packet.receipt.reviewedKnowledge ?? [];
  const knowledgeOmissions = state?.packet.receipt.reviewedKnowledgeOmissions ?? [];
  const suppliedKnowledge = permittedKnowledgeRows(reviewedKnowledge, knowledgeCoverage, restrictedAudience, true);
  const availableKnowledge = permittedKnowledgeRows(reviewedKnowledge, knowledgeCoverage, restrictedAudience, false);
  const readHandle = (handle: string, expectedSource?: SourceRef) => {
    const item = items.find(source => source.handle === handle && (!expectedSource || sameSource(source.source, expectedSource)));
    if (item) void read(item);
  };
  function promiseRows(used: boolean) {
    return <PromiseContextRows key={`${identity}/${used}`} rows={used ? suppliedPromises : availablePromises} sources={items} used={used} busy={busy} onRead={readHandle} onHistory={id => void findPromiseHistory(id)} />;
  }
  function knowledgeRows(used: boolean) {
    return <KnowledgeContextRows key={`${identity}/knowledge/${used}`} rows={used ? suppliedKnowledge : availableKnowledge} sources={items} used={used} busy={busy} onRead={readHandle} onHistory={(characterId, topicId) => void findKnowledgeHistory(characterId, topicId)} />;
  }
  function permittedReviewedRecord(record: ReviewedEvidenceSet['records'][number]): boolean {
    return !restrictedAudience || record.audience === 'reader';
  }
  function reviewedSourceName(sourceHandle: string): string {
    return items.find(item => item.handle === sourceHandle)?.displayName ?? 'Source unavailable';
  }
  function reviewedRecords(used: boolean) {
    return reviewedEvidence.flatMap(set => {
      const coverage = reviewedEvidenceCoverage.find(item => item.sourceHandle === set.sourceHandle
        && item.bundleId === set.bundleId && item.recordsHash === set.recordsHash);
      const delivered = new Set(coverage?.recordIds ?? []);
      return set.records.filter(permittedReviewedRecord).filter(record => !used || delivered.has(record.id)).map(record => ({ set, record, coverage }));
    });
  }
  function reviewedRows(used: boolean) {
    return reviewedRecords(used).map(({ set, record, coverage }) => <li key={`${set.bundleId}/${record.id}`} className="context-reviewed-evidence">
      <strong>{record.object.label}</strong>
      <button className="quiet-button" disabled={busy} onClick={() => void findHistory(record.object.id)} aria-label={`Find recorded history for ${record.object.label}`}>Find recorded history</button>
      <span className="context-detail">{record.holder?.label ?? 'Holder unknown'} · {record.timing === 'atPassage' ? 'Known at this passage' : record.timing === 'earlier' ? 'Known earlier' : 'Timing unknown'} · {record.audience === 'reader' ? 'Reader-visible' : 'Author room only'}</span>
      <blockquote>{record.evidence.quote}</blockquote>
      <span className="context-detail">Author-recorded evidence from {reviewedSourceName(set.sourceHandle)}. {record.timing === 'atPassage' ? 'Known at this passage; later transfers may be missing.' : record.timing === 'earlier' ? 'Known earlier; later transfers may be missing.' : 'Timing is unknown; later transfers may be missing.'}{used && coverage && !coverage.completeRecordSet ? ' Some reviewed details were not included.' : ''}</span>
    </li>);
  }
  function reviewedProvenanceRows(used: boolean) {
    if (!restrictedAudience) return [];
    return reviewedEvidence.flatMap(set => {
      const coverage = reviewedEvidenceCoverage.find(item => item.sourceHandle === set.sourceHandle
        && item.bundleId === set.bundleId && item.recordsHash === set.recordsHash);
      if (used && !coverage) return [];
      const privateCount = set.records.filter(record => record.audience !== 'reader').length;
      if (!privateCount) return [];
      return <li key={`${set.bundleId}/provenance`} className="context-reviewed-evidence-provenance">
        <strong>Reviewed set · {reviewedSourceName(set.sourceHandle)}</strong>
        <span className="context-detail">{privateCount} private {privateCount === 1 ? 'detail from this chapter is' : 'details from this chapter are'} excluded from this writing request. Only reader-visible details are shown.</span>
      </li>;
    });
  }
  function reviewedOmission(text: typeof reviewedEvidenceOmissions[number]): string {
    const source = reviewedSourceName(text.sourceHandle);
    return String(text.reason).toLowerCase() === 'disclosure'
      ? `${text.count} author-only reviewed ${text.count === 1 ? 'detail was' : 'details were'} withheld from ${source} by the reader disclosure policy.`
      : `${text.count} reviewed ${text.count === 1 ? 'detail was' : 'details were'} withheld from ${source} by the request budget.`;
  }
  function knowledgeOmission(text: typeof knowledgeOmissions[number]): string {
    const source = reviewedSourceName(text.sourceHandle);
    return text.reason === 'disclosure'
      ? `${text.count} author-only character knowledge ${text.count === 1 ? 'observation was' : 'observations were'} withheld from ${source} by the reader disclosure policy.`
      : `${text.count} character knowledge ${text.count === 1 ? 'observation was' : 'observations were'} withheld from ${source} by the request budget.`;
  }
  function navigationSource(view: FrozenNavigationView): SourceDescriptor | undefined {
    if (view.dependencies.length !== 1 || !sameSource(view.dependencies[0], view.candidate.source)) return undefined;
    return items.find(item => sameSource(item.source, view.dependencies[0]));
  }
  function summaryRows(used: boolean) {
    return (used ? suppliedSummaries : acceptedSummaries).map(set => {
      const original = items.find(item => item.handle === set.sourceHandle && sameSource(item.source, set.summary.source));
      return <li key={set.summary.id} className="context-accepted-summary"><details>
        <summary>Accepted narrative summary · {reviewedSourceName(set.sourceHandle)}</summary>
        <p className="small-copy">Reviewed by you for this saved chapter version. {set.summary.audience === 'reader' ? 'Approved for reader-facing writing.' : 'Author room only.'} The original prose remains available.</p>
        <p style={{ whiteSpace: 'pre-wrap' }}>{set.summary.text}</p>
        <button className="text-button" disabled={!original || busy} onClick={() => original && void read(original)}>Open summary source{original ? ` · ${original.displayName}` : ''}</button>
      </details></li>;
    });
  }
  function summaryOmission(item: typeof summaryOmissions[number]): string {
    const reason = item.reason === 'disclosure' ? 'it is private to the author room'
      : item.reason === 'originalTextIncluded' ? 'the original prose was supplied instead'
        : item.reason === 'notSmaller' ? 'it would not reduce the supplied context'
          : 'it did not fit within the request budget';
    return `${reviewedSourceName(item.sourceHandle)} accepted summary: ${reason}.`;
  }
  function navigationRows(used: boolean) {
    return (used ? suppliedNavigation : navigation).map(view => {
      const original = navigationSource(view);
      return <li key={view.reference.viewId} className="context-navigation"><details>
        <summary>Generated summary · {original?.displayName ?? 'Source unavailable'}</summary>
        <p className="small-copy">Unreviewed chapter memory. It may contain mistakes and does not establish story facts. The original prose remains available.</p>
        {view.candidate.items.map((item, index) => <div key={index}>
          <p>{item.text}</p>
          {item.uncertainty && <p className="small-copy">Uncertainty: {item.uncertainty}</p>}
          <details><summary>Evidence for this summary</summary>{item.evidence.map((evidence, quoteIndex) => <blockquote key={quoteIndex}>{evidence.quote}</blockquote>)}</details>
        </div>)}
        <button className="text-button" disabled={!original} onClick={() => original && void read(original)}>Open original evidence{original ? ` · ${original.displayName}` : ''}</button>
      </details></li>;
    });
  }
  function navigationOmission(viewId: string, reason: string): string {
    const view = navigation.find(item => item.reference.viewId === viewId);
    const title = view ? navigationSource(view)?.displayName ?? 'A chapter' : 'A chapter';
    const explanation = reason === 'originalTextIncluded' ? 'the original text was included instead'
      : reason === 'acceptedSummaryIncluded' ? 'an accepted narrative summary was supplied instead'
      : reason === 'notSmaller' ? 'it would not reduce the size of the supplied context'
        : 'it did not fit within the request budget';
    return `${title} summary: ${explanation}.`;
  }
  function conversationRows(used: boolean) {
    return [...(used ? suppliedTurns : conversation)].reverse().map((turn, index) => <li key={turn.runId} className="context-turn"><details>
      <summary>Earlier exchange {index + 1}</summary>
      <p className="small-copy">Prior discussion, not saved guidance or established story facts.</p>
      <strong>You</strong>{turn.user.scope && <blockquote>{turn.user.scope.quote}</blockquote>}<p>{turn.user.content}</p>
      <strong>Assistant</strong><p>{turn.assistant.content}</p>
    </details></li>);
  }
  function guidanceRows(used: boolean) {
    return guidance.filter(item => !used || suppliedGuidance.has(item.handle)).map(item => <li key={item.handle} className="context-guidance"><strong>Author guidance · {item.version.scope === 'project' ? 'Project' : item.version.scope === 'request' ? 'This request' : 'This document'} · version {item.version.version}</strong><p>{item.version.text}</p><span className="small-copy">Adopted writing instruction. It does not establish a story fact.</span></li>);
  }
  function omission(text: string): string {
    const match = /^handle:([^;]+);reason:(.*)$/u.exec(text);
    if (!match) return text;
    const title = items.find(item => item.handle === match[1])?.displayName ?? 'A source';
    if (suppliedSummaries.some(set => set.sourceHandle === match[1])) return `${title}: an accepted narrative summary was supplied. The original text was not included.`;
    if (suppliedNavigation.some(view => navigationSource(view)?.handle === match[1])) return `${title}: a generated summary was supplied. The original text was not included.`;
    const count = /(?:^|;)(?:blocks|remaining):(\d+)/u.exec(match[2]);
    return `${title}: ${count ? `${count[1]} blocks were not included` : 'some text was not included'} within the request budget.`;
  }
  function row(item: SourceDescriptor) {
    const coverage = state?.packet.receipt.coverage.find(entry => entry.handle === item.handle);
    return <li key={item.handle}>
      <button className="text-button" onClick={() => void read(item)}>{item.displayName}</button>
      {showVersionLinks && state && <SourceVersionComparison key={`${identity}/${refreshKey}/${reload}/${item.handle}`} access={access} snapshotId={state.frozen.snapshot.snapshotId} source={item} />}
      {state?.frozen.snapshot.basis === 'reviewed' && <span className="context-detail">{item.kind === 'reviewedAuthority' ? 'Author-reviewed chapter' : 'Working chapter'}</span>}
      {coverage && <span className="context-detail">{coverage.label === 'fullText' ? 'Full text' : coverage.label === 'wholeBlocks' ? 'Selected passages' : coverage.detail === 'digest' ? 'Summary' : 'Source reference'}</span>}
      {state?.packet.receipt.mandatorySourceHandles?.includes(item.handle) && <span className="context-detail">Required source</span>}
      {onPin && <button className="quiet-button" disabled={pinDisabled} onClick={() => onPin(item.source.documentId, item.displayName)} aria-label={`Include ${item.displayName} in the next request`}>Include next time</button>}
      {onKeepSource && <button className="quiet-button" disabled={pinDisabled} onClick={() => onKeepSource(item.source.documentId)} aria-label={`Keep ${item.displayName} for future discussions`}>Keep source…</button>}
    </li>;
  }
  return <details className="context-inspector">
    <summary>Story context</summary>
    {error && <p role="alert" className="error-status">{error} <button onClick={() => setReload(value => value + 1)}>Try again</button></p>}
    {!state && !error && <p role="status">Reading saved context…</p>}
    {state && <>
      {!state.current && <p className="stale-notice">Needs refresh. The story changed after this request. These sources show the earlier version.</p>}
      {state.frozen.snapshot.basis === 'reviewed' && <p className="small-copy context-reviewed-basis">The earlier chapters use your reviewed versions. The current chapter is still a working draft. These reviews did not run AI checks.</p>}
      <p className="small-copy">{appServerDelivery
        ? appServerDelivery.submission === 'notSent'
          ? 'The prepared packet was retained, but the Codex app-server turn was not sent.'
          : appServerDelivery.submission === 'uncertain'
            ? 'The Codex app-server may have accepted the turn, but delivery is not confirmed. It was not automatically retried.'
            : !appServerDelivery.requestSettled || appServerDelivery.connection === 'unresolved'
              ? 'The Codex app-server acknowledged the owned turn, but request settlement is unresolved. It was not automatically retried.'
              : 'The Codex app-server acknowledged the owned turn and received the prepared packet. This confirms delivery, not that the model understood every source.'
        : delivered ? state.packet.options.providerBinding?.http ? 'Response headers were received for the prepared request. The saved result may be partial; this does not establish that the model understood every source.' : state.packet.options.providerBinding ? 'The complete prepared packet was written to Codex for this response. This confirms local delivery, not that the model understood every source.' : 'Sources supplied for this response.' : 'Prepared sources. Delivery has not been confirmed.'} Opening a source reads its saved version.</p>
      {state.packet.options.providerBinding && <p className="small-copy">This packet uses {Number(state.packet.receipt.inputTokens).toLocaleString()} bytes of the app’s {Number(state.packet.options.providerBinding.inputLimitBytes).toLocaleString()}-byte input allowance. This is not a model token count or context-window limit. The full prepared input is preserved; mandatory text is never shortened to fit.</p>}
      {state.packet.options.providerBinding?.runtime && <p className="small-copy">{state.packet.options.providerBinding.runtime.appServer
        ? 'Prepared for the Codex app-server. The saved request retains the checked server identity.'
        : `Prepared for Codex CLI ${state.packet.options.providerBinding.runtime.cliVersion}. The saved request retains the checked executable identity.`}</p>}
      {state.packet.receipt.safeBrief && <section className="context-safe-brief" aria-label="Approved writing brief"><strong>Author-approved writing brief</strong><p>{state.packet.receipt.safeBrief.text}</p><p className="small-copy">Exact directions shared for this edit request. The originating discussion was not added as context.</p></section>}
      {state.packet.receipt.lookup && <LookupContextView lookup={state.packet.receipt.lookup} sources={items} busy={busy} delivery={lookupDelivery ?? (delivered ? 'delivered' : 'prepared')} onRead={readHandle} />}
      <details open><summary>{delivered ? 'Used' : 'Prepared'} · {supplied.size} {supplied.size === 1 ? 'source' : 'sources'}{navigation.length ? ` · ${suppliedNavigation.length} generated ${suppliedNavigation.length === 1 ? 'summary' : 'summaries'}` : ''}{reviewedRecords(true).length ? ` · ${reviewedRecords(true).length} reviewed ${reviewedRecords(true).length === 1 ? 'detail' : 'details'}` : ''}{suppliedPromises.length ? ` · ${suppliedPromises.length} promise ${suppliedPromises.length === 1 ? 'detail' : 'details'}` : ''}{suppliedKnowledge.length ? ` · ${suppliedKnowledge.length} knowledge ${suppliedKnowledge.length === 1 ? 'observation' : 'observations'}` : ''}{suppliedSummaries.length ? ` · ${suppliedSummaries.length} accepted ${suppliedSummaries.length === 1 ? 'summary' : 'summaries'}` : ''}{guidance.length > 0 ? ` · ${suppliedGuidance.size} ${suppliedGuidance.size === 1 ? 'instruction' : 'instructions'}` : ''}{conversation.length ? ` · ${suppliedTurns.length} earlier ${suppliedTurns.length === 1 ? 'exchange' : 'exchanges'}` : ''}</summary><ul>{items.filter(item => supplied.has(item.handle)).map(row)}{reviewedProvenanceRows(true)}{reviewedRows(true)}{promiseRows(true)}{knowledgeRows(true)}{summaryRows(true)}{navigationRows(true)}{guidanceRows(true)}{conversationRows(true)}</ul></details>
      <details><summary>Available · {items.length} {items.length === 1 ? 'source' : 'sources'}{navigation.length ? ` · ${navigation.length} generated ${navigation.length === 1 ? 'summary' : 'summaries'}` : ''}{reviewedEvidence.length ? ` · ${reviewedRecords(false).length} reviewed ${reviewedRecords(false).length === 1 ? 'detail' : 'details'}` : ''}{availablePromises.length ? ` · ${availablePromises.length} promise ${availablePromises.length === 1 ? 'detail' : 'details'}` : ''}{availableKnowledge.length ? ` · ${availableKnowledge.length} knowledge ${availableKnowledge.length === 1 ? 'observation' : 'observations'}` : ''}{acceptedSummaries.length ? ` · ${acceptedSummaries.length} accepted ${acceptedSummaries.length === 1 ? 'summary' : 'summaries'}` : ''}{guidance.length > 0 ? ` · ${guidance.length} ${guidance.length === 1 ? 'instruction' : 'instructions'}` : ''}{conversation.length ? ` · ${conversation.length} earlier ${conversation.length === 1 ? 'exchange' : 'exchanges'}` : ''}</summary><p className="small-copy">Permitted for this request. Availability does not mean the model read every source.</p><ul>{items.map(row)}{reviewedProvenanceRows(false)}{reviewedRows(false)}{promiseRows(false)}{knowledgeRows(false)}{summaryRows(false)}{navigationRows(false)}{guidanceRows(false)}{conversationRows(false)}</ul></details>
      <details><summary>Not included</summary>
        {state.packet.receipt.omissions.length ? <ul>{state.packet.receipt.omissions.map((text, i) => <li key={i}>{omission(text)}</li>)}</ul> : <p className="small-copy">All permitted sources fit this request.</p>}
        {summaryOmissions.length > 0 && <ul>{summaryOmissions.map(item => <li key={`summary/${item.sourceHandle}`}>{summaryOmission(item)}</li>)}</ul>}
        {navigationOmissions.length > 0 && <ul>{navigationOmissions.map(item => <li key={item.viewId}>{navigationOmission(item.viewId, item.reason)}</li>)}</ul>}
        {reviewedEvidenceOmissions.length > 0 && <ul>{reviewedEvidenceOmissions.map((item, index) => <li key={`${item.sourceHandle}/${item.bundleId}/${item.reason}/${index}`}>{reviewedOmission(item)}</li>)}</ul>}
        {promiseOmissions.length > 0 && <ul>{promiseOmissions.map((item, index) => <li key={`promise/${item.sourceHandle}/${item.bundleId}/${item.reason}/${index}`}>{item.count} promise {item.count === 1 ? 'detail was' : 'details were'} withheld from {reviewedSourceName(item.sourceHandle)} by {item.reason === 'disclosure' ? 'the reader disclosure policy' : 'the request budget'}.</li>)}</ul>}
        {knowledgeOmissions.length > 0 && <ul>{knowledgeOmissions.map((item, index) => <li key={`knowledge/${item.sourceHandle}/${item.bundleId}/${item.reason}/${index}`}>{knowledgeOmission(item)}</li>)}</ul>}
        {state.frozen.excludedSourceCount > 0 && <p className="small-copy">{state.frozen.excludedSourceCount} sources excluded by the request’s information boundary.</p>}
        {omittedTurns > 0 && <p className="small-copy">{omittedTurns} earlier complete {omittedTurns === 1 ? 'exchange was' : 'exchanges were'} not included. The full discussion remains saved.</p>}
      </details>
      <form className="context-search" onSubmit={event => { event.preventDefault(); void find(); }}>
        <label>Find an exact phrase in these sources<input value={query} onChange={event => setQuery(event.target.value)} maxLength={200} /></label>
        <button disabled={busy || !query.trim()}>Find</button>
      </form>
      {search && <div className="context-search-results">
        <p className="small-copy">{search.hits.length ? `${search.hits.length}${search.hasMore ? '+' : ''} passages found.` : 'No exact match in the searched sources. This does not establish that an event never happened.'}</p>
        {search.hits.map((hit, index) => <div key={`${hit.passage.handle}/${hit.passage.blockId}/${index}`}><strong>{items.find(item => item.handle === hit.passage.handle)?.displayName}</strong><blockquote>{hit.passage.text}</blockquote></div>)}
      </div>}
      {history && <EvidenceHistoryView key={`${history.snapshotId}/${history.history.objectId}`} result={history} onClose={() => setHistory(null)} onRead={handle => { const item = items.find(source => source.handle === handle); if (item) void read(item); }} />}
      {promiseHistory && <PromiseHistoryView key={`${promiseHistory.snapshotId}/${promiseHistory.history.promiseId}`} result={promiseHistory} onClose={() => setPromiseHistory(null)} onRead={readHandle} />}
      {knowledgeHistory && <KnowledgeHistoryView key={`${knowledgeHistory.snapshotId}/${knowledgeHistory.history.characterId}/${knowledgeHistory.history.topicId ?? 'all'}`} result={knowledgeHistory} onClose={() => setKnowledgeHistory(null)} onRead={readHandle} />}
      {source && <section className="context-source" aria-label="Saved story source"><div className="header-actions"><h3>{source.descriptor.displayName}</h3><button onClick={() => setSource(null)}>Close source</button></div><p className="small-copy">Exact source version retained with this request.</p>{source.passages.map(passage => source.body.body.content.find(block => block.attrs.id === passage.blockId)?.type === 'sceneBreak' ? <hr key={passage.blockId} /> : <p key={passage.blockId}>{passage.text || <br />}</p>)}</section>}
    </>}
  </details>;
}
