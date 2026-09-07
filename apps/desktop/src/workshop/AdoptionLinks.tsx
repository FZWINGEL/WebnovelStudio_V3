import type { DocumentRecord } from '../ipc/projects';

export interface AdoptionMaterialDraft {
  id: string; documentId: string; title: string; kind: string; mode: 'add' | 'replace'; text: string;
}
export interface AdoptionLinkDraft {
  id: string; fromDocumentId: string; toDocumentId: string; type: string; description: string; uncertainty: string;
}
export interface AdoptionParticipant { id: string; title: string; isNew: boolean }

/** New material keeps its identity while the author adds, edits, or reorders the packet. */
export function adoptionParticipants(documents: DocumentRecord[], targets: AdoptionMaterialDraft[]): AdoptionParticipant[] {
  const options = new Map(documents.filter(document => ['character', 'world'].includes(document.kind))
    .map(document => [document.head.documentId, { id: document.head.documentId, title: document.title, isNew: false }]));
  for (const target of targets) {
    if (!target.documentId && ['character', 'world'].includes(target.kind)) options.set(target.id, { id: target.id, title: target.title || 'Untitled new material', isNew: true });
  }
  return [...options.values()];
}

export function validAdoptionLinks(links: AdoptionLinkDraft[], participants: AdoptionParticipant[]) {
  return links.every(link => link.fromDocumentId !== link.toDocumentId && link.type.trim() && link.description.trim()
    && participants.some(item => item.id === link.fromDocumentId) && participants.some(item => item.id === link.toDocumentId));
}

export function AdoptionLinks({ participants, links, onChange }: {
  participants: AdoptionParticipant[]; links: AdoptionLinkDraft[]; onChange(links: AdoptionLinkDraft[]): void;
}) {
  function edit(id: string, change: Partial<AdoptionLinkDraft>) { onChange(links.map(link => link.id === id ? { ...link, ...change } : link)); }
  return <section aria-label="Relationships in this adoption">
    <h3>Connect this material</h3><p className="small-copy">Optionally choose a relationship in the same review, including people or groups you are creating above. This records one direction of an author intention.</p>
    {links.map((link, index) => <fieldset key={link.id}><legend>Relationship {index + 1}</legend>
      {(['fromDocumentId', 'toDocumentId'] as const).map((field, position) => <label key={field}>{position === 0 ? 'From' : 'To'}<select value={link[field]} onChange={event => edit(link.id, { [field]: event.target.value })}>
        <option value="">Choose a person or group</option>
        {!participants.some(item => item.id === link[field]) && link[field] && <option value={link[field]}>Removed destination — choose again</option>}
        {participants.map(item => <option key={item.id} value={item.id}>{item.title}{item.isNew ? ' · New in this decision' : ''}</option>)}
      </select></label>)}
      <label>Relationship type<input value={link.type} maxLength={160} onChange={event => edit(link.id, { type: event.target.value })} placeholder="trusts, owes, fears, depends on…" /></label>
      <label>What this relationship means<textarea value={link.description} maxLength={6000} onChange={event => edit(link.id, { description: event.target.value })} /></label>
      <label>What remains uncertain<textarea value={link.uncertainty} maxLength={2000} onChange={event => edit(link.id, { uncertainty: event.target.value })} /></label>
      <button onClick={() => onChange(links.filter(item => item.id !== link.id))}>Remove relationship</button>
    </fieldset>)}
    {links.length > 0 && !validAdoptionLinks(links, participants) && <p role="status">Each relationship needs two different available participants, a type, and a description.</p>}
    <button disabled={participants.length < 2} onClick={() => onChange([...links, { id: crypto.randomUUID(), fromDocumentId: participants[0]?.id ?? '', toDocumentId: participants[1]?.id ?? '', type: 'trusts', description: '', uncertainty: '' }])}>Include a relationship</button>
    {participants.length < 2 && <p className="small-copy">Add two character or world destinations to connect new people or groups.</p>}
  </section>;
}
