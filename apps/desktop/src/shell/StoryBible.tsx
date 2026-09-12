import { sameHead } from '../kernel';
import { useEffect, useRef, useState } from 'react';
import { readDocument, type DocumentRecord, type OpenedProject } from '../ipc/projects';
import { readDocumentRevision } from '../ipc/history';
import { bodyHash, canonicalJson, type WnsDocument } from '../editor';
import { readWorkshop, type WorkshopDecision } from '../ipc/workshop';
import { describeWorkshopError } from '../workshop';
import { plainText } from '../workshop';

type StoryBibleItem = {
  decision: WorkshopDecision;
  document: DocumentRecord | null;
  text: string | null;
  changed: boolean;
  sourceAvailable: boolean;
};

async function readExactRevision(project: OpenedProject, decision: WorkshopDecision): Promise<WnsDocument> {
  const revision = await readDocumentRevision(project.access, decision.documentId, decision.revisionId);
  if (!revision || revision.id !== decision.revisionId || !sameHead(revision.head, decision.head)
    || await bodyHash(canonicalJson(revision.body)) !== decision.head.bodyHash) {
    throw new Error('The saved version did not match this choice.');
  }
  return revision.body;
}

async function readDecision(project: OpenedProject, decision: WorkshopDecision): Promise<StoryBibleItem> {
  let current: DocumentRecord | null = null;
  try {
    current = await readDocument(project.access, decision.documentId);
  } catch {
    // A deleted or unavailable current source can still have a readable,
    // immutable accepted revision. Try that exact revision before reporting it.
  }

  if (current && sameHead(current.head, decision.head)) {
    try {
      if (await bodyHash(canonicalJson(current.body)) === decision.head.bodyHash) {
        return { decision, document: current, text: plainText(current.body), changed: false, sourceAvailable: true };
      }
    } catch {
      // Treat a malformed current payload like any other source change and
      // use only a separately verified immutable revision.
    }
  }

  try {
    const body = await readExactRevision(project, decision);
    return { decision, document: current, text: plainText(body), changed: true, sourceAvailable: current !== null };
  } catch {
    return { decision, document: current, text: null, changed: true, sourceAvailable: current !== null };
  }
}

export function StoryBible({ project, onClose, onOpenDocument }: { project: OpenedProject; onClose(): void; onOpenDocument(documentId: string): void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const sequence = useRef(0);
  const [items, setItems] = useState<StoryBibleItem[]>([]);
  const [loading, setLoading] = useState(true); const [error, setError] = useState('');
  useEffect(() => {
    const previous = document.activeElement;
    if (!dialog.current?.open) dialog.current?.showModal();
    const request = ++sequence.current;
    let disposed = false;
    const owns = () => !disposed && sequence.current === request;
    setItems([]); setError(''); setLoading(true);
    void readWorkshop(project.access).then(async view => {
      if (!owns()) return;
      const chosen = view.state.decisions.filter(decision => decision.status === 'chosen');
      const material = await Promise.all(chosen.map(async decision => {
        try {
          return await readDecision(project, decision);
        } catch {
          // Keep a single malformed/unavailable source from hiding the other
          // accepted material. The item remains inspectable as unavailable.
          return { decision, document: null, text: null, changed: true, sourceAvailable: false } satisfies StoryBibleItem;
        }
      }));
      if (owns()) setItems(material);
    }).catch(reason => { if (owns()) setError(describeWorkshopError(reason)); }).finally(() => { if (owns()) setLoading(false); });
    return () => { disposed = true; if (previous instanceof HTMLElement) previous.focus(); };
  }, [project.project.projectId, project.access.projectId, project.access.operationNamespace, project.access.session, project.access.writerLease]);
  return <dialog className="story-bible" ref={dialog} aria-labelledby="story-bible-title" onCancel={event => { event.preventDefault(); onClose(); }}>
    <header><div><h1 id="story-bible-title">Story Bible</h1><p>Chosen project material, shown from its saved sources.</p></div><button onClick={onClose} aria-label="Close Story Bible">Close</button></header>
    <p className="small-copy">These are author decisions. Writing access, manuscript evidence, and what a character knows remain separate.</p>
    {loading && <p role="status">Opening chosen material…</p>}{error && <p role="alert">{error}</p>}
    {!loading && !error && !items.length && <p>No versions chosen yet. Develop an idea and use a version when you want it in your working story. You can write at any time.</p>}
    {items.map(({ decision, document: source, text, changed, sourceAvailable }) => <article key={decision.id}>
      <div className="workshop-section-heading"><h2>{decision.title}</h2><span>Chosen · Author only{decision.fixed ? ' · Keep fixed' : ''}</span></div>
      <p className="small-copy">Source: {source?.title ?? decision.title}, saved version {decision.head.version}{changed ? text !== null ? sourceAvailable ? ' · Source changed since this choice; showing the saved version.' : ' · Current source unavailable; showing the saved version.' : ' · Saved version unavailable.' : ''}</p>
      {decision.rationale && <p><strong>Why this version:</strong> {decision.rationale}</p>}
      {text !== null ? <div className="workshop-prose">{text}</div> : <p role="status">The saved source version is unavailable. Try reopening the project. Your choice and reason are still saved.</p>}
      {text !== null && sourceAvailable && <button onClick={() => { onClose(); onOpenDocument(decision.documentId); }}>{changed ? 'Open current source' : 'Open source'}: {source?.title ?? decision.documentId}</button>}
    </article>)}
  </dialog>;
}
