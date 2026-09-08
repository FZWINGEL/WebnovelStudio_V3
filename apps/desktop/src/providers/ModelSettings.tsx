import { isTauri } from '@tauri-apps/api/core';
import { useEffect, useRef, useState } from 'react';
import { readCodexTransport, sameModel, saveCodexTransport, type CodexTransport, type CodexTransportSettings } from '../ipc/providers';
import { useProviders } from './ProviderContext';
import './providers.css';
import { EndpointSettings } from './EndpointSettings';

function transportErrorDetail(reason: unknown): string {
  if (reason && typeof reason === 'object' && 'detail' in reason && typeof reason.detail === 'string') return reason.detail;
  return 'Codex transport settings could not be saved. Reload Settings before trying again.';
}

export function ModelSettings() {
  const [open, setOpen] = useState(false); const button = useRef<HTMLButtonElement>(null);
  return <><button ref={button} aria-haspopup="dialog" onClick={() => setOpen(true)}>Settings</button>
    {open && <SettingsDialog onClose={() => { setOpen(false); button.current?.focus(); }} />}</>;
}
function SettingsDialog({ onClose }: { onClose(): void }) {
  const { state, busy, error, refresh, checkConnection, checkClaudeConnection, save, saveStoryMemory } = useProviders(); const dialog = useRef<HTMLDialogElement>(null);
  const [codexTransport, setCodexTransport] = useState<CodexTransportSettings>({ revision: '0', transport: 'exec' });
  const [transportBusy, setTransportBusy] = useState(false); const [transportError, setTransportError] = useState('');
  useEffect(() => { const element = dialog.current!; element.showModal(); return () => element.close(); }, []);
  useEffect(() => {
    if (!isTauri) return;
    let mounted = true;
    void readCodexTransport().then(settings => { if (mounted) setCodexTransport(settings); }).catch(reason => { if (mounted) setTransportError(transportErrorDetail(reason)); });
    return () => { mounted = false; };
  }, []);
  const active = state?.settings.active;
  const model = state?.catalog.models.find(model => sameModel(model.key, active!));
  const codexModel = state?.catalog.models.find(model => model.key.providerId === 'codex' && model.key.modelId === 'gpt-5.6-luna');
  const codexReady = state?.codexConnection?.ready === true;
  const codexStatus = codexReady ? 'Connected' : busy ? 'Checking…' : state?.codexConnection?.checked ? 'Unavailable' : 'Not checked';
  const claudeModels = state?.catalog.models.filter(model => model.key.providerId === 'claude') ?? [];
  const claudeReady = state?.claudeConnection?.ready === true;
  const activeCodex = active?.providerId === 'codex';
  const activeClaude = active?.providerId === 'claude';
  const activeCodexLuna = !!active && !!codexModel && sameModel(active, codexModel.key);
  const exactCodexTraits = active?.reasoning === 'xhigh' && active.serviceTier === 'priority';
  const storyMemory = state?.storyMemory;
  const memoryEndpointOptions = state?.catalog.models
    .filter(item => item.key.providerId.startsWith('openai-compatible:'))
    .reduce<Array<{ providerId: string; label: string; available: boolean }>>((options, item) => {
      if (options.some(option => option.providerId === item.key.providerId)) return options;
      const astra = state.catalog.models.some(candidate => candidate.key.providerId === item.key.providerId && candidate.key.modelId === 'gpt-6-astra');
      options.push({ providerId: item.key.providerId, label: item.providerLabel, available: astra });
      return options;
    }, []) ?? [];
  if (storyMemory && storyMemory.providerId.startsWith('openai-compatible:') && !memoryEndpointOptions.some(option => option.providerId === storyMemory.providerId)) {
    memoryEndpointOptions.push({ providerId: storyMemory.providerId, label: storyMemory.providerLabel, available: false });
  }
  async function saveTransport(transport: CodexTransport) {
    if (transportBusy) return;
    setTransportBusy(true); setTransportError('');
    try { setCodexTransport(await saveCodexTransport(codexTransport.revision, transport)); await refresh(); }
    catch (reason) {
      setTransportError(transportErrorDetail(reason));
      try { setCodexTransport(await readCodexTransport()); await refresh(); } catch { /* Keep the visible choice until an explicit reload succeeds. */ }
    } finally { setTransportBusy(false); }
  }
  function close() { dialog.current?.close(); onClose(); }
  return <dialog ref={dialog} className="model-dialog settings-dialog" aria-labelledby="model-settings-title" onCancel={event => { event.preventDefault(); close(); }}>
    <div className="provider-dialog-heading"><h2 id="model-settings-title">Settings</h2><button onClick={close} aria-label="Close settings">Close</button></div>
    <div className="settings-content">
    <h3>Writing assistant</h3>
    {model && active && state ? <>
      <p className="provider-active-name">{model.label}<span>{model.providerLabel}</span></p>
      <p className="provider-note">{state.dispatch.detail || model.statusDetail}</p>
      {((active.reasoning && !model.reasoningLevels.includes(active.reasoning)) || (active.serviceTier && !model.serviceTiers.some(tier=>tier.id===active.serviceTier))) && model.reasoningLevels.length > 0 && <button type="button" disabled={busy} onClick={()=>void save({...active,reasoning:model.defaultReasoning ?? model.reasoningLevels[0],serviceTier:model.defaultServiceTier ?? null},state.settings.favorites)}>Use available traits</button>}
      {(!!model.reasoningLevels.length || !!active.reasoning) && <div className="provider-field"><label htmlFor="model-reasoning">Reasoning</label>
        <select id="model-reasoning" value={active.reasoning ?? ''} disabled={busy} onChange={event => void save({ ...active, reasoning: event.target.value || null }, state.settings.favorites)}>
          {active.reasoning && !model.reasoningLevels.includes(active.reasoning) && <option value={active.reasoning} disabled>{active.reasoning} (unavailable)</option>}
          <option value="">Provider default</option>{model.reasoningLevels.map(level => <option key={level} value={level}>{level === 'xhigh' ? 'Extra high' : level.charAt(0).toUpperCase() + level.slice(1)}</option>)}
        </select>
      </div>}
      {(!!model.serviceTiers.length || !!active.serviceTier) && <div className="provider-field"><label htmlFor="model-speed">Response speed</label>
        <select id="model-speed" value={active.serviceTier ?? ''} disabled={busy} onChange={event => void save({ ...active, serviceTier: event.target.value || null }, state.settings.favorites)}>
          {active.serviceTier && !model.serviceTiers.some(tier => tier.id === active.serviceTier) && <option value={active.serviceTier} disabled>{active.serviceTier} (unavailable)</option>}
          <option value="">Provider default</option>{model.serviceTiers.map(tier => <option key={tier.id} value={tier.id}>{tier.label}</option>)}
        </select>
      </div>}
      {model.origin === 'reference' && active.providerId === 'codex' && <p className="provider-note">These options come from the saved reference catalog. The connection check verifies the supported Codex installation and sign-in; model context and output limits remain unverified.</p>}
      {model.origin === 'reference' && active.providerId === 'claude' && <p className="provider-note">These options come from the saved reference catalog. The connection check verifies the installed Claude Code CLI and sign-in; model context and output limits remain unverified.</p>}
      {model.origin === 'codexDiscovery' && <p className="provider-note">This model was reported by the installed Codex CLI. Its traits are saved from that discovery; the connection check controls whether it can send.</p>}
      {(activeCodex || activeClaude) && state.dispatch.kind === 'blocked' && <p className="provider-note">{state.dispatch.detail}</p>}
      {activeCodexLuna && codexReady && !exactCodexTraits && model.reasoningLevels.includes('xhigh') && model.serviceTiers.some(tier=>tier.id==='priority') && <button type="button" disabled={busy} onClick={() => void save({ ...active, reasoning: 'xhigh', serviceTier: 'priority' }, state.settings.favorites)}>Use Extra high reasoning + Fast response speed</button>}
      <p className="provider-note">Changes are saved on this computer and apply to new requests. Your current response keeps its original model. You can continue writing without an assistant.</p>
    </> : <p className="provider-note">{busy ? 'Loading model settings…' : 'Model settings could not be loaded.'}</p>}
    <section className="provider-connection" aria-labelledby="story-memory-provider-title">
      <h3 id="story-memory-provider-title">Story memory and summaries</h3>
      {storyMemory ? <>
        <p className="provider-active-name">{storyMemory.providerLabel}<span>{storyMemory.modelId === 'local-editorial-v1' ? 'Local test model' : `${storyMemory.modelId} · ${storyMemory.reasoning === 'xhigh' ? 'Extra high' : storyMemory.reasoning ?? 'Provider default'}`}</span></p>
        <p className="provider-note" role="status" aria-live="polite">{storyMemory.detail}</p>
        <div className="provider-field"><label htmlFor="story-memory-provider">Maintenance provider</label>
          <select id="story-memory-provider" value={storyMemory.providerId} disabled={busy} onChange={event => void saveStoryMemory(event.target.value)}>
            <option value="codex">Codex · GPT-6 Astra · Low · Fast</option>
            <option value="mock">Local test model</option>
            {memoryEndpointOptions.map(option => <option key={option.providerId} value={option.providerId} disabled={!option.available}>{option.label} · {option.available ? 'GPT-6 Astra · Low' : 'GPT-6 Astra unavailable'}</option>)}
          </select>
        </div>
        <p className="provider-note">This provider is independent of the writing assistant picker. New live maintenance requests use GPT-6 Astra with Low reasoning; an API connection must list gpt-6-astra.</p>
      </> : <p className="provider-note">Story-memory provider state is unavailable. Reload Settings before refreshing story memory.</p>}
    </section>
    {state && codexModel && <section className={`provider-connection ${codexReady ? 'is-ready' : 'is-unavailable'}`} aria-labelledby="codex-connection-title">
      <div className="provider-connection-heading"><h3 id="codex-connection-title">Codex connection</h3><span className="provider-connection-status">{codexStatus}</span></div>
      <p className="provider-note" role="status" aria-live="polite">{state.codexConnection?.detail ?? 'Check this computer for the installed Codex sign-in.'}</p>
      <button type="button" disabled={busy} onClick={() => void checkConnection()}>Check Codex connection</button>
    </section>}
    {state && codexModel && <section className="provider-connection" aria-labelledby="codex-transport-title">
      <h3 id="codex-transport-title">Codex transport</h3>
      <div className="provider-field"><label htmlFor="codex-transport">Request transport</label>
        <select id="codex-transport" value={codexTransport.transport} disabled={busy || transportBusy} onChange={event => void saveTransport(event.target.value as CodexTransport)}>
          <option value="exec">Codex exec · Default</option>
          <option value="appServer">Codex app-server · Development / qualification</option>
        </select>
      </div>
      <p className="provider-note">Exec is the current qualification baseline. App-server is an experimental development transport and may require rechecking Settings after it changes.</p>
      <p className="provider-note">Changing this preference only saves the transport for future requests. It does not start, stop, retry, or reroute an active request.</p>
      {transportError && <p role="alert" className="provider-error">{transportError}</p>}
    </section>}
    {state && claudeModels.length > 0 && <section className={`provider-connection ${claudeReady ? 'is-ready' : 'is-unavailable'}`} aria-labelledby="claude-connection-title">
      <div className="provider-connection-heading"><h3 id="claude-connection-title">Claude Code connection</h3><span className="provider-connection-status">{claudeReady ? 'Connected' : 'Not checked'}</span></div>
      <p className="provider-note" role="status" aria-live="polite">{state.claudeConnection?.detail ?? 'Check this computer for the installed Claude Code sign-in.'}</p>
      <button type="button" disabled={busy} onClick={() => void checkClaudeConnection()}>Check Claude connection</button>
    </section>}
    <EndpointSettings />
    {error && <p role="alert" className="provider-error">{error}</p>}
    <button type="button" disabled={busy} onClick={() => void refresh()}>Reload saved settings</button>
    <p className="provider-note">Choose a model from the selector in the app header.</p>
    </div>
  </dialog>;
}
