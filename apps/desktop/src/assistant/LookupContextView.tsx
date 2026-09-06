import type { LookupPacketInput, SourceDescriptor, SourceRef } from '../ipc/context';

function sameSource(left: SourceRef, right: SourceRef): boolean {
  return left.projectId === right.projectId && left.documentId === right.documentId
    && left.revisionId === right.revisionId && left.bodyHash === right.bodyHash;
}

function sourceName(sources: SourceDescriptor[], handle: string, source?: SourceRef): string {
  return sources.find(item => item.handle === handle || (source && sameSource(item.source, source)))?.displayName ?? handle;
}

/** Renders only Rust-authenticated lookup evidence from the packet receipt. */
export function LookupContextView({ lookup, sources, busy = false, onRead }: {
  lookup: LookupPacketInput;
  sources: SourceDescriptor[];
  busy?: boolean;
  onRead: (handle: string) => void;
}) {
  return <section className="context-lookup" aria-label="Supplied story lookup evidence">
    <h3>Additional story lookups</h3>
    <p className="small-copy">These exact search results and passages were supplied after the initial context packet. They are evidence for this response, not new canon.</p>
    <p className="context-detail">{lookup.completedInvocations} of {lookup.allowance.maxAdditionalInvocations} additional lookups completed · {lookup.exchanges.length} evidence {lookup.exchanges.length === 1 ? 'exchange' : 'exchanges'}</p>
    {lookup.exchanges.length === 0 && <p className="small-copy">No additional story evidence was supplied.</p>}
    <ol className="context-lookup-exchanges">
      {lookup.exchanges.map((exchange, index) => {
        const request = exchange.request;
        const result = exchange.result;
        const title = request.kind === 'search' ? `Search ${index + 1} · “${request.query}”` : `Read ${index + 1} · ${sourceName(sources, request.handle)}`;
        return <li key={`${request.id}/${index}`} className="context-lookup-exchange">
          <strong>{title}</strong>
          {request.kind === 'search' && <span className="context-detail">{request.mode} search · limit {request.limit}</span>}
          {request.kind === 'read' && <span className="context-detail">{request.blockIds?.length ? `${request.blockIds.length} requested ${request.blockIds.length === 1 ? 'passage' : 'passages'}` : 'Complete source requested'}</span>}
          {result.kind === 'unavailable' && <p className="small-copy">Lookup gap · {result.code}: {result.detail}</p>}
          {result.kind === 'search' && <div className="context-lookup-result">
            <p className="small-copy">Searched {result.result.searchedSources} sources · {result.result.coverage}{result.result.hasMore ? ' · more matches exist' : ''}</p>
            {result.result.hits.length === 0 && <p className="small-copy">No match in the searched sources. This does not establish that the event never happened.</p>}
            {result.result.hits.map((hit, hitIndex) => <div key={`${hit.passage.handle}/${hit.passage.blockId}/${hitIndex}`} className="context-lookup-passage">
              <span className="context-detail">{sourceName(sources, hit.passage.handle, hit.passage.source)}</span>
              <blockquote>{hit.passage.text}</blockquote>
              <button className="text-button" disabled={busy || !sources.some(item => item.handle === hit.passage.handle)} onClick={() => onRead(hit.passage.handle)}>Open exact source</button>
            </div>)}
            {result.result.sourceMatches.length > 0 && <p className="small-copy">Source matches: {result.result.sourceMatches.map(item => item.displayName).join(', ')}</p>}
          </div>}
          {result.kind === 'read' && <div className="context-lookup-result">
            <p className="small-copy">{result.complete ? 'Complete read supplied.' : 'Partial read supplied; other passages were not included.'} · {result.passages.length} {result.passages.length === 1 ? 'passage' : 'passages'}</p>
            {result.passages.map((passage, passageIndex) => <div key={`${passage.blockId}/${passageIndex}`} className="context-lookup-passage"><blockquote>{passage.text}</blockquote></div>)}
            <button className="text-button" disabled={busy || !sources.some(item => item.handle === result.handle)} onClick={() => onRead(result.handle)}>Open exact source</button>
          </div>}
        </li>;
      })}
    </ol>
  </section>;
}
