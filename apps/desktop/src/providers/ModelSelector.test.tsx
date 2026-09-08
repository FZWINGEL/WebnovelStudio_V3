// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as ipc from '../ipc/providers';
import { ModelSelector } from './ModelSelector';
import { ModelSettings } from './ModelSettings';
import { ProviderSettingsProvider, useProviders } from './ProviderContext';
vi.mock('../ipc/providers', async original => ({ ...await original<typeof import('../ipc/providers')>(), readProviderState: vi.fn(), readEndpointSettings: vi.fn(), checkCodexConnection: vi.fn(), checkClaudeConnection: vi.fn(), saveModelSettings: vi.fn(), saveStoryMemoryProvider: vi.fn(), readCodexTransport: vi.fn(), saveCodexTransport: vi.fn() }));
const luna: ipc.ModelSelection = { providerId: 'codex', modelId: 'gpt-5.6-luna', reasoning: 'xhigh', serviceTier: 'priority' };
function initial(): ipc.ProviderState {
  return { settings: { revision: '0', active: { ...ipc.localModel }, favorites: [] }, dispatch: { kind: 'localMock', detail: 'No live AI connected' }, codexConnection: { ready: false, detail: 'Check Settings to connect Codex.' }, catalog: { models: [
    { key: { providerId: 'mock', modelId: 'mock-story-context' }, label: 'Local test model', providerLabel: 'Local', reasoningLevels: [], serviceTiers: [], contextWindowTokens: null, maxOutputTokens: null, origin: 'builtIn', ready: true, statusDetail: 'No live AI connected' },
    { key: { providerId: 'codex', modelId: 'gpt-5.6-luna' }, label: 'GPT-5.6-Luna', providerLabel: 'Codex CLI', reasoningLevels: ['low', 'medium', 'high', 'xhigh', 'max'], serviceTiers: [{ id: 'priority', label: 'Fast' }], contextWindowTokens: null, maxOutputTokens: null, origin: 'reference', ready: false, statusDetail: 'Not connected' },
    { key: { providerId: 'openai-compatible:11111111-1111-1111-1111-111111111111', modelId: 'nova' }, label: 'Nova Writer', providerLabel: 'Local API', reasoningLevels: ['low', 'high'], serviceTiers: [{ id: 'standard', label: 'Standard' }], contextWindowTokens: null, maxOutputTokens: null, origin: 'openAiCompatible', ready: true, statusDetail: 'Ready' },
    { key: { providerId: 'openai-compatible:11111111-1111-1111-1111-111111111111', modelId: 'gpt-5.6-luna' }, label: 'GPT-5.6-Luna', providerLabel: 'Local API', reasoningLevels: [], serviceTiers: [], contextWindowTokens: null, maxOutputTokens: null, origin: 'openAiCompatible', ready: true, statusDetail: 'Configured' },
    { key: { providerId: 'openai-compatible:11111111-1111-1111-1111-111111111111', modelId: 'gpt-6-astra' }, label: 'GPT-6 Astra', providerLabel: 'Local API', reasoningLevels: ['low'], serviceTiers: [], contextWindowTokens: null, maxOutputTokens: null, origin: 'openAiCompatible', ready: true, statusDetail: 'Configured' },
  ] }, storyMemory: { revision: '0', providerId: 'codex', providerLabel: 'Codex CLI', modelId: 'gpt-6-astra', reasoning: 'low', serviceTier: 'priority', ready: false, detail: 'Check Codex connection.' } };
}
let state: ipc.ProviderState; let host: HTMLDivElement; let root: Root;
function Probe() { const value = useProviders(); return <output>{value.state?.settings.active.modelId ?? 'unavailable'}:{value.state?.dispatch.kind ?? 'blocked'}:{String(value.busy)}</output>; }
function button(label: string) { return [...host.querySelectorAll('button')].find(button => button.textContent === label || button.getAttribute('aria-label') === label)!; }
async function click(label: string) { await act(async () => button(label).click()); }
async function render() { await act(async () => root.render(<ProviderSettingsProvider><ModelSelector /><ModelSettings /><Probe /></ProviderSettingsProvider>)); }
describe('unavailable saved traits', () => {
  it.each(['settings', 'traits'])('shows the retained values and repairs both traits together in %s', async surface => {
    state.settings.active = { ...luna, reasoning: 'ultra', serviceTier: 'removed' };
    const model = state.catalog.models.find(model=>model.key.modelId===luna.modelId)!;
    model.reasoningLevels=['high']; model.defaultReasoning='high'; model.serviceTiers=[]; model.defaultServiceTier=null;
    state.dispatch={kind:'blocked',detail:'Saved traits are unavailable'};
    await render();
    if(surface==='settings') await click('Settings');
    else await act(async()=>host.querySelector<HTMLButtonElement>('[aria-label^="Edit model traits"]')!.click());
    const selects=[...host.querySelectorAll<HTMLSelectElement>('select')];
    expect(selects.some(select=>select.value==='ultra'&&select.selectedOptions[0].textContent?.includes('unavailable'))).toBe(true);
    expect(selects.some(select=>select.value==='removed'&&select.selectedOptions[0].textContent?.includes('unavailable'))).toBe(true);
    expect(ipc.saveModelSettings).not.toHaveBeenCalled();
    await click('Use available traits');
    expect(ipc.saveModelSettings).toHaveBeenCalledTimes(1);
    expect(vi.mocked(ipc.saveModelSettings).mock.calls[0][1]).toEqual({...luna,reasoning:'high',serviceTier:null});
  });
});
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true, isTauri: false }); vi.resetAllMocks(); state = initial();
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value() { this.setAttribute('open', ''); } });
  Object.defineProperty(HTMLDialogElement.prototype, 'close', { configurable: true, value() { this.removeAttribute('open'); } });
  vi.mocked(ipc.readProviderState).mockImplementation(async () => structuredClone(state));
  vi.mocked(ipc.readEndpointSettings).mockResolvedValue({ revision: '0', profiles: [] });
  vi.mocked(ipc.checkCodexConnection).mockImplementation(async () => structuredClone(state));
  vi.mocked(ipc.checkClaudeConnection).mockImplementation(async () => structuredClone(state));
  vi.mocked(ipc.readCodexTransport).mockResolvedValue({ revision: '0', transport: 'exec' });
  vi.mocked(ipc.saveCodexTransport).mockImplementation(async (expectedRevision, transport) => ({ revision: String(Number(expectedRevision) + 1), transport }));
  vi.mocked(ipc.saveModelSettings).mockImplementation(async (_revision, active, favorites) => { state = { ...state, settings: { revision: String(Number(state.settings.revision) + 1), active, favorites }, dispatch: { kind: active.providerId === 'mock' ? 'localMock' : 'blocked', detail: '' } }; return structuredClone(state); });
  vi.mocked(ipc.saveStoryMemoryProvider).mockImplementation(async (_revision, providerId) => { state = { ...state, storyMemory: { ...state.storyMemory!, revision: String(Number(state.storyMemory?.revision ?? '0') + 1), providerId, providerLabel: providerId.startsWith('openai-compatible:') ? 'Local API' : providerId === 'mock' ? 'WebnovelStudio' : 'Codex CLI', modelId: providerId === 'mock' ? 'local-editorial-v1' : 'gpt-6-astra', reasoning: providerId === 'mock' ? null : 'low', serviceTier: providerId === 'codex' ? 'priority' : null, ready: true, detail: 'Configured' } }; return structuredClone(state); });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
