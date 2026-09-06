// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EndpointSettings } from './EndpointSettings';
import * as ipc from '../ipc/providers';
vi.mock('../ipc/providers', async original => ({ ...await original<typeof import('../ipc/providers')>(), readEndpointSettings: vi.fn(), saveEndpointSettings: vi.fn(), discoverEndpointModels: vi.fn(), cancelEndpointDiscovery: vi.fn() }));
const { refresh } = vi.hoisted(() => ({ refresh: vi.fn(async () => {}) }));
vi.mock('./ProviderContext', () => ({ useProviders: () => ({ refresh }) }));
const profile: ipc.EndpointProfile = { id: 'openai-compatible:00000000-0000-0000-0000-000000000001', label: 'My local service', baseUrl: 'http://localhost:1234/v1', enabled: true, jsonMode: false, configRevision: '4', hasApiKey: true, apiKeyConfigured: true, manualModelIds: ['fiction-v1'], cachedModelIds: ['discovered-v2'] };
let host: HTMLDivElement; let root: Root;
function button(label: string) { const result = [...host.querySelectorAll('button')].find(b => b.textContent === label || b.getAttribute('aria-label') === label); if (!result) throw new Error(`Missing button ${label}`); return result; }
async function click(label: string) { await act(async () => button(label).click()); }
async function fill(id: string, value: string) { const input = host.querySelector(`#${id}`)! as HTMLInputElement | HTMLTextAreaElement; await act(async () => { Object.getOwnPropertyDescriptor(input.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype, 'value')!.set!.call(input, value); input.dispatchEvent(new Event('input', { bubbles: true })); }); }
async function render(profiles: ipc.EndpointProfile[] = []) { vi.mocked(ipc.readEndpointSettings).mockResolvedValue({ revision: '8', profiles }); await act(async () => root.render(<EndpointSettings />)); }
beforeEach(() => { Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); vi.resetAllMocks(); host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
describe('API connection settings', () => {
  it('lets the author explicitly remove an unavailable saved credential reference', async () => {
    await render([{ ...profile, hasApiKey: false }]);
    expect(host.textContent).toContain('Saved key unavailable');
    await click('Edit connection My local service');
    await act(async () => (host.querySelectorAll('input[type=checkbox]')[0] as HTMLInputElement).click());
    vi.mocked(ipc.saveEndpointSettings).mockResolvedValue({ revision: '9', profiles: [{ ...profile, hasApiKey: false, apiKeyConfigured: false }] });
    await click('Save connection');
    expect(vi.mocked(ipc.saveEndpointSettings).mock.calls[0][0].apiKey).toEqual({ kind: 'remove' });
    expect(host.textContent).toContain('No API key');
  });
  it('opens saved settings without probing endpoints or reading keys into inputs', async () => {
    await render([profile]); expect(ipc.discoverEndpointModels).not.toHaveBeenCalled(); expect(ipc.saveEndpointSettings).not.toHaveBeenCalled();
    expect(host.textContent).toContain('API key saved'); await click('Edit connection My local service');
    expect((host.querySelector('#endpoint-key') as HTMLInputElement).value).toBe('');
    expect(document.activeElement?.id).toBe('endpoint-name');
  });
  it('saves a URL, exact key and unique manual IDs without silently enabling JSON mode', async () => {
    await render(); await click('Add API connection'); await fill('endpoint-name', 'Local service'); await fill('endpoint-url', 'http://localhost:1234/custom'); await fill('endpoint-models', 'fiction-v1\nfiction-v1\nsecond-model'); await fill('endpoint-key', 'synthetic-ui-key');
    vi.mocked(ipc.saveEndpointSettings).mockResolvedValue({ revision: '9', profiles: [profile] });
    await click('Save connection'); expect(ipc.saveEndpointSettings).toHaveBeenCalledExactlyOnceWith({ expectedRevision: '8', profileId: null, label: 'Local service', baseUrl: 'http://localhost:1234/custom', enabled: true, jsonMode: false, manualModelIds: ['fiction-v1', 'second-model'], apiKey: { kind: 'replace', value: 'synthetic-ui-key' } });
    expect(host.querySelector('#endpoint-key')).toBeNull(); expect(refresh).toHaveBeenCalledOnce(); expect(host.textContent).not.toContain('synthetic-ui-key');
  });
  it('keeps saved keys by default and requires an explicit removal choice', async () => {
    await render([profile]); await click('Edit connection My local service');
    vi.mocked(ipc.saveEndpointSettings).mockResolvedValue({ revision: '9', profiles: [profile] });
    await click('Save connection'); expect(vi.mocked(ipc.saveEndpointSettings).mock.calls[0][0].apiKey).toEqual({ kind: 'keep' });
    await click('Edit connection My local service'); await act(async () => (host.querySelectorAll('input[type=checkbox]')[0] as HTMLInputElement).click()); await click('Save connection');
    expect(vi.mocked(ipc.saveEndpointSettings).mock.calls[1][0].apiKey).toEqual({ kind: 'remove' });
  });
  it('reconciles a lost save acknowledgement without repeating a secret write', async () => {
    await render(); await click('Add API connection'); await fill('endpoint-name', 'New endpoint'); await fill('endpoint-url', 'http://localhost:1234/v1'); await fill('endpoint-key', 'synthetic-once');
    vi.mocked(ipc.saveEndpointSettings).mockRejectedValue({ detail: 'Save acknowledgement lost.' }); vi.mocked(ipc.readEndpointSettings).mockResolvedValue({ revision: '9', profiles: [profile] });
    await click('Save connection'); expect(ipc.saveEndpointSettings).toHaveBeenCalledOnce(); expect(ipc.readEndpointSettings).toHaveBeenCalledTimes(2); expect((host.querySelector('#endpoint-key') as HTMLInputElement).value).toBe(''); expect(host.querySelector('[role=alert]')?.textContent).toContain('acknowledgement lost');
  });
  it('discovers only on request and fences the model refresh to the displayed configuration', async () => {
    await render([profile]); vi.mocked(ipc.discoverEndpointModels).mockResolvedValue({ revision: '9', profiles: [profile] });
    await click('Find models for My local service'); expect(ipc.discoverEndpointModels).toHaveBeenCalledExactlyOnceWith(profile.id, '4', expect.any(String)); expect(refresh).toHaveBeenCalledOnce();
  });
  it('stops only the current discovery and leaves the saved catalog intact', async () => {
    await render([profile]);
    let reject!: (reason: unknown) => void;
    vi.mocked(ipc.discoverEndpointModels).mockImplementation(() => new Promise((_resolve, fail) => { reject = fail; }));
    vi.mocked(ipc.cancelEndpointDiscovery).mockResolvedValue();
    await click('Find models for My local service');
    const id = vi.mocked(ipc.discoverEndpointModels).mock.calls[0][2];
    await click('Stop model search'); expect(ipc.cancelEndpointDiscovery).toHaveBeenCalledExactlyOnceWith(id);
    await act(async () => reject({ code: 'DiscoveryCancelled', detail: 'Model search stopped.' }));
    expect(host.textContent).toContain('Model search stopped. Your saved model list is unchanged.'); expect(host.querySelector('[role=alert]')).toBeNull(); expect(host.textContent).toContain('My local service');
    expect(refresh).not.toHaveBeenCalled(); expect(ipc.saveEndpointSettings).not.toHaveBeenCalled();
  });
  it('retains manual-model setup when discovery is unsupported', async () => {
    await render([profile]); vi.mocked(ipc.discoverEndpointModels).mockRejectedValue({ detail: 'Model discovery is unavailable.' });
    await click('Find models for My local service'); expect(host.querySelector('[role=alert]')?.textContent).toContain('enter a model ID'); expect(host.textContent).toContain('My local service'); expect(ipc.saveEndpointSettings).not.toHaveBeenCalled();
  });
});
