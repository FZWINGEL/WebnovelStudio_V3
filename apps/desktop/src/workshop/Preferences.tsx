import { useState } from 'react';
import { exportWorkshopPreset, importWorkshopPreset, type WorkshopPreference, type WorkshopSession, type WorkshopState } from '../ipc/workshop';
import { describeWorkshopError } from './store';
import { FAMILIES, PREFERENCE_SUGGESTIONS, PRESET_SUGGESTIONS, type PreferenceSuggestion } from './catalog';

export function preferenceConflict(preferences: WorkshopPreference[], incoming: WorkshopPreference): string | null {
  if (incoming.polarity === 'neutral') return null;
  const conflicting = preferences.find(item => item.id !== incoming.id && item.scope === 'project' && item.confirmed && item.strength === 'hard' && item.polarity !== 'neutral' && item.polarity !== incoming.polarity && item.label.trim().toLocaleLowerCase() === incoming.label.trim().toLocaleLowerCase());
  return conflicting ? `This conflicts with the project’s ${conflicting.polarity === 'avoid' ? 'Never' : 'Must'} preference for “${conflicting.label}”. Edit that project preference explicitly before changing its local meaning.` : null;
}
export function applicablePreferences(state: WorkshopState, session: WorkshopSession) {
  return state.preferences.filter(preference => preference.confirmed && preference.polarity !== 'neutral' && (preference.scope === 'project' || preference.scope === 'element' && preference.targetId === session.focusDocumentId || preference.scope === 'exploration' && preference.targetId === session.id));
}
export function preferenceLabel(preference: WorkshopPreference) { return preference.polarity === 'neutral' ? 'Unspecified' : preference.strength === 'hard' ? preference.polarity === 'want' ? 'Must' : 'Never' : preference.polarity === 'want' ? 'Want' : 'Avoid'; }

export function Preferences({ state, session, onChange }: { state: WorkshopState; session: WorkshopSession; onChange(change: (state: WorkshopState) => WorkshopState): void }) {
  const [browse, setBrowse] = useState(false); const [query, setQuery] = useState('');
  const [editing, setEditing] = useState<WorkshopPreference | null>(null); const [error, setError] = useState('');
  const [presetText, setPresetText] = useState(''); const [presetOpen, setPresetOpen] = useState(false); const [presetName, setPresetName] = useState('My starting preferences');
  const [presetNotice, setPresetNotice] = useState('');
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
  function importPreset() {
    try {
      const parsed: unknown = JSON.parse(presetText);
      if (!parsed || typeof parsed !== 'object' || !('schemaVersion' in parsed) || parsed.schemaVersion !== 'workshop-preset.v1' || !('preferences' in parsed) || !Array.isArray(parsed.preferences) || parsed.preferences.length > 100) throw new Error('Use a Workshop preset with at most 100 preferences.');
      const preferences = parsed.preferences.map((input: unknown): WorkshopPreference => {
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
      for (const preference of preferences) { const conflict = preferenceConflict([...state.preferences, ...preferences], preference); if (conflict) throw new Error(conflict); }
      onChange(current => ({ ...current, preferences: [...current.preferences, ...preferences], presets: [...current.presets, { id: crypto.randomUUID(), name: presetName, preferences }] }));
      setPresetOpen(false); setError('');
    } catch (reason) { setError(reason instanceof Error ? reason.message : 'Could not read this preset.'); }
  }
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
        const preferences = preset.labels.map(label => ({ ...draft(PREFERENCE_SUGGESTIONS.find(item => item.label === label)), scope: 'project', targetId: null }));
        setPresetName(preset.name); setPresetText(JSON.stringify({ schemaVersion: 'workshop-preset.v1', name: preset.name, preferences }, null, 2)); setPresetOpen(true); event.target.value = '';
      }}><option value="">Review a preset…</option>{PRESET_SUGGESTIONS.map(preset => <option key={preset.name}>{preset.name}</option>)}</select></label>
      <button onClick={() => { setPresetText(JSON.stringify({ schemaVersion: 'workshop-preset.v1', name: presetName, preferences: state.preferences.filter(item => item.scope === 'project') }, null, 2)); setPresetOpen(true); }}>Export or import preferences</button>
      <button onClick={() => { void exportWorkshopPreset({ id: crypto.randomUUID(), name: presetName, preferences: state.preferences.filter(item => item.scope === 'project') }).then(path => { if (path) setPresetNotice(`Preset saved to ${path}`); }).catch(reason => setError(describeWorkshopError(reason))); }}>Save project preset as file</button>
      <button onClick={() => { void importWorkshopPreset().then(preset => { if (!preset) return; setPresetName(preset.name); setPresetText(JSON.stringify({ schemaVersion: 'workshop-preset.v1', name: preset.name, preferences: preset.preferences }, null, 2)); setPresetOpen(true); setPresetNotice('File opened for review. No preferences have been added yet.'); }).catch(reason => setError(describeWorkshopError(reason))); }}>Open preset file for review</button>
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
    {presetOpen && <section className="workshop-preset-review" aria-label="Review preference preset"><h4>Review preferences before adding them</h4><p className="small-copy">Only these editable preferences will be added to this project. This does not create a world, cast, or plot. Copy the text to share a preset, or paste one to import.</p><label>Preset name<input value={presetName} maxLength={160} onChange={event => setPresetName(event.target.value)} /></label><label>Preset text<textarea className="workshop-preset-text" value={presetText} onChange={event => setPresetText(event.target.value)} maxLength={100000} /></label><div className="workshop-actions"><button onClick={() => setPresetOpen(false)}>Close</button><button onClick={importPreset}>Add these project preferences</button></div></section>}
    {error && <p role="alert">{error}</p>}
    {presetNotice && <p role="status">{presetNotice}</p>}
  </section>;
}
