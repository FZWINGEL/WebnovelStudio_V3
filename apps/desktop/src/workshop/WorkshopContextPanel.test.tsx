// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WorkshopContextPanel } from './WorkshopContextPanel';

let host: HTMLDivElement;
let root: Root;
const showModal = vi.fn(function (this: HTMLDialogElement) { this.open = true; });
const close = vi.fn(function (this: HTMLDialogElement) { this.open = false; });
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value: showModal });
  Object.defineProperty(HTMLDialogElement.prototype, 'close', { configurable: true, value: close });
  showModal.mockClear(); close.mockClear();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('Workshop context reference and drawer', () => {
  it('leaves the wide workbench interactive beside a named reference panel', async () => {
    window.innerWidth = 1440;
    await act(async () => root.render(<WorkshopContextPanel onClose={vi.fn()}><p>Current story direction</p></WorkshopContextPanel>));
    expect(host.querySelector('aside')?.getAttribute('aria-label')).toBe('Working story and exploration context');
    expect(host.querySelector('dialog')).toBeNull();
    expect(showModal).not.toHaveBeenCalled();
  });
  it('uses a modal drawer on small windows and requests closure on Escape or the close control', async () => {
    window.innerWidth = 800;
    const onClose = vi.fn();
    await act(async () => root.render(<WorkshopContextPanel onClose={onClose}><p>Saved material</p></WorkshopContextPanel>));
    const dialog = host.querySelector('dialog')!;
    expect(dialog.open).toBe(true);
    const cancel = new Event('cancel', { cancelable: true, bubbles: true });
    await act(async () => dialog.dispatchEvent(cancel));
    expect(cancel.defaultPrevented).toBe(true);
    expect(onClose).toHaveBeenCalledOnce();
    await act(async () => host.querySelector('button')!.click());
    expect(onClose).toHaveBeenCalledTimes(2);
  });
  it('switches presentation at the drawer breakpoint and closes the old modal on widening', async () => {
    window.innerWidth = 1440;
    await act(async () => root.render(<WorkshopContextPanel onClose={vi.fn()}><textarea aria-label="Direction" value="Keep the ending open" readOnly /></WorkshopContextPanel>));
    await act(async () => { window.innerWidth = 1190; window.dispatchEvent(new Event('resize')); });
    const dialog = host.querySelector('dialog')!;
    expect(dialog.open).toBe(true);
    expect(host.querySelector('textarea')?.value).toBe('Keep the ending open');
    await act(async () => { window.innerWidth = 1440; window.dispatchEvent(new Event('resize')); });
    expect(close).toHaveBeenCalledOnce();
    expect(dialog.open).toBe(false);
    expect(host.querySelector('aside')).not.toBeNull();
    expect(host.querySelector('textarea')?.value).toBe('Keep the ending open');
  });
  it('cycles keyboard focus between visible enabled controls and restores the opener on removal', async () => {
    window.innerWidth = 800;
    const opener = document.createElement('button'); document.body.append(opener); opener.focus();
    await act(async () => root.render(<WorkshopContextPanel onClose={vi.fn()}><button disabled>Unavailable</button><button>Last action</button><button hidden>Hidden</button></WorkshopContextPanel>));
    const buttons = [...host.querySelectorAll('button')];
    buttons.forEach(button => vi.spyOn(button, 'getClientRects').mockReturnValue((button.hidden ? [] : [{}]) as unknown as DOMRectList));
    const first = buttons[0], last = buttons[2];
    first.focus();
    const backwards = new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true });
    await act(async () => first.dispatchEvent(backwards));
    expect(backwards.defaultPrevented).toBe(true); expect(document.activeElement).toBe(last);
    const forwards = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true });
    await act(async () => last.dispatchEvent(forwards));
    expect(forwards.defaultPrevented).toBe(true); expect(document.activeElement).toBe(first);
    await act(async () => root.render(null));
    expect(document.activeElement).toBe(opener); opener.remove();
  });
});
