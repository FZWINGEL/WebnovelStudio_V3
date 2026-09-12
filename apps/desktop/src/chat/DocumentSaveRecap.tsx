import { useEffect, useRef, useState } from 'react';
import type { ChatDocumentSave } from '../ipc/projectChat';
import type { DocumentRecord, ProjectAccess, Revision } from '../ipc/projects';
import { readDocumentRevision } from '../ipc/history';
import { SavedProse } from '../editor/HistoryPanel';
import { sameDocumentHead } from '../kernel';

/** A projection of retained save receipts, never added to model history. */
export function DocumentSaveRecap({ access, saves, documents, onOpenDocument }: {
  access: ProjectAccess; saves: ChatDocumentSave[]; documents: DocumentRecord[];
  onOpenDocument?: (document: DocumentRecord) => Promise<void> | void;
}) {
  const [selected, setSelected] = useState<ChatDocumentSave | null>(null);
  const [revision, setRevision] = useState<Revision | null>(null);
  const [error, setError] = useState('');
  const identity = `${access.projectId}/${access.operationNamespace}/${access.session}/${access.writerLease}`;
  const identityRef = useRef(identity); identityRef.current = identity;
  const attempt = useRef(0);
  useEffect(() => { ++attempt.current; setSelected(null); setRevision(null); setError(''); return () => { ++attempt.current; }; }, [identity]);
  async function inspect(save: ChatDocumentSave) {
    if (!save.revisionId) return;
    const request = ++attempt.current;
    setSelected(save); setRevision(null); setError('');
    try {
      const value = await readDocumentRevision(access, save.head.documentId, save.revisionId);
      if (request !== attempt.current || identityRef.current !== identity) return;
      if (value.id !== save.revisionId || !sameDocumentHead(value.head, save.head)) throw new Error('The retained version does not match this save.');
      setRevision(value);
    } catch (reason) {
      if (request === attempt.current && identityRef.current === identity) setError(reason instanceof Error ? reason.message : 'This saved version could not be read.');
    }
  }
  if (!saves.length) return null;
  return <details className="chat-document-save-recap">
    <summary>Recent manual saves · {saves.length} document{saves.length === 1 ? '' : 's'}</summary>
    <p className="chat-muted">Latest save for up to 20 documents since this conversation began. These are local save receipts; returning here never starts a model request.</p>
    {saves.map(save => {
      const current = documents.find(document => document.head.documentId === save.head.documentId && (document.role ?? 'ordinary') === 'ordinary');
      return <article className="chat-message chat-message-event" key={save.operationId}>
        <p>You saved <strong>{save.title}</strong> · Version {save.head.version}</p>
        {current && onOpenDocument && <button type="button" onClick={() => void onOpenDocument(current)}>Open {save.title}</button>}
        {save.revisionId ? <button type="button" onClick={() => void inspect(save)}>Inspect saved version {save.head.version}</button> : <p className="chat-muted">This save receipt has no separate retained checkpoint.</p>}
      </article>;
    })}
    {selected && <section aria-label="Saved document event"><h3>{selected.title} · Version {selected.head.version}</h3>{error ? <p role="alert">{error}</p> : revision ? <SavedProse body={revision.body} /> : <p role="status">Reading the retained version…</p>}<button type="button" onClick={() => { ++attempt.current; setSelected(null); setRevision(null); }}>Close saved version</button></section>}
  </details>;
}
