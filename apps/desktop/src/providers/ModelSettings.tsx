import { useEffect, useRef, useState } from 'react';
import { sameModel } from '../ipc/providers';
import { useProviders } from './ProviderContext';
import './providers.css';
import { EndpointSettings } from './EndpointSettings';

export function ModelSettings() {
  const [open, setOpen] = useState(false); const button = useRef<HTMLButtonElement>(null);
  return <><button ref={button} aria-haspopup="dialog" onClick={() => setOpen(true)}>Settings</button>
    {open && <SettingsDialog onClose={() => { setOpen(false); button.current?.focus(); }} />}</>;
}
function SettingsDialog({ onClose }: { onClose(): void }) {
  const { state, busy, error, refresh, checkConnection, save } = useProviders(); const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => { const element = dialog.current!; element.showModal(); return () => element.close(); }, []);
  const active = state?.settings.active;
  const model = state?.catalog.models.find(model => sameModel(model.key, active!));
  const codexModel = state?.catalog.models.find(model => model.key.providerId === 'codex' && model.key.modelId === 'gpt-5.6-luna');
  const codexReady = state?.codexConnection?.ready === true;
  const activeCodex = active?.providerId === 'codex';
  const activeCodexLuna = !!active && !!codexModel && sameModel(active, codexModel.key);
  const exactCodexTraits = active?.reasoning === 'xhigh' && active.serviceTier === 'priority';
  function close() { dialog.current?.close(); onClose(); }
  return <dialog ref={dialog} className="model-dialog settings-dialog" aria-labelledby="model-settings-title" onCancel={event => { event.preventDefault(); close(); }}>
    <div className="provider-dialog-heading"><h2 id="model-settings-title">Settings</h2><button onClick={close} aria-label="Close settings">Close</button></div>
    <div className="settings-content">
    <h3>Writing assistant</h3>
    {model && active && state ? <>
      <p className="provider-active-name">{model.label}<span>{model.providerLabel}</span></p>
      <p className="provider-note">{state.dispatch.detail || model.statusDetail}</p>
      {!!model.reasoningLevels.length && <div className="provider-field"><label htmlFor="model-reasoning">Reasoning</label>
        <select id="model-reasoning" value={active.reasoning ?? ''} disabled={busy} onChange={event => void save({ ...active, reasoning: event.target.value || null }, state.settings.favorites)}>
          <option value="">Provider default</option>{model.reasoningLevels.map(level => <option key={level} value={level}>{level === 'xhigh' ? 'Extra high' : level.charAt(0).toUpperCase() + level.slice(1)}</option>)}
        </select>
      </div>}
      {!!model.serviceTiers.length && <div className="provider-field"><label htmlFor="model-speed">Response speed</label>
        <select id="model-speed" value={active.serviceTier ?? ''} disabled={busy} onChange={event => void save({ ...active, serviceTier: event.target.value || null }, state.settings.favorites)}>
          <option value="">Provider default</option>{model.serviceTiers.map(tier => <option key={tier.id} value={tier.id}>{tier.label}</option>)}
        </select>
      </div>}
      {model.origin === 'reference' && <p className="provider-note">These options come from the saved reference catalog. The connection check verifies the supported Codex installation and sign-in; model context and output limits remain unverified.</p>}
      {activeCodex && state.dispatch.kind === 'blocked' && <p className="provider-note">{state.dispatch.detail}</p>}
      {activeCodexLuna && codexReady && !exactCodexTraits && <button type="button" disabled={busy} onClick={() => void save({ ...active, reasoning: 'xhigh', serviceTier: 'priority' }, state.settings.favorites)}>Use Extra high reasoning + Fast response speed</button>}
      <p className="provider-note">Changes are saved on this computer and apply to new requests. Your current response keeps its original model. You can continue writing without an assistant.</p>
    </> : <p className="provider-note">{busy ? 'Loading model settings…' : 'Model settings could not be loaded.'}</p>}
    {state && codexModel && <section className={`provider-connection ${codexReady ? 'is-ready' : 'is-unavailable'}`} aria-labelledby="codex-connection-title">
      <div className="provider-connection-heading"><h3 id="codex-connection-title">Codex connection</h3><span className="provider-connection-status">{codexReady ? 'Connected' : 'Not checked'}</span></div>
      <p className="provider-note" role="status" aria-live="polite">{state.codexConnection?.detail ?? 'Check this computer for the installed Codex sign-in.'}</p>
      <button type="button" disabled={busy} onClick={() => void checkConnection()}>Check Codex connection</button>
    </section>}
    <EndpointSettings />
    {error && <p role="alert" className="provider-error">{error}</p>}
    <button type="button" disabled={busy} onClick={() => void refresh()}>Reload saved settings</button>
    <p className="provider-note">Choose a model from the selector in the app header.</p>
    </div>
  </dialog>;
}
