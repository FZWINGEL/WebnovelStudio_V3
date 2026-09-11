import { errorCode } from '../kernel';
import { useEffect, useMemo, useRef, useState } from 'react';
import { rejectProposal, type PreparedProposal, type Proposal, type StructuredBlock } from '../ipc/proposals';
import type { ProjectAccess } from '../ipc/projects';
import { canonicalJson } from '../editor/document';
import { deriveProposalSourceContext, type ProposalSourceContext } from './proposalSourceContext';
import { StructuredProse, StructuredSuggestionEditor } from './StructuredSuggestionEditor';

export interface ProposalPanelProps {
  access: ProjectAccess;
  proposals: Proposal[];
  disabled?: boolean;
  onPrepareProposal?: (proposal: Proposal, text: string, operationId: string) => Promise<PreparedProposal>;
  onApplyProposal?: (proposal: Proposal, prepared: PreparedProposal) => Promise<void>;
  /** Re-read retained records so a lost preparation/rejection acknowledgment can settle. */
  onRefresh?: () => Promise<void>;
}

type ProposalStatus = 'pending' | 'stale' | 'decided' | 'historicalCopy';
type PrepareAttempt = { proposal: Proposal; text: string; operationId: string; expectedPreparedVersion: string };

function detail(reason: unknown): string {
  return reason && typeof reason === 'object' && 'detail' in reason
    ? String(reason.detail)
    : reason instanceof Error
      ? reason.message
      : 'The suggestion could not be updated. Your manuscript is unchanged.';
}

function statusOf(proposal: Proposal): { kind: ProposalStatus; label: string } {
  if (proposal.decision) return { kind: 'decided', label: proposal.decision.kind === 'apply' ? 'Applied' : 'Rejected' };
  if (proposal.historicalCopy) return { kind: 'historicalCopy', label: 'Historical copy' };
  if (!proposal.current) return { kind: 'stale', label: 'Needs refresh' };
  return { kind: 'pending', label: 'Current · pending review' };
}

function versionNumber(value: string | undefined): bigint {
  try { return BigInt(value ?? '0'); } catch { return 0n; }
}


function retainPreparationAttempt(reason: unknown): boolean {
  const code = errorCode(reason);
  return !code || code === 'UncertainOutcome' || code === 'ProtocolError';
}

function isContinuation(proposal: Proposal): boolean {
  // The durable kind is the authority. Do not infer an append operation from
  // an untrusted candidate shape returned by an older or malformed record.
  return proposal.kind === 'continuation';
}

function candidateText(proposal: Proposal): string {
  if (proposal.kind === 'structured') return canonicalJson((proposal.candidate as { blocks: StructuredBlock[] }).blocks);
  return isContinuation(proposal)
    ? (proposal.candidate as { paragraphs: string[] }).paragraphs.join('\n\n')
    : (proposal.candidate as { replacementText: string }).replacementText;
}

function preparedText(proposal: Proposal, prepared: PreparedProposal): string | null {
  if (proposal.kind === 'structured') return prepared.blocks ? canonicalJson(prepared.blocks) : null;
  if (isContinuation(proposal)) return prepared.paragraphs ? prepared.paragraphs.join('\n\n') : null;
  return prepared.replacementText;
}

function paragraphs(text: string): string[] {
  // Keep empty entries. Rust must reject blank paragraphs instead of the UI
  // silently repairing the author's edited candidate.
  return text.split('\n\n');
}

function ProposalSourceContext({ context }: { context: ProposalSourceContext }) {
  return <section className="proposal-source-context" aria-label="Frozen source context">
    <div className="proposal-source-context-meta"><span>Source version {context.sourceVersion}</span><span>Scope: {context.scopeLabel}</span></div>
    {context.unavailable
      ? <p className="proposal-source-context-unavailable" role="alert">{context.unavailable}</p>
      : <>
        {context.parts.map((part, index) => <div className="proposal-source-context-part" key={`${part.label}-${index}`}>
          <span className="preview-label">{part.label}</span>
          {part.blocks ? <StructuredProse blocks={part.blocks} /> : <blockquote>{part.text}</blockquote>}
        </div>)}
        {context.note && <p className="small-copy">{context.note}</p>}
      </>}
  </section>;
}

