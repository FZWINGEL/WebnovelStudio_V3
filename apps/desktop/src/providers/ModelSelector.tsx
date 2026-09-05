import { useEffect, useRef, useState } from 'react';
import { sameModel, type ModelDescriptor, type ModelSelection } from '../ipc/providers';
import { useProviders } from './ProviderContext';
import './providers.css';

function matches(label: string, query: string): boolean {
  let index = 0; const needle = query.toLocaleLowerCase().replace(/\s+/g, '');
  for (const char of label.toLocaleLowerCase()) if (char === needle[index]) index++;
  return index === needle.length;
}
function initialChoice(model: ModelDescriptor): ModelSelection {
  const luna = model.key.providerId === 'codex' && model.key.modelId === 'gpt-5.6-luna';
  return { ...model.key, reasoning: luna && model.reasoningLevels.includes('max') ? 'max' : model.reasoningLevels.includes('medium') ? 'medium' : model.reasoningLevels[0] ?? null,
    serviceTier: luna && model.serviceTiers.some(tier => tier.id === 'priority') ? 'priority' : null };
}
export function ModelSelector() {
  const providers = useProviders(); const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const selected = providers.state?.catalog.models.find(model => sameModel(model.key, providers.state!.settings.active));
  return <div className="model-selector">
    <button ref={button} type="button" className="model-trigger" aria-label={`Choose model: ${selected?.label ?? 'unavailable'}`} aria-haspopup="dialog" onClick={() => setOpen(true)}>
      <span>{selected?.label ?? (providers.busy ? 'Loading model…' : 'Model unavailable')}</span><span aria-hidden="true">⌄</span>
    </button>
    {open && <ModelPicker onClose={() => { setOpen(false); button.current?.focus(); }} />}
  </div>;
}
function ModelPicker({ onClose }: { onClose(): void }) {
  const { state, busy, error, save, refresh } = useProviders();
  const [query, setQuery] = useState(''); const [favoritesOnly, setFavoritesOnly] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null); const search = useRef<HTMLInputElement>(null);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; const element = dialog.current!; element.showModal(); search.current?.focus(); return () => { mounted.current = false; element.close(); }; }, []);
  const favorite = (model: ModelDescriptor) => state?.settings.favorites.some(key => sameModel(key, model.key)) ?? false;
  const models = state?.catalog.models.filter(model => (!favoritesOnly || favorite(model)) && matches(`${model.providerLabel} ${model.label} ${model.key.modelId}`, query.trim())) ?? [];
  function close() { dialog.current?.close(); onClose(); }
  async function choose(model: ModelDescriptor) {
    if (!state) return;
    const selection = sameModel(model.key, state.settings.active) ? state.settings.active : initialChoice(model);
    if (await save(selection, state.settings.favorites) && mounted.current) close();
  }
  return <dialog ref={dialog} className="model-dialog" aria-labelledby="model-picker-title" onCancel={event => { event.preventDefault(); close(); }} onKeyDown={event => {
    if ((event.ctrlKey || event.metaKey) && /^[1-9]$/.test(event.key)) {
      event.preventDefault(); const model = models[Number(event.key) - 1]; if (model && !busy) void choose(model); return;
    }
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    const choices = [...dialog.current!.querySelectorAll<HTMLButtonElement>('.model-choice:not(:disabled)')];
    if (!choices.length) return;
    const index = choices.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === 'ArrowDown' ? (index + 1) % choices.length : (index <= 0 ? choices.length : index) - 1;
    event.preventDefault(); choices[next].focus();
  }}>
    <div className="provider-dialog-heading"><h2 id="model-picker-title">Choose a model</h2><button onClick={close} aria-label="Close model picker">Close</button></div>
    <p className="provider-note">Your choice applies to new requests. Writing stays available with every model.</p>
    <label className="sr-only" htmlFor="model-search">Search models</label>
    <input ref={search} id="model-search" type="search" value={query} placeholder="Search models or providers" onChange={event => setQuery(event.target.value)} onKeyDown={event => {
      if (event.key === 'Enter' && models[0] && !busy) { event.preventDefault(); void choose(models[0]); }
    }} />
    <button type="button" className="provider-filter" aria-pressed={favoritesOnly} onClick={() => setFavoritesOnly(value => !value)}>Favorites</button>
    {error && <p role="alert" className="provider-error">{error}</p>}
    {!state && <button disabled={busy} onClick={() => void refresh()}>Check model settings</button>}
    <ul className="model-options" aria-label="Models">
      {models.map(model => <li key={`${model.key.providerId}/${model.key.modelId}`}>
        <button className="model-choice" disabled={busy} aria-pressed={sameModel(model.key, state!.settings.active)} onClick={() => void choose(model)}>
          <span className="model-label">{model.label}<small>{model.providerLabel}{model.ready ? '' : ' · Not available yet'}</small></span>
          {sameModel(model.key, state!.settings.active) && <span aria-hidden="true">✓</span>}
        </button>
        <button className="model-favorite" disabled={busy} aria-label={`${favorite(model) ? 'Unfavorite' : 'Favorite'} ${model.label}`} aria-pressed={favorite(model)} onClick={() => {
          const favorites = favorite(model) ? state!.settings.favorites.filter(key => !sameModel(key, model.key)) : [...state!.settings.favorites, model.key];
          void save(state!.settings.active, favorites);
        }}>{favorite(model) ? '★' : '☆'}</button>
      </li>)}
    </ul>
    {state && models.length === 0 && <p className="provider-note">No models match this search.</p>}
    <p className="provider-note">Models marked unavailable keep your preference but cannot answer yet. The app never chooses a different model for you.</p>
  </dialog>;
}
