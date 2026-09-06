import { useEffect, useRef, useState } from 'react';
import { bodyHash, canonicalJson } from '../editor/document';
import type { Scope } from '../editor/selection';
import type { DocumentSession, SessionState } from '../editor/session';
import { chapterReviewStatus, markReady, readReviewedRecordSet, reviewedEntityCatalog, readReviewStage, stageAuthorReview, type MarkReady, type PossessionRecord, type ReviewedEntityChoice, type ReviewMember, type ReviewStage, type ReviewStatus, type StageAuthorReview } from '../ipc/reviews';
import { readDocumentRevision } from '../ipc/history';
import type { ProjectAccess, Revision } from '../ipc/projects';
import { SavedProse } from './HistoryPanel';
import { ReviewEvidenceEditor } from './ReviewEvidenceEditor';

type Pending = { kind: 'stage'; request: StageAuthorReview } | { kind: 'mark'; request: MarkReady; reviewed: ReviewStage };
const labels: Record<ReviewStatus['state'], string> = {
  noReview: 'Not reviewed yet', ready: 'Reviewed version is current', changedProse: 'Writing changed since review',
  earlierBasisChanged: 'Earlier story needs review', reviewNeeded: 'Review needs attention',
};
function message(error: unknown): string {
  return error && typeof error === 'object' && 'detail' in error ? String(error.detail)
    : error instanceof Error ? error.message : 'Could not confirm the review. Check its saved result before trying again.';
}
function uncertain(error: unknown): boolean {
  return !error || typeof error !== 'object' || !('code' in error)
    || ['UncertainOutcome', 'ReconciliationRequired', 'PersistenceUnavailable', 'ProtocolError'].includes(String(error.code));
}

function EarlierReview({ access, member }: { access: ProjectAccess; member: ReviewMember }) {
  const [open, setOpen] = useState(false);
  const [revision, setRevision] = useState<Revision | null>(null);
  const [error, setError] = useState('');
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let active = true; setRevision(null); setError('');
    if (!open) return;
    void readDocumentRevision(access, member.documentId, member.revisionId).then(async result => {
      if (!active) return;
      if (result.id !== member.revisionId || canonicalJson(result.head) !== canonicalJson(member.head)
        || await bodyHash(canonicalJson(result.body)) !== member.head.bodyHash) throw new Error('The earlier writing did not match this review. Try reading it again.');
      if (active) setRevision(result);
    }).catch(reason => { if (active) setError(message(reason)); });
    return () => { active = false; };
  }, [open, access.projectId, access.operationNamespace, access.writerLease, member.revisionId, attempt]);
  return <details onToggle={event => setOpen(event.currentTarget.open)}>
    <summary>{member.title}</summary>
    {open && <div className="review-earlier-prose">{error ? <><p role="alert">{error}</p><button onClick={() => setAttempt(value => value + 1)}>Try reading again</button></>
      : revision ? <SavedProse body={revision.body} /> : <p role="status">Reading the reviewed version…</p>}</div>}
  </details>;
}

