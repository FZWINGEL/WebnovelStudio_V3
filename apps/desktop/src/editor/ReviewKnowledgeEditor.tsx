import { useEffect, useMemo, useRef, useState } from 'react';
import type { Scope } from '../editor/selection';
import {
  knowledgeAttitudeLabels,
  type EvidenceAnchor,
  type KnowledgeAttitude,
  type KnowledgeRecord,
  type ReviewedEntityChoice,
  type StoryEntityRef,
} from '../ipc/reviews';
import { evidenceQuoteHash, reviewAnchor } from './reviewEvidence';

interface FormState {
  id: string | null;
  characterId: string;
  characterLabel: string;
  topicId: string;
  topicLabel: string;
  attitude: KnowledgeAttitude;
  statement: string;
  timing: KnowledgeRecord['timing'];
  audience: KnowledgeRecord['audience'];
  evidence: EvidenceAnchor;
}

const NEW_CHARACTER = '__new_character__';
const NEW_TOPIC = '__new_topic__';

function validLabel(label: string): boolean {
  return !!label && new TextEncoder().encode(label).length <= 160 && !/[\u0000-\u001f\u007f-\u009f]/u.test(label);
}

function choicesFor(records: KnowledgeRecord[], projectChoices: ReviewedEntityChoice[], key: 'character' | 'topic') {
  const entities = new Map<string, { entity: StoryEntityRef; title: string }>();
  for (const record of records) {
    const entity = record[key];
    if (!entities.has(entity.id)) entities.set(entity.id, { entity, title: '' });
  }
  for (const choice of projectChoices) {
    const current = entities.get(choice.entity.id);
    entities.set(choice.entity.id, { entity: current?.entity ?? choice.entity, title: choice.firstDocumentTitle });
  }
  const names = [...entities.values()].map(({ entity, title }) => title ? `${entity.label} · ${title}` : entity.label);
  const counts = new Map<string, number>();
  for (const name of names) counts.set(name, (counts.get(name) ?? 0) + 1);
  const seen = new Map<string, number>();
  return [...entities.values()].map(({ entity, title }, index) => {
    const base = names[index];
    const ordinal = (seen.get(base) ?? 0) + 1;
    seen.set(base, ordinal);
    return { entity, name: counts.get(base)! > 1 ? `${base} (${ordinal})` : base };
  });
}

function defaultForm(scope: Scope | null, record?: KnowledgeRecord): FormState | null {
  const evidence = scope ? reviewAnchor(scope) : record?.evidence ?? null;
  if (!evidence) return null;
  return {
    id: record?.id ?? null,
    characterId: record?.character.id ?? NEW_CHARACTER,
    characterLabel: record?.character.label ?? '',
    topicId: record?.topic.id ?? NEW_TOPIC,
    topicLabel: record?.topic.label ?? '',
    attitude: record?.attitude ?? 'unclear',
    statement: record?.statement ?? '',
    timing: record?.timing ?? 'unknown',
    audience: record?.audience ?? 'authorRoom',
    evidence,
  };
}

/**
 * Records an author's interpretation of what a character knows or believes.
 * The editor only stores the selected passage and the author's explicit
 * statement; it never turns an empty result into a claim about story truth.
 */
