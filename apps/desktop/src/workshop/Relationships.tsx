import { useState } from 'react';
import type { DocumentRecord } from '../ipc/projects';
import type { WorkshopRelationship, WorkshopSession, WorkshopState } from '../ipc/workshop';

export function Relationships({ state, documents, session, onChange, onOpenDocument }: { state: WorkshopState; documents: DocumentRecord[]; session: WorkshopSession; onChange(change: (state: WorkshopState) => WorkshopState): void; onOpenDocument(documentId: string): void }) {
  const participants = documents.filter(document => ['character', 'world'].includes(document.kind));
  const [editing, setEditing] = useState<WorkshopRelationship | null>(null);
  const [focus, setFocus] = useState(session.focusDocumentId ?? '');
  const [error, setError] = useState('');
  const shown = state.relationships.filter(item => !focus || item.fromDocumentId === focus || item.toDocumentId === focus);
  const name = (id: string) => documents.find(document => document.head.documentId === id)?.title ?? 'Unavailable participant';
  const fresh = (relationship: WorkshopRelationship) => relationship.sourceHeads.every(head => documents.some(document => document.head.documentId === head.documentId && document.head.version === head.version && document.head.bodyHash === head.bodyHash));
  function add() {
    setEditing({ id: crypto.randomUUID(), fromDocumentId: participants[0]?.head.documentId ?? '', toDocumentId: participants[1]?.head.documentId ?? '', type: 'trusts', description: '', uncertainty: '', status: 'tentative', sourceHeads: [] }); setError('');
  }
  return <section className="workshop-relationships" aria-label="Local relationships"><div className="workshop-section-heading"><h2>Relationships</h2><button disabled={participants.length < 2} onClick={add}>Connect people or groups</button></div><p className="small-copy">A relationship has a direction. One person’s trust does not establish the other’s. These are author-room intentions, not character knowledge.</p>
    {participants.length < 2 && <p>Choose two people or groups as saved material before connecting them.</p>}
    <label>Near this person or group<select value={focus} onChange={event => setFocus(event.target.value)}><option value="">All saved relationships</option>{participants.map(document => <option key={document.head.documentId} value={document.head.documentId}>{document.title}</option>)}</select></label>
    {shown.map(relationship => <article key={relationship.id}><h3>{name(relationship.fromDocumentId)} <span>{relationship.type}</span> {name(relationship.toDocumentId)}</h3><p>{relationship.description}</p><p className="small-copy">{relationship.status}{!fresh(relationship) ? ' · Participant source changed; review this relationship.' : ''}{relationship.uncertainty ? ` · Uncertainty: ${relationship.uncertainty}` : ''}</p><div className="workshop-actions"><button onClick={() => { setEditing(structuredClone(relationship)); setError(''); }}>Review relationship</button><button onClick={() => onOpenDocument(relationship.fromDocumentId)}>Open {name(relationship.fromDocumentId)}</button><button onClick={() => onOpenDocument(relationship.toDocumentId)}>Open {name(relationship.toDocumentId)}</button></div></article>)}
    {editing && <form onSubmit={event => {
      event.preventDefault();
      const from = participants.find(document => document.head.documentId === editing.fromDocumentId);
      const to = participants.find(document => document.head.documentId === editing.toDocumentId);
      if (!from || !to || from.head.documentId === to.head.documentId || !editing.description.trim()) { setError('Choose two different saved participants and describe this direction of the relationship.'); return; }
      const relationship = { ...editing, sourceHeads: [from.head, to.head] };
      onChange(current => ({ ...current, relationships: [...current.relationships.filter(item => item.id !== editing.id), relationship] })); setEditing(null);
    }}><h3>Describe one direction</h3><label>From<select value={editing.fromDocumentId} onChange={event => setEditing({ ...editing, fromDocumentId: event.target.value })}>{participants.map(document => <option key={document.head.documentId} value={document.head.documentId}>{document.title}</option>)}</select></label><label>Relationship type<input value={editing.type} maxLength={160} required onChange={event => setEditing({ ...editing, type: event.target.value })} placeholder="trusts, owes, fears, depends on…" /></label><label>To<select value={editing.toDocumentId} onChange={event => setEditing({ ...editing, toDocumentId: event.target.value })}>{participants.map(document => <option key={document.head.documentId} value={document.head.documentId}>{document.title}</option>)}</select></label><label>What this person wants, misunderstands, or values<textarea required value={editing.description} maxLength={6000} onChange={event => setEditing({ ...editing, description: event.target.value })} /></label><label>What remains uncertain<textarea value={editing.uncertainty} maxLength={2000} onChange={event => setEditing({ ...editing, uncertainty: event.target.value })} /></label><label>Decision status<select value={editing.status} onChange={event => setEditing({ ...editing, status: event.target.value as WorkshopRelationship['status'] })}><option value="tentative">Tentative</option><option value="chosen">Chosen author intention</option><option value="archived">Archived</option></select></label><div className="workshop-actions"><button type="button" onClick={() => setEditing(null)}>Cancel</button><button className="primary-button">Save relationship</button></div></form>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
