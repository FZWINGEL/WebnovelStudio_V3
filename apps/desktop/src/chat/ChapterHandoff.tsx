import { useState } from 'react';
import type { DocumentRecord } from '../ipc/projects';

export interface ChapterHandoffProposal {
  targetHandle: string | null;
  proposedTitle: string;
  instruction: string;
  brief: string;
}

export function parseChapterHandoff(value: unknown): ChapterHandoffProposal | null {
  if (!value || typeof value !== 'object') return null;
  const proposal = value as Record<string, unknown>;
  if (proposal.targetHandle !== null && proposal.targetHandle !== undefined && typeof proposal.targetHandle !== 'string') return null;
  if (typeof proposal.proposedTitle !== 'string' || typeof proposal.instruction !== 'string' || typeof proposal.brief !== 'string') return null;
  return { targetHandle: proposal.targetHandle as string | null ?? null, proposedTitle: proposal.proposedTitle, instruction: proposal.instruction, brief: proposal.brief };
}

export function ChapterHandoff({ proposal, documents, onPrepare }: {
  proposal: ChapterHandoffProposal;
  documents: DocumentRecord[];
  onPrepare(targetId: string | null, title: string): Promise<void>;
}) {
  const [target, setTarget] = useState(proposal.targetHandle ? '' : 'new');
  const [title, setTitle] = useState(proposal.proposedTitle);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const chapters = documents.filter(document => document.kind === 'chapter' && (document.role ?? 'ordinary') === 'ordinary');
  const prepare = async () => {
    setBusy(true); setError('');
    try { await onPrepare(target === 'new' ? null : target, title.trim()); }
    catch (reason) { setError(reason instanceof Error ? reason.message : 'The chapter task could not be prepared.'); }
    finally { setBusy(false); }
  };
  return <section className="chat-chapter-handoff" aria-label="Proposed chapter writing task">
    <h3>Ready to write a chapter?</h3>
    <p>{proposal.instruction}</p>
    {proposal.brief && <details><summary>Proposed writing guidance</summary><p>{proposal.brief}</p></details>}
    <label>Chapter destination<select aria-label="Chapter destination" value={target} disabled={busy} onChange={event => setTarget(event.target.value)}>
      <option value="">Choose a chapter</option><option value="new">New blank chapter</option>
      {chapters.map(document => <option key={document.head.documentId} value={document.head.documentId}>{document.title}</option>)}
    </select></label>
    {target === 'new' && <label>New chapter title<input aria-label="New chapter title" maxLength={160} value={title} disabled={busy} onChange={event => setTitle(event.target.value)} /></label>}
    <p className="chat-muted">Prepare the target and review the writing brief, then Send when ready. Continuation adds prose after the current text.</p>
    <button type="button" disabled={busy || !target || (target === 'new' && !title.trim())} onClick={() => void prepare()}>{busy ? 'Preparing chapter…' : target === 'new' ? 'Create blank chapter and prepare writing' : 'Prepare chapter continuation'}</button>
    {error && <p role="alert">{error}</p>}
  </section>;
}
