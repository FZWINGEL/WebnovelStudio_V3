import { useEffect, useRef, useState } from 'react';
import {
  preparedStoryContext, preparedStoryContextIsCurrent, readStoryContextSource, searchStoryContext, storyContextSnapshot,
  type CompiledPacket, type FrozenContext, type SourceDescriptor, type SourceRead,
} from '../ipc/context';
import type { ProjectAccess } from '../ipc/projects';

function message(reason: unknown): string {
  if (reason && typeof reason === 'object' && 'detail' in reason) return String(reason.detail);
  return reason instanceof Error ? reason.message : 'Could not read this story context. Try again.';
}

/** Reads the immutable receipt; never reconstructs a past request from current prose. */
export function ContextInspector({ access, packetId, delivered, refreshKey, onPin, onKeepSource, pinDisabled = false }: {
  access: ProjectAccess; packetId: string; delivered: boolean; refreshKey: string;
  onPin?: (documentId: string, title: string) => void;
  onKeepSource?: (documentId: string) => void;
  pinDisabled?: boolean;
}) {
  const [state, setState] = useState<{ packet: CompiledPacket; frozen: FrozenContext; current: boolean } | null>(null);
  const [source, setSource] = useState<SourceRead | null>(null);
  const [query, setQuery] = useState('');
  const [search, setSearch] = useState<Awaited<ReturnType<typeof searchStoryContext>> | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [reload, setReload] = useState(0);
  const identity = `${access.projectId}/${access.operationNamespace}/${access.session}/${access.writerLease}/${packetId}`;
  const active = useRef(identity); active.current = identity;
  const sourceRequest = useRef(0);
  useEffect(() => {
    let cancelled = false;
    setError(''); setState(null); setSource(null); setSearch(null); sourceRequest.current += 1;
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
  async function read(item: SourceDescriptor) {
    if (!state) return;
    const captured = identity; const request = ++sourceRequest.current;
    setError(''); setSource(null);
    try {
      const result = await readStoryContextSource(access, state.frozen.snapshot.snapshotId, item.handle);
      if (active.current === captured && sourceRequest.current === request) setSource(result);
    } catch (reason) { if (active.current === captured && sourceRequest.current === request) setError(message(reason)); }
  }
  async function find() {
    if (!state || busy || !query.trim()) return;
    const captured = identity; const request = ++sourceRequest.current;
    setBusy(true); setError(''); setSearch(null);
    try {
      const result = await searchStoryContext({ access, snapshotId: state.frozen.snapshot.snapshotId, query, mode: 'literal', limit: 20 });
      if (active.current === captured && sourceRequest.current === request) setSearch(result);
    } catch (reason) { if (active.current === captured && sourceRequest.current === request) setError(message(reason)); }
    finally { if (active.current === captured) setBusy(false); }
  }
  const items = state?.frozen.snapshot.sources ?? [];
  const supplied = new Set(state?.packet.receipt.sourceHandles ?? []);
  const guidance = state?.frozen.guidance ?? [];
  const suppliedGuidance = new Set(state?.packet.receipt.guidanceHandles ?? []);
  const conversation = state?.frozen.conversation?.turns ?? [];
  const suppliedMessages = new Set(state?.packet.receipt.conversationMessageIds ?? []);
  const suppliedTurns = conversation.filter(turn => suppliedMessages.has(turn.user.id) && suppliedMessages.has(turn.assistant.id));
  const omittedTurns = state?.packet.receipt.omittedDiscussionTurns ?? 0;
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
    const count = /(?:^|;)(?:blocks|remaining):(\d+)/u.exec(match[2]);
    return `${title}: ${count ? `${count[1]} blocks were not included` : 'some text was not included'} within the request budget.`;
  }
  function row(item: SourceDescriptor) {
    const coverage = state?.packet.receipt.coverage.find(entry => entry.handle === item.handle);
    return <li key={item.handle}>
      <button className="text-button" onClick={() => void read(item)}>{item.displayName}</button>
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
      <p className="small-copy">{delivered ? 'Sources supplied for this response.' : 'Prepared sources. This request has not been sent to a model.'} Opening a source reads its saved version.</p>
      {state.packet.receipt.safeBrief && <section className="context-safe-brief" aria-label="Approved writing brief"><strong>Author-approved writing brief</strong><p>{state.packet.receipt.safeBrief.text}</p><p className="small-copy">Exact directions shared for this edit request. The originating discussion was not added as context.</p></section>}
      <details open><summary>{delivered ? 'Used' : 'Prepared'} · {supplied.size} {supplied.size === 1 ? 'source' : 'sources'}{guidance.length > 0 ? ` · ${suppliedGuidance.size} ${suppliedGuidance.size === 1 ? 'instruction' : 'instructions'}` : ''}{conversation.length ? ` · ${suppliedTurns.length} earlier ${suppliedTurns.length === 1 ? 'exchange' : 'exchanges'}` : ''}</summary><ul>{items.filter(item => supplied.has(item.handle)).map(row)}{guidanceRows(true)}{conversationRows(true)}</ul></details>
      <details><summary>Available · {items.length} {items.length === 1 ? 'source' : 'sources'}{guidance.length > 0 ? ` · ${guidance.length} ${guidance.length === 1 ? 'instruction' : 'instructions'}` : ''}{conversation.length ? ` · ${conversation.length} earlier ${conversation.length === 1 ? 'exchange' : 'exchanges'}` : ''}</summary><p className="small-copy">Permitted for this request. Availability does not mean the model read every source.</p><ul>{items.map(row)}{guidanceRows(false)}{conversationRows(false)}</ul></details>
      <details><summary>Not included</summary>
        {state.packet.receipt.omissions.length ? <ul>{state.packet.receipt.omissions.map((text, i) => <li key={i}>{omission(text)}</li>)}</ul> : <p className="small-copy">All permitted sources fit this request.</p>}
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
      {source && <section className="context-source" aria-label="Saved story source"><div className="header-actions"><h3>{source.descriptor.displayName}</h3><button onClick={() => setSource(null)}>Close source</button></div><p className="small-copy">Exact source version retained with this request.</p>{source.passages.map(passage => source.body.body.content.find(block => block.attrs.id === passage.blockId)?.type === 'sceneBreak' ? <hr key={passage.blockId} /> : <p key={passage.blockId}>{passage.text || <br />}</p>)}</section>}
    </>}
  </details>;
}
