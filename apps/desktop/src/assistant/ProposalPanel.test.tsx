// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ProposalPanel } from './ProposalPanel';
import * as proposalIpc from '../ipc/proposals';
import type { WnsDocument } from '../editor/document';
import type { ScopeGrant } from '../ipc/context';
import type { Head, ProjectAccess } from '../ipc/projects';

vi.mock('../ipc/proposals', () => ({
  rejectProposal: vi.fn(),
}));

const access: ProjectAccess = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const source: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'paragraph-1' }, content: [{ type: 'text', text: 'The original passage.' }] }] } };
const head: Head = { documentId: 'chapter', version: '2', bodyHash: 'a'.repeat(64) };
const scope: ScopeGrant = { kind: 'passage', start: { blockId: 'paragraph-1', utf16Offset: 0 }, end: { blockId: 'paragraph-1', utf16Offset: 21 }, quote: 'The original passage.', sourceHash: head.bodyHash, quoteHash: 'b'.repeat(64), prefix: null, suffix: null };
function proposal(overrides: Partial<proposalIpc.Proposal> = {}): proposalIpc.Proposal {
  return { id: 'proposal-1', runId: 'run-1', candidate: { title: 'Clearer wording', replacementText: 'A clearer passage.', explanation: 'Keeps the moment direct.' }, source: head, sourceBody: source, scope, snapshotId: 'snapshot', packetId: 'packet', current: true, historicalCopy: false, prepared: null, decision: null, ...overrides };
}
function prepared(text: string): proposalIpc.PreparedProposal { return { id: 'prepared-1', proposalId: 'proposal-1', version: '1', replacementText: text, body: source, bodyHash: 'c'.repeat(64) }; }
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(accept => { resolve = accept; }); return { promise, resolve }; }

