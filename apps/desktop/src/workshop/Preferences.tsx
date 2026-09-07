import { useState } from 'react';
import { exportWorkshopPreset, importWorkshopPreset, type WorkshopPreference, type WorkshopPreset, type WorkshopSession, type WorkshopState } from '../ipc/workshop';
import { describeWorkshopError } from './store';
import { FAMILIES, PREFERENCE_SUGGESTIONS, PRESET_SUGGESTIONS, type PreferenceSuggestion } from './catalog';

const PRESET_SCHEMA_VERSION = 'workshop-preset.v1' as const;

function normalizePresetName(value: unknown, fallback?: string): string {
  const candidate = value === undefined ? fallback : value;
  if (typeof candidate !== 'string' || !candidate.trim()) throw new Error('A preset name is required.');
  const normalized = candidate.trim();
  if (normalized.length > 160) throw new Error('A preset name is too long.');
  return normalized;
}

function serializePreset(name: string, preferences: WorkshopPreference[]) {
  return JSON.stringify({ schemaVersion: PRESET_SCHEMA_VERSION, name, preferences }, null, 2);
}

export function preferenceConflict(preferences: WorkshopPreference[], incoming: WorkshopPreference): string | null {
  if (!incoming.confirmed || incoming.polarity === 'neutral') return null;
  const normalizedLabel = incoming.label.trim().toLowerCase();
  const incomingIsHardProject = incoming.scope === 'project' && incoming.strength === 'hard';
  const conflicting = preferences.find(item => item.id !== incoming.id && item.confirmed && item.polarity !== 'neutral' && item.polarity !== incoming.polarity && item.label.trim().toLowerCase() === normalizedLabel && (incomingIsHardProject || item.scope === 'project' && item.strength === 'hard'));
  if (!conflicting) return null;
  if (conflicting.scope === 'project' && conflicting.strength === 'hard') return `This conflicts with the project’s ${conflicting.polarity === 'avoid' ? 'Never' : 'Must'} preference for “${conflicting.label}”. Edit that project preference explicitly before changing its local meaning.`;
  const incomingDirection = incoming.polarity === 'avoid' ? 'Never' : 'Must';
  const currentDirection = conflicting.polarity === 'avoid' ? 'Avoid' : 'Want';
  return `Your ${incomingDirection} preference for “${incoming.label}” conflicts with the current ${conflicting.scope} ${currentDirection} preference for “${conflicting.label}”. Edit either preference before saving.`;
}
export function applicablePreferences(state: WorkshopState, session: WorkshopSession) {
  return state.preferences.filter(preference => preference.confirmed && preference.polarity !== 'neutral' && (preference.scope === 'project' || preference.scope === 'element' && preference.targetId === session.focusDocumentId || preference.scope === 'exploration' && preference.targetId === session.id));
}
export function preferenceLabel(preference: WorkshopPreference) { return preference.polarity === 'neutral' ? 'Unspecified' : preference.strength === 'hard' ? preference.polarity === 'want' ? 'Must' : 'Never' : preference.polarity === 'want' ? 'Want' : 'Avoid'; }

