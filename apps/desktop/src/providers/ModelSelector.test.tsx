// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as ipc from '../ipc/providers';
import { ModelSelector } from './ModelSelector';
import { ModelSettings } from './ModelSettings';
import { ProviderSettingsProvider, useProviders } from './ProviderContext';
vi.mock('../ipc/providers', async original => ({ ...await original<typeof import('../ipc/providers')>(), readProviderState: vi.fn(), readEndpointSettings: vi.fn(), checkCodexConnection: vi.fn(), saveModelSettings: vi.fn() }));
const luna: ipc.ModelSelection = { providerId: 'codex', modelId: 'gpt-5.6-luna', reasoning: 'xhigh', serviceTier: 'priority' };
function initial(): ipc.ProviderState {
  return { settings: { revision: '0', active: { ...ipc.localModel }, favorites: [] }, dispatch: { kind: 'localMock', detail: 'No live AI connected' }, codexConnection: { ready: false, detail: 'Check Settings to connect Codex.' }, catalog: { models: [
    { key: { providerId: 'mock', modelId: 'mock-story-context' }, label: 'Local test model', providerLabel: 'Local', reasoningLevels: [], serviceTiers: [], contextWindowTokens: null, maxOutputTokens: null, origin: 'builtIn', ready: true, statusDetail: 'No live AI connected' },
    { key: { providerId: 'codex', modelId: 'gpt-5.6-luna' }, label: 'GPT-5.6-Luna', providerLabel: 'Codex CLI', reasoningLevels: ['low', 'medium', 'high', 'xhigh', 'max'], serviceTiers: [{ id: 'priority', label: 'Fast' }], contextWindowTokens: null, maxOutputTokens: null, origin: 'reference', ready: false, statusDetail: 'Not connected' },
    { key: { providerId: 'openai-compatible:11111111-1111-1111-1111-111111111111', modelId: 'nova' }, label: 'Nova Writer', providerLabel: 'Local API', reasoningLevels: ['low', 'high'], serviceTiers: [{ id: 'standard', label: 'Standard' }], contextWindowTokens: null, maxOutputTokens: null, origin: 'openAiCompatible', ready: true, statusDetail: 'Ready' },
  ] } };
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
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); vi.resetAllMocks(); state = initial();
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value() { this.setAttribute('open', ''); } });
  Object.defineProperty(HTMLDialogElement.prototype, 'close', { configurable: true, value() { this.removeAttribute('open'); } });
  vi.mocked(ipc.readProviderState).mockImplementation(async () => structuredClone(state));
  vi.mocked(ipc.readEndpointSettings).mockResolvedValue({ revision: '0', profiles: [] });
  vi.mocked(ipc.checkCodexConnection).mockImplementation(async () => structuredClone(state));
  vi.mocked(ipc.saveModelSettings).mockImplementation(async (_revision, active, favorites) => { state = { ...state, settings: { revision: String(Number(state.settings.revision) + 1), active, favorites }, dispatch: { kind: active.providerId === 'mock' ? 'localMock' : 'blocked', detail: '' } }; return structuredClone(state); });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
describe('persistent model selection', () => {
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
    expect(host.querySelectorAll('.model-choice')).toHaveLength(1); expect(state.settings.active).toEqual(ipc.localModel);
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
    await click('Edit model traits: Extra high · Fast');
    expect(host.querySelector('dialog.model-traits-dialog')).not.toBeNull();
    const reasoning = host.querySelector('#traits-reasoning') as HTMLSelectElement;
    await act(async () => { reasoning.value = 'high'; reasoning.dispatchEvent(new Event('change', { bubbles: true })); });
    expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('8', { ...luna, reasoning: 'high' }, [luna]);
    expect(host.querySelector('dialog.model-picker-dialog')).toBeNull();
    await click('Close model traits'); expect(document.activeElement).toBe(button('Edit model traits: High · Fast'));
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
    const selects = host.querySelectorAll('select'); expect(selects).toHaveLength(2); expect(selects[1].value).toBe('priority');
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
  it('reconciles a failed Codex check without changing preferences', async () => {
    const before = structuredClone(state.settings);
    vi.mocked(ipc.checkCodexConnection).mockRejectedValueOnce({ detail: 'The connection check could not finish.' });
    await render(); await click('Settings'); await click('Check Codex connection');
    expect(ipc.checkCodexConnection).toHaveBeenCalledExactlyOnceWith(); expect(ipc.readProviderState).toHaveBeenCalledTimes(2);
    expect(state.settings).toEqual(before); expect(host.querySelector('[role=alert]')!.textContent).toContain('connection check could not finish');
  });
  it('offers the exact Codex traits when a connection is ready', async () => {
    state.settings = { revision: '8', active: { ...luna, reasoning: 'high', serviceTier: null }, favorites: [luna] };
    state.dispatch = { kind: 'blocked', detail: 'Choose Extra high reasoning and Fast response speed to send.' };
    state.codexConnection = { ready: true, detail: 'Signed in through Codex.' };
    await render(); await click('Settings'); await click('Use Extra high reasoning + Fast response speed');
    expect(ipc.saveModelSettings).toHaveBeenCalledExactlyOnceWith('8', luna, [luna]);
  });
});