let host: HTMLDivElement;
let root: Root;
async function render(proposals: proposalIpc.Proposal[], props: Partial<React.ComponentProps<typeof ProposalPanel>> = {}) {
  await act(async () => root.render(<ProposalPanel access={access} proposals={proposals} onPrepareProposal={async () => prepared('A clearer passage.')} onApplyProposal={async () => {}} {...props} />));
}
async function click(label: string) {
  const button = Array.from(host.querySelectorAll('button')).find(item => item.textContent === label) as HTMLButtonElement;
  await act(async () => button.click());
}
async function waitFor(assertion: () => unknown) { await act(async () => { await vi.waitFor(assertion); }); }

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.resetAllMocks();
  vi.mocked(proposalIpc.rejectProposal).mockResolvedValue({ id: 'decision-1', proposalId: 'proposal-1', kind: 'reject', preparedId: null, beforeRevisionId: null, afterRevisionId: null });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('ProposalPanel review boundary', () => {
  it('keeps stale suggestions reviewable while disabling Apply and allows Reject', async () => {
    const stale = proposal({ current: false });
    await render([stale]);
    expect(host.textContent).toContain('Needs refresh');
    expect((Array.from(host.querySelectorAll('button')).find(item => item.textContent === 'Apply') as HTMLButtonElement).disabled).toBe(true);
    await click('Reject');
    await waitFor(() => expect(proposalIpc.rejectProposal).toHaveBeenCalledWith(access, 'proposal-1', expect.any(String)));
  });

  it('only applies the exact wording that was previewed', async () => {
    const prepare = vi.fn(async (_proposal: proposalIpc.Proposal, text: string) => prepared(text));
    const apply = vi.fn(async () => {});
    await render([proposal()], { onPrepareProposal: prepare, onApplyProposal: apply });
    const textarea = host.querySelector('textarea') as HTMLTextAreaElement;
    await act(async () => { const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!; setter.call(textarea, 'A different passage.'); textarea.dispatchEvent(new Event('input', { bubbles: true })); });
    await click('Preview');
    await waitFor(() => expect(prepare).toHaveBeenCalledWith(expect.objectContaining({ id: 'proposal-1' }), 'A different passage.', expect.any(String)));
    expect((Array.from(host.querySelectorAll('button')).find(item => item.textContent === 'Apply') as HTMLButtonElement).disabled).toBe(false);
    await act(async () => { const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!; setter.call(textarea, 'Edited after preview.'); textarea.dispatchEvent(new Event('input', { bubbles: true })); });
    expect((Array.from(host.querySelectorAll('button')).find(item => item.textContent === 'Apply') as HTMLButtonElement).disabled).toBe(true);
    expect(apply).not.toHaveBeenCalled();
  });

  it('retries a lost preparation acknowledgment with the same operation and wording', async () => {
    const gate = deferred<proposalIpc.PreparedProposal>();
    const prepare = vi.fn().mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'The preview response was lost.' }).mockReturnValueOnce(gate.promise);
    await render([proposal()], { onPrepareProposal: prepare });
    await click('Preview');
    await waitFor(() => expect(host.textContent).toContain('Check preview'));
    await click('Check preview');
    expect(prepare).toHaveBeenCalledTimes(2);
    expect(prepare.mock.calls[1][1]).toBe(prepare.mock.calls[0][1]);
    expect(prepare.mock.calls[1][2]).toBe(prepare.mock.calls[0][2]);
    await act(async () => gate.resolve(prepared('A clearer passage.')));
  });

  it('unlocks an invalid preview for correction and uses a new operation', async () => {
    const prepare = vi.fn().mockRejectedValueOnce({ code: 'InvalidProposal', detail: 'Use one line of replacement text.' }).mockResolvedValueOnce(prepared('A corrected passage.'));
    await render([proposal()], { onPrepareProposal: prepare });
    const textarea = host.querySelector('textarea') as HTMLTextAreaElement;
    await act(async () => { const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!; setter.call(textarea, 'An invalid\nreplacement.'); textarea.dispatchEvent(new Event('input', { bubbles: true })); });
    await click('Preview');
    await waitFor(() => expect(host.textContent).toContain('Edit the wording'));
    expect(textarea.disabled).toBe(false);
    await act(async () => { const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!; setter.call(textarea, 'A corrected passage.'); textarea.dispatchEvent(new Event('input', { bubbles: true })); });
    await click('Preview');
    await waitFor(() => expect(prepare).toHaveBeenCalledTimes(2));
    expect(prepare.mock.calls[1][2]).not.toBe(prepare.mock.calls[0][2]);
  });

  it('keeps a rejection operation ID stable until the acknowledgment is confirmed', async () => {
    const reject = vi.mocked(proposalIpc.rejectProposal);
    reject.mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'The rejection response was lost.' }).mockResolvedValueOnce({ id: 'decision-1', proposalId: 'proposal-1', kind: 'reject', preparedId: null, beforeRevisionId: null, afterRevisionId: null });
    await render([proposal()]);
    await click('Reject');
    await waitFor(() => expect(host.textContent).toContain('lost'));
    await click('Reject');
    expect(reject.mock.calls[1][2]).toBe(reject.mock.calls[0][2]);
  });

  it('does not let a late preparation response update a new owner', async () => {
    const gate = deferred<proposalIpc.PreparedProposal>();
    const prepare = vi.fn().mockReturnValue(gate.promise);
    await render([proposal()], { onPrepareProposal: prepare });
    await click('Preview');
    const destination = { ...access, projectId: 'other-project', writerLease: 'other-lease' };
    await act(async () => root.render(<ProposalPanel access={destination} proposals={[proposal({ source: { ...head, documentId: 'other-chapter' } })]} onPrepareProposal={prepare} onApplyProposal={async () => {}} />));
    await act(async () => gate.resolve(prepared('A clearer passage.')));
    expect(host.textContent).not.toContain('Preview ready. Apply only this reviewed wording.');
  });

  it('settles a retained preparation only after its version advances', async () => {
    const gate = deferred<proposalIpc.PreparedProposal>();
    const prepare = vi.fn().mockReturnValue(gate.promise);
    await render([proposal()], { onPrepareProposal: prepare });
    await click('Preview');
    await waitFor(() => expect(host.textContent).toContain('Preparing preview'));

    const sameVersion = prepared('A clearer passage.');
    sameVersion.version = '0';
    await render([proposal({ prepared: sameVersion })], { onPrepareProposal: prepare });
    expect(host.textContent).toContain('Preparing…');
    expect((host.querySelector('textarea') as HTMLTextAreaElement).disabled).toBe(true);

    await render([proposal({ prepared: prepared('A clearer passage.') })], { onPrepareProposal: prepare });
    expect(host.textContent).not.toContain('Check preview');
    expect((host.querySelector('textarea') as HTMLTextAreaElement).disabled).toBe(false);
    await act(async () => gate.resolve(prepared('A clearer passage.')));
  });

  it('restores the retained wording and preview after a remount', async () => {
    const retained = prepared('Her sister');
    await render([proposal({ candidate: { title: 'Clearer wording', replacementText: 'Mei', explanation: 'Keeps the moment direct.' }, prepared: retained })]);
    const textarea = host.querySelector('textarea') as HTMLTextAreaElement;
    expect(textarea.value).toBe('Her sister');
    expect(host.querySelector('.after-text')?.textContent).toBe('Her sister');
  });
});