export function ProposalPanel({ access, proposals, disabled = false, onPrepareProposal, onApplyProposal, onRefresh }: ProposalPanelProps) {
  const [replacement, setReplacement] = useState<Map<string, string>>(() => new Map());
  const [prepared, setPrepared] = useState<Map<string, PreparedProposal>>(() => new Map());
  const [preparing, setPreparing] = useState<Map<string, PrepareAttempt>>(() => new Map());
  const [applying, setApplying] = useState<Set<string>>(() => new Set());
  const [rejecting, setRejecting] = useState<Set<string>>(() => new Set());
  const [errors, setErrors] = useState<Map<string, string>>(() => new Map());
  const [notices, setNotices] = useState<Map<string, string>>(() => new Map());
  const preparingRef = useRef<Map<string, PrepareAttempt>>(new Map());
  const applyingRef = useRef<Set<string>>(new Set());
  const rejectingRef = useRef<Set<string>>(new Set());
  const rejectOperationRef = useRef<Map<string, string>>(new Map());
  const mounted = useRef(true);
  const accessKey = `${access.projectId}/${access.operationNamespace}/${access.session}/${access.writerLease}`;
  const activeAccessKey = useRef(accessKey);
  activeAccessKey.current = accessKey;

  function isLive(): boolean { return mounted.current && activeAccessKey.current === accessKey; }

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, [accessKey]);

  useEffect(() => {
    // A lease rotation fences the old mutation response. Keep preparation's
    // immutable request for receipt reconciliation, while allowing the
    // parent session to settle Apply and a rejection to be retried safely.
    applyingRef.current.clear();
    rejectingRef.current.clear();
    setApplying(new Set());
    setRejecting(new Set());
  }, [accessKey]);

  useEffect(() => {
    const ids = new Set(proposals.map(proposal => proposal.id));
    setReplacement(previous => {
      const next = new Map<string, string>();
      for (const proposal of proposals) {
        const retained = proposal.prepared ? preparedText(proposal, proposal.prepared) : null;
        next.set(proposal.id, previous.get(proposal.id) ?? retained ?? candidateText(proposal));
      }
      return next;
    });
    setPrepared(previous => {
      const next = new Map<string, PreparedProposal>();
      for (const proposal of proposals) {
        const local = previous.get(proposal.id);
        const retained = proposal.prepared;
        if (retained && (!local || versionNumber(retained.version) >= versionNumber(local.version))) next.set(proposal.id, retained);
        else if (local) next.set(proposal.id, local);
      }
      return next;
    });
    // A terminal decision observed by the reader settles any local mutation
    // spinner. Pending preparation remains until its exact operation is read
    // back with the matching prepared version.
    for (const proposal of proposals) {
      if (proposal.decision) {
        preparingRef.current.delete(proposal.id);
        applyingRef.current.delete(proposal.id);
        rejectingRef.current.delete(proposal.id);
        rejectOperationRef.current.delete(proposal.id);
      } else {
        const attempt = preparingRef.current.get(proposal.id);
        if (attempt && proposal.prepared && preparedText(proposal, proposal.prepared) === attempt.text && versionNumber(proposal.prepared.version) > versionNumber(attempt.expectedPreparedVersion)) preparingRef.current.delete(proposal.id);
      }
    }
    setPreparing(previous => new Map([...previous].filter(([id, attempt]) => {
      const proposal = proposals.find(item => item.id === id);
      return !!proposal && !proposal.decision && !(proposal.prepared && preparedText(proposal, proposal.prepared) === attempt.text && versionNumber(proposal.prepared.version) > versionNumber(attempt.expectedPreparedVersion));
    })));
    setApplying(previous => new Set([...previous].filter(id => ids.has(id) && !proposals.find(proposal => proposal.id === id)?.decision)));
    setRejecting(previous => new Set([...previous].filter(id => ids.has(id) && !proposals.find(proposal => proposal.id === id)?.decision)));
  }, [proposals]);

  const ordered = useMemo(() => proposals.slice(), [proposals]);
  function setError(id: string, message: string) {
    setErrors(previous => { const next = new Map(previous); next.set(id, message); return next; });
  }
  function clearError(id: string) {
    setErrors(previous => { const next = new Map(previous); next.delete(id); return next; });
  }
  function setNotice(id: string, message: string) {
    setNotices(previous => { const next = new Map(previous); next.set(id, message); return next; });
  }
  function exactPrepared(proposal: Proposal): PreparedProposal | null {
    return prepared.get(proposal.id) ?? proposal.prepared;
  }
  function currentReplacement(proposal: Proposal): string {
    return replacement.get(proposal.id) ?? candidateText(proposal);
  }

  async function refreshAfterMutation(id: string): Promise<void> {
    if (!isLive() || !onRefresh) return;
    try { await onRefresh(); }
    catch (reason) { if (isLive()) setError(id, detail(reason)); }
  }

  async function prepareSuggestion(proposal: Proposal): Promise<void> {
    if (disabled || proposal.historicalCopy || proposal.decision || !onPrepareProposal) return;
    const id = proposal.id;
    const existing = preparingRef.current.get(id);
    const ready = exactPrepared(proposal);
    const attempt = existing ?? { proposal: structuredClone(ready ? { ...proposal, prepared: ready } : proposal), text: currentReplacement(proposal), operationId: crypto.randomUUID(), expectedPreparedVersion: ready?.version ?? '0' };
    if (!existing && ready && preparedText(proposal, ready) === attempt.text) {
      setNotice(id, isContinuation(proposal) ? 'These paragraphs are already previewed.' : 'This wording is already previewed.');
      clearError(id);
      return;
    }
    if (!existing) {
      preparingRef.current.set(id, attempt);
      setPreparing(previous => new Map(previous).set(id, attempt));
    }
    clearError(id); setNotice(id, existing ? 'Checking the same preview request…' : 'Preparing preview…');
    try {
      const result = await onPrepareProposal(attempt.proposal, attempt.text, attempt.operationId);
      if (!isLive() || preparingRef.current.get(id)?.operationId !== attempt.operationId) return;
      if (result.proposalId !== id || preparedText(proposal, result) !== attempt.text) throw new Error('The preview response did not match this suggestion.');
      setPrepared(previous => new Map(previous).set(id, result));
      preparingRef.current.delete(id); setPreparing(previous => { const next = new Map(previous); next.delete(id); return next; });
      setNotice(id, isContinuation(proposal) ? 'Preview ready. Apply only this reviewed continuation.' : 'Preview ready. Apply only this reviewed wording.');
      await refreshAfterMutation(id);
    } catch (reason) {
      if (!isLive() || preparingRef.current.get(id)?.operationId !== attempt.operationId) return;
      if (retainPreparationAttempt(reason)) {
        // Keep the immutable request in the ref and state. Check preview retries
        // the exact operation/payload, which lets native receipt reconciliation
        // settle a lost acknowledgment without creating another version.
        setError(id, detail(reason)); setNotice(id, 'Preview could not be confirmed. Check the same request.');
        void refreshAfterMutation(id);
      } else {
        // Validation and stale-version errors are actionable author edits. A
        // fresh Preview gets a fresh operation while preserving the wording.
        preparingRef.current.delete(id);
        setPreparing(previous => { const next = new Map(previous); next.delete(id); return next; });
        setError(id, detail(reason)); setNotice(id, isContinuation(proposal) ? 'Edit the paragraphs, then preview them again.' : 'Edit the wording, then preview it again.');
      }
    }
  }

  async function applySuggestion(proposal: Proposal): Promise<void> {
    const value = exactPrepared(proposal);
    const text = currentReplacement(proposal);
    if (disabled || proposal.historicalCopy || proposal.decision || !proposal.current || !value || value.proposalId !== proposal.id || preparedText(proposal, value) !== text || !onApplyProposal || applyingRef.current.has(proposal.id)) return;
    const id = proposal.id;
    applyingRef.current.add(id); setApplying(previous => new Set(previous).add(id)); clearError(id); setNotice(id, isContinuation(proposal) ? 'Applying continuation…' : 'Applying reviewed change…');
    try {
      await onApplyProposal(proposal, value);
      if (!isLive()) return;
      applyingRef.current.delete(id); setApplying(previous => new Set([...previous].filter(item => item !== id)));
      setNotice(id, isContinuation(proposal) ? 'Continuation applied to the manuscript.' : 'Applied to the manuscript.');
      await refreshAfterMutation(id);
    } catch (reason) {
      if (!isLive()) return;
      applyingRef.current.delete(id); setApplying(previous => new Set([...previous].filter(item => item !== id)));
      setError(id, detail(reason));
    }
  }

  async function rejectSuggestion(proposal: Proposal): Promise<void> {
    if (disabled || proposal.historicalCopy || proposal.decision || rejectingRef.current.has(proposal.id)) return;
    const id = proposal.id;
    const operationId = rejectOperationRef.current.get(id) ?? crypto.randomUUID();
    rejectOperationRef.current.set(id, operationId);
    rejectingRef.current.add(id); setRejecting(previous => new Set(previous).add(id)); clearError(id); setNotice(id, 'Rejecting suggestion…');
    try {
      const decision = await rejectProposal(access, id, operationId);
      if (!isLive()) return;
      if (decision.proposalId !== id || decision.kind !== 'reject') throw new Error('The rejection response did not match this suggestion.');
      rejectingRef.current.delete(id); setRejecting(previous => new Set([...previous].filter(item => item !== id)));
      rejectOperationRef.current.delete(id);
      setNotice(id, isContinuation(proposal) ? 'Continuation rejected. The manuscript is unchanged.' : 'Suggestion rejected. The manuscript is unchanged.');
      await refreshAfterMutation(id);
    } catch (reason) {
      if (!isLive()) return;
      rejectingRef.current.delete(id); setRejecting(previous => new Set([...previous].filter(item => item !== id)));
      setError(id, detail(reason));
    }
  }

  if (!ordered.length) return null;
  const continuationOnly = ordered.every(isContinuation);
  return <section className="proposal-panel" aria-label={continuationOnly ? 'Suggested continuation' : 'Suggested edits'}>
    <div className="proposal-heading"><h3>{continuationOnly ? 'Suggested continuation' : 'Suggested edits'}</h3><span>{ordered.length} {ordered.length === 1 ? 'option' : 'options'}</span></div>
    <p className="small-copy">{continuationOnly ? 'Review the generated paragraphs after the chapter ending. Edit them, preview them, then apply them when you are ready.' : 'Review each alternative against its captured scope. Edit it, preview it, then apply it when you are ready.'}</p>
    <div className="proposal-list">
      {ordered.map(proposal => {
        const status = statusOf(proposal);
        const value = exactPrepared(proposal);
        const text = currentReplacement(proposal);
        const continuation = isContinuation(proposal);
        const structured = proposal.kind === 'structured';
        const attempt = preparing.get(proposal.id);
        const isPreparing = !!attempt;
        const isApplying = applying.has(proposal.id);
        const isRejecting = rejecting.has(proposal.id);
        const canMutate = !disabled && !proposal.historicalCopy && !proposal.decision;
        const canApply = canMutate && proposal.current && !!value && value.proposalId === proposal.id && preparedText(proposal, value) === text && !isPreparing && !isApplying && !isRejecting;
        const error = errors.get(proposal.id);
        const notice = notices.get(proposal.id);
        const replacementId = `proposal-replacement-${proposal.id}`;
        const previewParagraphs = value && continuation && value.paragraphs ? value.paragraphs : [];
        const sourceContext = deriveProposalSourceContext(proposal.source, proposal.sourceBody, proposal.scope);
        return <article className={`proposal-card proposal-${status.kind} ${continuation ? 'proposal-continuation' : ''}`} key={proposal.id} data-testid={`proposal-${proposal.id}`}>
          <div className="proposal-card-heading"><div><h4>{proposal.candidate.title}</h4><span className="proposal-status">{status.label}</span></div>{status.kind === 'stale' && <span className="proposal-warning">Review only</span>}</div>
          <ProposalSourceContext context={sourceContext} />
          <span className="preview-label">{continuation ? 'Append after chapter ending' : structured ? proposal.scope.kind === 'wholeDocument' ? 'Before · whole chapter' : 'Before · selected paragraphs' : 'Before'}</span><blockquote className="proposal-before">{proposal.scope.quote}</blockquote>
          <p className="proposal-explanation">{proposal.candidate.explanation}</p>
          {structured ? <><span className="suggestion-edit-label">Replacement prose</span><StructuredSuggestionEditor label={`Replacement prose: ${proposal.candidate.title}`} blocks={JSON.parse(text) as StructuredBlock[]} disabled={!canMutate || isPreparing || isApplying || isRejecting} onChange={blocks => { setReplacement(previous => new Map(previous).set(proposal.id, canonicalJson(blocks))); clearError(proposal.id); setNotice(proposal.id, ''); }} />{proposal.scope.kind === 'blocks' && <button type="button" className="text-button remove-suggested-paragraphs" disabled={!canMutate || isPreparing || isApplying || isRejecting} onClick={() => { setReplacement(previous => new Map(previous).set(proposal.id, '[]')); clearError(proposal.id); setNotice(proposal.id, 'Preview this removal before applying it.'); }}>Remove selected paragraphs</button>}</> : <><label htmlFor={replacementId}>{continuation ? 'Continuation paragraphs' : 'Replacement wording'}</label><textarea id={replacementId} value={text} disabled={!canMutate || isPreparing || isApplying || isRejecting} onChange={event => { setReplacement(previous => new Map(previous).set(proposal.id, event.target.value)); clearError(proposal.id); setNotice(proposal.id, ''); }} /></>}
          {value && (structured ? !!value.blocks : !continuation || value.paragraphs) && <div className="proposal-preview"><span className="preview-label">{continuation ? 'New paragraphs' : 'Preview after'}</span>{structured ? <StructuredProse blocks={value.blocks!} /> : continuation ? <div className="continuation-after">{previewParagraphs.map((paragraph, index) => <p key={`${proposal.id}-preview-${index}`}>{paragraph}</p>)}</div> : <blockquote className="after-text">{value.replacementText || <em>Remove the selected passage</em>}</blockquote>}{preparedText(proposal, value) !== text && <p className="stale-notice">{structured ? 'The prose or formatting changed after this preview. Preview again before applying.' : continuation ? 'The paragraphs changed after this preview. Preview them again before applying.' : 'The wording changed after this preview. Preview again before applying.'}</p>}</div>}
          {error && <p className="proposal-error" role="alert">{error}</p>}
          {notice && <p className="proposal-status-message" role="status">{notice}</p>}
          <div className="proposal-actions">
            <button type="button" className="secondary-button" disabled={!canMutate || isApplying || isRejecting || !onPrepareProposal} onClick={() => void prepareSuggestion(proposal)}>{isPreparing ? (error ? 'Check preview' : 'Preparing…') : 'Preview'}</button>
            <button type="button" className="primary-button" disabled={!canApply || !onApplyProposal} title={proposal.current ? (continuation ? 'Apply this exact continuation' : 'Apply this exact preview') : 'This suggestion is stale and must be refreshed before applying.'} onClick={() => void applySuggestion(proposal)}>{isApplying ? 'Applying…' : 'Apply'}</button>
            <button type="button" className="text-button proposal-reject" disabled={!canMutate || isPreparing || isApplying || isRejecting} onClick={() => void rejectSuggestion(proposal)}>{isRejecting ? 'Rejecting…' : 'Reject'}</button>
          </div>
          {proposal.historicalCopy && <p className="small-copy">This suggestion is retained from another project copy and is read-only.</p>}
        </article>;
      })}
    </div>
  </section>;
}
