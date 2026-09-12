// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppCloseDialog } from './AppCloseDialog';

const status = { startingRequests: 1, activeJobs: 2, activeWorkers: 1, pendingResults: 1, ready: false };

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  if (!HTMLDialogElement.prototype.showModal) HTMLDialogElement.prototype.showModal = function (this: HTMLDialogElement) { this.open = true; };
  if (!HTMLDialogElement.prototype.close) HTMLDialogElement.prototype.close = function (this: HTMLDialogElement) { this.open = false; };
  vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
  vi.spyOn(HTMLDialogElement.prototype, 'close').mockImplementation(function (this: HTMLDialogElement) { this.open = false; });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove(); vi.restoreAllMocks();
});

describe('AppCloseDialog', () => {
  it('shows plain author-facing status and offers stop or stay open', async () => {
    const onStop = vi.fn(); const onStayOpen = vi.fn();
    await act(async () => root.render(<AppCloseDialog phase="waiting" status={status} message="Local work is still running." onStop={onStop} onStayOpen={onStayOpen} />));
    expect(host.textContent).toContain('Local work is still running.');
    expect(host.textContent).not.toContain('Requests starting');
    expect(host.textContent).not.toContain('Local workers');
    await act(async () => (host.querySelector('button.primary-button') as HTMLButtonElement).click());
    await act(async () => [...host.querySelectorAll('button')].find(button => button.textContent === 'Stay open')!.click());
    expect(onStop).toHaveBeenCalledOnce();
    expect(onStayOpen).toHaveBeenCalledOnce();
  });

  it('uses Escape as Stay open and does not provide a destructive close control', async () => {
    const onStayOpen = vi.fn();
    await act(async () => root.render(<AppCloseDialog phase="blocked" status={null} message="Some replies still need saving." onStop={vi.fn()} onStayOpen={onStayOpen} />));
    const dialog = host.querySelector('dialog')!;
    await act(async () => dialog.dispatchEvent(new Event('cancel', { bubbles: true, cancelable: true })));
    expect(onStayOpen).toHaveBeenCalledOnce();
    expect(host.querySelector('button.primary-button')).toBeNull();
    expect(host.textContent).not.toContain('Close');
  });
});
