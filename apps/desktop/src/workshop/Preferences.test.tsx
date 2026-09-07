// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { WorkshopState } from '../ipc/workshop';
import { Preferences } from './Preferences';
import { emptyWorkshop, newSession } from './store';

let host: HTMLDivElement;
let root: Root;
beforeEach(() => { Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
function button(label: string) { return [...host.querySelectorAll('button')].find(item => item.textContent === label)!; }
function control(label: string) { return [...host.querySelectorAll('label')].find(item => item.firstChild?.textContent === label)!.querySelector('input,textarea,select') as HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement; }
async function change(label: string, value: string) {
  const input = control(label);
  await act(async () => {
    Object.getOwnPropertyDescriptor(Object.getPrototypeOf(input), 'value')!.set!.call(input, value);
    input.dispatchEvent(new Event(input instanceof HTMLSelectElement ? 'change' : 'input', { bubbles: true }));
  });
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
