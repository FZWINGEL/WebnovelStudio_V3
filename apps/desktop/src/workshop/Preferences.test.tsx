// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { exportWorkshopPreset, importWorkshopPreset, type WorkshopPreference, type WorkshopPreset, type WorkshopState } from '../ipc/workshop';
import { preferenceConflict, Preferences } from './Preferences';
import { emptyWorkshop, newSession } from './store';

vi.mock('../ipc/workshop', async importOriginal => {
  const actual = await importOriginal<typeof import('../ipc/workshop')>();
  return { ...actual, exportWorkshopPreset: vi.fn(), importWorkshopPreset: vi.fn() };
});

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.mocked(exportWorkshopPreset).mockReset().mockResolvedValue(null);
  vi.mocked(importWorkshopPreset).mockReset().mockResolvedValue(null);
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
function button(label: string) { return [...host.querySelectorAll('button')].find(item => item.textContent === label)!; }
function control(label: string) {
  const field = [...host.querySelectorAll('label')].find(item => item.childNodes[0]?.textContent?.trim() === label);
  if (!field) throw new Error(`Missing control: ${label}\n${host.innerHTML}`);
  return field.querySelector('input,textarea,select') as HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;
}
async function change(label: string, value: string) {
  const input = control(label);
  await act(async () => {
    Object.getOwnPropertyDescriptor(Object.getPrototypeOf(input), 'value')!.set!.call(input, value);
    input.dispatchEvent(new Event(input instanceof HTMLSelectElement ? 'change' : 'input', { bubbles: true }));
  });
}
function projectPreference(overrides: Partial<WorkshopPreference> = {}): WorkshopPreference {
  return { id: 'preference-1', label: 'Texture', family: 'Style', meaning: 'Keep tactile detail.', examples: '', timing: '', polarity: 'want', strength: 'soft', scope: 'project', targetId: null, confirmed: true, ...overrides };
}
function presetPayload(name: string, preference = projectPreference()) {
  return JSON.stringify({ schemaVersion: 'workshop-preset.v1', name, preferences: [preference] });
}

