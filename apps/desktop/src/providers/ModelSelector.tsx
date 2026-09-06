import { useEffect, useMemo, useRef, useState } from 'react';
import { sameModel, type ModelDescriptor, type ModelSelection } from '../ipc/providers';
import { useProviders } from './ProviderContext';
import { ModelTraits } from './ModelTraits';
import './providers.css';

type BrowseFilter = { kind: 'all' } | { kind: 'favorites' } | { kind: 'provider'; providerId: string };

function matches(label: string, query: string): boolean {
  const needle = query.toLocaleLowerCase().replace(/\s+/g, '');
  if (!needle) return true;
  let index = 0;
  for (const character of label.toLocaleLowerCase()) if (character === needle[index]) index += 1;
  return index === needle.length;
}

function initialChoice(model: ModelDescriptor): ModelSelection {
  const luna = model.key.providerId === 'codex' && model.key.modelId === 'gpt-5.6-luna';
  return {
    ...model.key,
    reasoning: luna && model.reasoningLevels.includes('xhigh')
      ? 'xhigh'
      : model.defaultReasoning && model.reasoningLevels.includes(model.defaultReasoning)
        ? model.defaultReasoning
      : model.reasoningLevels.includes('medium')
        ? 'medium'
        : model.reasoningLevels[0] ?? null,
    serviceTier: luna && model.serviceTiers.some(tier => tier.id === 'priority')
      ? 'priority'
      : model.defaultServiceTier && model.serviceTiers.some(tier => tier.id === model.defaultServiceTier)
        ? model.defaultServiceTier : null,
  };
}

function providerGroups(models: ModelDescriptor[]): Array<{ providerId: string; label: string }> {
  const groups = new Map<string, string>();
  for (const model of models) if (!groups.has(model.key.providerId)) groups.set(model.key.providerId, model.providerLabel);
  return [...groups.entries()].map(([providerId, label]) => ({ providerId, label }));
}

export function ModelSelector() {
  const providers = useProviders();
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const selected = providers.state?.catalog.models.find(model => sameModel(model.key, providers.state!.settings.active));
  const activeLabel = selected?.label ?? (providers.busy ? 'Loading model…' : 'Model unavailable');
  return <div className="model-controls">
    <div className="model-selector">
      <button ref={button} type="button" className="model-trigger" aria-label={`Choose model: ${activeLabel}`} aria-haspopup="dialog" onClick={() => setOpen(true)}>
        <span>{activeLabel}</span><span className="model-trigger-chevron" aria-hidden="true" />
      </button>
      {open && <ModelPicker onClose={() => { setOpen(false); button.current?.focus(); }} />}
    </div>
    <ModelTraits />
  </div>;
}

