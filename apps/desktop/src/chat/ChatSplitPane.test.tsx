// @vitest-environment jsdom
import { act, useEffect } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { ChatSplitPane, chatSplitPanePreferenceKey } from './ChatSplitPane';

let host: HTMLDivElement;
let root: Root;

function Surface({ side }: { side: 'left' | 'right' }) {
  return <section data-surface={side}>{side} surface</section>;
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  localStorage.clear();
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

function TrackedSurface({ side }: { side: 'left' | 'right' }) {
  return <Surface side={side} />;
}

function renderPane(projectKey = 'project-1') {
  return act(async () => {
    root.render(<ChatSplitPane projectKey={projectKey} left={<TrackedSurface side="left" />} right={<TrackedSurface side="right" />} />);
  });
}

describe('ChatSplitPane', () => {
  it('exposes an accessible vertical separator and clamps keyboard resizing', async () => {
    await renderPane();
    const separator = host.querySelector('[role="separator"]') as HTMLElement;
    expect(separator.getAttribute('aria-orientation')).toBe('vertical');
    expect(separator.getAttribute('aria-valuenow')).toBe('56');

    await act(async () => { separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true })); });
    expect(separator.getAttribute('aria-valuenow')).toBe('54');
    await act(async () => {
      separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }));
    });
    await act(async () => {
      separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }));
    });
    expect(separator.getAttribute('aria-valuenow')).toBe('30');
    await act(async () => { separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true })); });
    expect(separator.getAttribute('aria-valuenow')).toBe('70');
  });

  it('persists a project layout preference without changing the story surface', async () => {
    await renderPane('project-1');
    const separator = host.querySelector('[role="separator"]') as HTMLElement;
    await act(async () => { separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true })); });
    expect(localStorage.getItem(chatSplitPanePreferenceKey('project-1'))).toBe('70');

    await act(async () => root.unmount());
    root = createRoot(host);
    await renderPane('project-1');
    expect(host.querySelector('[role="separator"]')?.getAttribute('aria-valuenow')).toBe('70');
  });

  it('uses a separate default or preference when switching projects', async () => {
    await renderPane('project-1');
    const first = host.querySelector('[role="separator"]') as HTMLElement;
    await act(async () => { first.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true })); });
    await renderPane('project-2');
    expect(host.querySelector('[role="separator"]')?.getAttribute('aria-valuenow')).toBe('56');
    await renderPane('project-1');
    expect(host.querySelector('[role="separator"]')?.getAttribute('aria-valuenow')).toBe('70');
  });

  it('changes width from pointer movement and retains both mounted children', async () => {
    await renderPane();
    const container = host.querySelector('.chat-split-pane') as HTMLDivElement;
    Object.defineProperty(container, 'getBoundingClientRect', { configurable: true, value: () => ({ width: 1000, height: 600, top: 0, left: 0, right: 1000, bottom: 600 }) });
    const separator = host.querySelector('[role="separator"]') as HTMLElement;
    await act(async () => {
      separator.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: 560 }));
      separator.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: 680 }));
      separator.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: 680 }));
    });
    expect(separator.getAttribute('aria-valuenow')).toBe('68');
    expect(host.querySelector('[data-surface="left"]')).not.toBeNull();
    expect(host.querySelector('[data-surface="right"]')).not.toBeNull();
  });

  it('does not force a child remount while resizing or changing project preference', async () => {
    const lifecycle = { leftMounts: 0, rightMounts: 0, leftUnmounts: 0, rightUnmounts: 0 };
    function Probe({ side }: { side: 'left' | 'right' }) {
      useEffect(() => {
        if (side === 'left') lifecycle.leftMounts += 1;
        else lifecycle.rightMounts += 1;
        return () => {
          if (side === 'left') lifecycle.leftUnmounts += 1;
          else lifecycle.rightUnmounts += 1;
        };
      }, [side]);
      return <section data-probe={side}>{side}</section>;
    }
    await act(async () => root.render(<ChatSplitPane projectKey="project-1" left={<Probe side="left" />} right={<Probe side="right" />} />));
    expect(lifecycle).toMatchObject({ leftMounts: 1, rightMounts: 1, leftUnmounts: 0, rightUnmounts: 0 });
    const left = host.querySelector('[data-probe="left"]');
    const right = host.querySelector('[data-probe="right"]');
    const separator = host.querySelector('[role="separator"]') as HTMLElement;
    await act(async () => { separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true })); });
    expect(host.querySelector('[data-probe="left"]')).toBe(left);
    expect(host.querySelector('[data-probe="right"]')).toBe(right);
    await act(async () => root.render(<ChatSplitPane projectKey="project-2" left={<Probe side="left" />} right={<Probe side="right" />} />));
    expect(host.querySelector('[data-probe="left"]')).toBe(left);
    expect(host.querySelector('[data-probe="right"]')).toBe(right);
    expect(lifecycle).toMatchObject({ leftMounts: 1, rightMounts: 1, leftUnmounts: 0, rightUnmounts: 0 });
  });
});
