import type {
  EvidenceHistory,
  KnowledgeHistory,
  LookupMemoryEntityEntry,
  LookupMemoryEntityKind,
  LookupMemoryRequest,
  LookupPacketInput,
  LookupResult,
  PromiseHistory,
  SourceDescriptor,
  SourceRef,
} from '../ipc/context';
import { knowledgeAttitudeLabels, promisePhaseLabels } from '../ipc/reviews';

function sameSource(left: SourceRef, right: SourceRef): boolean {
  return left.projectId === right.projectId && left.documentId === right.documentId
    && left.revisionId === right.revisionId && left.bodyHash === right.bodyHash;
}

function sourceName(lookup: LookupPacketInput, sources: SourceDescriptor[], handle: string, source?: SourceRef): string {
  if (!source) return handle;
  const projection = lookup.sourceProjection;
  if (projection?.schemaVersion === 'story-lookup-source.v1') {
    const projected = projection.sources.find(item => item.handle === handle && sameSource(item.source, source));
    if (projected) return projected.displayName;
  }
  return sources.find(item => item.handle === handle && sameSource(item.source, source))?.displayName ?? handle;
}

export type LookupDeliveryState = 'prepared' | 'delivered' | 'unconfirmed';

function deliveryLabel(delivery: LookupDeliveryState): string {
  return delivery === 'delivered' ? 'Delivered to model' : delivery === 'unconfirmed' ? 'Delivery not confirmed' : 'Prepared for model';
}

function timingLabel(timing: 'atPassage' | 'earlier' | 'unknown'): string {
  return timing === 'atPassage' ? 'At this passage' : timing === 'earlier' ? 'An earlier time; exact order not established' : 'Timing unclear';
}

function readDeliveryLabel(delivery: LookupDeliveryState, complete: boolean): string {
  const extent = complete ? 'Complete read' : 'Partial read';
  return delivery === 'delivered' ? `${extent} delivered.` : delivery === 'unconfirmed' ? `${extent} prepared; delivery is not confirmed.` : `${extent} prepared.`;
}

function entityKindLabel(kind: LookupMemoryEntityKind): string {
  return `${kind}s`;
}

function uncertaintyLabel(value: string): string {
  const labels: Record<string, string> = {
    disclosureLimited: 'Some story material is outside this request’s information boundary.',
    excludedSources: 'Some eligible sources were excluded from this request.',
    earlierTiming: 'Some observations use earlier timing; source order does not establish fictional chronology.',
    unknownTiming: 'Some observations have unknown timing.',
    earlierOrUnknownTiming: 'Some observations have earlier or unknown timing; source order does not establish fictional chronology.',
    unknownHolder: 'Some observations do not record a holder.',
    differingHolders: 'Different recorded holders appear. Read the passages before drawing a conclusion.',
    multipleRecordedAttitudes: 'Different attitudes are recorded. Read the passages before drawing a conclusion.',
    conflictingOutcomes: 'Different outcomes are recorded. Their fictional order or relationship is not established here.',
    unclearObservation: 'Some observations have an unclear outcome.',
    noEligibleObservations: 'No eligible observation appears in the permitted saved evidence. This does not establish absence or unawareness.',
  };
  return labels[value] ?? value;
}

type MemoryRequest = LookupMemoryRequest;

function memoryRequestTitle(request: MemoryRequest, result: LookupResult, index: number): string {
  if (request.kind === 'findEntities') return `Find ${request.entityKind} identities · ${request.query}`;
  if (request.kind === 'knowledgeHistory') return `Knowledge history · ${result.kind === 'knowledgeHistory' ? result.history.labelVariants[0] ?? 'selected character' : `request ${index + 1}`}`;
  if (request.kind === 'promiseHistory') return `Promise history · ${result.kind === 'promiseHistory' ? result.history.labelVariants[0] ?? 'selected promise' : `request ${index + 1}`}`;
  return `Possession history · ${result.kind === 'possessionHistory' ? result.history.labelVariants[0] ?? 'selected object' : `request ${index + 1}`}`;
}

function MemorySourceButton({ lookup, sources, handle, source, busy, onRead }: {
  lookup: LookupPacketInput; sources: SourceDescriptor[]; handle: string; source: SourceRef; busy: boolean; onRead: (handle: string, source?: SourceRef) => void;
}) {
  const descriptor = sources.find(item => item.handle === handle && sameSource(item.source, source));
  return <>
    <span className="context-detail">{sourceName(lookup, sources, handle, source)}</span>
    <button className="text-button" disabled={busy || !descriptor} onClick={() => onRead(handle, source)}>Open exact source</button>
  </>;
}