export function ReviewKnowledgeEditor({ records, projectCharacters = [], projectTopics = [], disabled, captureSelection, onChange, onEditingChange }: {
  records: KnowledgeRecord[];
  projectCharacters?: ReviewedEntityChoice[];
  projectTopics?: ReviewedEntityChoice[];
  disabled?: boolean;
  captureSelection: () => Scope | null;
  onChange: (records: KnowledgeRecord[]) => void;
  onEditingChange?: (editing: boolean) => void;
}) {
  const [form, setForm] = useState<FormState | null>(null);
  const [error, setError] = useState('');
  const epoch = useRef(0);
  useEffect(() => () => { epoch.current += 1; }, []);
  useEffect(() => { epoch.current += 1; }, [records, disabled]);
  useEffect(() => { onEditingChange?.(!!form); return () => onEditingChange?.(false); }, [form, onEditingChange]);

  const characterChoices = useMemo(() => choicesFor(records, projectCharacters, 'character'), [records, projectCharacters]);
  const topicChoices = useMemo(() => choicesFor(records, projectTopics, 'topic'), [records, projectTopics]);
  const characters = useMemo(() => new Map(characterChoices.map(choice => [choice.entity.id, choice.entity])), [characterChoices]);
  const topics = useMemo(() => new Map(topicChoices.map(choice => [choice.entity.id, choice.entity])), [topicChoices]);

  function update(next: FormState) { epoch.current += 1; setForm(next); }
  function begin(scope: Scope | null, record?: KnowledgeRecord) {
    if (disabled) return;
    const next = defaultForm(scope, record);
    if (!next) { setError('Select a non-empty passage within one paragraph to record character knowledge.'); return; }
    setError(''); update(next);
  }
  async function save() {
    if (!form || disabled) return;
    const token = ++epoch.current;
    const statement = form.statement.trim();
    if (!statement || new TextEncoder().encode(statement).length > 1024 || /[\u0000-\u001f\u007f-\u009f]/u.test(statement)) {
      setError('Describe the character’s knowledge in one short statement of at most 1024 UTF-8 bytes.'); return;
    }
    const character = form.characterId === NEW_CHARACTER ? { id: crypto.randomUUID(), label: form.characterLabel.trim() } : characters.get(form.characterId);
    const topic = form.topicId === NEW_TOPIC ? { id: crypto.randomUUID(), label: form.topicLabel.trim() } : topics.get(form.topicId);
    if (!character || !validLabel(character.label)) { setError('Name the character briefly or choose an existing character.'); return; }
    if (!topic || !validLabel(topic.label)) { setError('Name the topic briefly or choose an existing topic.'); return; }
    try {
      const evidence = { ...form.evidence, quoteHash: await evidenceQuoteHash(form.evidence.quote) };
      if (epoch.current !== token || disabled) return;
      const record: KnowledgeRecord = {
        id: form.id ?? crypto.randomUUID(), character, topic, attitude: form.attitude,
        statement, timing: form.timing, audience: form.audience, evidence,
      };
      onChange(form.id ? records.map(item => item.id === form.id ? record : item) : [...records, record]);
      setForm(null); setError('');
    } catch (reason) {
      if (epoch.current === token) setError(reason instanceof Error ? reason.message : 'Could not prepare this knowledge observation. Try again.');
    }
  }
  function reselect() {
    if (!form || disabled) return;
    epoch.current += 1;
    const scope = captureSelection(); const next = scope ? reviewAnchor(scope) : null;
    if (!next) { setError('Select a non-empty passage within one paragraph before replacing the evidence.'); return; }
    update({ ...form, evidence: next }); setError('');
  }
  const timingLabel = (timing: KnowledgeRecord['timing']) => timing === 'atPassage' ? 'At this passage' : timing === 'earlier' ? 'An earlier time' : 'Timing unclear';
  return <section className="review-details" aria-labelledby="review-knowledge-heading">
    <div className="review-details-heading"><div><h3 id="review-knowledge-heading">Character knowledge</h3><p>Record what a character knows, believes, suspects, rejects, or is explicitly unaware of.</p></div>
      <button disabled={disabled || !!form || records.length >= 64} onMouseDown={event => event.preventDefault()} onClick={() => begin(captureSelection())}>Add knowledge observation</button></div>
    {error && <p className="history-error" role="alert">{error}</p>}
    {!records.length && !form && <p className="review-details-empty">No character knowledge is recorded for this review. Adding it is optional.</p>}
    {!!records.length && <ul className="review-detail-list">{records.map(record => <li key={record.id} className="review-detail-card">
      <strong>{record.character.label}</strong><span> · {record.topic.label} · {knowledgeAttitudeLabels[record.attitude]}</span><p>{record.statement}</p><blockquote>{record.evidence.quote}</blockquote>
      <small>{record.audience === 'reader' ? 'Explicitly disclosed to the reader' : 'Author room only'} · {timingLabel(record.timing)}</small>
      <div className="review-detail-actions"><button disabled={disabled || !!form} onClick={() => begin(null, record)}>Edit knowledge</button><button disabled={disabled || !!form} onClick={() => onChange(records.filter(item => item.id !== record.id))}>Remove knowledge</button></div>
    </li>)}</ul>}
    {form && <div className="review-detail-form" aria-label={form.id ? 'Edit character knowledge' : 'Add character knowledge'}>
      <label>Character<select disabled={disabled} aria-label="Character" value={form.characterId} onChange={event => update({ ...form, characterId: event.target.value })}><option value={NEW_CHARACTER}>New character…</option>{characterChoices.map(choice => <option key={choice.entity.id} value={choice.entity.id}>{choice.name}</option>)}</select></label>
      {form.characterId === NEW_CHARACTER && <label>Character name<input disabled={disabled} value={form.characterLabel} onChange={event => update({ ...form, characterLabel: event.target.value })} placeholder="Mei" /></label>}
      <label>Topic<select disabled={disabled} aria-label="Topic" value={form.topicId} onChange={event => update({ ...form, topicId: event.target.value })}><option value={NEW_TOPIC}>New topic…</option>{topicChoices.map(choice => <option key={choice.entity.id} value={choice.entity.id}>{choice.name}</option>)}</select></label>
      {form.topicId === NEW_TOPIC && <label>Topic name<input disabled={disabled} value={form.topicLabel} onChange={event => update({ ...form, topicLabel: event.target.value })} placeholder="Why Mei left" /></label>}
      <label>Recorded attitude<select aria-label="Recorded attitude" disabled={disabled} value={form.attitude} onChange={event => update({ ...form, attitude: event.target.value as KnowledgeAttitude })}>{Object.entries(knowledgeAttitudeLabels).map(([attitude, label]) => <option key={attitude} value={attitude}>{label}</option>)}</select></label>
      <label>What this character knows or believes<textarea aria-label="Knowledge statement" disabled={disabled} maxLength={1024} value={form.statement} onChange={event => update({ ...form, statement: event.target.value })} placeholder="Mei believes her brother hid the key." /></label>
      <label>Knowledge timing<select aria-label="Knowledge timing" disabled={disabled} value={form.timing} onChange={event => update({ ...form, timing: event.target.value as KnowledgeRecord['timing'] })}><option value="atPassage">At this passage</option><option value="earlier">Describes an earlier time</option><option value="unknown">Timing unclear</option></select></label>
      <label className="review-detail-checkbox"><input disabled={disabled} type="checkbox" checked={form.audience === 'reader'} onChange={event => update({ ...form, audience: event.target.checked ? 'reader' : 'authorRoom' })} /> Explicitly disclosed to the reader</label>
      <blockquote>{form.evidence.quote}</blockquote><p className="small-copy">This is your recorded interpretation of the passage. It does not establish world truth, and no missing observation becomes “unaware.”</p>
      <div className="review-detail-actions"><button disabled={disabled} onMouseDown={event => event.preventDefault()} onClick={reselect}>Use current selection</button><button disabled={disabled} className="primary-button" onClick={() => void save()}>Keep knowledge</button><button onClick={() => { epoch.current += 1; setForm(null); setError(''); }}>Cancel</button></div>
    </div>}
  </section>;
}
