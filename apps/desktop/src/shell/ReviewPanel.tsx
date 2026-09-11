import { errorTextFor, sameDocumentHead } from '../kernel';
import { useEffect, useRef, useState } from 'react';
import { bodyHash, canonicalJson } from '../editor/document';
import type { Scope } from '../editor/selection';
import type { DocumentSession, SessionState } from '../editor/session';
import { chapterReviewStatus, markReady, readReviewedRecordSet, reviewedEntityCatalog, reviewedKnowledgeCharacterCatalog, reviewedKnowledgeTopicCatalog, reviewedPromiseCatalog, readReviewStage, stageAuthorReview, type KnowledgeRecord, type MarkReady, type PossessionRecord, type PromiseRecord, type ReviewedEntityChoice, type ReviewMember, type ReviewStage, type ReviewStatus, type StageAuthorReview } from '../ipc/reviews';
import type { SourceRef } from '../ipc/context';
import type { SummaryChange, SummaryRevision } from '../ipc/reviews';
import { readDocumentRevision } from '../ipc/history';
import type { ProjectAccess, Revision } from '../ipc/projects';
import { SavedProse } from './HistoryPanel';
import { ReviewEvidenceEditor } from './ReviewEvidenceEditor';
import { ReviewPromiseEditor } from './ReviewPromiseEditor';
import { ReviewKnowledgeEditor } from './ReviewKnowledgeEditor';
import { ReviewSummaryEditor, type ReviewSummaryDraft } from './ReviewSummaryEditor';

