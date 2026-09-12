// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MemoryPanel, type MemoryPanelProps, type MemoryPanelState, type MemoryView } from '../story/MemoryPanel';

const view = (kind: MemoryView['kind'], id?: string): MemoryView => ({
  id: id ?? kind, kind, createdAt: '2026-09-06T12:00:00Z', sourceLabel: kind === 'revoked' ? 'Private notes' : 'The return',
  items: [{ text: `${kind} memory`, uncertainty: kind === 'changedSource' ? 'The source changed.' : undefined, evidence: [{ quote: `${kind} source quote`, display: 'Chapter text' }] }],
});
const baseState: MemoryPanelState = { kind: 'completed', disposition: 'candidate', views: [view('current')] };
let host: HTMLDivElement; let root: Root;
let props: MemoryPanelProps;

async function render(overrides: Partial<MemoryPanelProps> = {}) {
  await act(async () => root.render(<MemoryPanel {...props} {...overrides} />));
}
function button(name: string) { return [...host.querySelectorAll('button')].find(item => item.textContent === name)!; }
async function click(name: string) { await act(async () => button(name).click()); }

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  props = { documentTitle: 'The return', modelLabel: 'Local test model', modelAvailable: true, allowanceLabel: '1,200 bytes per request', state: baseState,
    onRefresh: vi.fn(), onStop: vi.fn(), onReconcile: vi.fn(), onInspect: vi.fn(), onClose: vi.fn() };
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

describe('story memory panel', () => {
  it('renders without generating and refreshes only after the explicit action', async () => {
    await render();
    expect(props.onRefresh).not.toHaveBeenCalled();
    await click('Refresh story memory');
    expect(props.onRefresh).toHaveBeenCalledOnce();
  });

  it('explains unavailable model selection and disables refresh', async () => {
    await render({ modelAvailable: false, modelLabel: 'Unavailable model' });
    expect(host.textContent).toContain('The story-memory model is unavailable. Check the Codex connection in Settings');
    expect(button('Refresh story memory').disabled).toBe(true);
    await click('Refresh story memory');
    expect(props.onRefresh).not.toHaveBeenCalled();
  });

  it('keeps active output readable while Stop and close remain explicit controls', async () => {
    const close = vi.fn(); const stop = vi.fn();
    await render({ state: { kind: 'active', phase: 'running', views: [view('current')] }, onClose: close, onStop: stop });
    expect(host.textContent).toContain('current memory'); expect(button('Stop')).toBeDefined();
    await click('Stop'); await click('Back to writing');
    expect(stop).toHaveBeenCalledOnce(); expect(close).toHaveBeenCalledOnce(); expect(props.onRefresh).not.toHaveBeenCalled();
  });

  it('keeps reconciliation separate from a new refresh', async () => {
    const reconcile = vi.fn(); const refresh = vi.fn();
    await render({ state: { kind: 'completed', disposition: 'needsReconciliation', views: [view('current')] }, onReconcile: reconcile, onRefresh: refresh });
    expect(host.textContent).toContain('Check the saved result');
    await click('Check saved result');
    expect(reconcile).toHaveBeenCalledOnce(); expect(refresh).not.toHaveBeenCalled(); expect(button('Refresh story memory').disabled).toBe(true);
  });

  it('labels changed and recovered views, while policy-revoked content stays hidden', async () => {
    await render({ state: { kind: 'completed', disposition: 'candidate', views: [view('changedSource'), view('recoveredHistorical', 'old'), view('revoked', 'hidden')] } });
    expect(host.textContent).toContain('Changed source'); expect(host.textContent).toContain('Recovered historical memory');
    expect(host.textContent).toContain('Unavailable memory'); expect(host.textContent).toContain('Content and evidence are hidden.');
    expect(host.textContent).not.toContain('revoked memory'); expect(host.textContent).not.toContain('revoked source quote');
  });

  it('focuses the heading once on mount and does not steal focus on state updates', async () => {
    await render();
    const heading = host.querySelector('h2')!; expect(document.activeElement).toBe(heading);
    const inspect = button('Inspect source'); inspect.focus();
    await render({ state: { kind: 'completed', disposition: 'candidate', views: [view('current')] } });
    expect(document.activeElement).toBe(inspect);
  });
});
