import { useEffect, useMemo, useRef, useState } from 'react';
import type { Scope } from '../editor/selection';
import { promisePhaseLabels, type EvidenceAnchor, type PromisePhase, type PromiseRecord, type ReviewedEntityChoice, type StoryEntityRef } from '../ipc/reviews';
import { evidenceQuoteHash, reviewAnchor } from './reviewEvidence';

interface FormState {
  id: string | null; promiseId: string; label: string; phase: PromisePhase;
  timing: PromiseRecord['timing']; audience: PromiseRecord['audience']; note: string; evidence: EvidenceAnchor;
}

export function ReviewPromiseEditor({ records, projectPromises = [], disabled, captureSelection, onChange, onEditingChange }: {
  records: PromiseRecord[]; projectPromises?: ReviewedEntityChoice[]; disabled?: boolean;
  captureSelection(): Scope | null; onChange(records: PromiseRecord[]): void; onEditingChange?(editing: boolean): void;
}) {
  const [form, setForm] = useState<FormState | null>(null);
  const [error, setError] = useState('');
  const epoch = useRef(0);
  useEffect(() => () => { epoch.current += 1; }, []);
  useEffect(() => { epoch.current += 1; }, [records, disabled]);
  useEffect(() => { onEditingChange?.(!!form); return () => onEditingChange?.(false); }, [form, onEditingChange]);
  const choices = useMemo(() => {
    const map = new Map<string, { entity: StoryEntityRef; title: string }>();
    for (const record of records) map.set(record.promise.id, { entity: record.promise, title: '' });
    for (const choice of projectPromises) {
      const existing = map.get(choice.entity.id);
      map.set(choice.entity.id, { entity: existing?.entity ?? choice.entity, title: choice.firstDocumentTitle });
    }
    const values = [...map.values()];
    const names = values.map(({ entity, title }) => title ? `${entity.label} · ${title}` : entity.label);
    return values.map((item, index) => ({ ...item, name: names.filter(name => name === names[index]).length > 1 ? `${names[index]} (${index + 1})` : names[index] }));
  }, [records, projectPromises]);
  function update(next: FormState) { epoch.current += 1; setForm(next); }
  function begin(scope: Scope | null, record?: PromiseRecord) {
    if (disabled) return;
    const evidence = scope ? reviewAnchor(scope) : record?.evidence ?? null;
    if (!evidence) { setError('Select a non-empty passage within one paragraph to record a promise or payoff.'); return; }
    setError('');
    update({ id: record?.id ?? null, promiseId: record?.promise.id ?? '__new__', label: record?.promise.label ?? '', phase: record?.phase ?? 'setup', timing: record?.timing ?? 'unknown', audience: record?.audience ?? 'authorRoom', note: record?.note ?? '', evidence });
  }
  async function save() {
    if (!form || disabled) return;
    const token = ++epoch.current;
    const note = form.note.trim();
    if (!note || new TextEncoder().encode(note).length > 1024 || /[\u0000-\u001f\u007f-\u009f]/u.test(note)) { setError('Describe what this passage records in one short note on a single line.'); return; }
    const promise = form.promiseId === '__new__' ? { id: crypto.randomUUID(), label: form.label.trim() } : choices.find(choice => choice.entity.id === form.promiseId)?.entity;
    if (!promise || !promise.label || new TextEncoder().encode(promise.label).length > 160 || /[\u0000-\u001f\u007f-\u009f]/u.test(promise.label)) { setError('Name this promise briefly or choose an existing promise.'); return; }
    try {
      const evidence = { ...form.evidence, quoteHash: await evidenceQuoteHash(form.evidence.quote) };
      if (epoch.current !== token) return;
      const record: PromiseRecord = { id: form.id ?? crypto.randomUUID(), promise, phase: form.phase, timing: form.timing, audience: form.audience, note, evidence };
      onChange(form.id ? records.map(item => item.id === form.id ? record : item) : [...records, record]);
      setForm(null); setError('');
    } catch (reason) { if (epoch.current === token) setError(reason instanceof Error ? reason.message : 'Could not prepare this promise. Try again.'); }
  }
  function reselect() {
    if (!form || disabled) return;
    epoch.current += 1;
    const scope = captureSelection(); const evidence = scope ? reviewAnchor(scope) : null;
    if (!evidence) { setError('Select a non-empty passage within one paragraph before replacing the evidence.'); return; }
    update({ ...form, evidence }); setError('');
  }
  return <section className="review-details" aria-labelledby="review-promises-heading">
    <div className="review-details-heading"><div><h3 id="review-promises-heading">Promises &amp; payoffs</h3><p>Connect a promise with passages that introduce, fulfil, or cancel it.</p></div>
      <button disabled={disabled || !!form || records.length >= 64} onMouseDown={event => event.preventDefault()} onClick={() => begin(captureSelection())}>Add promise detail</button></div>
    {error && <p className="history-error" role="alert">{error}</p>}
    {!records.length && !form && <p className="review-details-empty">No promises recorded for this review. Adding one is optional.</p>}
    {!!records.length && <ul className="review-detail-list">{records.map(record => <li key={record.id} className="review-detail-card">
      <strong>{record.promise.label}</strong><span> · {promisePhaseLabels[record.phase]}</span><p>{record.note}</p><blockquote>{record.evidence.quote}</blockquote>
      <small>{record.audience === 'reader' ? 'Explicitly reader-disclosed' : 'Author room only'} · {record.timing === 'atPassage' ? 'At this passage' : record.timing === 'earlier' ? 'An earlier time' : 'Timing unclear'}</small>
      <div className="review-detail-actions"><button disabled={disabled || !!form} onClick={() => begin(null, record)}>Edit promise</button><button disabled={disabled || !!form} onClick={() => onChange(records.filter(item => item.id !== record.id))}>Remove promise</button></div>
    </li>)}</ul>}
    {form && <div className="review-detail-form" aria-label={form.id ? 'Edit promise detail' : 'Add promise detail'}>
      <label>Promise<select disabled={disabled} aria-label="Promise" value={form.promiseId} onChange={event => update({ ...form, promiseId: event.target.value })}><option value="__new__">New promise…</option>{choices.map(choice => <option key={choice.entity.id} value={choice.entity.id}>{choice.name}</option>)}</select></label>
      {form.promiseId === '__new__' && <label>Promise name<input disabled={disabled} value={form.label} onChange={event => update({ ...form, label: event.target.value })} placeholder="Return the silver key" /></label>}
      <label>What this passage records<select aria-label="What this passage records" disabled={disabled} value={form.phase} onChange={event => update({ ...form, phase: event.target.value as PromisePhase })}>{Object.entries(promisePhaseLabels).map(([phase, label]) => <option key={phase} value={phase}>{label}</option>)}</select></label>
      <label>Promise note<input disabled={disabled} value={form.note} onChange={event => update({ ...form, note: event.target.value })} placeholder="Ren promises to return the key before dawn." /></label>
      <label>Promise timing<select aria-label="Promise timing" disabled={disabled} value={form.timing} onChange={event => update({ ...form, timing: event.target.value as PromiseRecord['timing'] })}><option value="atPassage">At this passage</option><option value="earlier">Describes an earlier time</option><option value="unknown">Timing unclear</option></select></label>
      <label className="review-detail-checkbox"><input disabled={disabled} type="checkbox" checked={form.audience === 'reader'} onChange={event => update({ ...form, audience: event.target.checked ? 'reader' : 'authorRoom' })} /> Explicitly disclosed to the reader</label>
      <blockquote>{form.evidence.quote}</blockquote><p className="small-copy">This records your reading of the passage. It does not establish that every later payoff has been found.</p>
      <div className="review-detail-actions"><button disabled={disabled} onMouseDown={event => event.preventDefault()} onClick={reselect}>Use current selection</button><button disabled={disabled} className="primary-button" onClick={() => void save()}>Keep promise</button><button onClick={() => { epoch.current += 1; setForm(null); setError(''); }}>Cancel</button></div>
    </div>}
  </section>;
}