function ModelPicker({ onClose }: { onClose(): void }) {
  const { state, busy, error, save, refresh } = useProviders();
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<BrowseFilter>({ kind: 'all' });
  const dialog = useRef<HTMLDialogElement>(null);
  const search = useRef<HTMLInputElement>(null);
  const mounted = useRef(true);
  const models = state?.catalog.models ?? [];
  const groups = useMemo(() => providerGroups(models), [models]);
  const queryActive = query.trim().length > 0;
  const favorite = (model: ModelDescriptor) => state?.settings.favorites.some(key => sameModel(key, model.key)) ?? false;
  const visibleModels = useMemo(() => {
    // A nonempty query deliberately searches the whole catalog, even when a
    // provider rail item is selected, so provider choice never hides a match.
    const scoped = queryActive
      ? models
      : models.filter(model => {
        if (filter.kind === 'favorites') return favorite(model);
        if (filter.kind === 'provider') return model.key.providerId === filter.providerId;
        return true;
      });
    // Opaque connection UUIDs are identity, not searchable model names.
    return scoped.filter(model => matches(`${model.providerLabel} ${model.label} ${model.key.modelId}`, query));
  }, [filter, models, query, queryActive, state?.settings.favorites]);

  useEffect(() => {
    mounted.current = true;
    const element = dialog.current;
    if (!element) return;
    element.showModal();
    search.current?.focus();
    return () => { mounted.current = false; if (element.open) element.close(); };
  }, []);

  function close() { dialog.current?.close(); onClose(); }
  async function choose(model: ModelDescriptor) {
    if (!state) return;
    const selection = sameModel(model.key, state.settings.active) ? state.settings.active : initialChoice(model);
    if (await save(selection, state.settings.favorites) && mounted.current) close();
  }
  async function toggleFavorite(model: ModelDescriptor) {
    if (!state) return;
    const favorites = favorite(model)
      ? state.settings.favorites.filter(key => !sameModel(key, model.key))
      : [...state.settings.favorites, model.key];
    await save(state.settings.active, favorites);
  }
  function moveFocus(direction: 1 | -1) {
    const choices = [...dialog.current!.querySelectorAll<HTMLButtonElement>('.model-choice:not(:disabled)')];
    if (!choices.length) return;
    const index = choices.indexOf(document.activeElement as HTMLButtonElement);
    const next = index < 0 ? (direction === 1 ? 0 : choices.length - 1) : (index + direction + choices.length) % choices.length;
    choices[next].focus();
  }

  return <dialog ref={dialog} className="model-dialog model-picker-dialog" aria-labelledby="model-picker-title" onCancel={event => { event.preventDefault(); close(); }} onKeyDown={event => {
    if ((event.ctrlKey || event.metaKey) && /^[1-9]$/.test(event.key)) {
      event.preventDefault(); const model = visibleModels[Number(event.key) - 1]; if (model && !busy) void choose(model); return;
    }
    if (event.key === 'ArrowDown') { event.preventDefault(); moveFocus(1); }
    if (event.key === 'ArrowUp') { event.preventDefault(); moveFocus(-1); }
  }}>
    <div className="provider-dialog-heading"><div><h2 id="model-picker-title">Choose a model</h2><p className="provider-note">Choose the assistant for your next request.</p></div><button type="button" onClick={close} aria-label="Close model picker">Close</button></div>
    <div className="model-picker-layout">
      <nav className="model-picker-rail" aria-label="Browse model catalog">
        <span className="model-picker-rail-label">Browse</span>
        <button type="button" className="model-rail-item" aria-pressed={filter.kind === 'all' && !queryActive} onClick={() => { setFilter({ kind: 'all' }); setQuery(''); }}>All models</button>
        <button type="button" className="model-rail-item" aria-pressed={filter.kind === 'favorites' && !queryActive} onClick={() => { setFilter({ kind: 'favorites' }); setQuery(''); }}>Favorites</button>
        {groups.length > 0 && <span className="model-picker-rail-label">Providers</span>}
        {groups.map(group => <button key={group.providerId} type="button" className="model-rail-item" aria-pressed={filter.kind === 'provider' && filter.providerId === group.providerId && !queryActive} onClick={() => { setFilter({ kind: 'provider', providerId: group.providerId }); setQuery(''); }}>{group.label}</button>)}
      </nav>
      <div className="model-picker-main">
        <label className="sr-only" htmlFor="model-search">Search models across providers</label>
        <input ref={search} id="model-search" type="search" value={query} placeholder="Search models or providers" autoComplete="off" onChange={event => setQuery(event.target.value)} onKeyDown={event => {
          if (event.key === 'Enter' && visibleModels[0] && !busy) { event.preventDefault(); void choose(visibleModels[0]); }
        }} />
        <p className="model-picker-summary" role="status">{queryActive ? `Searching all providers · ${visibleModels.length} ${visibleModels.length === 1 ? 'model' : 'models'}` : `${visibleModels.length} ${visibleModels.length === 1 ? 'model' : 'models'}`}</p>
        {error && <p role="alert" className="provider-error">{error}</p>}
        {!state && <button type="button" disabled={busy} onClick={() => void refresh()}>Check model settings</button>}
        <ul className="model-options" aria-label="Models">
          {visibleModels.map((model, index) => <li key={`${model.key.providerId}/${model.key.modelId}`}>
            <button type="button" className="model-choice" disabled={busy} aria-pressed={sameModel(model.key, state?.settings.active ?? { providerId: '', modelId: '' })} onClick={() => void choose(model)}>
              <span className="model-label"><span>{model.label}</span><small>{model.providerLabel} · {model.statusDetail}</small></span>
              <span className="model-choice-index" aria-hidden="true">{index < 9 ? `Ctrl ${index + 1}` : ''}</span>
            </button>
            <button type="button" className="model-favorite" disabled={busy} aria-label={`${favorite(model) ? 'Unfavorite' : 'Favorite'} ${model.label}`} aria-pressed={favorite(model)} onClick={() => void toggleFavorite(model)}><svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="m12 3 2.8 5.7 6.3.9-4.55 4.45 1.07 6.28L12 17.36l-5.62 2.97 1.07-6.28L2.9 9.6l6.3-.9Z" fill={favorite(model) ? 'currentColor' : 'none'} stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" /></svg></button>
          </li>)}
        </ul>
        {state && visibleModels.length === 0 && <p className="provider-note">No models match this view. Search by a model name or provider.</p>}
        <p className="provider-note">Selecting a model saves it for new requests. Favorites change only the browse list.</p>
      </div>
    </div>
  </dialog>;
}
