import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RecentProjectPicker, type RecentProjectPickerItem } from './RecentProjectPicker';

const current: RecentProjectPickerItem = {
  projectId: 'current', title: 'Current story', path: 'current', lastOpened: '2026-09-09T00:00:00Z', missing: false, archived: false,
  current: true, pendingDrafts: 2,
};
const other: RecentProjectPickerItem = {
  projectId: 'other', title: 'Earlier story', path: 'other', lastOpened: '2026-09-08T00:00:00Z', missing: false, archived: false, activityLabel: 'Recent',
};

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});

afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('RecentProjectPicker', () => {
  it('keeps the current project prominent and exposes its pending draft badge', async () => {
    await act(async () => root.render(<RecentProjectPicker current={current} recent={[current, other]} onOpen={() => {}} />));
    expect(host.querySelector('summary')?.textContent).toContain('Projects');
    expect(host.querySelector('summary')?.getAttribute('aria-label')).toContain('Current story');
    expect(host.querySelector('[aria-label="Current project"]')?.textContent).toContain('Current');
    expect(host.querySelector('[aria-label="Current project"]')?.textContent).toContain('2 drafts to review');
    expect(host.querySelector('nav[aria-label="Recent projects"]')?.textContent).toContain('Earlier story');
  });

  it('only opens a selected known project and never auto-opens a catalog entry', async () => {
    const onOpen = vi.fn();
    const onVisibilityChange = vi.fn();
    await act(async () => root.render(<RecentProjectPicker recent={[other]} onOpen={onOpen} onVisibilityChange={onVisibilityChange} />));
    expect(onOpen).not.toHaveBeenCalled();
    const details = host.querySelector<HTMLDetailsElement>('details')!;
    details.open = true;
    await act(async () => host.querySelector<HTMLButtonElement>('.project-picker-item')!.click());
    expect(onOpen).toHaveBeenCalledOnce();
    expect(onOpen).toHaveBeenCalledWith('other');
    expect(details.open).toBe(false);
    expect(document.activeElement).toBe(host.querySelector('summary'));
    expect(onVisibilityChange).toHaveBeenLastCalledWith(false);
  });

  it('closes on Escape and restores focus to the project summary', async () => {
    const onVisibilityChange = vi.fn();
    await act(async () => root.render(<RecentProjectPicker recent={[other]} onOpen={() => {}} onVisibilityChange={onVisibilityChange} />));
    const details = host.querySelector<HTMLDetailsElement>('details')!;
    const summary = host.querySelector('summary')!;
    details.open = true;
    const menu = host.querySelector('.recent-project-picker-menu')!;
    menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(details.open).toBe(false);
    expect(document.activeElement).toBe(summary);
    expect(onVisibilityChange).toHaveBeenLastCalledWith(false);
  });

  it('keeps moved projects visible but disabled', async () => {
    const missing = { ...other, projectId: 'missing', missing: true };
    await act(async () => root.render(<RecentProjectPicker recent={[missing]} onOpen={() => {}} />));
    const button = host.querySelector<HTMLButtonElement>('.project-picker-item')!;
    expect(button.disabled).toBe(true);
    expect(button.textContent).toContain('Folder moved or unavailable');
  });
});
