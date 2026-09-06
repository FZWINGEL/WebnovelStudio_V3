import { useEffect, useId, useRef, type ReactNode } from 'react';
import './MemoryPanel.css';

export type MemoryViewKind = 'current' | 'changedSource' | 'revoked' | 'recoveredHistorical';

export interface MemoryEvidence {
  quote: string;
  /** Optional plain-language context supplied by the caller. */
  display?: string;
}

export interface MemoryItem {
  text: string;
  uncertainty?: string;
  evidence: MemoryEvidence[];
}

export interface MemoryView {
  id: string;
  kind: MemoryViewKind;
  createdAt: string;
  items: MemoryItem[];
  /** A human-facing source name; do not put source IDs here. */
  sourceLabel?: string;
}

export interface MemoryInspectedSource {
  viewId: string;
  label: string;
  passages: Array<{ blockId: string; text: string }>;
}

interface MemoryStateBase {
  /** Keep prior readable views here while a new refresh is in progress. */
  views: MemoryView[];
}

export type MemoryPanelState =
  | (MemoryStateBase & { kind: 'loading' })
  | (MemoryStateBase & { kind: 'empty' })
  | (MemoryStateBase & { kind: 'active'; phase: 'queued' | 'running' | 'stopping' })
  | (MemoryStateBase & { kind: 'completed'; disposition: 'candidate' | 'needsReconciliation'; message?: string })
  | (MemoryStateBase & { kind: 'interrupted'; message?: string })
  | (MemoryStateBase & { kind: 'failed'; message?: string });

/** Parent-facing display data; callbacks stay separate from native lifecycle state. */
export interface MemoryPanelViewModel {
  documentTitle: string;
  modelLabel: string;
  modelAvailable: boolean;
  allowanceLabel?: string;
  state: MemoryPanelState;
  busy?: boolean;
  error?: string;
  inspectedSource?: MemoryInspectedSource;
  inspectedPacket?: ReactNode;
}

export interface MemoryPanelProps extends MemoryPanelViewModel {
  onRefresh(): void;
  onStop(): void;
  onReconcile?(): void;
  onInspect(viewId: string): void;
  onCloseInspectedSource?(): void;
  onInspectPacket?(viewId: string): void;
  onCloseInspectedPacket?(): void;
  onClose(): void;
}

function viewLabel(kind: MemoryViewKind): string {
  switch (kind) {
    case 'current': return 'Current story memory';
    case 'changedSource': return 'Changed source';
    case 'revoked': return 'Unavailable memory';
    case 'recoveredHistorical': return 'Recovered historical memory';
  }
}

function viewNote(kind: MemoryViewKind): string | null {
  switch (kind) {
    case 'changedSource': return 'The source has changed since this memory was prepared. Check it against the chapter before relying on it.';
    case 'recoveredHistorical': return 'Recovered from an earlier result. It may be stale.';
    case 'revoked': return 'This memory is unavailable because its source access changed. Content and evidence are hidden.';
    default: return null;
  }
}

