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
const continuationScope: ScopeGrant = { kind: 'append', start: null, end: { blockId: 'paragraph-1', utf16Offset: 21 }, quote: 'The original passage.', sourceHash: head.bodyHash, quoteHash: 'b'.repeat(64), prefix: null, suffix: null };
function proposal(overrides: Partial<proposalIpc.Proposal> = {}): proposalIpc.Proposal {
  return { id: 'proposal-1', runId: 'run-1', candidate: { title: 'Clearer wording', replacementText: 'A clearer passage.', explanation: 'Keeps the moment direct.' }, source: head, sourceBody: source, scope, snapshotId: 'snapshot', packetId: 'packet', current: true, historicalCopy: false, prepared: null, decision: null, ...overrides };
}
function prepared(text: string): proposalIpc.PreparedProposal { return { id: 'prepared-1', proposalId: 'proposal-1', version: '1', replacementText: text, body: source, bodyHash: 'c'.repeat(64) }; }
function continuationProposal(overrides: Partial<proposalIpc.Proposal> = {}): proposalIpc.Proposal {
  return proposal({ kind: 'continuation', candidate: { title: 'Continue the chapter', paragraphs: ['She waited.', 'Then dawn broke.'], explanation: 'Carries the scene through its next beat.' }, scope: continuationScope, ...overrides });
}
function continuationPrepared(text: string): proposalIpc.PreparedProposal {
  return { ...prepared(text), paragraphs: text.split('\n\n') };
}
const structuredBlocks: proposalIpc.StructuredBlock[] = [
  { type: 'paragraph', content: [{ type: 'text', text: 'A revised paragraph.' }] },
  { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'The turn', marks: [{ type: 'bold' }] }] },
];
const structuredSource: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [
  { type: 'paragraph', attrs: { id: 'before' }, content: [{ type: 'text', text: 'Before.' }] },
  { type: 'paragraph', attrs: { id: 'selected' }, content: [{ type: 'text', text: 'Selected paragraphs.' }] },
  { type: 'paragraph', attrs: { id: 'after' }, content: [{ type: 'text', text: 'After.' }] },
] } };
const structuredScope: ScopeGrant = { kind: 'blocks', start: { blockId: 'selected', utf16Offset: 0 }, end: { blockId: 'selected', utf16Offset: 20 }, quote: 'Selected paragraphs.', sourceHash: head.bodyHash, quoteHash: 'b'.repeat(64), prefix: null, suffix: null };
function structuredProposal(overrides: Partial<proposalIpc.Proposal> = {}): proposalIpc.Proposal {
  return proposal({ kind: 'structured', sourceBody: structuredSource, scope: structuredScope, candidate: { title: 'Reshape the beat', blocks: structuredBlocks, explanation: 'Keeps the surrounding chapter intact.' }, ...overrides });
}
function structuredPrepared(blocks: proposalIpc.StructuredBlock[] = structuredBlocks): proposalIpc.PreparedProposal {
  return { ...prepared(''), blocks, body: structuredSource, bodyHash: 'd'.repeat(64) };
}
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

  it('previews and applies continuation paragraphs with the append location', async () => {
    const prepare = vi.fn(async (_proposal: proposalIpc.Proposal, text: string) => continuationPrepared(text));
    const apply = vi.fn(async () => {});
    await render([continuationProposal()], { onPrepareProposal: prepare, onApplyProposal: apply });
    expect(host.textContent).toContain('Append after chapter ending');
    expect(host.textContent).toContain('Continuation paragraphs');
    expect(host.textContent).not.toContain('Replacement wording');
    const textarea = host.querySelector('textarea') as HTMLTextAreaElement;
    expect(textarea.value).toBe('She waited.\n\nThen dawn broke.');
    await act(async () => { const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!; setter.call(textarea, 'The room went quiet.\n\nA new bell answered.'); textarea.dispatchEvent(new Event('input', { bubbles: true })); });
    await click('Preview');
    await waitFor(() => expect(prepare).toHaveBeenCalledWith(expect.objectContaining({ kind: 'continuation' }), 'The room went quiet.\n\nA new bell answered.', expect.any(String)));
    expect(Array.from(host.querySelectorAll('.continuation-after p')).map(item => item.textContent)).toEqual(['The room went quiet.', 'A new bell answered.']);
    expect((Array.from(host.querySelectorAll('button')).find(item => item.textContent === 'Apply') as HTMLButtonElement).disabled).toBe(false);
    await click('Apply');
    await waitFor(() => expect(apply).toHaveBeenCalledWith(expect.objectContaining({ kind: 'continuation' }), expect.objectContaining({ paragraphs: ['The room went quiet.', 'A new bell answered.'] })));
    expect(host.textContent).toContain('Continuation applied to the manuscript.');
  });

  it('passes blank continuation entries through for Rust validation instead of repairing them', async () => {
    const prepare = vi.fn().mockRejectedValue({ code: 'InvalidProposal', detail: 'Paragraphs must not be blank.' });
    await render([continuationProposal()], { onPrepareProposal: prepare });
    const textarea = host.querySelector('textarea') as HTMLTextAreaElement;
    const invalid = 'First paragraph.\n\n\n\nSecond paragraph.';
    await act(async () => { const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!; setter.call(textarea, invalid); textarea.dispatchEvent(new Event('input', { bubbles: true })); });
    await click('Preview');
    await waitFor(() => expect(host.textContent).toContain('Edit the paragraphs, then preview them again.'));
    expect(prepare).toHaveBeenCalledWith(expect.objectContaining({ kind: 'continuation' }), invalid, expect.any(String));
    expect(textarea.value).toBe(invalid);
    expect(textarea.disabled).toBe(false);
  });

  it('retries uncertain continuation preparation with the same operation and serialized body', async () => {
    const gate = deferred<proposalIpc.PreparedProposal>();
    const prepare = vi.fn().mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'The preview response was lost.' }).mockReturnValueOnce(gate.promise);
    await render([continuationProposal()], { onPrepareProposal: prepare });
    await click('Preview');
    await waitFor(() => expect(host.textContent).toContain('Check preview'));
    await click('Check preview');
    expect(prepare).toHaveBeenCalledTimes(2);
    expect(prepare.mock.calls[1][1]).toBe(prepare.mock.calls[0][1]);
    expect(prepare.mock.calls[1][2]).toBe(prepare.mock.calls[0][2]);
    await act(async () => gate.resolve(continuationPrepared('She waited.\n\nThen dawn broke.')));
    await waitFor(() => expect(host.textContent).toContain('Preview ready. Apply only this reviewed continuation.'));
  });

  it('fails closed when a continuation preparation has no durable paragraph payload', async () => {
    const apply = vi.fn(async () => {});
    await render([continuationProposal({ prepared: prepared('A malformed serialized replacement.') })], { onApplyProposal: apply });
    expect(host.querySelector('.continuation-after')).toBeNull();
    const applyButton = Array.from(host.querySelectorAll('button')).find(item => item.textContent === 'Apply') as HTMLButtonElement;
    expect(applyButton.disabled).toBe(true);
    await act(async () => applyButton.click());
    expect(apply).not.toHaveBeenCalled();
  });

  it('uses the durable passage kind when a candidate has stray paragraph-shaped data', async () => {
    const ambiguous = proposal({ candidate: { title: 'Passage wording', replacementText: 'Keep this passage.', paragraphs: ['must not select continuation UI'], explanation: 'A passage candidate with an ignored extra field.' } as unknown as proposalIpc.Proposal['candidate'] });
    await render([ambiguous]);
    expect(host.querySelector('section')?.getAttribute('aria-label')).toBe('Suggested edits');
    expect(host.textContent).toContain('Replacement wording');
    expect(host.textContent).not.toContain('Append after chapter ending');
    expect((host.querySelector('textarea') as HTMLTextAreaElement).value).toBe('Keep this passage.');
  });

  it('shows one editable rich preview, retains the explicit block scope, and invalidates after formatting changes', async () => {
    const prepare = vi.fn(async (_proposal: proposalIpc.Proposal, text: string) => structuredPrepared(JSON.parse(text) as proposalIpc.StructuredBlock[]));
    await render([structuredProposal()], { onPrepareProposal: prepare });
    expect(host.textContent).toContain('Before · selected paragraphs');
    expect(host.textContent).toContain('A revised paragraph.');
    expect(host.textContent).not.toContain('"type":"paragraph"');
    expect(host.querySelector('[aria-label="Suggestion formatting"]')).not.toBeNull();

    await click('Preview');
    await waitFor(() => expect(host.textContent).toContain('Preview ready. Apply only this reviewed wording.'));
    const apply = Array.from(host.querySelectorAll('button')).find(item => item.textContent === 'Apply') as HTMLButtonElement;
    expect(apply.disabled).toBe(false);

    const prose = host.querySelector('.structured-suggestion-editor .ProseMirror') as HTMLElement;
    // jsdom does not implement layout ranges, so mutate the editable rich DOM
    // as a browser input event would after an author applies bold formatting.
    prose.firstElementChild!.innerHTML = '<strong>A revised paragraph.</strong>';
    await act(async () => prose.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText' })));
    await waitFor(() => expect(apply.disabled).toBe(true));
    expect(host.textContent).toContain('Before · selected paragraphs');
    expect(prepare).toHaveBeenCalledTimes(1);
    expect(host.textContent).toContain('The prose or formatting changed after this preview. Preview again before applying.');
  });

  it('retries a lost structured preparation acknowledgment with the same serialized blocks and operation', async () => {
    const gate = deferred<proposalIpc.PreparedProposal>();
    const prepare = vi.fn().mockRejectedValueOnce({ code: 'UncertainOutcome', detail: 'The structured preview response was lost.' }).mockReturnValueOnce(gate.promise);
    await render([structuredProposal()], { onPrepareProposal: prepare });
    await click('Preview');
    await waitFor(() => expect(host.textContent).toContain('Check preview'));
    await click('Check preview');
    expect(prepare).toHaveBeenCalledTimes(2);
    expect(prepare.mock.calls[1][1]).toBe(prepare.mock.calls[0][1]);
    expect(prepare.mock.calls[1][2]).toBe(prepare.mock.calls[0][2]);
    expect(JSON.parse(prepare.mock.calls[1][1])).toEqual(structuredBlocks);
    await act(async () => gate.resolve(structuredPrepared()));
  });

  it('fails closed when a structured prepared record has no durable blocks', async () => {
    const apply = vi.fn(async () => {});
    await render([structuredProposal({ prepared: { ...structuredPrepared(), blocks: undefined } })], { onApplyProposal: apply });
    const applyButton = Array.from(host.querySelectorAll('button')).find(item => item.textContent === 'Apply') as HTMLButtonElement;
    expect(host.querySelector('.structured-prose')).toBeNull();
    expect(applyButton.disabled).toBe(true);
    await act(async () => applyButton.click());
    expect(apply).not.toHaveBeenCalled();
  });
});
