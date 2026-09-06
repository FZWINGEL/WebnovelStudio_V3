import { useEffect, useRef, useState } from 'react';
import { sameModel } from '../ipc/providers';
import { useProviders } from './ProviderContext';

function traitLabel(value: string): string {
  return value === 'xhigh' ? 'Extra high' : value.charAt(0).toUpperCase() + value.slice(1);
}

export function ModelTraits() {
  const { state } = useProviders();
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const active = state?.settings.active;
  const model = state?.catalog.models.find(candidate => active && sameModel(candidate.key, active));
  const summary = [
    active?.reasoning && traitLabel(active.reasoning),
    active?.serviceTier && model?.serviceTiers.find(tier => tier.id === active.serviceTier)?.label,
  ].filter(Boolean).join(' · ');

  return <div className="model-traits">
    <button ref={button} type="button" className="traits-trigger" disabled={!state} aria-haspopup="dialog" aria-label={`Edit model traits${summary ? `: ${summary}` : ''}`} onClick={() => setOpen(true)}>
      <span>Traits</span>{summary && <small>{summary}</small>}
    </button>
    {open && <TraitsDialog onClose={() => { setOpen(false); button.current?.focus(); }} />}
  </div>;
}

function TraitsDialog({ onClose }: { onClose(): void }) {
  const { state, busy, save } = useProviders();
  const dialog = useRef<HTMLDialogElement>(null);
  const active = state?.settings.active;
  const model = state?.catalog.models.find(candidate => active && sameModel(candidate.key, active));
  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    element.showModal();
    return () => { if (element.open) element.close(); };
  }, []);
  function close() { dialog.current?.close(); onClose(); }
  function update(changes: Partial<Pick<NonNullable<typeof active>, 'reasoning' | 'serviceTier'>>) {
    if (state && active) void save({ ...active, ...changes }, state.settings.favorites);
  }
  return <dialog ref={dialog} className="model-dialog model-traits-dialog" aria-labelledby="model-traits-title" onCancel={event => { event.preventDefault(); close(); }}>
    <div className="provider-dialog-heading"><div><h2 id="model-traits-title">Model traits</h2><p className="provider-note">These choices apply to new requests.</p></div><button type="button" onClick={close} aria-label="Close model traits">Close</button></div>
    {model && active ? <>
      <p className="provider-active-name">{model.label}<span>{model.providerLabel}</span></p>
      {!!model.reasoningLevels.length && <label className="provider-field" htmlFor="traits-reasoning"><span>Reasoning</span><select id="traits-reasoning" value={active.reasoning ?? ''} disabled={busy} onChange={event => update({ reasoning: event.target.value || null })}>
        <option value="">Provider default</option>{model.reasoningLevels.map(level => <option key={level} value={level}>{traitLabel(level)}</option>)}
      </select></label>}
      {!!model.serviceTiers.length && <label className="provider-field" htmlFor="traits-service-tier"><span>Response speed</span><select id="traits-service-tier" value={active.serviceTier ?? ''} disabled={busy} onChange={event => update({ serviceTier: event.target.value || null })}>
        <option value="">Provider default</option>{model.serviceTiers.map(tier => <option key={tier.id} value={tier.id}>{tier.label}</option>)}
      </select></label>}
      {!model.reasoningLevels.length && !model.serviceTiers.length && <p className="provider-note">No additional controls are available for this model.</p>}
    </> : <p className="provider-note">Model traits are unavailable until saved model settings load.</p>}
  </dialog>;
}
