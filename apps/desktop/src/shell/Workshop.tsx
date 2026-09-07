import { forwardRef, useEffect, useImperativeHandle, useMemo, useReducer, useRef, useState } from 'react';
import { readDocument, type DocumentRecord, type OpenedProject } from '../ipc/projects';
import { adoptWorkshop, previewWorkshopAdoption, startWorkshop, workshopHistory, type CandidateChoice, type WorkshopAdoptionPreview, type WorkshopAdoptionTarget, type WorkshopCandidate, type WorkshopImpactDraft, type WorkshopResult, type WorkshopSession, type WorkshopSnapshot } from '../ipc/workshop';
import { retryDiscussionSave, stopDiscussion } from '../ipc/discussions';
import { useProviders } from '../providers/ProviderContext';
import { sameModel, type ModelSelection } from '../ipc/providers';
import { ContextInspector } from '../assistant/ContextInspector';
import { CandidateBoard } from '../workshop/CandidateBoard';
import { Preferences, applicablePreferences, preferenceLabel } from '../workshop/Preferences';
import { ACTIONS, LENSES, SUBVERSIONS, WORLD_QUESTIONS, type WorkshopLens } from '../workshop/catalog';
import { describeWorkshopError, newSession, WorkshopStore } from '../workshop/store';
import { appendText, plainText, textDocument } from '../workshop/text';
import { Relationships } from '../workshop/Relationships';
import { AdoptionLinks, adoptionParticipants, validAdoptionLinks, type AdoptionMaterialDraft, type AdoptionLinkDraft } from '../workshop/AdoptionLinks';
import { AdoptionImpacts, IMPACT_LABELS } from '../workshop/AdoptionImpacts';
import { AdoptionPreview } from '../workshop/AdoptionPreview';
import { WorkshopRecap } from '../workshop/WorkshopRecap';
import { RequestContext } from '../workshop/RequestContext';
import '../workshop/workshop.css';

export interface WorkshopHandle { flush(): Promise<void> }
interface Props { project: OpenedProject; onOpenDocument(documentId: string): void; onDocumentsChanged(documents: DocumentRecord[]): void; onError(message: string): void }
type Capture = { from: number; to: number; text: string; generation: string };