function createdLabel(createdAt: string): string | null {
  const date = new Date(createdAt);
  if (Number.isNaN(date.getTime())) return null;
  return date.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

function statusText(state: MemoryPanelState): string | null {
  if (state.kind === 'loading') return 'Reading story memory…';
  if (state.kind === 'active') {
    if (state.phase === 'queued') return 'Story memory refresh is queued.';
    if (state.phase === 'stopping') return 'Stopping story memory refresh…';
    return 'Refreshing story memory… Existing memory stays readable while this runs.';
  }
  if (state.kind === 'interrupted') return state.message || 'Story memory refresh stopped before it finished. Any existing memory remains available to inspect.';
  return null;
}

function MemoryViewCard({ view, headingId, onInspect, onInspectPacket }: { view: MemoryView; headingId: string; onInspect(viewId: string): void; onInspectPacket?: (viewId: string) => void }) {
  const date = createdLabel(view.createdAt);
  const note = viewNote(view.kind);
  if (view.kind === 'revoked') {
    return <section className="memory-view memory-view-revoked" aria-labelledby={headingId}>
      <div className="memory-view-heading"><h3 id={headingId}>{viewLabel(view.kind)}</h3></div>
      <p className="memory-view-note">{note}</p>
    </section>;
  }
  return <section className={`memory-view memory-view-${view.kind}`} aria-labelledby={headingId}>
    <div className="memory-view-heading">
      <h3 id={headingId}>{viewLabel(view.kind)}</h3>
      {date && <span className="memory-view-date">{date}</span>}
    </div>
    {view.sourceLabel && <p className="memory-view-source">Source: {view.sourceLabel}</p>}
    {note && <p className="memory-view-note">{note}</p>}
    {view.items.length ? <ul className="memory-items">
      {view.items.map((item, index) => <li className="memory-item" key={`${view.id}-${index}`}>
        <p className="memory-item-text">{item.text}</p>
        {item.uncertainty && <p className="memory-item-uncertainty">Needs checking: {item.uncertainty}</p>}
        {item.evidence.length > 0 && <details className="memory-evidence">
          <summary>Show source evidence</summary>
          <div className="memory-evidence-list">
            {item.evidence.map((evidence, evidenceIndex) => <figure key={`${view.id}-${index}-evidence-${evidenceIndex}`}>
              <blockquote>{evidence.quote}</blockquote>
              {evidence.display && <figcaption>{evidence.display}</figcaption>}
            </figure>)}
          </div>
        </details>}
      </li>)}
    </ul> : <p className="small-copy">No summary is available in this result.</p>}
    <div className="memory-inspect-actions">
      <button type="button" className="text-button memory-inspect" onClick={() => onInspect(view.id)}>Inspect source</button>
      {onInspectPacket && <button type="button" className="text-button memory-inspect" onClick={() => onInspectPacket(view.id)}>Inspect saved request</button>}
    </div>
  </section>;
}

/**
 * Presentational story-memory panel. The parent owns provider calls, operation
 * identity, persistence, and the mapping from native DTOs into this view model.
 */
export function MemoryPanel({ documentTitle, modelLabel, modelAvailable, allowanceLabel, state, busy = false, error,
  inspectedSource, inspectedPacket, onRefresh, onStop, onReconcile, onInspect, onCloseInspectedSource, onInspectPacket, onCloseInspectedPacket, onClose }: MemoryPanelProps) {
  const headingId = useId();
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { heading.current?.focus(); }, []);

  const active = state.kind === 'active';
  const stopping = active && state.phase === 'stopping';
  const awaitingReconciliation = state.kind === 'completed' && state.disposition === 'needsReconciliation';
  const refreshDisabled = busy || modelAvailable === false || active || state.kind === 'loading' || awaitingReconciliation;
  const views = state.views;
  const status = statusText(state);

  return <aside className="history-panel memory-panel" aria-labelledby={headingId} aria-busy={state.kind === 'loading' || active}>
    <div className="feedback-heading memory-heading">
      <h2 id={headingId} tabIndex={-1} ref={heading}>Story memory</h2>
      <button type="button" className="memory-back" onClick={onClose}>Back to writing</button>
    </div>
    <p className="panel-intro">Generated summaries can help you navigate {documentTitle ? `“${documentTitle}”` : 'this chapter'}. Check them against the source before relying on them.</p>

    <div className="memory-controls">
      <dl className="memory-model-details">
        {modelLabel && <div><dt>Model</dt><dd>{modelLabel}</dd></div>}
        {allowanceLabel && <div><dt>Application limit</dt><dd>{allowanceLabel}</dd></div>}
      </dl>
      {!modelAvailable && <p className="memory-model-unavailable" role="note">The selected model is unavailable for story memory. Choose an available model in Settings before refreshing.</p>}
      <button type="button" className="primary-button memory-refresh" disabled={refreshDisabled} onClick={onRefresh}>Refresh story memory</button>
      {active && <div className="memory-run-status" role="status" aria-live="polite">
        <p>{status}</p>
        <button type="button" className="secondary-button" disabled={stopping} onClick={onStop}>{stopping ? 'Stopping…' : 'Stop'}</button>
      </div>}
    </div>

    {error && <p className="memory-error" role="alert">{error}</p>}
    {state.kind === 'failed' && <p className="memory-error" role="alert">{state.message || 'Story memory could not be refreshed. Existing memory remains available to inspect.'}</p>}
    {state.kind === 'interrupted' && !active && <p className="memory-status" role="status">{status}</p>}
    {state.kind === 'completed' && state.message && <p className="memory-status" role="status">{state.message}</p>}
    {awaitingReconciliation && <div className="memory-reconcile" role="status">
      <p>The result may not have been saved. Check the saved result before refreshing story memory again.</p>
      {onReconcile && <button type="button" className="secondary-button" disabled={busy} onClick={onReconcile}>Check saved result</button>}
    </div>}

    <div className="memory-scroll" aria-label="Story memory results">
      {inspectedPacket && <section className="memory-packet" aria-label="Saved story memory request">
        <div className="memory-source-heading"><h3>Saved request</h3>{onCloseInspectedPacket && <button type="button" className="text-button" onClick={onCloseInspectedPacket}>Close request</button>}</div>
        {inspectedPacket}
      </section>}
      {inspectedSource && <section className="memory-source" aria-label="Saved story source">
        <div className="memory-source-heading"><h3>{inspectedSource.label}</h3>{onCloseInspectedSource && <button type="button" className="text-button" onClick={onCloseInspectedSource}>Close source</button>}</div>
        <p className="small-copy">Exact source text retained with this memory.</p>
        {inspectedSource.passages.map(passage => <p key={passage.blockId}>{passage.text || <br />}</p>)}
      </section>}
      {state.kind === 'empty' && <div className="memory-empty"><h3>No story memory yet</h3><p>Keep writing without it, or refresh when a short, source-linked summary would help you find your place in this chapter.</p></div>}
      {state.kind === 'loading' && !views.length && <p className="memory-loading" role="status">Reading story memory…</p>}
      {views.map((view, index) => <MemoryViewCard key={view.id} view={view} headingId={`${headingId}-view-${index}`} onInspect={onInspect} onInspectPacket={view.kind === 'revoked' ? undefined : onInspectPacket} />)}
      {state.kind !== 'empty' && state.kind !== 'loading' && !views.length && state.kind !== 'failed' && state.kind !== 'interrupted' && <p className="small-copy">No readable story memory is available yet.</p>}
    </div>
  </aside>;
}
