import type { DiscussionRun } from '../ipc/discussions';
import type { ProviderBinding } from '../ipc/context';
import type { ModelSelection } from '../ipc/providers';
import type { ConversationStatus } from './conversationStore';
import type { StoryFreshness } from './useStoryFreshness';

export interface RequestContextSummary {
  surface: 'authorRoom' | 'chapterWriting';
  targetLabel: string;
  scopeLabel: string;
  selection: ModelSelection | null;
  run?: Pick<RequestContextSummary, 'surface' | 'targetLabel' | 'scopeLabel'>;
}

export interface RequestStatusProps {
  status: ConversationStatus;
  run: DiscussionRun | null;
  error?: string | null;
  uncertainOperationId?: string | null;
  workerIssues?: Array<{ runId: string; detail: string }>;
  onStop?: () => void;
  onRetrySave?: (runId: string) => void;
  onReconcile?: () => void;
  requestContext?: RequestContextSummary;
  freshness?: StoryFreshness;
}

function label(status: ConversationStatus): string {
  switch (status) {
    case 'loading': return 'Opening conversation';
    case 'saving': return 'Saving request';
    case 'queued': return 'Request queued';
    case 'running': return 'Assistant is thinking';
    case 'stopping': return 'Stopping request';
    case 'uncertain': return 'Request status unknown';
    case 'failed': return 'Request needs attention';
    default: return 'Ready';
  }
}

function modeLabel(surface: RequestContextSummary['surface']): string {
  return surface === 'chapterWriting' ? 'Chapter writing' : 'Author room';
}

function requestedBinding(selection: ModelSelection | null, run: DiscussionRun | null): ProviderBinding | ModelSelection | null {
  if (run) return run.providerBinding ?? run.providerResult?.binding ?? null;
  return selection;
}

function settingValue(value: string | null | undefined): string {
  return value && value.trim() ? value : 'Default / unspecified';
}

function selectionText(selection: Pick<ModelSelection, 'providerId' | 'modelId' | 'reasoning' | 'serviceTier'>): string {
  return `${selection.providerId} / ${selection.modelId} · reasoning ${settingValue(selection.reasoning)} · tier ${settingValue(selection.serviceTier)}`;
}

export function RequestStatus({ status, run, error, uncertainOperationId, workerIssues = [], onStop, onRetrySave, onReconcile, requestContext, freshness }: RequestStatusProps) {
  const active = status === 'queued' || status === 'running' || status === 'stopping';
  const uncertain = status === 'uncertain';
  const failed = status === 'failed';
  const binding = requestedBinding(requestContext?.selection ?? null, run);
  const reportedModel = run?.providerResult?.reportedModel;
  const effectiveIdentity = run?.providerResult?.effectiveIdentity;
  const basis = run && requestContext?.run ? requestContext.run : requestContext;
  const technicalDetails = [run?.operationId ? `Operation ${run.operationId}` : '', effectiveIdentity ? `Provider identity ${effectiveIdentity}` : '', uncertainOperationId ? `Reconcile operation ${uncertainOperationId}` : ''].filter(Boolean);
  return <section className={`chat-request-status chat-request-status-${status}`} aria-live="polite" aria-atomic="true">
    <div className="chat-request-status-copy">
      <strong>{label(status)}</strong>
      {run && <span>{run.status === 'completed' ? 'Response saved' : run.status === 'stopped' ? 'Stopped; received text retained' : run.status === 'failed' || run.status === 'interrupted' ? (run.stopReason ?? run.providerResult?.error ?? 'The last request failed before a usable response was saved.') : 'Request in progress'}</span>}
      {uncertainOperationId && <span>Request needs reconciliation; its exact operation is retained.</span>}
      {error && <span role={failed || uncertain ? 'alert' : undefined}>{error}</span>}
      {workerIssues.map(issue => <span key={issue.runId} role="alert">Local save issue: {issue.detail}</span>)}
      {requestContext && basis && <div className="chat-request-context" aria-label="Request basis and model settings">
        <span><strong>{modeLabel(basis.surface)}</strong> · Target: {run && basis.surface === 'chapterWriting' ? `${basis.targetLabel} · v${run.target.version}` : basis.targetLabel} · Scope: {basis.scopeLabel}</span>
        {binding && <span><strong>Requested</strong> · {selectionText(binding)} · {run ? 'frozen for this request' : 'applies to the next request'}</span>}
        {!binding && <span><strong>Requested settings</strong> · {run ? 'Not recorded for this run' : 'No model selected; choose one before sending.'}</span>}
        {run && requestContext.selection && <span><strong>Next request</strong> · {selectionText(requestContext.selection)} · picker changes apply here</span>}
        {run && <span><strong>Provider report</strong> · model {reportedModel ?? 'not reported'} · identity {effectiveIdentity ? 'available' : 'not reported'}</span>}
      </div>}
      {technicalDetails.length > 0 && <details className="chat-request-technical"><summary>Technical details</summary><span>{technicalDetails.join(' · ')}</span></details>}
      {freshness?.status === 'stale' && <div className="chat-request-freshness chat-request-freshness-stale" role="status">
        <strong>Based on older sources</strong>
        <span>{freshness.detail ?? 'Story sources or disclosure policy changed after this request.'} Generation continues; this does not stop the request.</span>
        <span>Review or refresh any proposed adoption before applying it.</span>
      </div>}
      {freshness?.status === 'checking' && <span className="chat-request-freshness">Checking the frozen story context…</span>}
      {freshness?.status === 'unknown' && <span className="chat-request-freshness" role="status">The frozen story context could not be compared. Inspect the supplied context before adopting a draft.</span>}
    </div>
    <div className="chat-request-status-actions">
      {active && onStop && <button type="button" onClick={onStop} disabled={status === 'stopping'}>{status === 'stopping' ? 'Stopping…' : 'Stop'}</button>}
      {uncertain && onReconcile && <button type="button" onClick={onReconcile}>Reconcile</button>}
      {!uncertain && onRetrySave && workerIssues.map(issue => <button type="button" key={issue.runId} onClick={() => onRetrySave(issue.runId)}>Retry local save</button>)}
    </div>
  </section>;
}
