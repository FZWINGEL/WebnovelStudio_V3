// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DiscussionRun } from '../ipc/discussions';
import type { CandidateChoice, WorkshopCandidate, WorkshopResult } from '../ipc/workshop';
import { CandidateBoard } from './CandidateBoard';

let host: HTMLDivElement;
let root: Root;

const run = (overrides: Partial<DiscussionRun> = {}): DiscussionRun => ({
  id: 'run-1', threadId: 'thread-1', owner: { projectId: 'project', operationNamespace: 'workshop', runId: 'run-1' },
  operationId: 'op-1', payloadHash: 'hash', target: { documentId: 'doc', version: '1', bodyHash: 'body' }, packetId: 'packet', previousRunId: null,
  status: 'completed', dispatchState: 'delivered', sequence: '1', outputText: '', stopReason: null, createdAt: '2026-09-07T00:00:00Z', updatedAt: '2026-09-07T00:00:00Z', ...overrides,
});
const candidate = (id: string, content = `A direction for ${id}. A second paragraph with a detail.`): WorkshopCandidate => ({ id, title: `Direction ${id}`, content, dimensionValue: `Value ${id}`, implications: [{ text: 'A possible consequence', basis: 'the chosen mechanism', assumption: 'people can act on it' }], assumptions: ['Access remains uneven.'], affectedTargets: [], preservedDetails: ['The central pressure'], changedDetails: ['The daily practice'] });
const result = (candidates: WorkshopCandidate[], overrides: Partial<WorkshopResult> = {}): WorkshopResult => ({ run: run(), sessionId: 'session-1', workingGeneration: '1', action: 'overview', output: { schemaVersion: 'story-workshop-output.v1', requestKind: 'world', question: 'How could this work?', questionReason: 'To make the direction tangible.', dimension: 'Core mechanism', interpretation: { youSaid: 'A world', possibleDirection: 'A specific practice', stillOpen: 'Its consequences' }, candidates }, validationError: null, stale: false, ...overrides });

function render(value: Partial<Parameters<typeof CandidateBoard>[0]> = {}) {
  const props: Parameters<typeof CandidateBoard>[0] = { result: result([candidate('a'), candidate('b'), candidate('c')]), choices: [], selectedDetails: [], onDevelop: vi.fn(), onSelectDetail: vi.fn(), onChoice: vi.fn(), onExplore: vi.fn(), ...value };
  act(() => root.render(<CandidateBoard {...props} />));
  return props;
}
function button(label: string): HTMLButtonElement { return [...host.querySelectorAll('button')].find(item => item.textContent === label)!; }
async function click(label: string) { await act(async () => button(label).click()); }