function memoryEntityRows(result: Extract<LookupResult, { kind: 'findEntities' }>, lookup: LookupPacketInput, sources: SourceDescriptor[], busy: boolean, onRead: (handle: string, source?: SourceRef) => void) {
  if (!result.entries.length) return <p className="small-copy">No reviewed {entityKindLabel(result.entityKind)} matched this query. That does not establish that the identity is absent.</p>;
  return <>
    {result.entries.map((entry: LookupMemoryEntityEntry, index: number) => <div key={`${entry.entity.id}/${entry.sourceHandle}/${index}`} className="context-lookup-memory-entry">
      <strong>{entry.labelVariants.length ? entry.labelVariants.join(' · ') : entry.entity.label}</strong>
      <p className="small-copy">Reviewed {result.entityKind} identity.</p>
      <MemorySourceButton lookup={lookup} sources={sources} handle={entry.sourceHandle} source={entry.source} busy={busy} onRead={onRead} />
    </div>)}
  </>;
}

function memoryHistoryRows(history: KnowledgeHistory | PromiseHistory | EvidenceHistory, lookup: LookupPacketInput, sources: SourceDescriptor[], busy: boolean, onRead: (handle: string, source?: SourceRef) => void) {
  if (!history.observations.length) return <p className="small-copy">No eligible observation appears in the permitted saved evidence. This does not establish absence, unawareness, or an unresolved promise.</p>;
  return <ol className="context-lookup-memory-observations">{history.observations.map((item, index) => {
    const label = 'statement' in item ? knowledgeAttitudeLabels[item.attitude]
      : 'phase' in item ? promisePhaseLabels[item.phase]
        : item.holder?.label ?? 'Holder not recorded';
    const detail = 'statement' in item ? `${item.topic.label} · ${item.statement}`
      : 'phase' in item ? item.note
        : `Recorded object: ${item.object.label}`;
    return <li key={`${item.sourceHandle}/${item.recordId}/${index}`} className="context-lookup-memory-observation">
      <MemorySourceButton lookup={lookup} sources={sources} handle={item.sourceHandle} source={item.source} busy={busy} onRead={onRead} />
      <p><strong>{label}</strong> · {timingLabel(item.timing)}</p>
      <p>{detail}</p>
      <blockquote>{item.evidence.quote}</blockquote>
      <p className="small-copy">{item.audience === 'reader' ? 'Explicitly reader-disclosed' : 'Author room only'}</p>
    </li>;
  })}</ol>;
}

function memoryResultView({ result, lookup, sources, busy, onRead }: {
  result: Extract<LookupResult, { kind: 'findEntities' | 'knowledgeHistory' | 'promiseHistory' | 'possessionHistory' }>;
  lookup: LookupPacketInput; sources: SourceDescriptor[]; busy: boolean; delivery: LookupDeliveryState; onRead: (handle: string, source?: SourceRef) => void;
}) {
  if (result.kind === 'findEntities') {
    return <div className="context-lookup-result">
      <p className="small-copy">{result.entries.length} of {result.totalMatches} reviewed {entityKindLabel(result.entityKind)} shown · page starts at {result.offset}. {result.incomplete ? 'The catalog is bounded.' : ''}</p>
      {memoryEntityRows(result, lookup, sources, busy, onRead)}
      {result.nextOffset !== null && <p className="small-copy">More matching identities are available from offset {result.nextOffset}.</p>}
    </div>;
  }
  const { history } = result;
  return <div className="context-lookup-result">
    <p className="small-copy">{history.observations.length ? `${result.offset + 1}–${result.offset + history.observations.length}` : '0'} of {result.totalObservations} recorded observations · {history.incomplete ? 'bounded evidence' : 'complete page'}. {result.nextOffset !== null ? `More evidence is available from offset ${result.nextOffset}.` : 'No further page is recorded in this snapshot.'}</p>
    {history.labelVariants.length > 0 && <p className="small-copy">Recorded as: {history.labelVariants.join(', ')}.</p>}
    {history.uncertainty.map(value => <p key={value} className="small-copy">Uncertainty: {uncertaintyLabel(value)}</p>)}
    {result.kind === 'promiseHistory' && <p className="small-copy">{result.history.hasRecordedPayoff
      ? 'At least one eligible observation records a payoff. This is a recorded passage observation; it does not prove that the promise is resolved.'
      : 'No eligible observation records a payoff in the retained history. This does not prove that the promise is unresolved.'}
      {result.history.hasRecordedPayoff && !result.history.observations.some(item => item.phase === 'payoff') ? ' This page may omit that payoff; read another page before drawing a conclusion.' : ''}</p>}
    {memoryHistoryRows(history, lookup, sources, busy, onRead)}
  </div>;
}