export const Workshop = forwardRef<WorkshopHandle, Props>(function Workshop({ project, onOpenDocument, onDocumentsChanged, onError }, ref) {
  const access = project.access;
  const store = useMemo(() => new WorkshopStore(access), [access.projectId, access.operationNamespace, access.session, access.writerLease]);
  const [, redraw] = useReducer(value => value + 1, 0);
  const [loadingError, setLoadingError] = useState(''); const [notice, setNotice] = useState('');
  const [requestBusy, setRequestBusy] = useState(false); const [adopting, setAdopting] = useState(false);
  const [contextOpen, setContextOpen] = useState(() => window.innerWidth >= 1190); const [selectedRun, setSelectedRun] = useState<string | null>(null);
  const [action, setAction] = useState('directions'); const [subversion, setSubversion] = useState('');
  const [capture, setCapture] = useState<Capture | null>(null); const [preview, setPreview] = useState<WorkshopAdoptionPreview | null>(null);
  const [adoptionForm, setAdoptionForm] = useState(false); const [targets, setTargets] = useState<AdoptionMaterialDraft[]>([]);
  const [adoptionLinks, setAdoptionLinks] = useState<AdoptionLinkDraft[]>([]);
  const [impactDrafts, setImpactDrafts] = useState<WorkshopImpactDraft[]>([]);
  const [rationale, setRationale] = useState(''); const adoptionOperation = useRef<string | null>(null); const previewGeneration = useRef(0);
  const pendingRequest = useRef<{ operationId: string; exploration: Parameters<typeof startWorkshop>[2]; selection: ModelSelection } | null>(null);
  const [history, setHistory] = useState<WorkshopSnapshot[] | null>(null); const [notesOpen, setNotesOpen] = useState(false);
  const composing = useRef(false); const workingEditor = useRef<HTMLTextAreaElement>(null); const heading = useRef<HTMLHeadingElement>(null);
  const providers = useProviders();
  const selectedModel = providers.state?.catalog.models.find(model => sameModel(model.key, providers.state!.settings.active));
  const canGenerate = !!providers.state && !providers.busy && selectedModel?.ready === true;
  const state = store.state;
  const session = state.sessions.find(item => item.id === state.currentSessionId) ?? state.sessions[0];
  const sessionResults = session ? store.results.filter(result => result.sessionId === session.id) : [];
  const result = sessionResults.find(item => item.run.id === selectedRun) ?? sessionResults.at(-1) ?? null;
  const live = sessionResults.find(item => ['queued', 'running', 'stopping'].includes(item.run.status));
  const anyLive = store.results.some(item => ['queued', 'running', 'stopping'].includes(item.run.status));
  const material = project.documents.filter(document => document.kind !== 'chapter' && !document.head.documentId.startsWith('workshop-'));
  const participants = adoptionParticipants(material, targets);
  store.locked = adopting || adoptionOperation.current !== null;

  useImperativeHandle(ref, () => ({ flush: async () => {
    if (adoptionOperation.current) throw new Error('Finish checking the Workshop adoption before leaving this project.');
    if (pendingRequest.current) throw new Error('Check the pending Workshop request before leaving this project.');
    await store.flush();
  } }), [store]);
  useEffect(() => {
    const unsubscribe = store.subscribe(redraw); let disposed = false;
    void store.load().then(() => {
      if (disposed || store.state.sessions.length) return;
      const first = newSession(); store.edit(current => ({ ...current, currentSessionId: first.id, sessions: [first] }));
    }).catch(reason => { if (!disposed) setLoadingError(describeWorkshopError(reason)); });
    return () => { disposed = true; unsubscribe(); store.dispose(); };
  }, [store]);
  useEffect(() => {
    if (!anyLive) return;
    let disposed = false; let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try { await store.refreshResults(); } catch (reason) { if (!disposed) setNotice(`Could not refresh the request: ${describeWorkshopError(reason)}`); }
      if (!disposed) timer = setTimeout(() => { void poll(); }, 1000);
    };
    timer = setTimeout(() => { void poll(); }, 500);
    return () => { disposed = true; clearTimeout(timer); };
  }, [store, anyLive]);
  useEffect(() => { setCapture(null); setSelectedRun(null); setPreview(null); setAdoptionForm(false); setHistory(null); }, [session?.id]);

  function edit(change: (session: WorkshopSession) => WorkshopSession, working = false) { if (session && !adopting) store.editSession(session.id, change, working); }
  function report(reason: unknown) { const message = describeWorkshopError(reason); setNotice(message); onError(message); }
  function chooseLens(lens: WorkshopLens) {
    if (!session) return;
    const focus = LENSES.find(item => item.id === lens)!;
    edit(current => ({ ...current, lens, focusQuestion: focus.question, focusReason: focus.reason }));
    setAction(lens === 'people' ? 'situation' : lens === 'themes' ? 'moment' : lens === 'possibilities' ? 'arc' : 'directions');
    setCapture(null); heading.current?.focus();
  }
  function addSession(lens: WorkshopLens = session?.lens ?? 'overview') {
    const next = newSession(lens);
    store.edit(current => ({ ...current, currentSessionId: next.id, sessions: [...current.sessions, next] })); setNotice('A new exploration is ready. Nothing has been generated.');
  }
  async function bringDocument(documentId: string) {
    if (!session || !documentId) return;
    try {
      const source = await readDocument(access, documentId);
      edit(current => ({ ...current, focusDocumentId: documentId, title: source.title, workingTitle: source.title, originalNotes: plainText(source.body), brief: plainText(source.body), includedDocumentIds: [...new Set([...current.includedDocumentIds, documentId])] }));
      setNotesOpen(false); setNotice(`Original ${source.title} preserved. Workshop changes stay here until you choose Use this version.`);
    } catch (reason) { report(reason); }
  }
  function selectDetail(candidate: WorkshopCandidate, text: string) {
    if (!text || !session) return;
    edit(current => ({ ...current, selectedDetails: [...current.selectedDetails, { id: crypto.randomUUID(), candidateId: candidate.id, text, fixed: false }] }), true);
    setNotice('Detail added to your tray. It has not been chosen for the story.');
  }
  function develop(candidate: WorkshopCandidate) {
    if (!session) return;
    const frozen = result?.workingSelection;
    const replacement = frozen;
    if (frozen && result?.workingGeneration !== session.workingGeneration) {
      setNotice('This scoped result belongs to an earlier working version. Select the passage and explore again before replacing it.'); return;
    }
    if (replacement && session.workingText.slice(replacement.from, replacement.to) !== replacement.text) { setNotice('The original selected passage changed. Explore the new selection before developing a replacement.'); return; }
    if (!replacement && capture) { setNotice('This direction was generated for the whole working version. Clear the selection to use it, or explore the selected passage first.'); return; }
    const fixed = session.selectedDetails.filter(detail => detail.fixed).map(detail => detail.text);
    const nextText = replacement ? session.workingText.slice(0, replacement.from) + candidate.content + session.workingText.slice(replacement.to) : candidate.content;
    if (fixed.some(text => !nextText.includes(text))) { setNotice('This version would change a Keep fixed detail. Edit the proposal or explicitly remove that protection first.'); return; }
    edit(current => ({ ...current,
      ...(result?.action === 'voiceGuidance' && !replacement ? { lens: 'themes' as const, focusDocumentId: null } : {}),
      workingTitle: replacement ? current.workingTitle : result?.action === 'voiceGuidance' ? `Voice guidance: ${candidate.title}`.slice(0, 160) : candidate.title,
      workingText: nextText, selectedDetails: [...current.selectedDetails, { id: crypto.randomUUID(), candidateId: candidate.id, text: candidate.content, fixed: false }] }), true);
    setCapture(null); setNotice(result?.action === 'moment' ? 'This is still a noncanon experiment. Choosing voice guidance will not adopt its events.' : 'Working version updated. Review it before choosing it for the story.'); workingEditor.current?.focus();
  }
  function choice(value: CandidateChoice) { edit(current => ({ ...current, choices: [...current.choices.filter(item => item.candidateId !== value.candidateId), value] })); }
  async function generate(nextAction = action, candidate?: WorkshopCandidate) {
    if (!session || requestBusy || live || (!pendingRequest.current && (!canGenerate || !providers.state)) || composing.current) return;
    if (nextAction === 'subvert' && !pendingRequest.current && (!subversion || !session.selectedScope.trim() || session.selectedScope === 'Whole working version')) {
      setNotice('Name the convention and choose a transformation before exploring its subversion.'); return;
    }
    setRequestBusy(true); setNotice('');
    try {
      if (candidate && !pendingRequest.current) {
        edit(current => ({ ...current, selectedDetails: [...current.selectedDetails, { id: crypto.randomUUID(), candidateId: candidate.id, text: candidate.content, fixed: false }], selectedScope: candidate.title }), true);
      }
      await store.flush();
      const current = store.state.sessions.find(item => item.id === session.id)!;
      if (!pendingRequest.current && !candidate && capture && (capture.generation !== current.workingGeneration || current.workingText.slice(capture.from, capture.to) !== capture.text)) throw new Error('The selected passage changed. Select it again before exploring.');
      const selectedAction = ACTIONS.find(item => item.id === nextAction) ?? ACTIONS[0];
      const instruction = [selectedAction.instruction, nextAction === 'subvert' ? `Convention to transform: ${current.selectedScope}.\nTransformation: ${subversion}.` : '', current.composer].filter(Boolean).join('\n\n');
      pendingRequest.current ??= { operationId: crypto.randomUUID(), exploration: {
        sessionId: current.id, expectedVersion: store.version, workingGeneration: current.workingGeneration,
        action: nextAction, instruction, selectedScope: nextAction === 'voiceGuidance' ? 'Voice qualities from the sample' : candidate?.title ?? (capture ? 'Selected passage in working version' : current.selectedScope), selectedText: candidate?.content ?? capture?.text ?? (nextAction === 'voiceGuidance' ? current.workingText : ''),
        workingSelection: nextAction !== 'voiceGuidance' && !candidate && capture ? { from: capture.from, to: capture.to, text: capture.text } : null,
      }, selection: structuredClone(providers.state!.settings.active) };
      const pending = pendingRequest.current;
      const started = await startWorkshop(access, pending.operationId, pending.exploration, pending.selection);
      pendingRequest.current = null;
      const provisional: WorkshopResult = { run: started.run, sessionId: pending.exploration.sessionId, workingGeneration: pending.exploration.workingGeneration, action: pending.exploration.action, workingSelection: pending.exploration.workingSelection, output: null, validationError: null, stale: false };
      store.results = [...store.results.filter(item => item.run.id !== started.run.id), provisional];
      store.editSession(pending.exploration.sessionId, value => ({ ...value, activeRunId: started.run.id }));
      if (store.state.currentSessionId === pending.exploration.sessionId) { setSelectedRun(started.run.id); setAction(pending.exploration.action); if (pending.exploration.action === 'voiceGuidance') setCapture(null); }
      await store.refreshResults();
    } catch (reason) {
      // Definitive preflight refusal is safe to edit and resubmit. Transport
      // uncertainty keeps the original request identity for reconciliation.
      const code = reason && typeof reason === 'object' && 'code' in reason ? String(reason.code) : '';
      if (code && !['UncertainOutcome', 'PersistenceUnavailable', 'ActorUnavailable'].includes(code)) pendingRequest.current = null;
      report(reason);
    }
    finally { setRequestBusy(false); }
  }
  async function stop() {
    if (!live) return;
    try { await stopDiscussion(access, live.run.id); await store.refreshResults(); setNotice('Stop requested. Work already sent may still incur provider usage. Partial output remains available.'); } catch (reason) { report(reason); }
  }
  function fork() {
    if (!session) return;
    const next = { ...structuredClone(session), id: crypto.randomUUID(), parentSessionId: session.id, branchKind: 'whatIf' as const, title: `What if: ${session.title}`, activeRunId: null };
    next.anchorDocumentId = `workshop-${next.id}`;
    const localPreferences = state.preferences.filter(preference => preference.scope === 'exploration' && preference.targetId === session.id).map(preference => ({ ...preference, id: crypto.randomUUID(), targetId: next.id }));
    store.edit(current => ({ ...current, sessions: [...current.sessions, next], currentSessionId: next.id, preferences: [...current.preferences, ...localPreferences] }));
    setNotice('An isolated what-if exploration is ready. Existing story documents are unchanged.');
  }
  function beginAdoption() {
    if (!session || !session.workingText.trim()) return;
    const kind = session.lens === 'world' ? 'world' : session.lens === 'people' ? 'character' : session.lens === 'themes' ? 'theme' : session.lens === 'possibilities' ? 'hook' : 'note';
    setTargets([{ id: crypto.randomUUID(), documentId: session.focusDocumentId ?? '', title: session.workingTitle || session.title, kind, mode: 'add', text: session.workingText }]);
    setAdoptionLinks([]);
    const selectedIds = new Set(session.selectedDetails.flatMap(detail => detail.candidateId ? [detail.candidateId] : []));
    const affected = new Map<string, WorkshopImpactDraft>();
    for (const saved of sessionResults) for (const candidate of saved.output?.candidates ?? []) {
      if (!selectedIds.has(candidate.id)) continue;
      for (const target of candidate.affectedTargets) {
        const prior = affected.get(target.documentId);
        affected.set(target.documentId, { documentId: target.documentId, kind: 'possibleTension', reason: prior ? `${prior.reason}\n${target.reason}` : target.reason });
      }
    }
    setImpactDrafts([...affected.values()]);
    setRationale(''); setPreview(null); setAdoptionForm(true);
  }
  function prepareVoiceGuidance() {
    setAction('voiceGuidance');
    edit(current => ({ ...current, composer: current.composer || 'Describe the voice qualities in this sample that could guide later writing. Keep its events separate.' }));
    setNotice('Explain what you like or would change in the sample, then choose Explore. The resulting guidance remains a proposal.');
  }
  function exploreCandidate(nextAction: string, candidate: WorkshopCandidate) {
    if (nextAction !== 'subvert') { void generate(nextAction, candidate); return; }
    const previous = session?.choices.find(value => value.candidateId === candidate.id);
    choice({ candidateId: candidate.id, status: 'saved', rationale: previous?.rationale ?? '', includeInContext: true });
    edit(current => ({ ...current, selectedScope: candidate.title }));
    setAction('subvert'); setSubversion(''); setCapture(null);
    setNotice('This direction is included as a comparison. Name its convention and choose the transformation below, then Explore.');
  }
  async function prepareAdoption() {
    if (!session || composing.current) return;
    if (!validAdoptionLinks(adoptionLinks, participants)) { setNotice('Review the relationship participants and descriptions before previewing.'); return; }
    setAdopting(true);
    try {
      await store.flush();
      const proposalTargets: WorkshopAdoptionTarget[] = await Promise.all(targets.map(async target => {
        const source = target.documentId ? await readDocument(access, target.documentId) : null;
        return { documentId: source?.head.documentId ?? target.id, expected: source?.head ?? null, title: source?.title ?? target.title, kind: source?.kind ?? target.kind, mode: target.mode, body: source && target.mode === 'add' ? appendText(source.body, target.text) : textDocument(target.text) };
      }));
      const current = store.state.sessions.find(item => item.id === session.id)!;
      const relationships = adoptionLinks.map(link => ({ ...link,
        fromExpected: material.find(document => document.head.documentId === link.fromDocumentId)?.head ?? null,
        toExpected: material.find(document => document.head.documentId === link.toDocumentId)?.head ?? null,
      }));
      const value = await previewWorkshopAdoption({ access, sessionId: current.id, expectedVersion: store.version, candidateIds: [...new Set(current.selectedDetails.flatMap(detail => detail.candidateId ? [detail.candidateId] : []))], targets: proposalTargets, rationale, protectedText: current.selectedDetails.filter(detail => detail.fixed).map(detail => detail.text), relationships, impactDrafts });
      previewGeneration.current = store.generation; setPreview(value); setAdoptionForm(false);
    } catch (reason) { report(reason); }
    finally { setAdopting(false); }
  }
  async function commitAdoption() {
    if (!preview || composing.current) return;
    setAdopting(true);
    try {
      if (!adoptionOperation.current) {
        await store.flush();
        if (store.version !== preview.expectedVersion) throw new Error('Your exploration changed after preview. Prepare the adoption again.');
        adoptionOperation.current = crypto.randomUUID();
      }
      const ack = await adoptWorkshop(access, adoptionOperation.current, preview.id);
      store.acceptAdoption(ack.snapshot, previewGeneration.current); adoptionOperation.current = null; store.locked = false;
      onDocumentsChanged(ack.documents); setPreview(null); setNotice('Version chosen. Its source and rationale are saved; writing access remains author only.');
    } catch (reason) {
      const code = reason && typeof reason === 'object' && 'code' in reason ? String(reason.code) : '';
      if (code && !['UncertainOutcome', 'PersistenceUnavailable', 'ActorUnavailable'].includes(code)) { adoptionOperation.current = null; store.locked = false; }
      report(reason);
    }
    finally { setAdopting(false); }
  }
  function questionStatus(status: 'notNow' | 'notRelevant' | 'keepMysterious') {
    if (!session) return;
    edit(current => ({ ...current, questions: [...current.questions.filter(question => question.text !== current.focusQuestion), { id: crypto.randomUUID(), text: current.focusQuestion, reason: current.focusReason, status, unknownTo: 'both' }] }));
    setNotice(status === 'keepMysterious' ? 'Mystery preserved. Choose whether it is unknown to you, the reader, or both below.' : status === 'notNow' ? 'Question saved for later.' : 'Question marked not relevant.');
  }

  function differentQuestion() {
    if (!session) return;
    const choices: string[] = [...WORLD_QUESTIONS.map(question => question.text), ...LENSES.map(lens => lens.question)];
    const start = choices.indexOf(session.focusQuestion);
    const next = Array.from({ length: choices.length }, (_, offset) => choices[(start + offset + 1) % choices.length]).find(text => text !== session.focusQuestion && !session.questions.some(question => question.text === text && question.status !== 'open'));
    if (!next) { setNotice('Your remaining questions are set aside. Reopen one below or ask your own question.'); return; }
    edit(current => ({ ...current, focusQuestion: next, focusReason: 'An optional question; follow it only if it matters to this exploration.' }));
  }

  if (loadingError) return <main className="workshop-loading"><h1>Workshop could not open</h1><p role="alert">{loadingError}</p><button onClick={() => { setLoadingError(''); void store.load().catch(reason => setLoadingError(describeWorkshopError(reason))); }}>Try again</button></main>;
  if (!store.loaded || !session) return <main className="workshop-loading" role="status">Opening your Workshop…</main>;
  const parent = state.sessions.find(item => item.id === session.parentSessionId);
  const fixed = session.selectedDetails.filter(detail => detail.fixed);
  const visibleResult = result ? { ...result, stale: result.stale || result.workingGeneration !== session.workingGeneration } : null;

  return <div className={`story-workshop ${contextOpen ? '' : 'context-hidden'}`} aria-label="Story Workshop">
    <aside className="workshop-navigation" aria-label="Development lenses">
      <h2>Develop your story</h2><nav>{LENSES.map(lens => <button key={lens.id} aria-current={session.lens === lens.id ? 'page' : undefined} disabled={adopting} onClick={() => chooseLens(lens.id)}>{lens.label}</button>)}</nav>
      <div className="workshop-section-heading"><h3>Explorations</h3><button disabled={adopting} onClick={() => addSession()}>New</button></div>
      <nav aria-label="Saved explorations">{state.sessions.map(item => <button key={item.id} aria-current={session.id === item.id ? 'true' : undefined} disabled={adopting || !!adoptionOperation.current} onClick={() => store.edit(current => ({ ...current, currentSessionId: item.id }))}>{item.branchKind === 'whatIf' ? 'What if · ' : ''}{item.title}</button>)}</nav>
      <p className="small-copy">Begin anywhere. Write whenever you want.</p>
    </aside>
    <main className="workshop-workbench" aria-label="Current exploration" onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }}>
      <div className="workshop-mobile-navigation"><label>Development lens<select value={session.lens} onChange={event => chooseLens(event.target.value as WorkshopLens)}>{LENSES.map(lens => <option key={lens.id} value={lens.id}>{lens.label}</option>)}</select></label><label>Exploration<select value={session.id} onChange={event => store.edit(current => ({ ...current, currentSessionId: event.target.value }))}>{state.sessions.map(item => <option value={item.id} key={item.id}>{item.title}</option>)}</select></label><button onClick={() => addSession()}>New exploration</button></div>
      <header className="workshop-heading"><div><h1 ref={heading} tabIndex={-1}>{session.lens === 'overview' && !session.brief ? 'What are you excited about?' : LENSES.find(lens => lens.id === session.lens)!.label}</h1><p>{session.branchKind === 'whatIf' ? 'What-if exploration · separate from your working story' : 'Find a direction. Keep the details that matter.'}</p></div><button aria-expanded={contextOpen} onClick={() => setContextOpen(!contextOpen)}>{contextOpen ? 'Hide working story' : 'Working story & context'}</button></header>
      <div className="workshop-save-status" role="status">{store.status}{store.error && <> · {store.error} <button onClick={() => { void store.flush().catch(report); }}>Retry saving</button></>}</div>
      <details className="workshop-brief" open={!result}><summary>{result ? 'Starting idea · edit or bring notes' : 'Begin with what interests you'}</summary>
        <label>{session.lens === 'overview' ? 'Your idea, image, dialogue, or attraction' : 'What you want to explore'}<textarea value={session.brief} disabled={adopting} maxLength={20000} rows={3} onChange={event => edit(current => ({ ...current, brief: event.target.value, title: current.title === 'A new exploration' ? event.target.value.trim().slice(0, 70) || current.title : current.title }))} placeholder="A city where people repair broken magic…" /></label>
        <div className="workshop-actions"><button disabled={adopting} onClick={() => { setAction('directions'); edit(current => ({ ...current, focusQuestion: 'What are three different ways to begin with this attraction?', focusReason: 'Compare a few mechanisms before committing to a setting, character, or ending.' })); }}>Start with an idea</button><button disabled={adopting} onClick={() => { setAction('directions'); edit(current => ({ ...current, composer: 'Help me find a direction. Offer three contrasting starting attractions without assuming a genre, cast, or ending.' })); }}>Help me find a direction</button><button aria-expanded={notesOpen} onClick={() => setNotesOpen(!notesOpen)}>Bring existing notes</button></div>
        {notesOpen && <div><label>Saved material<select value="" onChange={event => { void bringDocument(event.target.value); }}><option value="">Choose existing notes…</option>{material.map(document => <option key={document.head.documentId} value={document.head.documentId}>{document.title}</option>)}</select></label><label>Or paste notes to preserve<textarea value={session.originalNotes} onChange={event => edit(current => ({ ...current, originalNotes: event.target.value, brief: event.target.value }))} maxLength={20000} /></label><p className="small-copy">Your original stays saved here. Organization is a proposal for you to edit.</p></div>}
      </details>
      {result?.output && <details className="workshop-interpretation"><summary>How this request interpreted your idea</summary><dl><dt>You said</dt><dd>{result.output.interpretation.youSaid}</dd><dt>Possible direction</dt><dd>{result.output.interpretation.possibleDirection}</dd><dt>Still open</dt><dd>{result.output.interpretation.stillOpen}</dd></dl><button onClick={() => edit(current => ({ ...current, direction: result.output!.interpretation.possibleDirection, stillOpen: result.output!.interpretation.stillOpen }))}>Use as an editable direction</button><h3>Suggested next question</h3><p>{result.output.question}</p><p>{result.output.questionReason}</p><button onClick={() => edit(current => ({ ...current, focusQuestion: result.output!.question, focusReason: result.output!.questionReason }))}>Explore this question</button></details>}
      <section className="workshop-question"><h2>{session.focusQuestion}</h2><p>{session.focusReason}</p><div className="workshop-actions"><button onClick={() => questionStatus('notNow')}>Not now</button><button onClick={() => questionStatus('notRelevant')}>Not relevant</button><button onClick={() => questionStatus('keepMysterious')}>Keep mysterious</button><button onClick={differentQuestion}>Show a different question</button></div>
        {session.lens === 'world' && <details><summary>Explore a different part of this world slice</summary>{WORLD_QUESTIONS.map(question => <button key={question.title} onClick={() => edit(current => ({ ...current, focusQuestion: question.text, focusReason: `Explore ${question.title.toLocaleLowerCase()} at the depth this place needs.` }))}>{question.title}</button>)}<label>Depth<select value={session.depth} onChange={event => edit(current => ({ ...current, depth: event.target.value as WorkshopSession['depth'] }))}><option value="sketch">Sketch</option><option value="develop">Develop</option><option value="document">Document</option></select></label></details>}
        {session.lens === 'people' && <details><summary>Optional character spine</summary><p>Desire · valued commitment · competence · costly habit · relationship pressure · capacity for change</p><button onClick={() => { setAction('situation'); edit(current => ({ ...current, composer: 'Explore behavior under pressure. Use a situation that makes their desire, competence, commitment, and costly habit pull in different directions.' })); }}>Explore through a situation</button></details>}
        {session.lens === 'possibilities' && <button onClick={() => edit(current => ({ ...current, stillOpen: `${current.stillOpen}${current.stillOpen ? '\n' : ''}The ending is intentionally open.` }))}>Leave the ending open</button>}
      </section>
      {!!sessionResults.length && <label className="workshop-result-picker">Saved requests<select value={result?.run.id ?? ''} onChange={event => setSelectedRun(event.target.value)}>{sessionResults.map((item, index) => <option value={item.run.id} key={item.run.id}>{index + 1}. {ACTIONS.find(action => action.id === item.action)?.label ?? item.action} · {item.run.status}</option>)}</select></label>}
      <CandidateBoard reviewKey={String(store.generation)} result={visibleResult} choices={session.choices} selectedDetails={session.selectedDetails} onDevelop={develop} onSelectDetail={selectDetail} onChoice={choice} onSteer={(candidate, instruction) => { choice({ candidateId: candidate.id, status: 'saved', rationale: '', includeInContext: true }); edit(current => ({ ...current, composer: instruction, selectedScope: candidate.title })); setAction('consequences'); }} onExplore={exploreCandidate} disabled={adopting} />
      {!!session.selectedDetails.length && <section className="workshop-tray" aria-label="Selected details"><div className="workshop-section-heading"><h2>Your selected details</h2><button onClick={() => { setAction('synthesize'); edit(current => ({ ...current, composer: 'Combine the selected details, preserving their wording and showing any new assumptions.' })); }}>Prepare to combine</button></div><p className="small-copy">These are in your tray, not yet chosen story material.</p>{session.selectedDetails.map(detail => <div className="workshop-tray-detail" key={detail.id}><textarea aria-label="Selected detail" value={detail.text} disabled={detail.fixed || adopting} onChange={event => edit(current => ({ ...current, selectedDetails: current.selectedDetails.map(item => item.id === detail.id ? { ...item, text: event.target.value } : item) }), true)} /><div><label><input type="checkbox" checked={detail.fixed} disabled={adopting} onChange={event => edit(current => ({ ...current, selectedDetails: current.selectedDetails.map(item => item.id === detail.id ? { ...item, fixed: event.target.checked } : item) }))} />Keep fixed</label><button disabled={detail.fixed || adopting} onClick={() => edit(current => ({ ...current, selectedDetails: current.selectedDetails.filter(item => item.id !== detail.id) }), true)}>Remove</button></div></div>)}</section>}
      <section className="workshop-working" aria-label="Editable working version"><div className="workshop-section-heading"><h2>Working version</h2><button disabled={adopting} onClick={fork}>Explore a what-if</button></div><label>Working title<input value={session.workingTitle} disabled={adopting} maxLength={160} onChange={event => edit(current => ({ ...current, workingTitle: event.target.value }), true)} /></label><label>Develop or edit directly<textarea ref={workingEditor} className="workshop-working-text" value={session.workingText} maxLength={40000} disabled={adopting} onSelect={event => { const element = event.currentTarget; if (element.selectionEnd > element.selectionStart) setCapture({ from: element.selectionStart, to: element.selectionEnd, text: element.value.slice(element.selectionStart, element.selectionEnd), generation: session.workingGeneration }); }} onChange={event => {
        const text = event.target.value;
        if (fixed.some(detail => session.workingText.includes(detail.text) && !text.includes(detail.text))) { setNotice('Remove Keep fixed on that detail before changing it.'); return; }
        edit(current => ({ ...current, workingText: text }), true);
      }} placeholder="Use a direction above, combine details, or write your own version." /></label>
        {capture && <div className="workshop-scope"><strong>Feedback scope: selected passage</strong><blockquote>{capture.text}</blockquote><button onClick={() => setCapture(null)}>Use whole working version</button></div>}
        <div className="workshop-actions"><button className="primary-button" disabled={!session.workingText.trim() || adopting} onClick={() => beginAdoption()}>Use this version</button>{(session.lens === 'themes' || result?.action === 'moment') && <button disabled={adopting || !session.workingText.trim()} onClick={prepareVoiceGuidance}>Propose voice guidance from this sample</button>}</div>
        {session.lens === 'themes' && <p className="small-copy">You can paste or edit your own short sample here and explain its qualities in Your direction. Reader experience and content intensity are separate choices in Creative preferences.</p>}
        {result?.action === 'moment' && <p className="small-copy">Noncanon experiment. Review any voice guidance separately; a pleasing passage does not adopt its events.</p>}
      </section>
      {parent && <details className="workshop-branch-compare"><summary>Compare with the working exploration</summary><div><section><h3>Working exploration now</h3><p className="workshop-prose">{parent.workingText || parent.direction || parent.brief}</p></section><section><h3>This what-if</h3><p className="workshop-prose">{session.workingText || session.direction || session.brief}</p></section></div><p className="small-copy">Only Use this version can propose changes to saved story material.</p></details>}
      <section className="workshop-composer" aria-label="Steer this exploration"><h2>What would you like to explore next?</h2><p><strong>Scope:</strong> {capture ? 'Selected passage in working version' : session.selectedScope}</p><label>Next action<select value={action} disabled={adopting} onChange={event => setAction(event.target.value)}>{ACTIONS.map(item => <option key={item.id} value={item.id}>{item.label}</option>)}</select></label>{action === 'subvert' && <><label>Convention to transform<input value={session.selectedScope === 'Whole working version' ? '' : session.selectedScope} maxLength={160} onChange={event => edit(current => ({ ...current, selectedScope: event.target.value }))} placeholder="For example, inherited special power" /></label><label>Transformation<select value={subversion} onChange={event => setSubversion(event.target.value)}><option value="">Choose a transformation…</option>{SUBVERSIONS.map(item => <option key={item}>{item}</option>)}</select></label></>}<label>Your direction<textarea value={session.composer} maxLength={12000} disabled={adopting} onChange={event => edit(current => ({ ...current, composer: event.target.value }))} placeholder="Keep the apprenticeship, add salvage expeditions, and make the guild’s safety concerns partly justified." /></label><div className="workshop-generation-footer"><span>{selectedModel?.label ?? 'No model available'}{providers.state?.settings.active.reasoning ? ` · ${providers.state.settings.active.reasoning}` : ''}{providers.state?.settings.active.serviceTier ? ` · ${providers.state.settings.active.serviceTier}` : ''}</span>{live ? <button disabled={live.run.status === 'stopping'} onClick={() => { void stop(); }}>{live.run.status === 'stopping' ? 'Stopping…' : 'Stop request'}</button> : <button className="primary-button" disabled={(!canGenerate && !pendingRequest.current) || requestBusy || adopting} onClick={() => { void generate(); }}>{requestBusy ? 'Preparing…' : pendingRequest.current ? 'Check request status' : 'Explore'}</button>}</div>
        {!canGenerate && <p className="small-copy">{selectedModel?.statusDetail || 'Connect an available model in Settings to generate.'} You can keep editing, saving, and organizing here.</p>}
        {result && ['failed', 'stopped', 'interrupted'].includes(result.run.status) && <div className="workshop-actions"><button disabled={!canGenerate || requestBusy} onClick={() => { void generate(result.action); }}>Try again with current context</button><button onClick={() => { void retryDiscussionSave(access, result.run.target.documentId, result.run.id).then(() => store.refreshResults()).catch(report); }}>Check retained result save</button></div>}
      </section>
      {(adoptionForm || preview) && <section className="workshop-adoption" aria-label="Adoption preview"><h2>{preview ? 'Choose this version for your story' : 'Where should this version go?'}</h2><p>Saved material stays author only. Chapters and character knowledge are not changed.</p>
        {adoptionForm && <AdoptionImpacts impacts={impactDrafts} documents={project.documents} onChange={setImpactDrafts} />}
        {adoptionForm && <>{targets.map((target, index) => <fieldset key={target.id}><legend>Material {index + 1}</legend><label>Destination<select value={target.documentId} onChange={event => setTargets(current => current.map((value, at) => at === index ? { ...value, documentId: event.target.value } : value))}><option value="">Create a new document</option>{material.map(document => <option value={document.head.documentId} key={document.head.documentId}>{document.title}</option>)}</select></label>{!target.documentId && <><label>Title<input required value={target.title} maxLength={160} onChange={event => setTargets(current => current.map((value, at) => at === index ? { ...value, title: event.target.value } : value))} /></label><label>Kind<select value={target.kind} onChange={event => setTargets(current => current.map((value, at) => at === index ? { ...value, kind: event.target.value } : value))}>{['world', 'character', 'theme', 'hook', 'scene', 'note'].map(kind => <option key={kind}>{kind}</option>)}</select></label></>}{target.documentId && <label>Change<select value={target.mode} onChange={event => setTargets(current => current.map((value, at) => at === index ? { ...value, mode: event.target.value as 'add' | 'replace' } : value))}><option value="add">Add to existing material</option><option value="replace">Replace this document’s content</option></select></label>}<label>Content to choose<textarea value={target.text} onChange={event => setTargets(current => current.map((value, at) => at === index ? { ...value, text: event.target.value } : value))} /></label>{targets.length > 1 && <button onClick={() => setTargets(current => current.filter((_, at) => at !== index))}>Remove this target</button>}</fieldset>)}<button onClick={() => setTargets(current => [...current, { id: crypto.randomUUID(), documentId: '', title: '', kind: 'note', mode: 'add', text: '' }])}>Include related material in this decision</button><AdoptionLinks participants={participants} links={adoptionLinks} onChange={setAdoptionLinks} /><label>Why this version?<textarea value={rationale} maxLength={4000} onChange={event => setRationale(event.target.value)} placeholder="Keep the guild morally mixed: it protects people and its own status." /></label><div className="workshop-actions"><button disabled={adopting} onClick={() => setAdoptionForm(false)}>Cancel</button><button className="primary-button" disabled={adopting || !validAdoptionLinks(adoptionLinks, participants) || targets.some(target => !target.text.trim() || !target.documentId && !target.title.trim())} onClick={() => { void prepareAdoption(); }}>Preview all changes</button></div></>}
        {preview && <><AdoptionPreview preview={preview} documents={project.documents} /><div className="workshop-actions"><button disabled={adopting || !!adoptionOperation.current} onClick={() => setPreview(null)}>Keep exploring</button><button className="primary-button" disabled={adopting} onClick={() => { void commitAdoption(); }}>{adoptionOperation.current ? 'Check adoption result' : 'Confirm Use this version'}</button></div></>}
      </section>}
      {session.lens === 'people' && <Relationships state={state} documents={material} session={session} onChange={change => store.edit(change)} onOpenDocument={onOpenDocument} />}
      {!!session.questions.length && <details><summary>Open questions and intentional unknowns</summary>{session.questions.map(question => <div className="workshop-question-record" key={question.id}><p>{question.text}</p><label>State<select value={question.status} onChange={event => edit(current => ({ ...current, questions: current.questions.map(item => item.id === question.id ? { ...item, status: event.target.value as typeof question.status } : item) }))}><option value="open">Open</option><option value="notNow">Not now</option><option value="notRelevant">Not relevant</option><option value="keepMysterious">Keep mysterious</option></select></label><label>Unknown to<select value={question.unknownTo} onChange={event => edit(current => ({ ...current, questions: current.questions.map(item => item.id === question.id ? { ...item, unknownTo: event.target.value as typeof question.unknownTo } : item) }))}><option value="author">Me, the author</option><option value="reader">The reader</option><option value="both">Both</option></select></label></div>)}</details>}
      <details className="workshop-recap"><summary>Your stopping point</summary><WorkshopRecap session={session} decisions={state.decisions} saved={store.generation === store.savedGeneration} onOpenDocument={onOpenDocument} /><button onClick={() => { void store.flush().then(() => workshopHistory(access)).then(setHistory).catch(report); }}>Saved exploration versions</button>{history && <ul>{history.slice(0, 30).map(snapshot => <li key={snapshot.version}><details><summary>Workshop version {snapshot.version}</summary>{snapshot.state.sessions.filter(item => item.id === session.id).map(item => <div key={item.id}><p className="workshop-prose">{item.workingText || item.brief}</p><button onClick={() => { const restored = { ...structuredClone(item), id: crypto.randomUUID(), title: `Recovered: ${item.title}`, parentSessionId: session.id, branchKind: 'whatIf' as const, activeRunId: null }; restored.anchorDocumentId = `workshop-${restored.id}`; store.edit(current => ({ ...current, currentSessionId: restored.id, sessions: [...current.sessions, restored] })); }}>Recover as a separate exploration</button></div>)}</details></li>)}</ul>}</details>
      {notice && <p className="workshop-notice" role="status">{notice}</p>}
    </main>
    {contextOpen && <aside className="workshop-context" aria-label="Working story and exploration context"><button className="workshop-context-close" onClick={() => setContextOpen(false)}>Close working story</button><h2>Working story</h2><label>Current direction<textarea value={session.direction} maxLength={12000} disabled={adopting} onChange={event => edit(current => ({ ...current, direction: event.target.value }))} placeholder="An editable direction, once you choose one." /></label><label>Still open<textarea value={session.stillOpen} maxLength={6000} disabled={adopting} onChange={event => edit(current => ({ ...current, stillOpen: event.target.value }))} /></label><Preferences state={state} session={session} onChange={change => { if (!adopting) store.edit(change); }} />
      <section className="workshop-context-preview"><h3>Using for the next exploration</h3><p className="small-copy">Current brief, working version, selected details, and the choices below. The saved request inspector shows what was actually delivered.</p><label><input type="checkbox" checked={session.outsideDirection} disabled={adopting} onChange={event => edit(current => ({ ...current, outsideDirection: event.target.checked }))} />Explore outside my current direction</label><p className="small-copy">Project Must/Never constraints and Keep fixed details remain in force.</p><ul>{applicablePreferences(state, session).map(preference => <li key={preference.id}>{preferenceLabel(preference)} {preference.label} · {preference.scope}</li>)}</ul><details><summary>Include saved material explicitly</summary>{material.map(document => <label key={document.head.documentId}><input type="checkbox" checked={session.includedDocumentIds.includes(document.head.documentId)} onChange={event => edit(current => ({ ...current, includedDocumentIds: event.target.checked ? [...current.includedDocumentIds, document.head.documentId] : current.includedDocumentIds.filter(id => id !== document.head.documentId) }))} />{document.title}</label>)}</details><p className="small-copy">Unrelated chat, rejected candidates, and noncanon moments stay out unless an eligible alternative is explicitly included.</p></section>
      {result && <section><h3>Using for this saved request</h3><RequestContext access={access} packetId={result.run.packetId} /><ContextInspector access={access} packetId={result.run.packetId} delivered={result.run.dispatchState === 'delivered'} refreshKey={result.run.sequence} /></section>}
      <section className="workshop-decisions"><h3>Chosen material</h3>{state.decisions.filter(decision => decision.status === 'chosen').map(decision => <details key={decision.id}><summary>{decision.title}{decision.fixed ? ' · Keep fixed' : ''}</summary><p>Author only · version {decision.head.version}</p><label>Why you chose it<textarea value={decision.rationale} maxLength={4000} onChange={event => store.edit(current => ({ ...current, decisions: current.decisions.map(item => item.id === decision.id ? { ...item, rationale: event.target.value } : item) }))} /></label><label><input type="checkbox" checked={decision.fixed} onChange={event => store.edit(current => ({ ...current, decisions: current.decisions.map(item => item.id === decision.id ? { ...item, fixed: event.target.checked } : item) }))} />Keep fixed</label><button onClick={() => onOpenDocument(decision.documentId)}>Open source</button><button onClick={() => { const source = project.documents.find(document => document.head.documentId === decision.documentId); edit(current => ({ ...current, focusDocumentId: decision.documentId, composer: `What might change if we revisit ${decision.title}? Distinguish basis, assumptions, and possible tensions.`, includedDocumentIds: [...new Set([...current.includedDocumentIds, decision.documentId])], selectedScope: source?.title ?? decision.title })); setAction('consequences'); }}>What might this change?</button><button onClick={() => store.edit(current => ({ ...current, decisions: current.decisions.map(item => item.id === decision.id ? { ...item, status: 'archived' } : item) }))}>Archive choice</button></details>)}{!state.decisions.some(decision => decision.status === 'chosen') && <p className="small-copy">No version chosen yet.</p>}</section>
      {!!state.impacts.length && <section><h3>Affected material</h3>{state.impacts.map(impact => <div key={impact.id}><strong>{IMPACT_LABELS[impact.kind]} · {project.documents.find(document => document.head.documentId === impact.documentId)?.title ?? 'Saved material'}</strong><p>{impact.reason}</p>{impact.candidateId && <p className="small-copy">Suggested by: {store.results.flatMap(saved => saved.output?.candidates ?? []).find(candidate => candidate.id === impact.candidateId)?.title ?? 'A retained exploration candidate'}</p>}{impact.relationshipId && <p className="small-copy">Relationship: {state.relationships.find(relationship => relationship.id === impact.relationshipId)?.description ?? 'A retained relationship'}</p>}<button onClick={() => onOpenDocument(impact.documentId)}>Review source</button><label>Review status<select value={impact.status} onChange={event => store.edit(current => ({ ...current, impacts: current.impacts.map(item => item.id === impact.id ? { ...item, status: event.target.value as typeof impact.status } : item) }))}><option value="needsReview">Needs review</option><option value="acknowledged">Reviewed</option><option value="intentional">Intentional ambiguity</option></select></label></div>)}</section>}
    </aside>}
  </div>;
});
