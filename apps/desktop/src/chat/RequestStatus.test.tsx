// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RequestStatus } from './RequestStatus';

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe('RequestStatus local recovery actions', () => {
  it('shows the frozen request settings separately from the provider report and stale context warning', async () => {
    const run = {
      id: 'run-1', operationId: 'operation-1', target: { documentId: 'chapter-1', version: '7', bodyHash: 'hash' },
      status: 'running', providerBinding: { providerId: 'codex', modelId: 'gpt-5.6-luna', reasoning: 'xhigh', serviceTier: 'priority' },
      providerResult: { reportedModel: 'gpt-5.6-luna', effectiveIdentity: 'codex-account-1' },
    } as unknown as import('../ipc/discussions').DiscussionRun;
    await act(async () => root.render(<RequestStatus
      status="running"
      run={run}
      requestContext={{ surface: 'authorRoom', targetLabel: 'Project conversation', scopeLabel: '2 exact sources', selection: { providerId: 'mock', modelId: 'mock-story-context', reasoning: null, serviceTier: null } }}
      freshness={{ status: 'stale', frozenSourceEpoch: '1', frozenPolicyEpoch: '1', detail: 'Story sources changed after this request.' }}
    />));
    expect(host.textContent).toContain('Author room');
    expect(host.textContent).toContain('gpt-5.6-luna');
    expect(host.textContent).toContain('reasoning xhigh');
    expect(host.textContent).toContain('Provider report');
    expect(host.textContent).toContain('Based on older sources');
    expect(host.textContent).toContain('Generation continues');
  });

  it('keeps a completed run frozen while showing the picker choice for the next request', async () => {
    const run = {
      id: 'run-1', operationId: 'operation-1', target: { documentId: 'anchor', version: '1', bodyHash: 'hash' }, status: 'completed',
      providerBinding: { providerId: 'codex', modelId: 'gpt-5.6-luna', reasoning: 'xhigh', serviceTier: 'priority' },
    } as unknown as import('../ipc/discussions').DiscussionRun;
    await act(async () => root.render(<RequestStatus
      status="idle"
      run={run}
      requestContext={{ surface: 'authorRoom', targetLabel: 'Project conversation', scopeLabel: 'No additional sources attached', selection: { providerId: 'codex', modelId: 'gpt-5.6-sol', reasoning: 'low', serviceTier: 'priority' } }}
    />));
    expect(host.textContent).toContain('gpt-5.6-luna');
    expect(host.textContent).toContain('frozen for this request');
    expect(host.textContent).toContain('gpt-5.6-sol');
    expect(host.textContent).toContain('Next request');
  });

  it('offers reconciliation, never local retry, for an uncertain start', async () => {
    const retry = vi.fn();
    const reconcile = vi.fn();
    await act(async () => root.render(<RequestStatus
      status="uncertain"
      run={null}
      workerIssues={[{ runId: 'run-new', detail: 'The terminal result needs saving.' }]}
      onRetrySave={retry}
      onReconcile={reconcile}
    />));

    expect(host.textContent).toContain('Reconcile');
    expect(host.textContent).not.toContain('Retry local save');
    await act(async () => (host.querySelector('button') as HTMLButtonElement).click());
    expect(reconcile).toHaveBeenCalledOnce();
    expect(retry).not.toHaveBeenCalled();
  });

  it('offers no retry for a definite failure without a retained local issue', async () => {
    await act(async () => root.render(<RequestStatus status="failed" run={null} onRetrySave={() => {}} />));
    expect(host.textContent).not.toContain('Retry local save');
  });

  it('binds each retry button to its exact retained run', async () => {
    const retry = vi.fn();
    await act(async () => root.render(<RequestStatus
      status="idle"
      run={null}
      workerIssues={[{ runId: 'run-old', detail: 'Old result needs saving.' }, { runId: 'run-new', detail: 'New result needs saving.' }]}
      onRetrySave={retry}
    />));

    const buttons = [...host.querySelectorAll('button')];
    expect(buttons).toHaveLength(2);
    await act(async () => buttons[1].click());
    expect(retry).toHaveBeenCalledWith('run-new');
  });
});