describe('persistent model selection', () => {
  it('saves the Codex app-server choice without changing the writing model or starting a request', async () => {
    Object.assign(globalThis, { isTauri: true });
    vi.mocked(ipc.readCodexTransport).mockResolvedValueOnce({ revision: '7', transport: 'exec' });
    vi.mocked(ipc.saveCodexTransport).mockResolvedValueOnce({ revision: '8', transport: 'appServer' });
    await render(); await click('Settings');
    await vi.waitFor(() => expect(ipc.readCodexTransport).toHaveBeenCalledOnce());
    const select = host.querySelector('#codex-transport') as HTMLSelectElement;
    expect(select.value).toBe('exec');
    await act(async () => { select.value = 'appServer'; select.dispatchEvent(new Event('change', { bubbles: true })); });
    expect(ipc.saveCodexTransport).toHaveBeenCalledExactlyOnceWith('7', 'appServer');
    expect(ipc.saveModelSettings).not.toHaveBeenCalled();
    expect(select.value).toBe('appServer');
  });

  it('shows a transport CAS conflict and rereads the persisted choice without generating', async () => {
    Object.assign(globalThis, { isTauri: true });
    vi.mocked(ipc.readCodexTransport).mockResolvedValueOnce({ revision: '2', transport: 'exec' }).mockResolvedValueOnce({ revision: '3', transport: 'appServer' });
    vi.mocked(ipc.saveCodexTransport).mockRejectedValueOnce({ code: 'PreferenceConflict', detail: 'The Codex transport changed. Read Settings again before saving.' });
    await render(); await click('Settings');
    await vi.waitFor(() => expect(ipc.readCodexTransport).toHaveBeenCalledOnce());
    const select = host.querySelector('#codex-transport') as HTMLSelectElement;
    await act(async () => { select.value = 'appServer'; select.dispatchEvent(new Event('change', { bubbles: true })); });
    await vi.waitFor(() => expect(host.querySelector('[role=alert]')?.textContent).toContain('transport changed'));
    expect(ipc.readCodexTransport).toHaveBeenCalledTimes(2);
    expect(select.value).toBe('appServer');
    expect(ipc.saveModelSettings).not.toHaveBeenCalled();
  });

  it('saves story-memory endpoint choice independently of the writing assistant', async () => {
    state.settings.active = { providerId: 'codex', modelId: 'gpt-5.4-mini', reasoning: null, serviceTier: null };
    await render(); await click('Settings');
    const select = host.querySelector('#story-memory-provider') as HTMLSelectElement;
    await act(async () => { select.value = 'openai-compatible:11111111-1111-1111-1111-111111111111'; select.dispatchEvent(new Event('change', { bubbles: true })); });
    expect(ipc.saveStoryMemoryProvider).toHaveBeenCalledExactlyOnceWith('0', 'openai-compatible:11111111-1111-1111-1111-111111111111');
    expect(ipc.saveModelSettings).not.toHaveBeenCalled();
  });

  it('keeps the requested Luna xhigh preference when discovery reports a lower default', async () => {
    const model = state.catalog.models.find(model => model.key.modelId === luna.modelId)!;
    model.origin = 'codexDiscovery'; model.defaultReasoning = 'medium';
    await render(); await click('Choose model: Local test model');
    await act(async () => (host.querySelectorAll('.model-choice')[1] as HTMLButtonElement).click());
    expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('0', luna, []);
  });
  it('uses discovered model defaults instead of forcing Luna traits', async () => {
    state.catalog.models.push({
      key: { providerId: 'codex', modelId: 'writer-v3' }, label: 'Writer V3', providerLabel: 'Codex CLI',
      reasoningLevels: ['low', 'high'], defaultReasoning: 'high', serviceTiers: [{ id: 'standard', label: 'Standard' }], defaultServiceTier: 'standard',
      contextWindowTokens: null, maxOutputTokens: null, origin: 'codexDiscovery', ready: false, statusDetail: 'Discovered',
    });
    await render(); await click('Choose model: Local test model');
    const input = host.querySelector('#model-search') as HTMLInputElement;
    await act(async () => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, 'writer-v3'); input.dispatchEvent(new Event('input', { bubbles: true })); });
    await act(async () => input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })));
    expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('0', { providerId: 'codex', modelId: 'writer-v3', reasoning: 'high', serviceTier: 'standard' }, []);
  });
  it('does not match opaque endpoint identity characters as part of a model name', async () => {
    const endpoint = state.catalog.models[2]; endpoint.key.modelId = 'test-editor-v2'; endpoint.label = 'test-editor-v2';
    await render(); await click('Choose model: Local test model');
    const input = host.querySelector('#model-search') as HTMLInputElement;
    await act(async () => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, 'test-editor-v1'); input.dispatchEvent(new Event('input', { bubbles: true })); });
    expect(host.querySelectorAll('.model-choice')).toHaveLength(0);
  });
  it('browses without changing the model and commits a keyboard choice with exact traits', async () => {
    await render(); await click('Choose model: Local test model');
    await act(async () => host.querySelector('#model-search')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true })));
    expect(document.activeElement).toBe(host.querySelector('.model-choice')); expect(ipc.saveModelSettings).not.toHaveBeenCalled();
    await act(async () => host.querySelector('dialog')!.dispatchEvent(new KeyboardEvent('keydown', { key: '2', ctrlKey: true, bubbles: true })));
    expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('0', luna, []);
    expect(host.querySelector('dialog')).toBeNull(); expect(document.activeElement).toBe(button('Choose model: GPT-5.6-Luna'));
    expect(host.querySelector('output')!.textContent).toBe('gpt-5.6-luna:blocked:false');
  });
  it('stores favorites without selecting them and keeps traits out of the model list', async () => {
    await render(); await click('Choose model: Local test model'); await click('Favorite GPT-5.6-Luna');
    expect(state.settings.active).toEqual(ipc.localModel); expect(state.settings.favorites).toEqual([{ providerId: 'codex', modelId: 'gpt-5.6-luna' }]);
    expect(host.querySelector('select')).toBeNull(); await click('Favorites'); expect(host.querySelectorAll('.model-choice')).toHaveLength(1);
    await click('Close model picker'); await click('Choose model: Local test model');
    const input = host.querySelector('#model-search') as HTMLInputElement;
    await act(async () => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, 'lun'); input.dispatchEvent(new Event('input', { bubbles: true })); });
    expect(host.querySelectorAll('.model-choice')).toHaveLength(2); expect(state.settings.active).toEqual(ipc.localModel);
  });
  it('searches every provider while a provider rail filter is active', async () => {
    await render(); await click('Choose model: Local test model');
    await click('Codex CLI');
    expect(host.querySelectorAll('.model-choice')).toHaveLength(1);
    expect(host.querySelector('.model-choice')?.textContent).toContain('GPT-5.6-Luna');
    const input = host.querySelector('#model-search') as HTMLInputElement;
    await act(async () => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, 'nova'); input.dispatchEvent(new Event('input', { bubbles: true })); });
    expect(host.querySelector('[role=status]')?.textContent).toContain('Searching all providers');
    expect(host.querySelectorAll('.model-choice')).toHaveLength(1);
    expect(host.querySelector('.model-choice')?.textContent).toContain('Nova Writer');
    expect(host.querySelectorAll('.model-rail-item[aria-pressed=true]')).toHaveLength(0);
  });
  it('edits declared traits beside the persistent selector without selecting a model', async () => {
    state.settings = { revision: '8', active: luna, favorites: [luna] }; state.dispatch.kind = 'blocked'; await render();
    expect((host.querySelector('#model-traits-reasoning') as HTMLSelectElement).getAttribute('aria-label')).toBe('Reasoning effort');
    expect((host.querySelector('#model-traits-service-tier') as HTMLSelectElement).getAttribute('aria-label')).toBe('Service tier');
    await act(async()=>host.querySelector<HTMLButtonElement>('[aria-label^="Edit model traits"]')?.click());
    expect(host.querySelector('dialog.model-traits-dialog')).not.toBeNull();
    const reasoning = host.querySelector('#traits-reasoning') as HTMLSelectElement;
    await act(async () => { reasoning.value = 'high'; reasoning.dispatchEvent(new Event('change', { bubbles: true })); });
    expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('8', { ...luna, reasoning: 'high' }, [luna]);
    expect(host.querySelector('dialog.model-picker-dialog')).toBeNull();
    await click('Close model traits'); expect(document.activeElement).toBe(button('Edit model traits: High · Fast'));
  });
  it('changes the visible reasoning and service-tier controls without opening the model list', async () => {
    state.settings = { revision: '8', active: luna, favorites: [luna] }; state.dispatch.kind = 'blocked'; await render();
    const reasoning = host.querySelector('#model-traits-reasoning') as HTMLSelectElement;
    await act(async () => { reasoning.value = 'high'; reasoning.dispatchEvent(new Event('change', { bubbles: true })); });
    expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('8', { ...luna, reasoning: 'high' }, [luna]);
    vi.mocked(ipc.saveModelSettings).mockClear();
    const serviceTier = host.querySelector('#model-traits-service-tier') as HTMLSelectElement;
    await act(async () => { serviceTier.value = ''; serviceTier.dispatchEvent(new Event('change', { bubbles: true })); });
    expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('9', { ...luna, reasoning: 'high', serviceTier: null }, [luna]);
  });
  it('preserves the companion trait and persists provider default as null', async () => {
    state.settings = { revision: '8', active: luna, favorites: [luna] }; state.dispatch.kind = 'blocked'; await render();
    const reasoning = host.querySelector('#model-traits-reasoning') as HTMLSelectElement;
    await act(async () => { reasoning.value = ''; reasoning.dispatchEvent(new Event('change', { bubbles: true })); });
    expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('8', { ...luna, reasoning: null }, [luna]);
  });
  it('reconciles a lost save acknowledgment by reading settings without replaying', async () => {
    await render(); await click('Choose model: Local test model');
    vi.mocked(ipc.saveModelSettings).mockImplementationOnce(async (_rev, active, favorites) => { state = { ...state, settings: { revision: '1', active, favorites }, dispatch: { kind: 'blocked', detail: '' } }; throw { detail: 'The acknowledgment was lost.' }; });
    await act(async () => (host.querySelectorAll('.model-choice')[1] as HTMLButtonElement).click());
    expect(ipc.saveModelSettings).toHaveBeenCalledOnce(); expect(ipc.readProviderState).toHaveBeenCalledTimes(2);
    expect(host.querySelector('output')!.textContent).toBe('gpt-5.6-luna:blocked:false'); expect(host.querySelector('[role=alert]')!.textContent).toContain('acknowledgment was lost');
    expect(host.querySelector('dialog')).not.toBeNull();
  });
  it('fails closed when both the save and its reconciliation read fail', async () => {
    await render(); await click('Choose model: Local test model');
    vi.mocked(ipc.saveModelSettings).mockRejectedValueOnce(new Error('lost')); vi.mocked(ipc.readProviderState).mockRejectedValueOnce(new Error('offline'));
    await act(async () => (host.querySelectorAll('.model-choice')[1] as HTMLButtonElement).click());
    expect(host.querySelector('output')!.textContent).toBe('unavailable:blocked:false');
    await click('Check model settings'); expect(host.querySelector('output')!.textContent).toBe('mock-story-context:localMock:false'); expect(ipc.saveModelSettings).toHaveBeenCalledOnce();
  });
  it('edits supported traits in Settings and preserves favorites and the active model', async () => {
    state.settings = { revision: '8', active: luna, favorites: [luna] }; state.dispatch.kind = 'blocked'; await render(); await click('Settings');
    const selects = host.querySelectorAll<HTMLSelectElement>('.settings-dialog select'); expect(selects).toHaveLength(4); expect(selects[1].value).toBe('priority');
    expect(selects[0].labels?.[0].textContent).toBe('Reasoning'); expect(selects[1].labels?.[0].textContent).toBe('Response speed');
    await act(async () => { selects[0].value = 'high'; selects[0].dispatchEvent(new Event('change', { bubbles: true })); });
    expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('8', { ...luna, reasoning: 'high' }, [luna]);
    await click('Close settings'); expect(document.activeElement).toBe(button('Settings'));
  });
  it('checks Codex only when explicitly requested and preserves the saved choice', async () => {
    const before = structuredClone(state.settings);
    vi.mocked(ipc.checkCodexConnection).mockImplementationOnce(async () => {
      state = { ...state, codexConnection: { ready: true, detail: 'Signed in through Codex. GPT-5.6-Luna is available with Extra high reasoning and Fast response speed.' } };
      return structuredClone(state);
    });
    await render(); expect(ipc.checkCodexConnection).not.toHaveBeenCalled(); await click('Settings');
    expect(ipc.checkCodexConnection).not.toHaveBeenCalled(); await click('Check Codex connection');
    expect(ipc.checkCodexConnection).toHaveBeenCalledExactlyOnceWith(); expect(state.settings).toEqual(before);
    expect(host.textContent).toContain('Signed in through Codex'); expect(host.querySelector('.provider-connection-status')!.textContent).toBe('Connected');
  });
  it('adopts the fresh Codex default when the user explicitly checks from the untouched fallback', async () => {
    vi.mocked(ipc.checkCodexConnection).mockImplementationOnce(async () => {
      state = structuredClone(state);
      state.codexConnection = { ready: true, checked: true, detail: 'Signed in through Codex.' };
      state.catalog.models.find(model => model.key.providerId === 'codex' && model.key.modelId === 'gpt-5.6-luna')!.ready = true;
      return structuredClone(state);
    });
    await render(); await click('Settings'); await click('Check Codex connection');
    expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('0', luna, []);
  });
  it('checks Codex on native startup and adopts the live default only for untouched settings', async () => {
    Object.assign(globalThis, { isTauri: true });
    vi.mocked(ipc.checkCodexConnection).mockImplementationOnce(async () => {
      state = structuredClone(state);
      state.codexConnection = { ready: true, checked: true, detail: 'Signed in through Codex.' };
      const model = state.catalog.models.find(candidate => candidate.key.providerId === 'codex' && candidate.key.modelId === 'gpt-5.6-luna')!;
      model.ready = true;
      return structuredClone(state);
    });
    vi.mocked(ipc.saveModelSettings).mockImplementationOnce(async (_revision, active, favorites) => {
      state = { ...state, settings: { revision: '1', active, favorites }, dispatch: { kind: 'codexCli', detail: 'Uses your Codex sign-in.' } };
      return structuredClone(state);
    });
    await render();
    expect(ipc.checkCodexConnection).toHaveBeenCalledExactlyOnceWith();
    expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('0', luna, []);
    expect(host.querySelector('output')!.textContent).toBe('gpt-5.6-luna:codexCli:false');
  });
  it('reattaches to an in-progress native Codex check without starting a duplicate probe', async () => {
    Object.assign(globalThis, { isTauri: true });
    vi.useFakeTimers();
    try {
      const checking = structuredClone(state);
      checking.codexConnection = { ...checking.codexConnection!, checked: true, checking: true };
      const ready = structuredClone(checking);
      ready.codexConnection = { ready: true, checked: true, checking: false, detail: 'Signed in through Codex.' };
      ready.catalog.models.find(model => model.key.providerId === 'codex' && model.key.modelId === 'gpt-5.6-luna')!.ready = true;
      state = structuredClone(ready);
      vi.mocked(ipc.saveModelSettings).mockImplementationOnce(async (_revision, active, favorites) => {
        state = { ...state, settings: { revision: '1', active, favorites }, dispatch: { kind: 'codexCli', detail: 'Uses your Codex sign-in.' } };
        return structuredClone(state);
      });
      const reads = [checking, ready];
      vi.mocked(ipc.readProviderState).mockImplementation(async () => structuredClone(reads.shift() ?? ready));
      await render();
      await act(async () => { await vi.advanceTimersByTimeAsync(500); });
      expect(ipc.checkCodexConnection).not.toHaveBeenCalled();
      expect(ipc.readProviderState).toHaveBeenCalledTimes(2);
      expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('0', luna, []);
      expect(host.querySelector('output')!.textContent).toBe('gpt-5.6-luna:codexCli:false');
    } finally { vi.useRealTimers(); }
  });
  it('joins an existing check when the startup probe races a renderer reattach', async () => {
    Object.assign(globalThis, { isTauri: true });
    vi.useFakeTimers();
    try {
      const initialState = structuredClone(state);
      initialState.codexConnection = { ...initialState.codexConnection!, checked: true, checking: false };
      const checking = structuredClone(initialState);
      checking.codexConnection = { ...checking.codexConnection!, checking: true };
      const ready = structuredClone(checking);
      ready.codexConnection = { ready: true, checked: true, checking: false, detail: 'Signed in through Codex.' };
      ready.catalog.models.find(model => model.key.providerId === 'codex' && model.key.modelId === 'gpt-5.6-luna')!.ready = true;
      state = structuredClone(ready);
      vi.mocked(ipc.saveModelSettings).mockImplementationOnce(async (_revision, active, favorites) => {
        state = { ...state, settings: { revision: '1', active, favorites }, dispatch: { kind: 'codexCli', detail: 'Uses your Codex sign-in.' } };
        return structuredClone(state);
      });
      const reads = [initialState, checking, ready];
      vi.mocked(ipc.readProviderState).mockImplementation(async () => structuredClone(reads.shift() ?? ready));
      vi.mocked(ipc.checkCodexConnection).mockRejectedValueOnce({ code: 'ConnectionCheckRunning', detail: 'A Codex connection check is already running.' });
      await render();
      await act(async () => { await vi.advanceTimersByTimeAsync(500); });
      expect(ipc.checkCodexConnection).toHaveBeenCalledExactlyOnceWith();
      expect(ipc.readProviderState).toHaveBeenCalledTimes(3);
      expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('0', luna, []);
      expect(host.querySelector('output')!.textContent).toBe('gpt-5.6-luna:codexCli:false');
    } finally { vi.useRealTimers(); }
  });
  it('keeps a concrete timeout when an existing Codex check never settles', async () => {
    Object.assign(globalThis, { isTauri: true });
    vi.useFakeTimers();
    try {
      const checking = structuredClone(state);
      checking.codexConnection = { ...checking.codexConnection!, checked: true, checking: true };
      vi.mocked(ipc.readProviderState).mockResolvedValue(checking);
      await render();
      await act(async () => { await vi.advanceTimersByTimeAsync(90_000); });
      expect(ipc.checkCodexConnection).not.toHaveBeenCalled();
      expect(vi.mocked(ipc.readProviderState).mock.calls.length).toBeGreaterThan(100);
      await click('Settings');
      expect(host.querySelector('[role=alert]')?.textContent).toContain('did not finish within 90 seconds');
      expect(host.textContent).toContain('Local test model');
      expect(button('Check Codex connection')).toBeTruthy();
    } finally { vi.useRealTimers(); }
  });
  it('keeps an explicit saved model when native startup finds Codex', async () => {
    Object.assign(globalThis, { isTauri: true });
    const explicit: ipc.ModelSelection = { providerId: 'openai-compatible:11111111-1111-1111-1111-111111111111', modelId: 'nova', reasoning: 'low', serviceTier: 'standard' };
    state.settings = { revision: '4', active: explicit, favorites: [] };
    state.dispatch = { kind: 'blocked', detail: 'Saved endpoint choice.' };
    vi.mocked(ipc.checkCodexConnection).mockImplementationOnce(async () => ({ ...structuredClone(state), codexConnection: { ready: true, checked: true, detail: 'Signed in through Codex.' } }));
    await render();
    expect(ipc.checkCodexConnection).not.toHaveBeenCalled();
    expect(ipc.saveModelSettings).not.toHaveBeenCalled();
    expect(host.querySelector('output')!.textContent).toBe('nova:blocked:false');
  });
  it('shows an unavailable status after a failed native probe and keeps the retry action', async () => {
    Object.assign(globalThis, { isTauri: true });
    vi.mocked(ipc.checkCodexConnection).mockImplementationOnce(async () => ({
      ...structuredClone(state),
      codexConnection: { ready: false, checked: true, detail: 'Codex is not installed or signed in on this computer.' },
    }));
    await render(); await click('Settings');
    expect(host.querySelector('.provider-connection-status')?.textContent).toBe('Unavailable');
    expect(host.textContent).toContain('Codex is not installed or signed in');
    expect(button('Check Codex connection')).toBeTruthy();
  });
  it('checks Claude only when explicitly requested and preserves the saved choice', async () => {
    state.catalog.models.push({
      key: { providerId: 'claude', modelId: 'claude-opus-5' }, label: 'Claude Opus 5', providerLabel: 'Claude Code',
      reasoningLevels: ['low', 'medium', 'high', 'xhigh', 'max'], defaultReasoning: 'high', serviceTiers: [], defaultServiceTier: null,
      contextWindowTokens: null, maxOutputTokens: null, origin: 'reference', ready: false, statusDetail: 'Check Claude connection.',
    });
    state.claudeConnection = { ready: false, detail: 'Check Settings to connect Claude Code.' };
    const before = structuredClone(state.settings);
    vi.mocked(ipc.checkClaudeConnection).mockImplementationOnce(async () => {
      state = { ...state, claudeConnection: { ready: true, detail: 'Claude Code is installed and signed in.' } };
      return structuredClone(state);
    });
    await render(); expect(ipc.checkClaudeConnection).not.toHaveBeenCalled(); await click('Settings');
    expect(ipc.checkClaudeConnection).not.toHaveBeenCalled(); await click('Check Claude connection');
    expect(ipc.checkClaudeConnection).toHaveBeenCalledExactlyOnceWith(); expect(state.settings).toEqual(before);
    expect(host.textContent).toContain('Claude Code is installed and signed in.'); expect(host.querySelector('#claude-connection-title')?.parentElement?.textContent).toContain('Connected');
  });
  it('reconciles a failed Codex check without changing preferences', async () => {
    const before = structuredClone(state.settings);
    vi.mocked(ipc.checkCodexConnection).mockRejectedValueOnce({ detail: 'The connection check could not finish.' });
    await render(); await click('Settings'); await click('Check Codex connection');
    expect(ipc.checkCodexConnection).toHaveBeenCalledExactlyOnceWith(); expect(ipc.readProviderState).toHaveBeenCalledTimes(2);
    expect(state.settings).toEqual(before); expect(host.querySelector('[role=alert]')!.textContent).toContain('connection check could not finish');
  });
  it('preserves an explicit saved choice when an explicit check joins another renderer', async () => {
    const explicit: ipc.ModelSelection = { providerId: 'openai-compatible:11111111-1111-1111-1111-111111111111', modelId: 'nova', reasoning: 'low', serviceTier: 'standard' };
    state.settings = { revision: '4', active: explicit, favorites: [] };
    state.dispatch = { kind: 'blocked', detail: 'Saved endpoint choice.' };
    const initialState = structuredClone(state);
    initialState.codexConnection = { ...initialState.codexConnection!, checked: true, checking: false };
    const ready = structuredClone(initialState);
    ready.codexConnection = { ready: true, checked: true, checking: false, detail: 'Signed in through Codex.' };
    vi.mocked(ipc.readProviderState).mockImplementationOnce(async () => initialState).mockImplementationOnce(async () => ready);
    vi.mocked(ipc.checkCodexConnection).mockRejectedValueOnce({ code: 'ConnectionCheckRunning', detail: 'A Codex connection check is already running.' });
    await render(); await click('Settings'); await click('Check Codex connection');
    expect(ipc.checkCodexConnection).toHaveBeenCalledExactlyOnceWith();
    expect(ipc.readProviderState).toHaveBeenCalledTimes(2);
    expect(ipc.saveModelSettings).not.toHaveBeenCalled();
    expect(state.settings.active).toEqual(explicit);
    expect(host.querySelector('output')!.textContent).toBe('nova:blocked:false');
  });
  it('offers the exact Codex traits when a connection is ready', async () => {
    state.settings = { revision: '8', active: { ...luna, reasoning: 'high', serviceTier: null }, favorites: [luna] };
    state.dispatch = { kind: 'blocked', detail: 'Choose Extra high reasoning and Fast response speed to send.' };
    state.codexConnection = { ready: true, detail: 'Signed in through Codex.' };
    await render(); await click('Settings'); await click('Use Extra high reasoning + Fast response speed');
    expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('8', luna, [luna]);
  });
});
