import { useEffect, useRef, useState } from 'react';
import { sameModel, type ModelSelection } from '../ipc/providers';
import { useProviders } from './ProviderContext';

function traitLabel(value: string): string {
  return value === 'xhigh' ? 'Extra high' : value.charAt(0).toUpperCase() + value.slice(1);
}

export function ModelTraits() {
  const { state, busy, save } = useProviders();
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const active = state?.settings.active;
  const model = state?.catalog.models.find(candidate => active && sameModel(candidate.key, active));
  const reasoning = active?.reasoning
    ? (model?.reasoningLevels.includes(active.reasoning) ? traitLabel(active.reasoning) : `${traitLabel(active.reasoning)} (unavailable)`)
    : 'Provider default';
  const speed = active?.serviceTier
    ? (model?.serviceTiers.find(tier => tier.id === active.serviceTier)?.label ?? `${active.serviceTier} (unavailable)`)
    : 'Provider default';
  const hasReasoning = !!model?.reasoningLevels.length || !!active?.reasoning;
  const hasSpeed = !!model?.serviceTiers.length || !!active?.serviceTier;
  const summary = [active?.reasoning && traitLabel(active.reasoning), active?.serviceTier && (model?.serviceTiers.find(tier => tier.id === active.serviceTier)?.label ?? `${active.serviceTier} (unavailable)`)].filter(Boolean).join(' · ');

  function update(changes: Partial<Pick<ModelSelection, 'reasoning' | 'serviceTier'>>) {
    if (!state || !active || !model) return;
    void save({ ...active, ...changes }, state.settings.favorites);
  }

  return <div className="model-traits">
    {hasReasoning && <label className="traits-inline-field"><span>Reasoning effort</span><select id="model-traits-reasoning" aria-label="Reasoning effort" value={active?.reasoning ?? ''} disabled={busy || !state} onChange={event => update({ reasoning: event.target.value || null })}>
      {active?.reasoning && !model?.reasoningLevels.includes(active.reasoning) && <option value={active.reasoning} disabled>{traitLabel(active.reasoning)} (unavailable)</option>}
      <option value="">Provider default</option>{model?.reasoningLevels.map(level => <option key={level} value={level}>{traitLabel(level)}</option>)}
    </select></label>}
    {hasSpeed && <label className="traits-inline-field"><span>Service tier</span><select id="model-traits-service-tier" aria-label="Service tier" value={active?.serviceTier ?? ''} disabled={busy || !state} onChange={event => update({ serviceTier: event.target.value || null })}>
      {active?.serviceTier && !model?.serviceTiers.some(tier => tier.id === active.serviceTier) && <option value={active.serviceTier} disabled>{active.serviceTier} (unavailable)</option>}
      <option value="">Provider default</option>{model?.serviceTiers.map(tier => <option key={tier.id} value={tier.id}>{tier.label}</option>)}
    </select></label>}
    {state && model && (hasReasoning || hasSpeed) ? <button ref={button} type="button" className="traits-trigger" disabled={busy} aria-haspopup="dialog" aria-label={`Edit model traits${summary ? `: ${summary}` : ''}`} onClick={() => setOpen(true)}>Details</button> : null}
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
    if (!state || !active || !model) return;
    void save({ ...active, ...changes }, state.settings.favorites);
  }
  return <dialog ref={dialog} className="model-dialog model-traits-dialog" aria-labelledby="model-traits-title" onCancel={event => { event.preventDefault(); close(); }}>
    <div className="provider-dialog-heading"><div><h2 id="model-traits-title">Model traits</h2><p className="provider-note">These choices apply to new requests.</p></div><button type="button" onClick={close} aria-label="Close model traits">Close</button></div>
    {model && active ? <>
      <p className="provider-active-name">{model.label}<span>{model.providerLabel}</span></p>
      {((active.reasoning && !model.reasoningLevels.includes(active.reasoning)) || (active.serviceTier && !model.serviceTiers.some(tier=>tier.id===active.serviceTier))) && model.reasoningLevels.length > 0 && <button type="button" disabled={busy} onClick={()=>update({reasoning:model.defaultReasoning ?? model.reasoningLevels[0],serviceTier:model.defaultServiceTier ?? null})}>Use available traits</button>}
      {(!!model.reasoningLevels.length || !!active.reasoning) && <label className="provider-field" htmlFor="traits-reasoning"><span>Reasoning</span><select id="traits-reasoning" value={active.reasoning ?? ''} disabled={busy} onChange={event => update({ reasoning: event.target.value || null })}>
        {active.reasoning && !model.reasoningLevels.includes(active.reasoning) && <option value={active.reasoning} disabled>{traitLabel(active.reasoning)} (unavailable)</option>}
        <option value="">Provider default</option>{model.reasoningLevels.map(level => <option key={level} value={level}>{traitLabel(level)}</option>)}
      </select></label>}
      {(!!model.serviceTiers.length || !!active.serviceTier) && <label className="provider-field" htmlFor="traits-service-tier"><span>Response speed</span><select id="traits-service-tier" value={active.serviceTier ?? ''} disabled={busy} onChange={event => update({ serviceTier: event.target.value || null })}>
        {active.serviceTier && !model.serviceTiers.some(tier => tier.id === active.serviceTier) && <option value={active.serviceTier} disabled>{active.serviceTier} (unavailable)</option>}
        <option value="">Provider default</option>{model.serviceTiers.map(tier => <option key={tier.id} value={tier.id}>{tier.label}</option>)}
      </select></label>}
      {!model.reasoningLevels.length && !model.serviceTiers.length && <p className="provider-note">No additional controls are available for this model.</p>}
    </> : <p className="provider-note">Model traits are unavailable until saved model settings load.</p>}
  </dialog>;
}
