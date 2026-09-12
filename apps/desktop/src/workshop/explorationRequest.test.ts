// @vitest-environment node
import { expect, it } from 'vitest';
import { explorationRequest } from './explorationRequest';
import { newSession } from './store';
import { NOTES_ORGANIZATION_SCOPE, ORGANIZE_NOTES_INSTRUCTION } from './catalog';

it('binds exact committed version and refuses stale captured text or generation', () => {
  const session = { ...newSession(), workingText: 'A lantern', workingGeneration: '7' };
  const input = { session, version: '12', action: 'directions', capture: { from: 2, to: 9, text: 'lantern', generation: '7' }, isCurrentSession: true, subversion: '' };
  const request = explorationRequest(input);
  expect(request).toMatchObject({ sessionId: session.id, expectedVersion: '12', workingGeneration: '7', selectedText: 'lantern', workingSelection: { from: 2, to: 9, text: 'lantern' } });
  for (const capture of [{ ...input.capture, generation: '6' }, { ...input.capture, text: 'changed' }]) {
    expect(() => explorationRequest({ ...input, capture })).toThrow('selected passage changed');
  }
  input.capture.text = 'later'; session.workingGeneration = '8';
  expect(request.workingSelection?.text).toBe('lantern');
  expect(request.workingGeneration).toBe('7');
});
it('does not leak another exploration capture and treats voice guidance as a sample', () => {
  const session = { ...newSession(), workingText: 'The sample.' };
  const base = { session, version: '1', action: 'voiceGuidance', capture: null, subversion: '' };
  expect(explorationRequest({ ...base, isCurrentSession: true })).toMatchObject({ selectedText: 'The sample.', workingSelection: undefined });
  expect(explorationRequest({ ...base, isCurrentSession: false, capture: { from: 0, to: 6, text: 'secret', generation: '0' } })).toMatchObject({ selectedText: '', workingSelection: undefined });
});
it('organization and candidate comparison retain explicit scope and fixed-detail instructions', () => {
  const session = { ...newSession('notebook'), selectedScope: NOTES_ORGANIZATION_SCOPE, composer: 'Keep my wording.' };
  const base = { session, version: '1', action: 'directions', capture: null, isCurrentSession: true, subversion: '' };
  expect(explorationRequest(base).instruction).toContain(ORGANIZE_NOTES_INSTRUCTION);
  const candidate = { id: 'c', title: 'Option', content: 'Provisional', dimensionValue: 'Quiet', implications: [], assumptions: [], affectedTargets: [], preservedDetails: [], changedDetails: [] };
  const compared = explorationRequest({ ...base, candidate, dimension: 'Pace' });
  expect(compared.instruction).not.toContain(ORGANIZE_NOTES_INSTRUCTION);
  expect(compared.instruction).toContain('Preserve author-chosen invariants and Keep fixed details');
  expect(compared.instruction).toContain('Pace');
  expect(compared).toMatchObject({ selectedScope: 'Option', selectedText: 'Provisional', workingSelection: undefined });
});
