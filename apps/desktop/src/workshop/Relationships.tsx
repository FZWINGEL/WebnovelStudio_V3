import { useEffect, useState } from 'react';
import type { DocumentRecord } from '../ipc/projects';
import type { WorkshopRelationship, WorkshopSession, WorkshopState } from '../ipc/workshop';

interface RelationshipsProps {
  state: WorkshopState;
  documents: DocumentRecord[];
  session: WorkshopSession;
  onChange(change: (state: WorkshopState) => WorkshopState): void;
  onOpenDocument(documentId: string): void;
  onExploreRelationship?: (relationship: WorkshopRelationship) => void;
  disabled?: boolean;
}

export function Relationships({ state, documents, session, onChange, onOpenDocument, onExploreRelationship, disabled = false }: RelationshipsProps) {
  const participants = documents.filter(document => ['character', 'world'].includes(document.kind));
  const [editing, setEditing] = useState<WorkshopRelationship | null>(null);
  const [focus, setFocus] = useState(session.focusDocumentId ?? '');
  const [error, setError] = useState('');
  useEffect(() => {
    setFocus(session.focusDocumentId ?? '');
    setEditing(null);
    setError('');
  }, [session.id, session.focusDocumentId]);
  const shown = state.relationships.filter(item => !focus || item.fromDocumentId === focus || item.toDocumentId === focus);
  const findDocument = (id: string) => documents.find(document => document.head.documentId === id);
  const name = (id: string) => findDocument(id)?.title ?? 'Unavailable participant';
  const fresh = (relationship: WorkshopRelationship) => {
    const from = findDocument(relationship.fromDocumentId);
    const to = findDocument(relationship.toDocumentId);
    const [fromHead, toHead] = relationship.sourceHeads;
    return !!from && !!to && relationship.sourceHeads.length === 2
      && fromHead?.documentId === relationship.fromDocumentId
      && toHead?.documentId === relationship.toDocumentId
      && fromHead.version === from.head.version && fromHead.bodyHash === from.head.bodyHash
      && toHead.version === to.head.version && toHead.bodyHash === to.head.bodyHash;
  };
  function add() {
    setEditing({ id: crypto.randomUUID(), fromDocumentId: participants[0]?.head.documentId ?? '', toDocumentId: participants[1]?.head.documentId ?? '', type: 'trusts', description: '', uncertainty: '', status: 'tentative', sourceHeads: [] }); setError('');
  }
  return <section className="workshop-relationships" aria-label="Local relationships"><div className="workshop-section-heading"><h2>Relationships</h2><button disabled={disabled || participants.length < 2} onClick={add}>Connect people or groups</button></div><p className="small-copy">A relationship has a direction. One person’s trust does not establish the other’s. These are author-room intentions, not character knowledge.</p>
    {participants.length < 2 && <p>Choose two people or groups as saved material before connecting them.</p>}
    <label>Near this person or group<select disabled={disabled} value={focus} onChange={event => setFocus(event.target.value)}><option value="">All saved relationships</option>{participants.map(document => <option key={document.head.documentId} value={document.head.documentId}>{document.title}</option>)}</select></label>
    {shown.map(relationship => {
      const from = findDocument(relationship.fromDocumentId);
      const to = findDocument(relationship.toDocumentId);
      const isFresh = fresh(relationship);
      const available = !!from && !!to;
      const exploreHintId = `relationship-explore-hint-${relationship.id}`;
      const reviewHint = !available
        ? 'Review this relationship first: one or both participants are no longer available.'
        : !isFresh
          ? 'Review this relationship first: a participant source changed.'
          : null;
      return <article key={relationship.id}><h3>{name(relationship.fromDocumentId)} <span>{relationship.type}</span> {name(relationship.toDocumentId)}</h3><p>{relationship.description}</p><p className="small-copy">{relationship.status}{!isFresh ? ' · Participant source changed; review this relationship.' : ''}{relationship.uncertainty ? ` · Uncertainty: ${relationship.uncertainty}` : ''}</p><div className="workshop-actions"><button disabled={disabled} onClick={() => { setEditing(structuredClone(relationship)); setError(''); }}>Review relationship</button><button disabled={disabled} onClick={() => onOpenDocument(relationship.fromDocumentId)}>Open {name(relationship.fromDocumentId)}</button><button disabled={disabled} onClick={() => onOpenDocument(relationship.toDocumentId)}>Open {name(relationship.toDocumentId)}</button>{relationship.status !== 'archived' && <button aria-describedby={reviewHint ? exploreHintId : undefined} disabled={disabled || !onExploreRelationship || !available || !isFresh} onClick={() => onExploreRelationship?.(relationship)}>Explore this relationship</button>}</div>{reviewHint && relationship.status !== 'archived' && <p id={exploreHintId} className="small-copy">{reviewHint}</p>}</article>;
    })}
    {editing && <form onSubmit={event => {
      event.preventDefault();
      const from = participants.find(document => document.head.documentId === editing.fromDocumentId);
      const to = participants.find(document => document.head.documentId === editing.toDocumentId);
      if (!from || !to || from.head.documentId === to.head.documentId || !editing.description.trim()) { setError('Choose two different saved participants and describe this direction of the relationship.'); return; }
      const relationship = { ...editing, sourceHeads: [from.head, to.head] };
      onChange(current => ({ ...current, relationships: [...current.relationships.filter(item => item.id !== editing.id), relationship] })); setEditing(null);
    }}><h3>Describe one direction</h3><label>From<select disabled={disabled} value={editing.fromDocumentId} onChange={event => setEditing({ ...editing, fromDocumentId: event.target.value })}>{participants.map(document => <option key={document.head.documentId} value={document.head.documentId}>{document.title}</option>)}</select></label><label>Relationship type<input disabled={disabled} value={editing.type} maxLength={160} required onChange={event => setEditing({ ...editing, type: event.target.value })} placeholder="trusts, owes, fears, depends on…" /></label><label>To<select disabled={disabled} value={editing.toDocumentId} onChange={event => setEditing({ ...editing, toDocumentId: event.target.value })}>{participants.map(document => <option key={document.head.documentId} value={document.head.documentId}>{document.title}</option>)}</select></label><label>What this person wants, misunderstands, or values<textarea disabled={disabled} required value={editing.description} maxLength={6000} onChange={event => setEditing({ ...editing, description: event.target.value })} /></label><label>What remains uncertain<textarea disabled={disabled} value={editing.uncertainty} maxLength={2000} onChange={event => setEditing({ ...editing, uncertainty: event.target.value })} /></label><label>Decision status<select disabled={disabled} value={editing.status} onChange={event => setEditing({ ...editing, status: event.target.value as WorkshopRelationship['status'] })}><option value="tentative">Tentative</option><option value="chosen">Chosen author intention</option><option value="archived">Archived</option></select></label><div className="workshop-actions"><button type="button" disabled={disabled} onClick={() => setEditing(null)}>Cancel</button><button className="primary-button" disabled={disabled}>Save relationship</button></div></form>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
