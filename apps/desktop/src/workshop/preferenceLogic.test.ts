// @vitest-environment node
import { expect, it } from 'vitest';
import type { WorkshopPreference } from '../ipc/workshop';
import { applicablePreferences, preferenceConflict } from './Preferences';
import { emptyWorkshop, newSession } from './store';

const preference = (id: string, overrides: Partial<WorkshopPreference> = {}): WorkshopPreference => ({
  id, label: 'Violence', family: 'Content boundaries', meaning: '', examples: '', timing: '',
  polarity: 'avoid', strength: 'hard', scope: 'project', targetId: null, confirmed: true, ...overrides,
});
it.each(['project', 'element', 'exploration'] as const)('hard project constraints remain visible to conflicting %s preferences', scope => {
  const hard = preference('hard');
  const incoming = preference('incoming', { scope, targetId: scope === 'project' ? null : 'target', strength: 'soft', polarity: 'want', label: ' violence ' });
  expect(preferenceConflict([hard], incoming)).toContain('Never');
  expect(preferenceConflict([hard], { ...incoming, confirmed: false })).toBeNull();
  expect(preferenceConflict([hard], { ...incoming, polarity: 'neutral' })).toBeNull();
});
it('resolves confirmed active scopes without adding unrelated, neutral or unconfirmed preferences', () => {
  const session = { ...newSession(), focusDocumentId: 'person' };
  const state = { ...emptyWorkshop(), preferences: [preference('global'),
    preference('element', { scope: 'element', targetId: 'person' }),
    preference('local', { scope: 'exploration', targetId: session.id }),
    preference('other', { scope: 'exploration', targetId: 'other' }),
    preference('neutral', { polarity: 'neutral' }), preference('unconfirmed', { confirmed: false })] };
  expect(applicablePreferences(state, session).map(item => item.id)).toEqual(['global', 'element', 'local']);
  expect(state.preferences).toHaveLength(6);
});