/** Renders Rust-authenticated lookup evidence from the immutable packet receipt. */
export function LookupContextView({ lookup, sources, busy = false, onRead, delivery = 'prepared' }: {
  lookup: LookupPacketInput;
  sources: SourceDescriptor[];
  busy?: boolean;
  delivery?: LookupDeliveryState;
  onRead: (handle: string, source?: SourceRef) => void;
}) {
  const hasMemory = lookup.exchanges.some(exchange => ['findEntities', 'knowledgeHistory', 'promiseHistory', 'possessionHistory'].includes(exchange.request.kind));
  const evidenceVerb = delivery === 'delivered' ? 'delivered' : delivery === 'unconfirmed' ? 'prepared, but delivery is not confirmed' : 'prepared';
  return <section className="context-lookup" aria-label={`${deliveryLabel(delivery)} story lookup evidence`}>
    <h3>Additional story lookups</h3>
    <p className="small-copy">These exact {hasMemory ? 'story passages and reviewed memory records' : 'search results and passages'} were {evidenceVerb} after the initial context packet. They are evidence for this response, not new canon.</p>
    <p className="context-detail"><strong>{deliveryLabel(delivery)}</strong> · {lookup.completedInvocations} of {lookup.allowance.maxAdditionalInvocations} additional lookups completed · {lookup.exchanges.length} evidence {lookup.exchanges.length === 1 ? 'exchange' : 'exchanges'}</p>
    {!lookup.reviewedMemory && hasMemory && <p className="small-copy">Reviewed memory lookup capability is missing from this packet. The memory result cannot be treated as authorized context.</p>}
    {lookup.exchanges.length === 0 && <p className="small-copy">No additional story evidence was supplied.</p>}
    <ol className="context-lookup-exchanges">
      {lookup.exchanges.map((exchange, index) => {
        const request = exchange.request;
        const result = exchange.result;
        const readSource = request.kind === 'read' && result.kind === 'read' ? result.source : undefined;
        const isMemoryRequest = ['findEntities', 'knowledgeHistory', 'promiseHistory', 'possessionHistory'].includes(request.kind);
        const title = request.kind === 'search' ? `Search ${index + 1} · “${request.query}”`
          : request.kind === 'read' ? `Read ${index + 1} · ${sourceName(lookup, sources, request.handle, readSource)}`
            : memoryRequestTitle(request as MemoryRequest, result, index);
        return <li key={`${request.id}/${index}`} className="context-lookup-exchange">
          <strong>{title}</strong>
          {request.kind === 'search' && <span className="context-detail">{request.mode} search · limit {request.limit}</span>}
          {request.kind === 'read' && <span className="context-detail">{request.blockIds?.length ? `${request.blockIds.length} requested ${request.blockIds.length === 1 ? 'passage' : 'passages'}` : 'Complete source requested'}</span>}
          {request.kind === 'findEntities' && <span className="context-detail">Reviewed identity catalog · limit {request.limit}</span>}
          {request.kind !== 'search' && request.kind !== 'read' && request.kind !== 'findEntities' && <span className="context-detail">Reviewed history · limit {request.limit}</span>}
          {result.kind === 'unavailable' && <p className="small-copy">Lookup gap · {result.code}{isMemoryRequest ? '' : `: ${result.detail}`}</p>}
          {result.kind === 'search' && <div className="context-lookup-result">
            <p className="small-copy">Searched {result.result.searchedSources} sources · {result.result.coverage}{result.result.hasMore ? ' · more matches exist' : ''}</p>
            {result.result.hits.length === 0 && <p className="small-copy">No match in the searched sources. This does not establish that the event never happened.</p>}
            {result.result.hits.map((hit, hitIndex) => <div key={`${hit.passage.handle}/${hit.passage.blockId}/${hitIndex}`} className="context-lookup-passage">
              <span className="context-detail">{sourceName(lookup, sources, hit.passage.handle, hit.passage.source)}</span>
              <blockquote>{hit.passage.text}</blockquote>
              <button className="text-button" disabled={busy || !sources.some(item => item.handle === hit.passage.handle && sameSource(item.source, hit.passage.source))} onClick={() => onRead(hit.passage.handle, hit.passage.source)}>Open exact source</button>
            </div>)}
            {result.result.sourceMatches.length > 0 && <p className="small-copy">Source matches: {result.result.sourceMatches.map(item => sourceName(lookup, sources, item.handle, item.source)).join(', ')}</p>}
          </div>}
          {result.kind === 'read' && <div className="context-lookup-result">
            <p className="small-copy">{readDeliveryLabel(delivery, result.complete)}{!result.complete ? ' Other passages were not included.' : ''} · {result.passages.length} {result.passages.length === 1 ? 'passage' : 'passages'}</p>
            {result.passages.map((passage, passageIndex) => <div key={`${passage.blockId}/${passageIndex}`} className="context-lookup-passage"><blockquote>{passage.text}</blockquote></div>)}
            <button className="text-button" disabled={busy || !sources.some(item => item.handle === result.handle && sameSource(item.source, result.source))} onClick={() => onRead(result.handle, result.source)}>Open exact source</button>
          </div>}
          {result.kind !== 'search' && result.kind !== 'read' && result.kind !== 'unavailable' && memoryResultView({ result, lookup, sources, busy, delivery, onRead })}
        </li>;
      })}
    </ol>
  </section>;
}
