import { useEffect, useMemo, useRef, useState } from 'react';
import { rejectProposal, type PreparedProposal, type Proposal } from '../ipc/proposals';
import type { ProjectAccess } from '../ipc/projects';

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

function errorCode(reason: unknown): string | null {
  return reason && typeof reason === 'object' && 'code' in reason && typeof reason.code === 'string' ? reason.code : null;
}

function retainPreparationAttempt(reason: unknown): boolean {
  const code = errorCode(reason);
  return !code || code === 'UncertainOutcome' || code === 'ProtocolError';
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
      for (const proposal of proposals) next.set(proposal.id, previous.get(proposal.id) ?? proposal.prepared?.replacementText ?? proposal.candidate.replacementText);
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
        if (attempt && proposal.prepared?.replacementText === attempt.text && versionNumber(proposal.prepared.version) > versionNumber(attempt.expectedPreparedVersion)) preparingRef.current.delete(proposal.id);
      }
    }
    setPreparing(previous => new Map([...previous].filter(([id, attempt]) => {
      const proposal = proposals.find(item => item.id === id);
      return !!proposal && !proposal.decision && !(proposal.prepared?.replacementText === attempt.text && versionNumber(proposal.prepared.version) > versionNumber(attempt.expectedPreparedVersion));
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
    return replacement.get(proposal.id) ?? proposal.candidate.replacementText;
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
    if (!existing && ready?.replacementText === attempt.text) {
      setNotice(id, 'This wording is already previewed.');
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
      if (result.proposalId !== id || result.replacementText !== attempt.text) throw new Error('The preview response did not match this suggestion.');
      setPrepared(previous => new Map(previous).set(id, result));
      preparingRef.current.delete(id); setPreparing(previous => { const next = new Map(previous); next.delete(id); return next; });
      setNotice(id, 'Preview ready. Apply only this reviewed wording.');
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
        setError(id, detail(reason)); setNotice(id, 'Edit the wording, then preview it again.');
      }
    }
  }

  async function applySuggestion(proposal: Proposal): Promise<void> {
    const value = exactPrepared(proposal);
    const text = currentReplacement(proposal);
    if (disabled || proposal.historicalCopy || proposal.decision || !proposal.current || !value || value.proposalId !== proposal.id || value.replacementText !== text || !onApplyProposal || applyingRef.current.has(proposal.id)) return;
    const id = proposal.id;
    applyingRef.current.add(id); setApplying(previous => new Set(previous).add(id)); clearError(id); setNotice(id, 'Applying reviewed change…');
    try {
      await onApplyProposal(proposal, value);
      if (!isLive()) return;
      applyingRef.current.delete(id); setApplying(previous => new Set([...previous].filter(item => item !== id)));
      setNotice(id, 'Applied to the manuscript.');
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
      setNotice(id, 'Suggestion rejected. The manuscript is unchanged.');
      await refreshAfterMutation(id);
    } catch (reason) {
      if (!isLive()) return;
      rejectingRef.current.delete(id); setRejecting(previous => new Set([...previous].filter(item => item !== id)));
      setError(id, detail(reason));
    }
  }

  if (!ordered.length) return null;
  return <section className="proposal-panel" aria-label="Suggested edits">
    <div className="proposal-heading"><h3>Suggested edits</h3><span>{ordered.length} {ordered.length === 1 ? 'option' : 'options'}</span></div>
    <p className="small-copy">Review each alternative against the captured passage. Preview an alternative, then apply it when you are ready.</p>
    <div className="proposal-list">
      {ordered.map(proposal => {
        const status = statusOf(proposal);
        const value = exactPrepared(proposal);
        const text = currentReplacement(proposal);
        const attempt = preparing.get(proposal.id);
        const isPreparing = !!attempt;
        const isApplying = applying.has(proposal.id);
        const isRejecting = rejecting.has(proposal.id);
        const canMutate = !disabled && !proposal.historicalCopy && !proposal.decision;
        const canApply = canMutate && proposal.current && !!value && value.proposalId === proposal.id && value.replacementText === text && !isPreparing && !isApplying && !isRejecting;
        const error = errors.get(proposal.id);
        const notice = notices.get(proposal.id);
        const replacementId = `proposal-replacement-${proposal.id}`;
        return <article className={`proposal-card proposal-${status.kind}`} key={proposal.id} data-testid={`proposal-${proposal.id}`}>
          <div className="proposal-card-heading"><div><h4>{proposal.candidate.title}</h4><span className="proposal-status">{status.label}</span></div>{status.kind === 'stale' && <span className="proposal-warning">Review only</span>}</div>
          <span className="preview-label">Before</span><blockquote className="proposal-before">{proposal.scope.quote}</blockquote>
          <p className="proposal-explanation">{proposal.candidate.explanation}</p>
          <label htmlFor={replacementId}>Replacement wording</label>
          <textarea id={replacementId} value={text} disabled={!canMutate || isPreparing || isApplying || isRejecting} onChange={event => { setReplacement(previous => new Map(previous).set(proposal.id, event.target.value)); clearError(proposal.id); setNotice(proposal.id, ''); }} />
          {value && <div className="proposal-preview"><span className="preview-label">Preview after</span><blockquote className="after-text">{value.replacementText || <em>Remove the selected passage</em>}</blockquote>{value.replacementText !== text && <p className="stale-notice">The wording changed after this preview. Preview again before applying.</p>}</div>}
          {error && <p className="proposal-error" role="alert">{error}</p>}
          {notice && <p className="proposal-status-message" role="status">{notice}</p>}
          <div className="proposal-actions">
            <button type="button" className="secondary-button" disabled={!canMutate || isApplying || isRejecting || !onPrepareProposal} onClick={() => void prepareSuggestion(proposal)}>{isPreparing ? (error ? 'Check preview' : 'Preparing…') : 'Preview'}</button>
            <button type="button" className="primary-button" disabled={!canApply || !onApplyProposal} title={proposal.current ? 'Apply this exact preview' : 'This suggestion is stale and must be refreshed before applying.'} onClick={() => void applySuggestion(proposal)}>{isApplying ? 'Applying…' : 'Apply'}</button>
            <button type="button" className="text-button proposal-reject" disabled={!canMutate || isPreparing || isApplying || isRejecting} onClick={() => void rejectSuggestion(proposal)}>{isRejecting ? 'Rejecting…' : 'Reject'}</button>
          </div>
          {proposal.historicalCopy && <p className="small-copy">This suggestion is retained from another project copy and is read-only.</p>}
        </article>;
      })}
    </div>
  </section>;
}