beforeEach(() => { Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('CandidateBoard', () => {
  it('keeps noncanon moments as samples and sends only an explicitly selected passage for voice guidance', async () => {
    const sample = candidate('sample', 'The bell rang twice. Mei hid the stolen key.');
    const props = render({ result: result([sample], { action: 'moment' }) });
    expect(button('Develop this')).toBeUndefined();
    expect(button('Select details')).toBeUndefined();
    await click('Save for later');
    expect(props.onChoice).toHaveBeenLastCalledWith(expect.objectContaining({ candidateId: sample.id, status: 'saved', includeInContext: false }));
    expect(host.querySelector('.candidate-include')).toBeNull();
    await click('Select a sample passage');
    expect(props.onExplore).not.toHaveBeenCalled();
    expect(button('Select full direction')).toBeUndefined();
    expect(button('Keep this implication in my tray')).toBeUndefined();
    const area = host.querySelector<HTMLTextAreaElement>('.candidate-exact-text')!;
    area.setSelectionRange(0, 'The bell rang twice.'.length);
    await act(async () => area.dispatchEvent(new MouseEvent('mouseup', { bubbles: true })));
    await click('Propose voice guidance from selected text');
    expect(props.onExplore).toHaveBeenCalledExactlyOnceWith('voiceGuidance', { ...sample, content: 'The bell rang twice.' });
    expect(props.onSelectDetail).not.toHaveBeenCalled();
    expect(props.onDevelop).not.toHaveBeenCalled();
  });

  it('does not reinstate context inclusion when a historical noncanon sample is saved again', async () => {
    const sample = candidate('sample');
    const props = render({ result: result([sample], { action: 'moment', stale: true }), choices: [{ candidateId: sample.id, status: 'saved', includeInContext: true, rationale: 'Keep the rhythm' }] });
    expect(button('Review against current work')).toBeUndefined();
    await click('Save for later');
    expect(props.onChoice).toHaveBeenLastCalledWith({ candidateId: sample.id, status: 'saved', includeInContext: false, rationale: 'Keep the rhythm' });
    await click('Propose voice guidance');
    expect(props.onExplore).toHaveBeenCalledExactlyOnceWith('voiceGuidance', sample);
    expect(props.onDevelop).not.toHaveBeenCalled();
  });

  it('shows an active request as progress without treating it as a failed result', () => {
    render({ result: result([], { run: run({ status: 'running', dispatchState: 'delivered' }), output: null }) });
    expect(host.textContent).toContain('Exploration in progress');
    expect(host.textContent).not.toContain('No usable text was returned');
    expect(host.querySelectorAll('.candidate-card')).toHaveLength(0);
  });

  it('shows three bounded cards with explicit dimensions and no generation on expansion', async () => {
    const explore = vi.fn(); const props = render({ onExplore: explore });
    expect(host.querySelectorAll('.candidate-card')).toHaveLength(3);
    expect(host.textContent).toContain('Core mechanism'); expect(host.textContent).toContain('Value a');
    await click('Select details');
    expect(host.textContent).toContain('Implications, basis, and assumptions');
    expect(explore).not.toHaveBeenCalled(); expect(props.onDevelop).not.toHaveBeenCalled();
  });

  it('gates a stale candidate behind an explicit current-work review', async () => {
    const develop = vi.fn(); render({ onDevelop: develop, result: result([candidate('a')], { stale: true }) });
    await click('Review against current work'); expect(develop).not.toHaveBeenCalled();
    await click('Develop this'); expect(develop).toHaveBeenCalledWith(expect.objectContaining({ id: 'a' }));
  });
  it('requires another review after current author work changes', async () => {
    const stale = result([candidate('a')], { stale: true });
    const develop = vi.fn();
    render({ result: stale, reviewKey: '1', onDevelop: develop });
    await click('Review against current work');
    render({ result: stale, reviewKey: '2', onDevelop: develop });
    expect(button('Develop this')).toBeUndefined();
    expect(develop).not.toHaveBeenCalled();
    expect(button('Review against current work')).toBeDefined();
  });

  it('forwards the exact selected substring from the read-only text area', async () => {
    const select = vi.fn(); render({ onSelectDetail: select }); await click('Select details');
    const area = host.querySelector('.candidate-exact-text') as HTMLTextAreaElement;
    const exact = 'direction for a'; const start = area.value.indexOf(exact); area.setSelectionRange(start, start + exact.length);
    await act(async () => area.dispatchEvent(new MouseEvent('mouseup', { bubbles: true })));
    await click('Select selected text'); expect(select).toHaveBeenCalledWith(expect.objectContaining({ id: 'a' }), exact);
  });

  it('requests candidate alternatives with the original candidate and leaves selection and adoption inert', async () => {
    const target = candidate('target');
    const explore = vi.fn();
    const select = vi.fn();
    const develop = vi.fn();
    const choice = vi.fn();
    const steer = vi.fn();
    render({ result: result([target]), onExplore: explore, onSelectDetail: select, onDevelop: develop, onChoice: choice, onSteer: steer });
    await click('Select details');
    await click('Give alternatives');

    expect(explore).toHaveBeenCalledTimes(1);
    expect(explore.mock.calls[0][0]).toBe('directions');
    expect(explore.mock.calls[0][1]).toBe(target);
    expect(select).not.toHaveBeenCalled();
    expect(develop).not.toHaveBeenCalled();
    expect(choice).not.toHaveBeenCalled();
    expect(steer).not.toHaveBeenCalled();
    expect(host.querySelectorAll('.candidate-card')).toHaveLength(1);
  });

  it('keeps implication basis visible and offers explicit accept, reject, and contrast actions', async () => {
    const implicationOnly = { ...candidate('implication-only'), assumptions: [], implications: [{
      text: 'Repair cooperatives spread beyond the guild.', basis: 'restricted guild teaching', assumption: 'residents can perform limited repairs safely with shared instruction',
    }] };
    const select = vi.fn(); const steer = vi.fn(); const explore = vi.fn();
    render({ result: result([implicationOnly]), onSelectDetail: select, onSteer: steer, onExplore: explore });
    await click('Select details');
    const card = host.querySelector('.candidate-card')!;
    expect(card.textContent).toContain('restricted guild teaching');
    expect(card.textContent).toContain('residents can perform limited repairs safely with shared instruction');
    const actions = [...card.querySelectorAll<HTMLButtonElement>('.candidate-implication-actions button')];
    expect(actions.map(action => action.textContent)).toEqual(['Keep this implication in my tray', 'Reject this assumption', 'Prepare a contrasting implication']);
    await act(async () => actions[0].click());
    await act(async () => actions[1].click());
    await act(async () => actions[2].click());
    expect(select).toHaveBeenCalledWith(implicationOnly, implicationOnly.implications[0].text);
    expect(steer).toHaveBeenCalledTimes(2);
    expect(steer.mock.calls[0][1]).toContain('Implication: Repair cooperatives spread beyond the guild.');
    expect(steer.mock.calls[0][1]).toContain('Basis: restricted guild teaching');
    expect(steer.mock.calls[0][1]).toContain('Assumption: residents can perform limited repairs safely with shared instruction');
    expect(steer.mock.calls[0][1]).toContain('Reject this assumption');
    expect(steer.mock.calls[1][1]).toContain('Prepare a contrasting implication');
    expect(steer.mock.calls[1][1]).not.toBe(steer.mock.calls[0][1]);
    expect(explore).not.toHaveBeenCalled();
  });

  it('saves with local rationale and only lets saved choices enter future context', async () => {
    const choice = vi.fn(); render({ onChoice: choice });
    const rationale = host.querySelector('#candidate-rationale-a') as HTMLTextAreaElement;
    await act(async () => { Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(rationale, 'Too familiar'); rationale.dispatchEvent(new Event('input', { bubbles: true })); });
    await click('Save for later');
    expect(choice).toHaveBeenCalledWith({ candidateId: 'a', status: 'saved', rationale: 'Too familiar', includeInContext: false });
    const include = host.querySelector('.candidate-include input') as HTMLInputElement;
    await act(async () => include.click());
    expect(choice).toHaveBeenLastCalledWith({ candidateId: 'a', status: 'saved', rationale: 'Too familiar', includeInContext: true });
  });

  it('retains incomplete output as inert text without candidate cards', () => {
    render({ result: result([candidate('a')], { run: run({ status: 'stopped', outputText: 'Useful partial direction.' }), output: null }) });
    expect(host.querySelectorAll('.candidate-card')).toHaveLength(0); expect(host.textContent).toContain('Useful partial direction.'); expect(host.textContent).toContain('not a completed proposal');
  });

  it('keeps rejected alternatives recoverable and excludes them by default', async () => {
    const choices: CandidateChoice[] = [{ candidateId: 'a', status: 'rejected', rationale: 'Wrong mood', includeInContext: false }]; const choice = vi.fn();
    render({ choices, onChoice: choice });
    expect(host.querySelectorAll('.candidate-card')).toHaveLength(2); expect(host.textContent).not.toContain('Direction a');
    const toggle = host.querySelector('.candidate-recovered-toggle input') as HTMLInputElement; await act(async () => toggle.click());
    expect(host.textContent).toContain('Direction a'); expect(host.textContent).toContain('Rejected alternatives stay out of future context.');
  });
});
