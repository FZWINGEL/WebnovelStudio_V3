import { useEffect, useMemo, useRef, useState } from 'react';
import type { Scope } from '../editor/selection';
import type { EvidenceAnchor, PossessionRecord, ReviewedEntityChoice, StoryEntityRef } from '../ipc/reviews';

type Timing = PossessionRecord['timing'];
type Audience = PossessionRecord['audience'];

interface FormState {
  id: string | null;
  objectId: string;
  objectLabel: string;
  holderId: string | null;
  holderLabel: string;
  timing: Timing;
  audience: Audience;
  evidence: EvidenceAnchor;
}

export function ReviewEvidenceEditor({ records, projectEntities = [], disabled, captureSelection, onChange, onEditingChange }: {
  records: PossessionRecord[];
  projectEntities?: ReviewedEntityChoice[];
  disabled?: boolean;
  captureSelection: () => Scope | null;
  onChange: (records: PossessionRecord[]) => void;
  onEditingChange?: (editing: boolean) => void;
}) {
  const [form, setForm] = useState<FormState | null>(null);
  const [error, setError] = useState('');
  const saveEpoch = useRef(0);
  useEffect(() => () => { saveEpoch.current += 1; }, []);
  useEffect(() => { saveEpoch.current += 1; }, [records]);
  useEffect(() => {
    onEditingChange?.(!!form);
    return () => onEditingChange?.(false);
  }, [form, onEditingChange]);
  useEffect(() => { if (disabled) saveEpoch.current += 1; }, [disabled]);
  function updateForm(next: FormState) {
    saveEpoch.current += 1;
    setForm(next);
  }
  const entities = useMemo(() => {
    const result: StoryEntityRef[] = [];
    const seen = new Set<string>();
    for (const record of records) {
      for (const entity of [record.object, record.holder].filter((value): value is StoryEntityRef => !!value)) {
        if (!seen.has(entity.id)) { seen.add(entity.id); result.push(entity); }
      }
    }
    for (const { entity } of projectEntities) {
      if (!seen.has(entity.id)) { seen.add(entity.id); result.push(entity); }
    }
    return result;
  }, [records, projectEntities]);
  const labels = useMemo(() => {
    const choices = new Map(projectEntities.map(choice => [choice.entity.id, choice]));
    const names = new Map(entities.map(entity => [entity.id, choices.has(entity.id) ? `${entity.label} · ${choices.get(entity.id)!.firstDocumentTitle}` : entity.label]));
    const counts = new Map<string, number>();
    for (const name of names.values()) counts.set(name, (counts.get(name) ?? 0) + 1);
    const seenLabels = new Map<string, number>();
    return new Map(entities.map(entity => {
      const name = names.get(entity.id)!;
      const index = (seenLabels.get(name) ?? 0) + 1;
      seenLabels.set(name, index);
      return [entity.id, counts.get(name)! > 1 ? `${name} (${index})` : name] as const;
    }));
  }, [entities, projectEntities]);
  const byId = (id: string | null): StoryEntityRef | null => id ? entities.find(entity => entity.id === id) ?? null : null;

  async function hashQuote(quote: string): Promise<string> {
    const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(quote));
    return [...new Uint8Array(digest)].map(byte => byte.toString(16).padStart(2, '0')).join('');
  }
  function anchor(scope: Scope): EvidenceAnchor | null {
    if (scope.start.blockId !== scope.end.blockId) return null;
    if (!scope.quote.trim()) return null;
    return { blockId: scope.start.blockId, fromUtf16: scope.start.utf16Offset, toUtf16: scope.end.utf16Offset, quote: scope.quote, quoteHash: '' };
  }
  function begin(scope: Scope | null, record: PossessionRecord | null = null) {
    if (disabled) return;
    const next = scope ? anchor(scope) : record?.evidence ?? null;
    if (!next) { setError('Select a non-empty passage within one paragraph before adding reviewed detail.'); return; }
    const objectId = record?.object.id ?? '__new_object__';
    setError('');
    setForm({ id: record?.id ?? null, objectId, objectLabel: record?.object.label ?? '', holderId: record?.holder?.id ?? null, holderLabel: record?.holder?.label ?? '', timing: record?.timing ?? 'unknown', audience: record?.audience ?? 'authorRoom', evidence: next });
  }
  async function save() {
    if (!form || disabled) return;
    const token = ++saveEpoch.current;
    try {
      const object = form.objectId === '__new_object__' ? { id: crypto.randomUUID(), label: form.objectLabel.trim() } : byId(form.objectId);
      if (!object || !object.label) { setError('Name the object or choose an existing entity.'); return; }
      const holder = form.holderId === '__new_holder__' ? { id: crypto.randomUUID(), label: form.holderLabel.trim() } : byId(form.holderId);
      if (form.holderId && !holder) { setError('This holder is no longer available. Choose it again or select Unknown holder.'); return; }
      if (form.holderId === '__new_holder__' && !holder?.label) { setError('Name the holder or choose Unknown holder.'); return; }
      const evidence = { ...form.evidence, quoteHash: await hashQuote(form.evidence.quote) };
      if (saveEpoch.current !== token || disabled) return;
      const record: PossessionRecord = { id: form.id ?? crypto.randomUUID(), object, holder, timing: form.timing, audience: form.audience, evidence };
      onChange(form.id ? records.map(item => item.id === form.id ? record : item) : [...records, record]);
      setForm(null); setError('');
    } catch (reason) {
      if (saveEpoch.current === token) setError(reason instanceof Error ? reason.message : 'Could not prepare this reviewed detail. Try again.');
    }
  }
  function reselect() {
    if (!form || disabled) return;
    saveEpoch.current += 1;
    const scope = captureSelection();
    const next = scope ? anchor(scope) : null;
    if (!next) { setError('Select a non-empty passage within one paragraph before replacing the evidence.'); return; }
    updateForm({ ...form, evidence: next }); setError('');
  }

  return <section className="review-details" aria-labelledby="review-details-heading">
    <div className="review-details-heading"><div><h3 id="review-details-heading">Reviewed story details</h3><p>Track who has an object using this saved passage.</p></div>
      <button disabled={disabled || !!form} onMouseDown={event => event.preventDefault()} onClick={() => begin(captureSelection())}>Add possession detail</button></div>
    {error && <p className="history-error" role="alert">{error}</p>}
    {!records.length && !form && <p className="review-details-empty">No reviewed details selected. You can mark the prose reviewed without adding one.</p>}
    {!!records.length && <ul className="review-detail-list">{records.map(record => <li key={record.id} className="review-detail-card">
      <div><strong>{record.object.label}</strong><span> · {record.holder?.label ?? 'holder unknown'}</span><span> · {record.timing === 'atPassage' ? 'known at this passage' : record.timing === 'earlier' ? 'describes an earlier time' : 'timing unclear'}</span></div>
      <blockquote>{record.evidence.quote}</blockquote>
      <small>{record.audience === 'reader' ? 'Explicitly reader-disclosed' : 'Author room only'}</small>
      <div className="review-detail-actions"><button disabled={disabled || !!form} onClick={() => begin(null, record)}>Edit</button><button disabled={disabled || !!form} onClick={() => onChange(records.filter(item => item.id !== record.id))}>Remove</button></div>
    </li>)}</ul>}
    {form && <div className="review-detail-form" aria-label={form.id ? 'Edit possession detail' : 'Add possession detail'}>
      {projectEntities.length > 0 && <p className="small-copy">Choose an existing object or holder to connect its passages across chapters. Chapter names identify where it was first recorded.</p>}
      <label>Object<select disabled={disabled} aria-label="Object" value={form.objectId} onChange={event => updateForm({ ...form, objectId: event.target.value })}><option value="__new_object__">New object…</option>{entities.map(entity => <option key={entity.id} value={entity.id}>{labels.get(entity.id)}</option>)}</select>{form.objectId === '__new_object__' && <input disabled={disabled} aria-label="New object name" value={form.objectLabel} onChange={event => updateForm({ ...form, objectLabel: event.target.value })} placeholder="Object name" />}</label>
      <label>Holder<select disabled={disabled} aria-label="Holder" value={form.holderId ?? ''} onChange={event => updateForm({ ...form, holderId: event.target.value || null })}><option value="">Unknown holder</option><option value="__new_holder__">New holder…</option>{entities.map(entity => <option key={entity.id} value={entity.id}>{labels.get(entity.id)}</option>)}</select>{form.holderId === '__new_holder__' && <input disabled={disabled} aria-label="New holder name" value={form.holderLabel} onChange={event => updateForm({ ...form, holderLabel: event.target.value })} placeholder="Holder name" />}</label>
      <label>Timing<select disabled={disabled} aria-label="Timing" value={form.timing} onChange={event => updateForm({ ...form, timing: event.target.value as Timing })}><option value="atPassage">Known at this passage</option><option value="earlier">Describes an earlier time</option><option value="unknown">Timing unclear</option></select></label>
      <label className="review-detail-checkbox"><input disabled={disabled} type="checkbox" checked={form.audience === 'reader'} onChange={event => updateForm({ ...form, audience: event.target.checked ? 'reader' : 'authorRoom' })} /> Explicitly disclosed to the reader</label>
      <blockquote>{form.evidence.quote}</blockquote>
      <div className="review-detail-actions"><button disabled={disabled} onMouseDown={event => event.preventDefault()} onClick={reselect}>Use current selection</button><button disabled={disabled} className="primary-button" onClick={() => void save()}>Keep detail</button><button onClick={() => { saveEpoch.current += 1; setForm(null); setError(''); }}>Cancel</button></div>
    </div>}
  </section>;
}