it('keeps rejection local until an edited preference and scope are explicitly saved', async () => {
  const session = newSession('world');
  session.choices = [{ candidateId: 'rejected', status: 'rejected', rationale: 'The guild became a cartoon villain.', includeInContext: false }];
  const state = { ...emptyWorkshop(), sessions: [session], currentSessionId: session.id };
  const onChange = vi.fn();
  act(() => root.render(<Preferences state={state} session={session} onChange={onChange} />));
  await act(async () => button('Review as a preference').click());
  expect(control('What it means to you').value).toBe(session.choices[0].rationale);
  expect(control('Direction').value).toBe('neutral');
  expect(control('Applies to').value).toBe('exploration');
  expect(onChange).not.toHaveBeenCalled();
  await change('Name', 'Morally mixed institutions');
  await change('What it means to you', 'Keep both the guild’s useful safety work and its self-interest.');
  await change('Direction', 'want');
  await change('Applies to', 'project');
  await act(async () => host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
  expect(onChange).toHaveBeenCalledTimes(1);
  const saved = (onChange.mock.calls[0][0] as (state: WorkshopState) => WorkshopState)(state);
  expect(saved.preferences[0]).toMatchObject({ label: 'Morally mixed institutions', polarity: 'want', scope: 'project', targetId: null, confirmed: true, strength: 'soft' });
  expect(saved.sessions[0].choices).toEqual(session.choices);
});

it('cancels promotion without storing a preference or including rejected prose', async () => {
  const session = newSession();
  session.choices = [{ candidateId: 'no', status: 'rejected', rationale: 'Wrong mood', includeInContext: false }];
  const onChange = vi.fn();
  act(() => root.render(<Preferences state={emptyWorkshop()} session={session} onChange={onChange} />));
  await act(async () => button('Review as a preference').click());
  await act(async () => button('Cancel').click());
  expect(host.querySelector('form')).toBeNull();
  expect(onChange).not.toHaveBeenCalled();
});

it('preserves an edited JSON preset name only when the author explicitly adds it', async () => {
  const session = newSession('world');
  const state = { ...emptyWorkshop(), sessions: [session], currentSessionId: session.id };
  const onChange = vi.fn();
  act(() => root.render(<Preferences state={state} session={session} onChange={onChange} />));

  await act(async () => button('Export or import preferences').click());
  await change('Preset text', presetPayload('Edited project vocabulary'));
  expect(control('Preset name').value).toBe('Edited project vocabulary');
  expect(onChange).not.toHaveBeenCalled();
  await act(async () => button('Add these project preferences').click());

  expect(onChange).toHaveBeenCalledTimes(1);
  const saved = (onChange.mock.calls[0][0] as (state: WorkshopState) => WorkshopState)(state);
  expect(saved.presets).toHaveLength(1);
  expect(saved.presets[0]).toMatchObject({ name: 'Edited project vocabulary' });
  expect(saved.presets[0].name).not.toBe('My starting preferences');
  expect(saved.preferences[0]).toMatchObject({ scope: 'project', targetId: null, confirmed: true });
});

it('keeps persisted presets reviewable after rerender and does not adopt them during review', async () => {
  const session = newSession('world');
  const preset: WorkshopPreset = { id: 'preset-archive', name: 'Archive vocabulary', preferences: [projectPreference()] };
  const state = { ...emptyWorkshop(), sessions: [session], currentSessionId: session.id, presets: [preset] };
  const onChange = vi.fn();
  act(() => root.render(<Preferences state={state} session={session} onChange={onChange} />));
  expect(host.textContent).toContain('Archive vocabulary');
  act(() => root.render(<Preferences state={state} session={session} onChange={onChange} />));

  await act(async () => button('Review saved preset').click());
  expect(control('Preset name').value).toBe('Archive vocabulary');
  expect(JSON.parse(control('Preset text').value).name).toBe('Archive vocabulary');
  expect(onChange).not.toHaveBeenCalled();
});

it('updates a reviewed saved preset definition in place without adopting its preferences', async () => {
  const session = newSession('world');
  const preset: WorkshopPreset = { id: 'preset-edit', name: 'Editable vocabulary', preferences: [projectPreference()] };
  const state = { ...emptyWorkshop(), sessions: [session], currentSessionId: session.id, presets: [preset] };
  const onChange = vi.fn();
  act(() => root.render(<Preferences state={state} session={session} onChange={onChange} />));

  await act(async () => button('Review saved preset').click());
  await change('Preset name', 'Edited definition');
  expect(JSON.parse(control('Preset text').value).name).toBe('Edited definition');
  await change('Preset text', presetPayload('JSON-renamed definition', projectPreference({ label: 'Updated texture' })));
  expect(control('Preset name').value).toBe('JSON-renamed definition');
  await act(async () => button('Save preset definition').click());

  expect(onChange).toHaveBeenCalledTimes(1);
  const saved = (onChange.mock.calls[0][0] as (state: WorkshopState) => WorkshopState)(state);
  expect(saved.preferences).toEqual([]);
  expect(saved.presets).toHaveLength(1);
  expect(saved.presets[0]).toMatchObject({ id: 'preset-edit', name: 'JSON-renamed definition', preferences: [{ label: 'Updated texture' }] });
});

it('adopts an edited saved preset only after the review confirmation', async () => {
  const session = newSession('world');
  const preset: WorkshopPreset = { id: 'preset-reuse', name: 'Reusable vocabulary', preferences: [projectPreference()] };
  const state = { ...emptyWorkshop(), sessions: [session], currentSessionId: session.id, presets: [preset] };
  const onChange = vi.fn();
  act(() => root.render(<Preferences state={state} session={session} onChange={onChange} />));

  await act(async () => button('Review saved preset').click());
  await change('Preset name', 'Edited reusable vocabulary');
  expect(JSON.parse(control('Preset text').value).name).toBe('Edited reusable vocabulary');
  await change('Preset text', presetPayload('JSON reusable vocabulary', projectPreference({ label: 'Edited texture' })));
  expect(control('Preset name').value).toBe('JSON reusable vocabulary');
  expect(onChange).not.toHaveBeenCalled();
  await act(async () => button('Add these project preferences').click());

  expect(onChange).toHaveBeenCalledTimes(1);
  const saved = (onChange.mock.calls[0][0] as (state: WorkshopState) => WorkshopState)(state);
  expect(saved.preferences).toHaveLength(1);
  expect(saved.preferences[0].label).toBe('Edited texture');
  expect(saved.preferences[0].id).not.toBe('preference-1');
  expect(saved.presets).toHaveLength(1);
  expect(saved.presets[0]).toMatchObject({ id: 'preset-reuse', name: 'JSON reusable vocabulary', preferences: [{ id: saved.preferences[0].id, label: 'Edited texture' }] });
  expect(host.querySelector('[role="status"]')?.textContent).toBe('1 project preference added.');
  expect(host.textContent).not.toContain('No preferences have been added yet.');
});

it('rejects a blank JSON preset name without creating a preset or preferences', async () => {
  const session = newSession('world');
  const state = { ...emptyWorkshop(), sessions: [session], currentSessionId: session.id };
  const onChange = vi.fn();
  act(() => root.render(<Preferences state={state} session={session} onChange={onChange} />));

  await act(async () => button('Export or import preferences').click());
  await change('Preset name', '   ');
  await change('Preset text', presetPayload('', projectPreference({ label: 'Should not save' })));
  await act(async () => button('Add these project preferences').click());

  expect(host.querySelector('[role="alert"]')?.textContent).toContain('preset name is required');
  expect(onChange).not.toHaveBeenCalled();
  expect(host.querySelector('[aria-label="Review preference preset"]')).not.toBeNull();
});

it('keeps malformed preset JSON editable while refusing to save it', async () => {
  const session = newSession('world');
  const state = { ...emptyWorkshop(), sessions: [session], currentSessionId: session.id };
  const onChange = vi.fn();
  act(() => root.render(<Preferences state={state} session={session} onChange={onChange} />));

  await act(async () => button('Export or import preferences').click());
  await change('Preset text', '{"schemaVersion":');
  await change('Preset name', 'Repairing preset');
  expect(control('Preset text').value).toBe('{"schemaVersion":');
  await act(async () => button('Add these project preferences').click());

  expect(host.querySelector('[role="alert"]')?.textContent).toContain('Unexpected end');
  expect(onChange).not.toHaveBeenCalled();
});

it('surfaces native preset export and import failures without changing workshop state', async () => {
  const session = newSession('world');
  const state = { ...emptyWorkshop(), sessions: [session], currentSessionId: session.id };
  const onChange = vi.fn();
  act(() => root.render(<Preferences state={state} session={session} onChange={onChange} />));

  vi.mocked(exportWorkshopPreset).mockRejectedValue({ detail: 'Save dialog unavailable.' });
  await act(async () => button('Save project preset as file').click());
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('Save dialog unavailable.');
  expect(onChange).not.toHaveBeenCalled();

  vi.mocked(importWorkshopPreset).mockRejectedValue({ detail: 'Open dialog unavailable.' });
  await act(async () => button('Open preset file for review').click());
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('Open dialog unavailable.');
  expect(onChange).not.toHaveBeenCalled();
});

it('only checks confirmed preferences for case-insensitive hard conflicts', () => {
  const hardMust = projectPreference({ id: 'hard-must', label: 'Élan', strength: 'hard', polarity: 'want' });
  const hardAvoid = projectPreference({ id: 'hard-avoid', label: 'élan', strength: 'hard', polarity: 'avoid' });
  expect(preferenceConflict([hardMust], hardAvoid)).toContain('Must');
  expect(preferenceConflict([hardMust], { ...hardAvoid, confirmed: false })).toBeNull();
  const softAvoid = projectPreference({ id: 'soft-avoid', label: 'ÉLAN', strength: 'soft', polarity: 'avoid' });
  expect(preferenceConflict([softAvoid], hardMust)).toBe('Your Must preference for “Élan” conflicts with the current project Avoid preference for “ÉLAN”. Edit either preference before saving.');
});
