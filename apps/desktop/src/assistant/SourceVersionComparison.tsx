import { useEffect, useRef, useState } from 'react';
import { readStoryContextSource, type SourceDescriptor } from '../ipc/context';
import { readDocument, type ProjectAccess } from '../ipc/projects';
import type { WnsDocument } from '../editor/document';
import { documentBlocks } from '../chat/DraftReviewDiff';

/** A local reading aid. Neither button changes the immutable request or its sources. */
export function SourceVersionComparison({ access, snapshotId, source }: {
  access: ProjectAccess; snapshotId: string; source: SourceDescriptor;
}) {
  const [value, setValue] = useState<{ label: string; body: WnsDocument } | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const attempt = useRef(0);
  const identity = `${access.projectId}/${access.operationNamespace}/${access.session}/${access.writerLease}/${snapshotId}/${source.handle}`;
  useEffect(() => { ++attempt.current; setValue(null); setError(''); setBusy(false); return () => { ++attempt.current; }; }, [identity]);
  async function read(current: boolean) {
    const request = ++attempt.current;
    setValue(null); setError(''); setBusy(true);
    try {
      if (source.source.projectId !== access.projectId) throw new Error('This reference belongs to another project. Open its saved history to read it.');
      const result = current
        ? await readDocument(access, source.source.documentId)
        : await readStoryContextSource(access, snapshotId, source.handle);
      if (request !== attempt.current) return;
      if ('head' in result && result.head.documentId !== source.source.documentId) throw new Error('The document did not match this source.');
      setValue({ label: 'head' in result ? `Current saved version · v${result.head.version}` : `Version discussed · revision ${source.source.revisionId}`, body: result.body });
    } catch (reason) {
      if (request === attempt.current) setError(reason && typeof reason === 'object' && 'detail' in reason ? String(reason.detail) : reason instanceof Error ? reason.message : 'This version could not be read.');
    } finally { if (request === attempt.current) setBusy(false); }
  }
  return <div className="context-source-versions">
    <button type="button" className="quiet-button" aria-label={`Version discussed of ${source.displayName}`} onClick={() => void read(false)}>Version discussed</button>
    <button type="button" className="quiet-button" aria-label={`Current version of ${source.displayName}`} onClick={() => void read(true)}>Current version</button>
    {busy && <p role="status">Reading the selected version…</p>}
    {error && <p role="alert">{error}</p>}
    {value && <section aria-label={value.label}><strong>{value.label}</strong><p className="small-copy">Read-only local view. This does not change what was supplied to the assistant; current saved text may differ from an unsaved editor.</p><div className="chat-prose" tabIndex={0} aria-label={`${value.label} text`}>{documentBlocks(value.body).map((text, index) => <p key={index}>{text || '\u00a0'}</p>)}</div><button type="button" className="quiet-button" onClick={() => setValue(null)}>Close version</button></section>}
  </div>;
}