/** Original prose and explicit author review only; no model or manuscript write. */
export function ReviewPanel({ session, state, visible, onClose, captureSelection }: {
  session: DocumentSession; state: SessionState; visible: boolean; onClose(): void; captureSelection?: () => Scope | null;
}) {
  const selectEvidence = captureSelection ?? (() => null);
  const [status, setStatus] = useState<ReviewStatus | null>(null);
  const [stage, setStage] = useState<ReviewStage | null>(null);
  const [loading, setLoading] = useState(false);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState('');
  const [readError, setReadError] = useState('');
  const [notice, setNotice] = useState('');
  const [retry, setRetry] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [currentRecords, setCurrentRecords] = useState<PossessionRecord[]>([]);
  const [currentRecordsCurrent, setCurrentRecordsCurrent] = useState(true);
  const [draftRecords, setDraftRecords] = useState<PossessionRecord[]>([]);
  const [orphanedRecords, setOrphanedRecords] = useState<PossessionRecord[] | null>(null);
  const [evidenceEditing, setEvidenceEditing] = useState(false);
  const [projectEntities, setProjectEntities] = useState<ReviewedEntityChoice[]>([]);
  const [entityError, setEntityError] = useState('');
  const pending = useRef<Pending | null>(null);
  const busy = useRef(false);
  const sequence = useRef(0);
  const heading = useRef<HTMLHeadingElement>(null);
  const focusAfterSave = useRef(false);
  const access = session.projectAccess;
  const owner = `${access.projectId}/${access.operationNamespace}/${state.head.documentId}`;
  const liveOwner = useRef(owner); liveOwner.current = owner;
  const liveVisible = useRef(visible); liveVisible.current = visible;
  const owns = () => liveOwner.current === owner;

  useEffect(() => { setStage(null); setCurrentRecords([]); setCurrentRecordsCurrent(true); setDraftRecords([]); setOrphanedRecords(null); setEvidenceEditing(false); setNotice(''); setError(''); setReadError(''); setRetry(false); pending.current = null; }, [owner]);
  useEffect(() => { if (visible) heading.current?.focus(); }, [visible]);
  useEffect(() => {
    if (visible && !working && focusAfterSave.current) { focusAfterSave.current = false; heading.current?.focus(); }
  }, [visible, working]);
  useEffect(() => {
    const read = ++sequence.current;
    if (!visible || !state.editable || working) return;
    setLoading(true);
    void chapterReviewStatus(access, state.head.documentId).then(async result => {
      if (read !== sequence.current || !owns() || !liveVisible.current) return;
      if (result.documentId !== state.head.documentId || canonicalJson(result.head) !== canonicalJson(state.head)) throw new Error('The review status belongs to a different saved version. Refresh to read it again.');
      let records: PossessionRecord[] = [];
      let bundleCurrent = true;
      if (result.activeBundleId) {
        const bundle = await readReviewedRecordSet(access, state.head.documentId);
        if (read !== sequence.current || !owns() || !liveVisible.current) return;
        if (bundle && (bundle.projectId !== access.projectId || bundle.operationNamespace !== access.operationNamespace || bundle.target.documentId !== state.head.documentId)) throw new Error('The reviewed details belong to a different chapter. Refresh to read them again.');
        records = bundle?.records ?? []; bundleCurrent = bundle?.current ?? true;
      }
      if (read !== sequence.current || !owns() || !liveVisible.current) return;
      setCurrentRecords(records); setCurrentRecordsCurrent(bundleCurrent); setOrphanedRecords(previous => previous ?? (!bundleCurrent && records.length ? structuredClone(records) : null)); setStatus(result); setReadError('');
    }).catch(reason => { if (read === sequence.current && owns() && liveVisible.current) setReadError(message(reason)); })
      .finally(() => { if (read === sequence.current && owns()) setLoading(false); });
    return () => { ++sequence.current; };
  }, [owner, access.writerLease, visible, state.head.version, state.head.bodyHash, state.editable, working, refresh]);

  useEffect(() => {
    let cancelled = false;
    setProjectEntities([]); setEntityError('');
    if (!visible || !state.editable || working) return;
    void reviewedEntityCatalog(access).then(catalog => {
      if (cancelled || !owns()) return;
      if (catalog.projectId !== access.projectId || catalog.operationNamespace !== access.operationNamespace) throw new Error('The object list belongs to another project. Refresh review to read it again.');
      setProjectEntities(catalog.entities);
    }).catch(reason => { if (!cancelled && owns()) setEntityError(message(reason)); });
    return () => { cancelled = true; };
  }, [owner, access.writerLease, visible, state.editable, state.head.version, working, refresh]);

  async function perform(operation: Pending) {
    // Reconciliation rotates only the lease. Every logical payload stays fixed.
    const currentAccess = session.projectAccess;
    if (operation.kind === 'stage') {
      const result = await stageAuthorReview({ ...operation.request, access: currentAccess });
      if (result.projectId !== currentAccess.projectId || result.operationNamespace !== currentAccess.operationNamespace
        || canonicalJson(result.target) !== canonicalJson(operation.request.expected)
        || canonicalJson(result.revision.head) !== canonicalJson(result.target)
        || await bodyHash(canonicalJson(result.revision.body)) !== result.target.bodyHash) throw new Error('The saved review did not match the selected writing. Check the review save.');
      if (operation.request.records !== undefined && canonicalJson(result.records ?? []) !== canonicalJson(operation.request.records)) throw new Error('The saved reviewed details did not match the requested complete set. Check the review save.');
      if (owns()) { setStage(result); setOrphanedRecords(null); setDraftRecords(structuredClone(result.records ?? [])); setNotice('Read this saved version, then confirm your review.'); }
    } else {
      const result = await markReady({ ...operation.request, access: currentAccess });
      if (result.projectId !== currentAccess.projectId || result.operationNamespace !== currentAccess.operationNamespace
        || result.stageId !== operation.request.stageId || canonicalJson(result.target) !== canonicalJson(operation.reviewed.target)) throw new Error('The review acknowledgment did not match your decision. Check the review save.');
      if (owns()) { setStage(null); setNotice('Your review is saved. The chapter remains editable.'); }
    }
    if (owns()) { pending.current = null; setRetry(false); focusAfterSave.current = true; setRefresh(value => value + 1); }
  }
  async function resume() {
    const stageId = status?.pendingStageId;
    if (!stageId || busy.current || !state.editable) return;
    busy.current = true; setWorking(true); setError(''); setNotice('');
    try {
      const currentAccess = session.projectAccess;
      const saved = await readReviewStage(currentAccess, stageId);
      if (saved.id !== stageId || saved.projectId !== currentAccess.projectId || saved.operationNamespace !== currentAccess.operationNamespace
        || saved.target.documentId !== state.head.documentId || canonicalJson(saved.revision.head) !== canonicalJson(saved.target)
        || await bodyHash(canonicalJson(saved.revision.body)) !== saved.target.bodyHash) throw new Error('The saved review did not match this chapter. Refresh its review status.');
      if (owns()) { setStage(saved); setOrphanedRecords(null); setDraftRecords(structuredClone(saved.records ?? [])); setNotice('Your saved review is open. Read it before confirming.'); focusAfterSave.current = true; }
    } catch (reason) { if (owns()) setError(message(reason)); }
    finally { busy.current = false; if (owns()) setWorking(false); }
  }
  async function act(kind: 'stage' | 'mark' | 'retry') {
    if (busy.current || (!state.editable && kind !== 'retry')) return;
    busy.current = true; setWorking(true); setError(''); setNotice('');
    try {
      if (kind === 'retry' && session.state.phase === 'reconciling') await session.reconcile();
      let reviewRejection: unknown;
      await session.projectWrite(async () => {
        if (kind === 'stage') {
          const request: StageAuthorReview = { access: session.projectAccess, operationId: crypto.randomUUID(), expected: session.state.head };
          if (stage && (outdated || canonicalJson(draftRecords) !== canonicalJson(stage.records ?? []))) request.records = structuredClone(draftRecords);
          else if (!stage && orphanedRecords !== null) request.records = structuredClone(orphanedRecords);
          pending.current = { kind: 'stage', request };
        } else if (kind === 'mark') {
          if (!stage) return;
          pending.current = { kind: 'mark', reviewed: stage, request: { access: session.projectAccess, operationId: crypto.randomUUID(), stageId: stage.id } };
        }
        if (pending.current) {
          try { await perform(pending.current); }
          catch (reason) {
            if (reason && typeof reason === 'object' && 'code' in reason
              && ['ReviewStageStale', 'ReviewBasisUnavailable', 'ReviewStageNotFound', 'ReviewLimitExceeded', 'InvalidReviewedRecords', 'OperationIdReusedWithDifferentPayload'].includes(String(reason.code))) {
              reviewRejection = reason;
            } else { throw reason; }
          }
        }
      });
      // A definite review rejection did not change prose or its save outcome.
      // Unknown outcomes still pass through the shared reconciliation fence.
      if (reviewRejection) throw reviewRejection;
    } catch (reason) {
      if (owns()) {
        setError(message(reason));
        const keep = !!pending.current && uncertain(reason);
        setRetry(keep); if (!keep) {
          const changedDraft = pending.current?.kind === 'stage' && !!stage && canonicalJson(draftRecords) !== canonicalJson(stage.records ?? []);
          if (changedDraft) setOrphanedRecords(structuredClone(draftRecords));
          pending.current = null;
          if (reason && typeof reason === 'object' && 'code' in reason
            && ['ReviewStageStale', 'ReviewBasisUnavailable', 'VersionConflict'].includes(String(reason.code))) {
            setStage(null); setRefresh(value => value + 1);
          }
        }
      }
    } finally { busy.current = false; if (owns()) { setWorking(false); setLoading(false); } }
  }
  if (!visible) return null;
  const outdated = !!stage && (state.dirty || canonicalJson(stage.target) !== canonicalJson(state.head));
  const recordsChanged = !!stage && canonicalJson(draftRecords) !== canonicalJson(stage.records ?? []);
  const needsStage = !stage || outdated || recordsChanged;
  const reviewActionLabel = !stage ? (status?.state === 'ready' ? 'Update reviewed details' : 'Review saved chapter')
    : outdated ? 'Review latest saved chapter' : recordsChanged ? 'Save reviewed details' : 'Mark this version reviewed';
  return <aside className="history-panel review-panel" aria-labelledby="review-heading">
    <div className="feedback-heading"><h2 id="review-heading" tabIndex={-1} ref={heading}>Story review</h2><button disabled={working} onClick={onClose}>Back to writing</button></div>
    <div className="review-summary">
      <p>Keep an exact chapter version as reviewed story material. This records your own review; it runs no AI analysis.</p>
      {status && <><h3>{labels[status.state]}</h3>{status.reason && <p>{status.reason}</p>}</>}
      {loading && <p role="status">Reading review status…</p>}
      <div className="history-list-actions"><button disabled={!state.editable || working || loading || evidenceEditing} onClick={() => setRefresh(value => value + 1)}>Refresh review</button>
        {!stage && status?.pendingStageId && !retry && <button disabled={!state.editable || working || evidenceEditing} onClick={() => void resume()}>Resume saved review</button>}
        {!stage && status?.canStage && <button className="primary-button" disabled={!state.editable || working || retry || evidenceEditing} onClick={() => void act('stage')}>{reviewActionLabel}</button>}</div>
      {evidenceEditing && <p className="small-copy" role="status">Finish this reviewed detail with Keep detail or Cancel before saving the review.</p>}
      {error && <p className="history-error" role="alert">{error}</p>}
      {readError && <p className="history-error" role="alert">{readError}</p>}
      {entityError && <p className="history-error" role="alert">Could not load objects from other chapters. {entityError} Use Refresh review to try again; you can still create a new object.</p>}
      {retry && <button disabled={working || state.phase === 'conflict' || state.phase === 'disposed'} onClick={() => void act('retry')}>Check review save</button>}
      {notice && <p role="status">{notice}</p>}
    </div>
    {stage ? <>
      <div className="review-basis"><h3>Saved version {stage.target.version}</h3><p>{stage.prefix.length ? 'Reviewed against these earlier chapters:' : 'This is the first chapter in the reviewed story.'}</p>
        {!!stage.prefix.length && <ul>{stage.prefix.map(member => <li key={member.documentId}><EarlierReview access={access} member={member} /></li>)}</ul>}
      </div>
      <ReviewEvidenceEditor projectEntities={projectEntities} records={draftRecords} disabled={!state.editable || working || retry || outdated} captureSelection={selectEvidence} onChange={setDraftRecords} onEditingChange={setEvidenceEditing} />
      <div className="history-preview" aria-label="Chapter under review"><SavedProse body={stage.revision.body} /></div>
      <div className="history-restore">{outdated ? <p role="status">Your writing changed. Prepare a new review of the saved chapter.</p>
        : <p>Confirm that you have reviewed this chapter against the earlier story. You can keep writing afterward.</p>}
        <button className="primary-button" disabled={!state.editable || working || retry || evidenceEditing} onClick={() => void act(needsStage ? 'stage' : 'mark')}>
          {working ? 'Saving review…' : reviewActionLabel}
        </button></div>
    </> : <div className="review-empty"><p>Your manuscript stays editable. Reviewing chapters is optional.</p>{orphanedRecords !== null ? <><p role="status">{currentRecordsCurrent ? 'Your unsubmitted reviewed details are still here. Prepare the saved chapter again to submit them.' : 'These details belong to an older reviewed version. Reselect or remove any passage before preparing the new review.'}</p><ReviewEvidenceEditor projectEntities={projectEntities} records={orphanedRecords} captureSelection={selectEvidence} onChange={setOrphanedRecords} onEditingChange={setEvidenceEditing} /></> : currentRecords.length ? <ReviewEvidenceEditor projectEntities={projectEntities} records={currentRecords} disabled captureSelection={() => null} onChange={() => {}} /> : <p>No reviewed story details are recorded yet.</p>}</div>}
  </aside>;
}