type Pending = { kind: 'stage'; request: StageAuthorReview } | { kind: 'mark'; request: MarkReady; reviewed: ReviewStage };
const labels: Record<ReviewStatus['state'], string> = {
  noReview: 'Not reviewed yet', ready: 'Reviewed version is current', changedProse: 'Writing changed since review',
  earlierBasisChanged: 'Earlier story needs review', reviewNeeded: 'Review needs attention',
};
const message = errorTextFor('Could not confirm the review. Check its saved result before trying again.');
function uncertain(error: unknown): boolean {
  return !error || typeof error !== 'object' || !('code' in error)
    || ['UncertainOutcome', 'ReconciliationRequired', 'PersistenceUnavailable', 'ProtocolError'].includes(String(error.code));
}
function summarySource(access: ProjectAccess, revision: Revision): SourceRef {
  return { projectId: access.projectId, documentId: revision.head.documentId, revisionId: revision.id, bodyHash: revision.head.bodyHash };
}
function summaryBasisMatches(summary: SummaryRevision, access: ProjectAccess, target: { documentId: string; bodyHash: string }, current: boolean): boolean {
  return current && summary.source.projectId === access.projectId && summary.source.documentId === target.documentId && summary.source.bodyHash === target.bodyHash;
}
function makeSummaryDraft(summary: SummaryRevision | null | undefined, canInherit: boolean): ReviewSummaryDraft {
  return summary ? { choice: canInherit ? 'inherit' : 'required', text: summary.text, audience: summary.audience } : { choice: 'inherit', text: '', audience: 'authorRoom' };
}
function summaryJson(summary: SummaryRevision): string {
  return JSON.stringify({
    id: summary.id,
    text: summary.text,
    audience: summary.audience,
    source: {
      projectId: summary.source.projectId,
      documentId: summary.source.documentId,
      revisionId: summary.source.revisionId,
      bodyHash: summary.source.bodyHash,
    },
    dependencies: summary.dependencies.map(member => ({
      documentId: member.documentId,
      title: member.title,
      bundleId: member.bundleId,
      revisionId: member.revisionId,
      head: { documentId: member.head.documentId, version: member.head.version, bodyHash: member.head.bodyHash },
    })),
  });
}
async function validateSummary(summary: SummaryRevision | null | undefined, summaryHash: string | null | undefined, access: ProjectAccess, revision: Revision, prefix?: ReviewMember[]): Promise<void> {
  if (!summary) {
    if (summaryHash !== undefined && summaryHash !== null) throw new Error('The saved narrative summary fingerprint was present without a summary. Check the review save.');
    return;
  }
  const source = summarySource(access, revision);
  if (!summary.id || new TextEncoder().encode(summary.text).length > 16 * 1024 || !['authorRoom', 'reader'].includes(summary.audience)
    || canonicalJson(summary.source) !== canonicalJson(source) || (prefix !== undefined && canonicalJson(summary.dependencies) !== canonicalJson(prefix))
    || !summaryHash || summaryHash !== await bodyHash(summaryJson(summary))) throw new Error('The saved narrative summary did not match the reviewed chapter, its earlier basis, or its fingerprint. Check the review save.');
}
function summaryChange(value: ReviewSummaryDraft): SummaryChange | undefined {
  if (value.choice === 'inherit') return undefined;
  if (value.choice === 'clear') return { kind: 'clear' };
  if (value.choice === 'required') throw new Error('This saved summary no longer matches the chapter. Choose Use this summary, edit it, or Clear summary before saving the review.');
  if (!value.text.trim()) throw new Error('Enter a narrative summary or choose Clear summary.');
  if (new TextEncoder().encode(value.text).length > 16 * 1024) throw new Error('The narrative summary is limited to 16 KiB. Shorten it before saving the review.');
  return { kind: 'set', text: value.text, audience: value.audience };
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
  const [currentSummary, setCurrentSummary] = useState<SummaryRevision | null>(null);
  const [currentBundleTarget, setCurrentBundleTarget] = useState<{ documentId: string; version: string; bodyHash: string } | null>(null);
  const [summaryDraft, setSummaryDraft] = useState<ReviewSummaryDraft>({ choice: 'inherit', text: '', audience: 'authorRoom' });
  const [draftRecords, setDraftRecords] = useState<PossessionRecord[]>([]);
  const [orphanedRecords, setOrphanedRecords] = useState<PossessionRecord[] | null>(null);
  const [evidenceEditing, setEvidenceEditing] = useState(false);
  const [projectEntities, setProjectEntities] = useState<ReviewedEntityChoice[]>([]);
  const [entityError, setEntityError] = useState('');
  const [currentPromises, setCurrentPromises] = useState<PromiseRecord[]>([]);
  const [draftPromises, setDraftPromises] = useState<PromiseRecord[]>([]);
  const [orphanedPromises, setOrphanedPromises] = useState<PromiseRecord[] | null>(null);
  const [projectPromises, setProjectPromises] = useState<ReviewedEntityChoice[]>([]);
  const [promiseError, setPromiseError] = useState('');
  const [currentKnowledge, setCurrentKnowledge] = useState<KnowledgeRecord[]>([]);
  const [draftKnowledge, setDraftKnowledge] = useState<KnowledgeRecord[]>([]);
  const [orphanedKnowledge, setOrphanedKnowledge] = useState<KnowledgeRecord[] | null>(null);
  const [projectCharacters, setProjectCharacters] = useState<ReviewedEntityChoice[]>([]);
  const [projectTopics, setProjectTopics] = useState<ReviewedEntityChoice[]>([]);
  const [knowledgeError, setKnowledgeError] = useState('');
  const [detailKind, setDetailKind] = useState<'possessions' | 'promises' | 'knowledge'>('possessions');
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

  useEffect(() => { setStage(null); setCurrentPromises([]); setDraftPromises([]); setOrphanedPromises(null); setCurrentKnowledge([]); setDraftKnowledge([]); setOrphanedKnowledge(null); setDetailKind('possessions'); setCurrentRecords([]); setCurrentRecordsCurrent(true); setCurrentSummary(null); setCurrentBundleTarget(null); setSummaryDraft({ choice: 'inherit', text: '', audience: 'authorRoom' }); setDraftRecords([]); setOrphanedRecords(null); setEvidenceEditing(false); setNotice(''); setError(''); setReadError(''); setRetry(false); pending.current = null; }, [owner]);
  useEffect(() => { if (visible) heading.current?.focus(); }, [visible]);
  useEffect(() => {
    if (visible && !working && focusAfterSave.current) { focusAfterSave.current = false; heading.current?.focus(); }
  }, [visible, working]);
  useEffect(() => {
    const read = ++sequence.current;
    if (!visible || !state.editable || working || stage) return;
    setLoading(true); setStatus(null);
    void chapterReviewStatus(access, state.head.documentId).then(async result => {
      if (read !== sequence.current || !owns() || !liveVisible.current) return;
      if (result.documentId !== state.head.documentId || canonicalJson(result.head) !== canonicalJson(state.head)) throw new Error('The review status belongs to a different saved version. Refresh to read it again.');
      let records: PossessionRecord[] = [];
      let promises: PromiseRecord[] = [];
      let knowledge: KnowledgeRecord[] = [];
      let summary: SummaryRevision | null = null;
      let bundleCurrent = true;
      let bundleTarget: { documentId: string; version: string; bodyHash: string } | null = null;
      if (result.activeBundleId) {
        const bundle = await readReviewedRecordSet(access, state.head.documentId);
        if (read !== sequence.current || !owns() || !liveVisible.current) return;
        if (bundle && (bundle.projectId !== access.projectId || bundle.operationNamespace !== access.operationNamespace || bundle.target.documentId !== state.head.documentId)) throw new Error('The reviewed details belong to a different chapter. Refresh to read them again.');
        records = bundle?.records ?? []; promises = bundle?.promises ?? []; knowledge = bundle?.knowledge ?? []; summary = bundle?.summary ?? null; bundleCurrent = bundle?.current ?? true; bundleTarget = bundle?.target ?? null;
        if (bundle) await validateSummary(bundle.summary, bundle.summaryHash, access, bundle.revision);
      }
      if (read !== sequence.current || !owns() || !liveVisible.current) return;
      setCurrentPromises(promises); setOrphanedPromises(previous => previous ?? (!bundleCurrent && promises.length ? structuredClone(promises) : null));
      setCurrentKnowledge(knowledge); setOrphanedKnowledge(previous => previous ?? (!bundleCurrent && knowledge.length ? structuredClone(knowledge) : null));
      setCurrentRecords(records); setCurrentRecordsCurrent(bundleCurrent); setCurrentBundleTarget(bundleTarget); setOrphanedRecords(previous => previous ?? (!bundleCurrent && records.length ? structuredClone(records) : null));
      setCurrentSummary(summary); setSummaryDraft(makeSummaryDraft(summary, bundleCurrent && !!summary && result.state === 'ready' && sameDocumentHead(bundleTarget, state.head) && summaryBasisMatches(summary, access, state.head, true))); setStatus(result); setReadError('');
    }).catch(reason => { if (read === sequence.current && owns() && liveVisible.current) setReadError(message(reason)); })
      .finally(() => { if (read === sequence.current && owns()) setLoading(false); });
    return () => { ++sequence.current; };
  }, [owner, access.writerLease, visible, state.head.version, state.head.bodyHash, state.editable, working, refresh, stage?.id]);

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

  useEffect(() => {
    let cancelled = false;
    setProjectPromises([]); setPromiseError('');
    if (!visible || !state.editable || working) return;
    void Promise.resolve().then(() => reviewedPromiseCatalog(access)).then(catalog => {
      if (cancelled || !owns()) return;
      if (catalog.projectId !== access.projectId || catalog.operationNamespace !== access.operationNamespace) throw new Error('The promise list belongs to another project. Refresh review to read it again.');
      setProjectPromises(catalog.entities);
    }).catch(reason => { if (!cancelled && owns()) setPromiseError(message(reason)); });
    return () => { cancelled = true; };
  }, [owner, access.writerLease, visible, state.editable, state.head.version, working, refresh]);

  useEffect(() => {
    let cancelled = false;
    setProjectCharacters([]); setProjectTopics([]); setKnowledgeError('');
    if (!visible || !state.editable || working) return;
    void Promise.all([reviewedKnowledgeCharacterCatalog(access), reviewedKnowledgeTopicCatalog(access)]).then(([characters, topics]) => {
      if (cancelled || !owns()) return;
      if (characters.projectId !== access.projectId || characters.operationNamespace !== access.operationNamespace
        || topics.projectId !== access.projectId || topics.operationNamespace !== access.operationNamespace) throw new Error('The character knowledge lists belong to another project. Refresh review to read them again.');
      setProjectCharacters(characters.entities); setProjectTopics(topics.entities);
    }).catch(reason => { if (!cancelled && owns()) setKnowledgeError(message(reason)); });
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
      if (operation.request.promises !== undefined && canonicalJson(result.promises ?? []) !== canonicalJson(operation.request.promises)) throw new Error('The saved promises did not match the requested complete set. Check the review save.');
      if (operation.request.knowledge !== undefined && canonicalJson(result.knowledge ?? []) !== canonicalJson(operation.request.knowledge)) throw new Error('The saved character knowledge did not match the requested complete set. Check the review save.');
      await validateSummary(result.summary, result.summaryHash, currentAccess, result.revision, result.prefix);
      if (operation.request.summary?.kind === 'set'
        && (!result.summary || result.summary.text !== operation.request.summary.text || result.summary.audience !== operation.request.summary.audience)) {
        throw new Error('The saved narrative summary did not match the requested text. Check the review save.');
      }
      if (operation.request.summary?.kind === 'clear' && result.summary) {
        throw new Error('The saved review did not clear its narrative summary. Check the review save.');
      }
      if (owns()) { setStage(result); setSummaryDraft(makeSummaryDraft(result.summary ?? null, true)); setOrphanedPromises(null); setDraftPromises(structuredClone(result.promises ?? [])); setOrphanedKnowledge(null); setDraftKnowledge(structuredClone(result.knowledge ?? [])); setOrphanedRecords(null); setDraftRecords(structuredClone(result.records ?? [])); setNotice('Read this saved version, then confirm your review.'); }
    } else {
      const result = await markReady({ ...operation.request, access: currentAccess });
      if (result.projectId !== currentAccess.projectId || result.operationNamespace !== currentAccess.operationNamespace
        || result.stageId !== operation.request.stageId || canonicalJson(result.target) !== canonicalJson(operation.reviewed.target)) throw new Error('The review acknowledgment did not match your decision. Check the review save.');
      if (operation.reviewed.knowledge !== undefined && canonicalJson(result.knowledge ?? []) !== canonicalJson(operation.reviewed.knowledge)) {
        throw new Error('The review acknowledgment did not preserve its character knowledge. Check the review save.');
      }
      await validateSummary(result.summary, result.summaryHash, currentAccess, operation.reviewed.revision, operation.reviewed.prefix);
      if ((result.summaryHash ?? null) !== (operation.reviewed.summaryHash ?? null)) {
        throw new Error('The review acknowledgment did not preserve its narrative summary fingerprint. Check the review save.');
      }
      if (canonicalJson(result.summary ?? null) !== canonicalJson(operation.reviewed.summary ?? null)) {
        throw new Error('The review acknowledgment did not preserve its narrative summary. Check the review save.');
      }
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
      await validateSummary(saved.summary, saved.summaryHash, currentAccess, saved.revision, saved.prefix);
      if (owns()) { setStage(saved); setSummaryDraft(makeSummaryDraft(saved.summary ?? null, true)); setOrphanedPromises(null); setDraftPromises(structuredClone(saved.promises ?? [])); setOrphanedKnowledge(null); setDraftKnowledge(structuredClone(saved.knowledge ?? [])); setOrphanedRecords(null); setDraftRecords(structuredClone(saved.records ?? [])); setNotice('Your saved review is open. Read it before confirming.'); focusAfterSave.current = true; }
    } catch (reason) { if (owns()) setError(message(reason)); }
    finally { busy.current = false; if (owns()) setWorking(false); }
  }
  async function act(kind: 'stage' | 'mark' | 'retry') {
    if (busy.current || (!state.editable && kind !== 'retry')) return;
    busy.current = true; setWorking(true); setError(''); setNotice('');
    try {
      if (kind === 'retry' && session.state.phase === 'reconciling') await session.reconcile();
      let requestedSummary = kind === 'stage' ? summaryChange(summaryDraft) : undefined;
      if (kind === 'stage' && requestedSummary === undefined && stage) {
        if (stage.summary) requestedSummary = { kind: 'set', text: stage.summary.text, audience: stage.summary.audience };
        else requestedSummary = { kind: 'clear' };
      }
      const oldSummary = stage ? stage.summary ?? null : currentSummary;
      if (kind === 'stage' && stage && outdated && oldSummary && summaryDraft.choice === 'inherit') {
        throw new Error('This chapter changed after the summary was accepted. Choose Use this summary, edit it, or Clear summary before saving the new review.');
      }
      let reviewRejection: unknown;
      await session.projectWrite(async () => {
        if (kind === 'stage') {
          const request: StageAuthorReview = { access: session.projectAccess, operationId: crypto.randomUUID(), expected: session.state.head };
          if (stage) request.records = structuredClone(draftRecords);
          else if (!stage && orphanedRecords !== null) request.records = structuredClone(orphanedRecords);
          if (stage) request.promises = structuredClone(draftPromises);
          else if (!stage && orphanedPromises !== null) request.promises = structuredClone(orphanedPromises);
          if (stage) request.knowledge = structuredClone(draftKnowledge);
          else if (!stage && orphanedKnowledge !== null) request.knowledge = structuredClone(orphanedKnowledge);
          if (requestedSummary !== undefined) request.summary = structuredClone(requestedSummary);
          pending.current = { kind: 'stage', request };
        } else if (kind === 'mark') {
          if (!stage) return;
          pending.current = { kind: 'mark', reviewed: stage, request: { access: session.projectAccess, operationId: crypto.randomUUID(), stageId: stage.id } };
        }
        if (pending.current) {
          try { await perform(pending.current); }
          catch (reason) {
            if (reason && typeof reason === 'object' && 'code' in reason
            && ['ReviewStageStale', 'ReviewBasisUnavailable', 'ReviewStageNotFound', 'ReviewLimitExceeded', 'InvalidReviewedRecords', 'InvalidReviewedPromises', 'InvalidReviewedKnowledge', 'InvalidReviewedSummary', 'ReviewSummaryRequired', 'OperationIdReusedWithDifferentPayload'].includes(String(reason.code))) {
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
          if (pending.current?.kind === 'stage' && stage && canonicalJson(draftPromises) !== canonicalJson(stage.promises ?? [])) setOrphanedPromises(structuredClone(draftPromises));
          if (pending.current?.kind === 'stage' && stage && canonicalJson(draftKnowledge) !== canonicalJson(stage.knowledge ?? [])) setOrphanedKnowledge(structuredClone(draftKnowledge));
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
  const recordsChanged = !!stage && (canonicalJson(draftRecords) !== canonicalJson(stage.records ?? []) || canonicalJson(draftPromises) !== canonicalJson(stage.promises ?? []) || canonicalJson(draftKnowledge) !== canonicalJson(stage.knowledge ?? []));
  const summaryChanged = !!stage && summaryDraft.choice !== 'inherit';
  const needsStage = !stage || outdated || recordsChanged || summaryChanged;
  const stagedSummary = stage ? stage.summary ?? null : currentSummary;
  const summaryCanInherit = !outdated && (stage ? stage.summary !== undefined
    : status?.state === 'ready' && sameDocumentHead(currentBundleTarget, state.head) && !!currentSummary && summaryBasisMatches(currentSummary, access, state.head, currentRecordsCurrent));
  const displayedSummaryDraft = outdated && stagedSummary && summaryDraft.choice === 'inherit'
    ? { choice: 'required' as const, text: stagedSummary.text, audience: stagedSummary.audience } : summaryDraft;
  const reviewActionLabel = !stage ? (status?.state === 'ready' ? 'Update reviewed details' : 'Review saved chapter')
    : outdated ? 'Review latest saved chapter' : recordsChanged || summaryChanged ? 'Save reviewed details' : 'Mark this version reviewed';
  return <aside className="history-panel review-panel" aria-labelledby="review-heading">
    <div className="feedback-heading"><h2 id="review-heading" tabIndex={-1} ref={heading}>Story review</h2><button disabled={working} onClick={onClose}>Back to writing</button></div>
    <div className="review-summary">
      <p>Keep an exact chapter version as reviewed story material. This records your own review; it runs no AI analysis.</p>
      {status && <><h3>{labels[status.state]}</h3>{status.reason && <p>{status.reason}</p>}</>}
      {loading && <p role="status">Reading review status…</p>}
      <div className="history-list-actions"><button disabled={!state.editable || working || loading || evidenceEditing} onClick={() => setRefresh(value => value + 1)}>Refresh review</button>
        {!stage && status?.pendingStageId && !retry && <button disabled={!state.editable || working || loading || evidenceEditing} onClick={() => void resume()}>Resume saved review</button>}
        {!stage && status?.canStage && <button className="primary-button" disabled={!state.editable || working || loading || retry || evidenceEditing} onClick={() => void act('stage')}>{reviewActionLabel}</button>}</div>
      {evidenceEditing && <p className="small-copy" role="status">Finish this reviewed detail before saving the review.</p>}
      {error && <p className="history-error" role="alert">{error}</p>}
      {readError && <p className="history-error" role="alert">{readError}</p>}
      {entityError && <p className="history-error" role="alert">Could not load objects from other chapters. {entityError} Use Refresh review to try again; you can still create a new object.</p>}
      {promiseError && <p className="history-error" role="alert">Could not load promises from other chapters. {promiseError} Use Refresh review to try again; you can still create a new promise.</p>}
      {knowledgeError && <p className="history-error" role="alert">Could not load character knowledge choices. {knowledgeError} Use Refresh review to try again; you can still create a new character or topic.</p>}
      {retry && <button disabled={working || state.phase === 'conflict' || state.phase === 'disposed'} onClick={() => void act('retry')}>Check review save</button>}
      {notice && <p role="status">{notice}</p>}
    </div>
    <div className="review-detail-tabs" role="group" aria-label="Reviewed detail type">
      <button aria-pressed={detailKind === 'possessions'} disabled={evidenceEditing || working} onClick={() => setDetailKind('possessions')}>Possessions ({(stage ? draftRecords : orphanedRecords ?? currentRecords).length})</button>
      <button aria-pressed={detailKind === 'promises'} disabled={evidenceEditing || working} onClick={() => setDetailKind('promises')}>Promises ({(stage ? draftPromises : orphanedPromises ?? currentPromises).length})</button>
      <button aria-pressed={detailKind === 'knowledge'} disabled={evidenceEditing || working} onClick={() => setDetailKind('knowledge')}>Knowledge ({(stage ? draftKnowledge : orphanedKnowledge ?? currentKnowledge).length})</button>
    </div>
    {stage ? <>
      <div className="review-basis"><h3>Saved version {stage.target.version}</h3><p>{stage.prefix.length ? 'Reviewed against these earlier chapters:' : 'This is the first chapter in the reviewed story.'}</p>
        {!!stage.prefix.length && <ul>{stage.prefix.map(member => <li key={member.documentId}><EarlierReview access={access} member={member} /></li>)}</ul>}
      </div>
      {detailKind === 'possessions' ? <ReviewEvidenceEditor projectEntities={projectEntities} records={draftRecords} disabled={!state.editable || working || retry || outdated} captureSelection={selectEvidence} onChange={setDraftRecords} onEditingChange={setEvidenceEditing} />
        : detailKind === 'promises' ? <ReviewPromiseEditor projectPromises={projectPromises} records={draftPromises} disabled={!state.editable || working || retry || outdated} captureSelection={selectEvidence} onChange={setDraftPromises} onEditingChange={setEvidenceEditing} />
          : <ReviewKnowledgeEditor projectCharacters={projectCharacters} projectTopics={projectTopics} records={draftKnowledge} disabled={!state.editable || working || retry || outdated} captureSelection={selectEvidence} onChange={setDraftKnowledge} onEditingChange={setEvidenceEditing} />}
      <ReviewSummaryEditor access={access} documentId={stage.target.documentId} target={state.head} current={stagedSummary} canInherit={summaryCanInherit} value={displayedSummaryDraft} disabled={!state.editable || working || retry} onChange={setSummaryDraft} />
      <div className="history-preview" aria-label="Chapter under review"><SavedProse body={stage.revision.body} /></div>
      <div className="history-restore">{outdated ? <p role="status">Your writing changed. Prepare a new review of the saved chapter.</p>
        : <p>Confirm that you have reviewed this chapter against the earlier story. You can keep writing afterward.</p>}
        <button className="primary-button" disabled={!state.editable || working || retry || evidenceEditing} onClick={() => void act(needsStage ? 'stage' : 'mark')}>
          {working ? 'Saving review…' : reviewActionLabel}
        </button></div>
    </> : <div className="review-empty"><p>Your manuscript stays editable. Reviewing chapters is optional.</p>{currentSummary && <ReviewSummaryEditor access={access} documentId={state.head.documentId} target={state.head} current={currentSummary} canInherit={summaryCanInherit} value={summaryDraft} disabled={!state.editable || working || retry} onChange={setSummaryDraft} />}{detailKind === 'promises' ? <>
      {orphanedPromises !== null && <p role="status">These promise details are retained. Reselect or remove outdated passages before preparing the saved chapter again.</p>}
      <ReviewPromiseEditor projectPromises={projectPromises} records={orphanedPromises ?? currentPromises} disabled={orphanedPromises === null || !state.editable || working || retry} captureSelection={selectEvidence} onChange={setOrphanedPromises} onEditingChange={setEvidenceEditing} />
    </> : detailKind === 'knowledge' ? <>
      {orphanedKnowledge !== null && <p role="status">These character knowledge observations are retained. Reselect or remove outdated passages before preparing the saved chapter again.</p>}
      <ReviewKnowledgeEditor projectCharacters={projectCharacters} projectTopics={projectTopics} records={orphanedKnowledge ?? currentKnowledge} disabled={orphanedKnowledge === null || !state.editable || working || retry} captureSelection={selectEvidence} onChange={setOrphanedKnowledge} onEditingChange={setEvidenceEditing} />
    </> : <>{orphanedRecords !== null ? <><p role="status">{currentRecordsCurrent ? 'Your unsubmitted reviewed details are still here. Prepare the saved chapter again to submit them.' : 'These details belong to an older reviewed version. Reselect or remove any passage before preparing the new review.'}</p><ReviewEvidenceEditor projectEntities={projectEntities} records={orphanedRecords} captureSelection={selectEvidence} onChange={setOrphanedRecords} onEditingChange={setEvidenceEditing} /></> : currentRecords.length ? <ReviewEvidenceEditor projectEntities={projectEntities} records={currentRecords} disabled captureSelection={() => null} onChange={() => {}} /> : <p>No reviewed story details are recorded yet.</p>}</>}</div>}
  </aside>;
}
