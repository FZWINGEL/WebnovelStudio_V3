import type { DocumentRecord } from '../ipc/projects';
import type { WorkshopAdoptionPreview } from '../ipc/workshop';
import { IMPACT_LABELS } from './AdoptionImpacts';
import { plainText } from './text';

export function AdoptionPreview({ preview, documents }: { preview: WorkshopAdoptionPreview; documents: DocumentRecord[] }) {
  const title = (id: string) => preview.targets.find(target => target.documentId === id)?.title
    ?? preview.endpointSources?.find(document => document.head.documentId === id)?.title
    ?? documents.find(document => document.head.documentId === id)?.title ?? 'Unavailable source';
  return <>
    {preview.targets.map(target => <article key={target.documentId}>
      <h3>{target.title} · {target.expected ? target.mode === 'add' ? 'Add' : 'Replace' : 'New document'}</h3>
      {target.expected && <details><summary>Before · version {target.expected.version}</summary><div className="workshop-prose">{plainText(preview.before.find(source => source.head.documentId === target.documentId)!.body)}</div></details>}
      <h4>After</h4><div className="workshop-prose">{plainText(target.body)}</div>
    </article>)}
    {!!preview.relationships?.length && <section aria-label="Relationships to choose"><h3>Relationships to choose</h3>{preview.relationships.map(link => <article key={link.id}>
      <h4>{title(link.fromDocumentId)} → {title(link.toDocumentId)} · {link.type}</h4><p>{link.description}</p>
      {link.uncertainty && <p>Still uncertain: {link.uncertainty}</p>}
      <p className="small-copy">Chosen author intention. The reverse relationship remains separate.</p>
      <ul>{link.sourceHeads.map(head => <li key={head.documentId}>{title(head.documentId)} · version {head.version}</li>)}</ul>
    </article>)}</section>}
    {!!preview.endpointSources?.length && <details><summary>Existing relationship sources</summary>{preview.endpointSources.map(source => <article key={source.head.documentId}><h4>{source.title} · version {source.head.version}</h4><p className="workshop-prose">{plainText(source.body)}</p></article>)}</details>}
    {!!preview.impacts?.length && <section aria-label="Review flags to save"><h3>Review flags to save</h3>{preview.impacts.map((impact, index) => <article key={`${impact.candidateId}-${impact.documentId}-${index}`}>
      <h4>{title(impact.documentId)} · {IMPACT_LABELS[impact.kind]}</h4><p>{impact.reason}</p><p className="small-copy">Needs review; its content will not be automatically repaired.</p>
    </article>)}</section>}
    {preview.rationale && <p><strong>Your rationale:</strong> {preview.rationale}</p>}
    <p>All {preview.targets.length} material targets and {preview.relationships?.length ?? 0} relationships are checked together. A changed source prevents the whole adoption.</p>
  </>;
}
