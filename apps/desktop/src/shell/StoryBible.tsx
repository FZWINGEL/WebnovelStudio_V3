import { useEffect, useRef, useState } from 'react';
import { documentHistory, readDocument, type DocumentRecord, type OpenedProject } from '../ipc/projects';
import { readWorkshop, type WorkshopDecision } from '../ipc/workshop';
import { describeWorkshopError } from '../workshop/store';
import { plainText } from '../workshop/text';

export function StoryBible({ project, onClose, onOpenDocument }: { project: OpenedProject; onClose(): void; onOpenDocument(documentId: string): void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [items, setItems] = useState<Array<{ decision: WorkshopDecision; document: DocumentRecord; text: string; changed: boolean }>>([]);
  const [loading, setLoading] = useState(true); const [error, setError] = useState('');
  useEffect(() => {
    const previous = document.activeElement;
    dialog.current?.showModal(); let disposed = false;
    void readWorkshop(project.access).then(async view => {
      const chosen = view.state.decisions.filter(decision => decision.status === 'chosen');
      const material = await Promise.all(chosen.map(async decision => {
        const current = await readDocument(project.access, decision.documentId);
        const changed = current.head.version !== decision.head.version || current.head.bodyHash !== decision.head.bodyHash;
        const source = changed ? (await documentHistory(project.access, decision.documentId)).find(revision => revision.id === decision.revisionId)?.body : current.body;
        if (!source) throw new Error(`The chosen version of ${decision.title} is unavailable. Open its source to review it.`);
        return { decision, document: current, text: plainText(source), changed };
      }));
      if (!disposed) setItems(material);
    }).catch(reason => { if (!disposed) setError(describeWorkshopError(reason)); }).finally(() => { if (!disposed) setLoading(false); });
    return () => { disposed = true; if (previous instanceof HTMLElement) previous.focus(); };
  }, [project.access]);
  return <dialog className="story-bible" ref={dialog} aria-labelledby="story-bible-title" onCancel={event => { event.preventDefault(); onClose(); }}>
    <header><div><h1 id="story-bible-title">Story Bible</h1><p>Chosen project material, shown from its saved sources.</p></div><button onClick={onClose} aria-label="Close Story Bible">Close</button></header>
    <p className="small-copy">These are author decisions. Writing access, manuscript evidence, and what a character knows remain separate.</p>
    {loading && <p role="status">Opening chosen material…</p>}{error && <p role="alert">{error}</p>}
    {!loading && !error && !items.length && <p>No versions chosen yet. Develop an idea and use a version when you want it in your working story. You can write at any time.</p>}
    {items.map(({ decision, document: source, text, changed }) => <article key={decision.id}>
      <div className="workshop-section-heading"><h2>{decision.title}</h2><span>Chosen · Author only{decision.fixed ? ' · Keep fixed' : ''}</span></div>
      <p className="small-copy">Source: {source.title}, saved version {decision.head.version}{changed ? ' · Source changed since this choice; review needed.' : ''}</p>
      {decision.rationale && <p><strong>Why this version:</strong> {decision.rationale}</p>}
      <div className="workshop-prose">{text}</div>
      <button onClick={() => { onClose(); onOpenDocument(decision.documentId); }}>Open source: {source.title}</button>
    </article>)}
  </dialog>;
}