export function Preferences({ state, session, onChange }: { state: WorkshopState; session: WorkshopSession; onChange(change: (state: WorkshopState) => WorkshopState): void }) {
  const [browse, setBrowse] = useState(false); const [query, setQuery] = useState('');
  const [editing, setEditing] = useState<WorkshopPreference | null>(null); const [error, setError] = useState('');
  const [presetText, setPresetText] = useState(''); const [presetOpen, setPresetOpen] = useState(false); const [presetName, setPresetName] = useState('My starting preferences');
  const [presetNotice, setPresetNotice] = useState(''); const [reviewedPresetId, setReviewedPresetId] = useState<string | null>(null);
  const applicable = state.preferences.filter(item => item.scope === 'project' || item.scope === 'element' && item.targetId === session.focusDocumentId || item.scope === 'exploration' && item.targetId === session.id);
  const rejectedReasons = session.choices.filter(choice => choice.status === 'rejected' && choice.rationale.trim());
  const suggestions = PREFERENCE_SUGGESTIONS.filter(item => (!query || `${item.label} ${item.meaning} ${item.family}`.toLocaleLowerCase().includes(query.toLocaleLowerCase())) && (browse || item.lenses.includes(session.lens))).slice(0, browse ? 30 : 3);
  function draft(suggestion?: PreferenceSuggestion): WorkshopPreference {
    return { id: crypto.randomUUID(), label: suggestion?.label ?? '', family: suggestion?.family ?? FAMILIES[0], meaning: suggestion?.meaning ?? '', examples: '', timing: '', polarity: 'want', strength: 'soft', scope: 'exploration', targetId: session.id, confirmed: true };
  }
  function save(preference: WorkshopPreference) {
    const conflict = preferenceConflict(state.preferences, preference);
    if (conflict) { setError(conflict); return; }
    onChange(current => ({ ...current, preferences: [...current.preferences.filter(item => item.id !== preference.id), preference] })); setEditing(null); setError('');
  }
  function openPresetReview(preset: Pick<WorkshopPreset, 'name' | 'preferences'>, notice = '', definitionId: string | null = null) {
    try {
      const name = normalizePresetName(preset.name);
      setPresetName(name); setPresetText(serializePreset(name, preset.preferences)); setPresetOpen(true); setReviewedPresetId(definitionId); setPresetNotice(notice); setError('');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Could not read this preset.');
    }
  }
  function savePresetFile(preset: WorkshopPreset) {
    try {
      const normalized = { ...preset, name: normalizePresetName(preset.name) };
      setError('');
      void exportWorkshopPreset(normalized).then(path => { if (path) setPresetNotice(`Preset saved to ${path}`); }).catch(reason => setError(describeWorkshopError(reason)));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'A preset name is required.');
    }
  }
  function updatePresetText(value: string) {
    setPresetText(value);
    try {
      const parsed: unknown = JSON.parse(value);
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return;
      const name = (parsed as Record<string, unknown>).name;
      if (typeof name === 'string') setPresetName(name);
    } catch {
      // Keep malformed JSON visible so the author can repair it before saving.
    }
  }
  function updatePresetName(value: string) {
    setPresetName(value);
    try {
      const parsed: unknown = JSON.parse(presetText);
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return;
      setPresetText(JSON.stringify({ ...(parsed as Record<string, unknown>), name: value }, null, 2));
    } catch {
      // Keep malformed JSON untouched; readPreset will refuse it until repaired.
    }
  }
  function readPreset() {
    try {
      const parsed: unknown = JSON.parse(presetText);
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error('Use a Workshop preset with at most 100 preferences.');
      const source = parsed as Record<string, unknown>;
      if (source.schemaVersion !== PRESET_SCHEMA_VERSION || !Array.isArray(source.preferences) || source.preferences.length > 100) throw new Error('Use a Workshop preset with at most 100 preferences.');
      const name = normalizePresetName(source.name, presetName);
      const preferences = source.preferences.map((input: unknown): WorkshopPreference => {
        if (!input || typeof input !== 'object' || Array.isArray(input)) throw new Error('Each preference must be an object with a label and meaning.');
        const item = input as Record<string, unknown>;
        const field = (name: string, limit: number, optional = false) => {
          if (optional && item[name] == null) return '';
          if (typeof item[name] !== 'string' || item[name].length > limit) throw new Error(`A preference has an invalid ${name}.`);
          return item[name];
        };
        const label = field('label', 160); const meaning = field('meaning', 4000);
        if (!label.trim() || !['neutral', 'want', 'avoid'].includes(String(item.polarity)) || !['soft', 'hard'].includes(String(item.strength))) throw new Error('A preference is missing its label, meaning, polarity, or strength.');
        return { id: crypto.randomUUID(), label, meaning, family: field('family', 160, true), examples: field('examples', 4000, true), timing: field('timing', 1000, true), polarity: item.polarity as WorkshopPreference['polarity'], strength: item.strength as WorkshopPreference['strength'], scope: 'project', targetId: null, confirmed: true };
      });
      return { name, preferences };
    } catch (reason) {
      throw reason instanceof Error ? reason : new Error('Could not read this preset.');
    }
  }
  function importPreset() {
    try {
      const { name, preferences } = readPreset();
      for (const preference of preferences) { const conflict = preferenceConflict([...state.preferences, ...preferences], preference); if (conflict) throw new Error(conflict); }
      onChange(current => {
        const savedPreset = { id: reviewedPresetId ?? crypto.randomUUID(), name, preferences };
        const hasReviewedPreset = reviewedPresetId !== null && current.presets.some(preset => preset.id === reviewedPresetId);
        return { ...current, preferences: [...current.preferences, ...preferences], presets: hasReviewedPreset ? current.presets.map(preset => preset.id === reviewedPresetId ? savedPreset : preset) : [...current.presets, savedPreset] };
      });
      setPresetName(name);
      setPresetNotice(preferences.length ? `${preferences.length} project preference${preferences.length === 1 ? '' : 's'} added.` : 'Preset saved with no preferences to add.');
      setPresetOpen(false); setReviewedPresetId(null); setError('');
    } catch (reason) { setError(reason instanceof Error ? reason.message : 'Could not read this preset.'); }
  }
  function savePresetDefinition() {
    if (!reviewedPresetId) return;
    try {
      const { name, preferences } = readPreset();
      onChange(current => ({ ...current, presets: current.presets.map(preset => preset.id === reviewedPresetId ? { ...preset, name, preferences } : preset) }));
      setPresetName(name); setPresetOpen(false); setReviewedPresetId(null); setPresetNotice('Saved preset definition updated. No project preferences were added.'); setError('');
    } catch (reason) { setError(reason instanceof Error ? reason.message : 'Could not save this preset definition.'); }
  }
  const projectPreferences = state.preferences.filter(item => item.scope === 'project');
  return <section className="workshop-preferences" aria-label="Creative preferences">
    <div className="workshop-section-heading"><h3>Creative preferences</h3><button onClick={() => { setEditing(draft()); setError(''); }}>Add preference</button></div>
    <p className="small-copy">Unspecified stays open. Your wording is the instruction.</p>
    {session.lens === 'themes' && <div className="workshop-actions">
      <button onClick={() => setEditing({ ...draft(), label: 'Reader experience', family: 'Reader experience', meaning: '', polarity: 'neutral' })}>Choose reader experience</button>
      <button onClick={() => setEditing({ ...draft(), label: 'Content intensity', family: 'Content boundaries', meaning: '', polarity: 'neutral' })}>Choose content intensity</button>
      <p className="small-copy">Warmth and hope can coexist with danger. Describe the feeling separately from limits on explicit detail or distress.</p>
    </div>}
    {applicable.map(preference => <button className="workshop-preference" key={preference.id} onClick={() => { setEditing({ ...preference }); setError(''); }}><strong>{preferenceLabel(preference)}</strong> {preference.label}<small>{preference.scope === 'project' ? 'Project' : preference.scope === 'element' ? 'This element' : 'This exploration'}</small></button>)}
    {!applicable.length && <p className="small-copy">No preferences chosen. Any genre or direction is still possible.</p>}
    {!!rejectedReasons.length && <details><summary>Turn a rejection reason into a preference</summary>
      <p className="small-copy">These reasons belong to individual choices. Edit the instruction and choose its scope before keeping it as a preference.</p>
      {rejectedReasons.map(choice => <div key={choice.candidateId}><blockquote>{choice.rationale}</blockquote><button onClick={() => {
        setEditing({ ...draft(), meaning: choice.rationale, polarity: 'neutral' }); setError('');
      }}>Review as a preference</button></div>)}
    </details>}
    <details><summary>Find a preference or preset</summary>
      <label>Search preferences<input type="search" value={query} onChange={event => { setQuery(event.target.value); setBrowse(true); }} /></label>
      <div className="workshop-suggestions">{suggestions.map(suggestion => <button key={suggestion.label} onClick={() => setEditing(draft(suggestion))}>{suggestion.label}</button>)}</div>
      <button onClick={() => setBrowse(!browse)}>{browse ? 'Show suggestions for this lens' : 'Browse all'}</button>
      <label>Optional starting vocabulary<select defaultValue="" onChange={event => {
        const preset = PRESET_SUGGESTIONS.find(item => item.name === event.target.value); if (!preset) return;
        const preferences = preset.labels.map(label => ({ ...draft(PREFERENCE_SUGGESTIONS.find(item => item.label === label)), scope: 'project' as const, targetId: null }));
        openPresetReview({ name: preset.name, preferences }); event.target.value = '';
      }}><option value="">Review a preset…</option>{PRESET_SUGGESTIONS.map(preset => <option key={preset.name}>{preset.name}</option>)}</select></label>
      {state.presets.length > 0 && <details className="workshop-saved-presets"><summary>Saved project presets</summary><p className="small-copy">Review a saved definition before reusing or exporting it. Nothing is added until you choose Add these project preferences.</p>{state.presets.map(preset => <article className="workshop-saved-preset" key={preset.id}><strong>{preset.name}</strong><small>{preset.preferences.length} project preferences</small><div className="workshop-actions"><button type="button" onClick={() => openPresetReview(preset, 'Saved preset opened for review. No preferences have been added yet.', preset.id)}>Review saved preset</button><button type="button" onClick={() => savePresetFile(preset)}>Save this preset as file</button></div></article>)}</details>}
      <button type="button" onClick={() => openPresetReview({ name: presetName, preferences: projectPreferences })}>Export or import preferences</button>
      <button type="button" onClick={() => savePresetFile({ id: crypto.randomUUID(), name: presetName, preferences: projectPreferences })}>Save project preset as file</button>
      <button type="button" onClick={() => { void importWorkshopPreset().then(preset => { if (!preset) return; openPresetReview(preset, 'File opened for review. No preferences have been added yet.'); }).catch(reason => setError(describeWorkshopError(reason))); }}>Open preset file for review</button>
    </details>
    {editing && <form className="workshop-preference-form" onSubmit={event => { event.preventDefault(); save(editing); }}>
      <h4>{state.preferences.some(item => item.id === editing.id) ? 'Edit preference' : 'Choose a preference'}</h4>
      <label>Name<input autoFocus required value={editing.label} maxLength={160} onChange={event => setEditing({ ...editing, label: event.target.value })} /></label>
      <label>What it means to you<textarea required value={editing.meaning} maxLength={4000} onChange={event => setEditing({ ...editing, meaning: event.target.value })} /></label>
      <label>Direction<select value={editing.polarity} onChange={event => setEditing({ ...editing, polarity: event.target.value as WorkshopPreference['polarity'] })}><option value="neutral">Unspecified</option><option value="want">Want</option><option value="avoid">Avoid</option></select></label>
      <label>Applies to<select value={editing.scope} onChange={event => { const scope = event.target.value as WorkshopPreference['scope']; setEditing({ ...editing, scope, targetId: scope === 'project' ? null : scope === 'element' ? session.focusDocumentId : session.id }); }}><option value="exploration">This exploration</option><option value="element" disabled={!session.focusDocumentId}>This element</option><option value="project">This project</option></select></label>
      <details><summary>Examples and stronger constraints</summary>
        <label>Family<select value={editing.family} onChange={event => setEditing({ ...editing, family: event.target.value })}>{FAMILIES.map(family => <option key={family}>{family}</option>)}</select></label>
        <label>Examples<textarea value={editing.examples} maxLength={4000} onChange={event => setEditing({ ...editing, examples: event.target.value })} /></label>
        <label>When it should matter<input value={editing.timing} maxLength={1000} onChange={event => setEditing({ ...editing, timing: event.target.value })} placeholder="For example, develops over the first arc" /></label>
        <label><input type="checkbox" checked={editing.strength === 'hard'} onChange={event => setEditing({ ...editing, strength: event.target.checked ? 'hard' : 'soft' })} />Treat as {editing.polarity === 'avoid' ? 'Never' : 'Must'}</label>
        <p className="small-copy">Protected text is checked literally. Narrative compliance still needs your review.</p>
      </details>
      <div className="workshop-actions"><button type="button" onClick={() => setEditing(null)}>Cancel</button><button className="primary-button">Save preference</button></div>
    </form>}
    {presetOpen && <section className="workshop-preset-review" aria-label="Review preference preset"><h4>Review preferences before adding them</h4><p className="small-copy">Only these editable preferences will be added to this project. This does not create a world, cast, or plot. Copy the text to share a preset, or paste one to import.</p><label>Preset name<input value={presetName} maxLength={160} onChange={event => updatePresetName(event.target.value)} /></label><label>Preset text<textarea className="workshop-preset-text" value={presetText} onChange={event => updatePresetText(event.target.value)} maxLength={100000} /></label><div className="workshop-actions"><button type="button" onClick={() => { setPresetOpen(false); setReviewedPresetId(null); }}>Close</button>{reviewedPresetId && <button type="button" onClick={savePresetDefinition}>Save preset definition</button>}<button type="button" onClick={importPreset}>Add these project preferences</button></div></section>}
    {error && <p role="alert">{error}</p>}
    {presetNotice && <p role="status">{presetNotice}</p>}
  </section>;
}
