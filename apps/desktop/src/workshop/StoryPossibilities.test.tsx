// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { StoryPossibility } from '../ipc/workshop';
import { StoryPossibilities } from './StoryPossibilities';

let host: HTMLDivElement;
let root: Root;

function possibility(id: string, kind: StoryPossibility['kind'], text: string, status: StoryPossibility['status'] = 'open'): StoryPossibility {
  return { id, kind, text, status };
}

function render(items: StoryPossibility[], selectedText?: string, onChange = vi.fn(), onExplore = vi.fn()) {
  act(() => root.render(<StoryPossibilities items={items} selectedText={selectedText} onChange={onChange} onExplore={onExplore} />));
  return { onChange, onExplore };
}

function control(label: string): HTMLTextAreaElement | HTMLSelectElement {
  const field = [...host.querySelectorAll('label')].find(item => item.childNodes[0]?.textContent?.trim() === label);
  if (!field) throw new Error(`Missing control: ${label}\n${host.innerHTML}`);
  return field.querySelector('textarea,select') as HTMLTextAreaElement | HTMLSelectElement;
}

async function change(label: string, value: string) {
  const input = control(label);
  await act(async () => {
    Object.getOwnPropertyDescriptor(Object.getPrototypeOf(input), 'value')!.set!.call(input, value);
    input.dispatchEvent(new Event(input instanceof HTMLSelectElement ? 'change' : 'input', { bubbles: true }));
  });
}

async function changePossibility(value: string, nextValue: string) {
  const input = [...host.querySelectorAll<HTMLTextAreaElement>('.workshop-possibility textarea')]
    .find(item => item.value === value);
  if (!input) throw new Error(`Missing possibility textarea: ${value}\n${host.innerHTML}`);
  await act(async () => {
    Object.getOwnPropertyDescriptor(Object.getPrototypeOf(input), 'value')!.set!.call(input, nextValue);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

function possibilityButton(text: string, label: string): HTMLButtonElement {
  const block = [...host.querySelectorAll<HTMLElement>('.workshop-possibility')]
    .find(item => item.querySelector('textarea')?.value === text);
  if (!block) throw new Error(`Missing possibility: ${text}\n${host.innerHTML}`);
  return [...block.querySelectorAll('button')].find(item => item.textContent === label)!;
}

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

describe('StoryPossibilities', () => {
  it('keeps the three author record kinds separate and requires explicit confirmation after selected-text prefill', async () => {
    const existing = [
      possibility('question', 'unresolvedQuestion', 'Who left the broken compass?'),
      possibility('payoff', 'intendedPayoff', 'The compass reveals a safe route.'),
      possibility('arc', 'possibleArc', 'Follow the mapmaker beyond the harbor.'),
    ];
    const { onChange, onExplore } = render(existing, '  The exact selected working text.\nKeep this line too.  ');

    expect(host.querySelector('[aria-label="Unresolved questions"]')?.textContent).toContain('Who left the broken compass?');
    expect(host.querySelector('[aria-label="Intended payoffs"]')?.textContent).toContain('The compass reveals a safe route.');
    expect(host.querySelector('[aria-label="Possible arc directions"]')?.textContent).toContain('Follow the mapmaker beyond the harbor.');
    expect(onChange).not.toHaveBeenCalled();
    expect(onExplore).not.toHaveBeenCalled();

    await act(async () => (host.querySelector('details summary') as HTMLElement).click());
    const draft = control('Possibility') as HTMLTextAreaElement;
    expect((host.querySelector('button[disabled]') as HTMLButtonElement).textContent).toBe('Keep possibility');
    await act(async () => [...host.querySelectorAll('button')].find(item => item.textContent === 'Use selected working text')!.click());
    expect(draft.value).toBe('  The exact selected working text.\nKeep this line too.  ');
    expect(onChange).not.toHaveBeenCalled();

    await act(async () => [...host.querySelectorAll('button')].find(item => item.textContent === 'Keep possibility')!.click());
    expect(onChange).toHaveBeenCalledOnce();
    expect(onChange.mock.calls[0][0]).toEqual([...existing, expect.objectContaining({
      kind: 'unresolvedQuestion', text: '  The exact selected working text.\nKeep this line too.  ', status: 'open',
    })]);
    expect(onExplore).not.toHaveBeenCalled();
  });

  it('edits, clears, archives, and reopens one record while preserving the other records', async () => {
    const initial = [
      possibility('question', 'unresolvedQuestion', 'Who left the broken compass?'),
      possibility('payoff', 'intendedPayoff', 'The compass reveals a safe route.'),
      possibility('arc', 'possibleArc', 'Follow the mapmaker beyond the harbor.'),
    ];
    const { onChange, onExplore } = render(initial);
    let current = initial;

    await changePossibility(initial[0].text, 'Edited question wording');
    current = onChange.mock.lastCall![0];
    expect(current).toEqual([
      possibility('question', 'unresolvedQuestion', 'Edited question wording'),
      initial[1], initial[2],
    ]);
    act(() => root.render(<StoryPossibilities items={current} onChange={onChange} onExplore={onExplore} />));

    await changePossibility('Edited question wording', '');
    current = onChange.mock.lastCall![0];
    expect(current[0]).toEqual(possibility('question', 'unresolvedQuestion', ''));
    expect(current.slice(1)).toEqual(initial.slice(1));
    act(() => root.render(<StoryPossibilities items={current} onChange={onChange} onExplore={onExplore} />));
    expect(possibilityButton('', 'Prepare to explore').disabled).toBe(true);

    await act(async () => possibilityButton(initial[1].text, 'Set aside').click());
    current = onChange.mock.lastCall![0];
    expect(current).toEqual([
      current[0],
      possibility('payoff', 'intendedPayoff', initial[1].text, 'archived'),
      initial[2],
    ]);
    act(() => root.render(<StoryPossibilities items={current} onChange={onChange} onExplore={onExplore} />));
    await act(async () => ([...host.querySelectorAll('details summary')].find(item => item.textContent === 'Possibilities set aside') as HTMLElement).click());
    await act(async () => ([...host.querySelectorAll('button')].find(item => item.textContent === 'Reopen possibility') as HTMLButtonElement).click());
    current = onChange.mock.lastCall![0];
    expect(current).toEqual([
      current[0],
      possibility('payoff', 'intendedPayoff', initial[1].text, 'open'),
      initial[2],
    ]);
    expect(onExplore).not.toHaveBeenCalled();
  });

  it('only prepares an existing possibility after its explicit button is clicked', async () => {
    const item = possibility('arc', 'possibleArc', 'Follow the mapmaker beyond the harbor.');
    const { onChange, onExplore } = render([item]);
    expect(onChange).not.toHaveBeenCalled();
    expect(onExplore).not.toHaveBeenCalled();

    await act(async () => possibilityButton(item.text, 'Prepare to explore').click());
    expect(onExplore).toHaveBeenCalledExactlyOnceWith(item);
    expect(onChange).not.toHaveBeenCalled();
  });

  it('disables empty creation and enforces the 64-record exploration cap', async () => {
    const empty = render([], '');
    await act(async () => (host.querySelector('details summary') as HTMLElement).click());
    expect([...host.querySelectorAll('button')].find(item => item.textContent === 'Keep possibility')!.disabled).toBe(true);
    expect((control('Possibility') as HTMLTextAreaElement).maxLength).toBe(4000);
    await change('Possibility', '   ');
    expect([...host.querySelectorAll('button')].find(item => item.textContent === 'Keep possibility')!.disabled).toBe(true);
    await change('Possibility', 'A real possibility');
    expect([...host.querySelectorAll('button')].find(item => item.textContent === 'Keep possibility')!.disabled).toBe(false);
    expect(empty.onChange).not.toHaveBeenCalled();

    const full = Array.from({ length: 64 }, (_, index) => possibility(`item-${index}`, 'possibleArc', `Possibility ${index}`));
    const { onChange } = render(full, 'A selected passage');
    await act(async () => (host.querySelector('details summary') as HTMLElement).click());
    expect(host.textContent).toContain('already holds 64 possibilities');
    expect([...host.querySelectorAll('button')].find(item => item.textContent === 'Keep possibility')!.disabled).toBe(true);
    expect(onChange).not.toHaveBeenCalled();
  });
});
