import { explorationRequest } from '../workshop/explorationRequest';
import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useReducer, useRef, useState } from 'react';
import { readDocument, type DocumentRecord, type OpenedProject } from '../ipc/projects';
import { adoptWorkshop, previewWorkshopAdoption, startWorkshop, workshopHistory, type CandidateChoice, type WorkshopAdoptionPreview, type WorkshopAdoptionTarget, type WorkshopCandidate, type WorkshopImpactDraft, type WorkshopRelationship, type WorkshopResult, type WorkshopSession, type WorkshopSnapshot, type WorkshopState } from '../ipc/workshop';
import { retryDiscussionSave, stopDiscussion } from '../ipc/discussions';
import { useProviders } from '../providers/ProviderContext';
import { sameModel, type ModelSelection } from '../ipc/providers';
import { ContextInspector } from '../assistant/ContextInspector';
import { CandidateBoard } from '../workshop/CandidateBoard';
import { Preferences, applicablePreferences, preferenceLabel } from '../workshop/Preferences';
import { ACTIONS, LENSES, NOTES_ORGANIZATION_BRIEF, NOTES_ORGANIZATION_SCOPE, SUBVERSIONS, WORLD_QUESTIONS, type WorkshopLens } from '../workshop/catalog';
import { describeWorkshopError, newSession, WorkshopStore } from '../workshop/store';
import { appendText, plainText, textDocument } from '../workshop/text';
import { Relationships } from '../workshop/Relationships';
import { DocumentAliases } from '../story/DocumentAliases';
import { AdoptionLinks, adoptionParticipants, validAdoptionLinks, type AdoptionMaterialDraft, type AdoptionLinkDraft } from '../workshop/AdoptionLinks';
import { AdoptionImpacts, IMPACT_LABELS } from '../workshop/AdoptionImpacts';
import { AdoptionPreview } from '../workshop/AdoptionPreview';
import { WorkshopRecap } from '../workshop/WorkshopRecap';
import { RequestContext } from '../workshop/RequestContext';
import { NextExplorationContext } from '../workshop/NextExplorationContext';
import { BranchComparison } from '../workshop/BranchComparison';
import { WorkshopContextPanel } from '../workshop/WorkshopContextPanel';
import { StoryPossibilities, POSSIBILITY_KINDS } from '../workshop/StoryPossibilities';
import { nextWorkshopQuestion } from '../workshop/questionSuggestions';
import { selectedBranchCandidates } from '../workshop/branchEvidence';
import '../workshop/workshop.css';

export interface WorkshopHandle { flush(): Promise<void> }
interface Props { project: OpenedProject; navigationBusy?: boolean; onOpenDocument(documentId: string): void; onDocumentsChanged(documents: DocumentRecord[]): void; onError(message: string): void }
type Capture = { from: number; to: number; text: string; generation: string };

export const Workshop = forwardRef<WorkshopHandle, Props>(function Workshop({ project, navigationBusy = false, onOpenDocument, onDocumentsChanged, onError }, ref) {
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
  const requestStarting = useRef(false);
  const [history, setHistory] = useState<WorkshopSnapshot[] | null>(null); const [notesOpen, setNotesOpen] = useState(false);
  const sourceRead = useRef(0);
  const navigationBusyRef = useRef(navigationBusy);
  navigationBusyRef.current = navigationBusy;
  const [namesDocumentId, setNamesDocumentId] = useState('');
  const namesSelect = useRef<HTMLSelectElement>(null);
  const namesGuard = useRef<(() => Promise<void>) | null>(null);
  const registerNamesGuard = useCallback((guard: (() => Promise<void>) | null) => { namesGuard.current = guard; }, []);
  const composing = useRef(false); const [composingState, setComposingState] = useState(false); const workingEditor = useRef<HTMLTextAreaElement>(null); const heading = useRef<HTMLHeadingElement>(null);
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
  const namedMaterial = material.filter(document => document.kind === 'character' || document.kind === 'world');
  const namesDocument = namedMaterial.find(document => document.head.documentId === namesDocumentId);
  const activeRelationship = session?.relationshipId ? state.relationships.find(item => item.id === session.relationshipId) : null;
  const relationshipUnavailable = !!session?.relationshipId && (!activeRelationship || activeRelationship.status === 'archived');
  const reviewableImpacts = state.impacts.filter(impact => !impact.documentId.startsWith('workshop-'));
  const participants = adoptionParticipants(material, targets);
  store.locked = adopting || adoptionOperation.current !== null;

  useImperativeHandle(ref, () => ({ flush: async () => {
    if (adoptionOperation.current) throw new Error('Finish checking the Workshop adoption before leaving this project.');
    if (requestStarting.current || pendingRequest.current) throw new Error('Check the pending Workshop request before leaving this project.');
    if (composing.current) throw new Error('Finish entering the current text before leaving this project.');
    await namesGuard.current?.();
    await store.flush();
  } }), [store]);
  useEffect(() => {
    const unsubscribe = store.subscribe(redraw); let disposed = false;
    void store.load().then(() => {
      if (disposed || store.state.sessions.length) return;
      const first = newSession(); store.edit(current => ({ ...current, currentSessionId: first.id, sessions: [first] }));
    }).catch(reason => { if (!disposed) setLoadingError(describeWorkshopError(reason)); });
    return () => { disposed = true; sourceRead.current += 1; unsubscribe(); store.dispose(); };
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
  useEffect(() => { if (navigationBusy) sourceRead.current += 1; }, [navigationBusy]);
  useEffect(() => { sourceRead.current += 1; setCapture(null); setSelectedRun(null); setPreview(null); setAdoptionForm(false); setHistory(null); }, [session?.id]);

  function edit(change: (session: WorkshopSession) => WorkshopSession, working = false) { if (session && !adopting && !store.locked) store.editSession(session.id, change, working); }
  function report(reason: unknown) { const message = describeWorkshopError(reason); setNotice(message); onError(message); }
  async function chooseNamesDocument(documentId: string) {
    if (navigationBusyRef.current) return;
    const sequence = sourceRead.current;
    try {
      await namesGuard.current?.();
      if (sequence === sourceRead.current) setNamesDocumentId(documentId);
    } catch (reason) { if (sequence === sourceRead.current) report(reason); }
  }
  async function saveRelationship(change: (state: WorkshopState) => WorkshopState) {
    if (store.locked || navigationBusyRef.current) return;
    const readSequence = sourceRead.current;
    store.edit(change);
    try {
      await store.flush();
      // Completed requests are not polled. Re-read their relationship freshness
      // after a deliberate relationship edit without starting another request.
      await store.refreshResults();
    } catch (reason) { if (readSequence === sourceRead.current) report(reason); }
  }
  function chooseLens(lens: WorkshopLens) {
    if (!session) return;
    const focus = LENSES.find(item => item.id === lens)!;
    chooseQuestion(focus.question, focus.reason, lens);
    setAction(lens === 'people' ? 'situation' : lens === 'themes' ? 'moment' : lens === 'possibilities' ? 'arc' : 'directions');
    setCapture(null); heading.current?.focus();
  }
  function canChangeExploration() {
    if (requestStarting.current || pendingRequest.current) {
      setNotice('Check the pending request before opening another exploration. You can keep editing this one.');
      return false;
    }
    return !adopting && !adoptionOperation.current;
  }
  function chooseExploration(id: string) {
    if (canChangeExploration()) store.edit(current => ({ ...current, currentSessionId: id }));
  }
  function addSession(lens: WorkshopLens = session?.lens ?? 'overview') {
    if (!canChangeExploration()) return;
    const next = newSession(lens);
    store.edit(current => ({ ...current, currentSessionId: next.id, sessions: [...current.sessions, next] })); setNotice('A new exploration is ready. Nothing has been generated.');
  }
  function canOrganizeNotes() {
    return !!session?.originalNotes.trim() && canGenerate && !anyLive && !requestStarting.current && !requestBusy && !pendingRequest.current
      && !adopting && !store.locked && !adoptionOperation.current && !navigationBusy && !composingState && !composing.current;
  }
  function organizeNotes() {
    if (!session || !canOrganizeNotes()) return;
    const source = session;
    const next = newSession('notebook');
    next.title = 'Organize original notes';
    next.workingTitle = 'Organized notes';
    next.parentSessionId = null;
    next.brief = NOTES_ORGANIZATION_BRIEF;
    next.direction = '';
    next.stillOpen = '';
    next.focusQuestion = 'How should these notes be organized for future writing?';
    next.focusReason = 'Compare structures that preserve the author’s material, wishes, and open questions without deciding canon.';
    next.focusDocumentId = null;
    next.includedDocumentIds = [];
    next.relationshipId = null;
    next.selectedScope = NOTES_ORGANIZATION_SCOPE;
    next.originalNotes = source.originalNotes;
    next.composer = '';
    const localPreferences = state.preferences.filter(preference => preference.scope === 'exploration' && preference.targetId === source.id)
      .map(preference => ({ ...preference, id: crypto.randomUUID(), targetId: next.id }));
    store.edit(current => ({ ...current, sessions: [...current.sessions, next], preferences: [...current.preferences, ...localPreferences], currentSessionId: next.id }));
    setSelectedRun(null); setAction('directions'); setCapture(null); setNotesOpen(false);
    setNotice('A separate notes organization proposal is ready. Your original notes and working draft remain unchanged.');
    void generate('directions', undefined, next.id);
  }
  async function bringDocument(documentId: string) {
    if (!session || !documentId || adopting || adoptionOperation.current || navigationBusyRef.current) return;
    const request = ++sourceRead.current;
    const generation = store.generation;
    const owns = () => request === sourceRead.current && store.state.currentSessionId === session.id && !store.locked && !navigationBusyRef.current;
    try {
      const source = await readDocument(access, documentId);
      if (!owns()) return;
      if (store.generation !== generation) { setNotice('Your exploration changed while these notes opened. Choose the material again when you are ready.'); return; }
      if (source.head.documentId !== documentId || source.kind === 'chapter') throw new Error('Choose available development material for this exploration.');
      edit(current => ({ ...current, relationshipId: null, focusDocumentId: documentId, title: source.title, workingTitle: source.title, originalNotes: plainText(source.body), brief: plainText(source.body), selectedScope: `Element: ${source.title}`, includedDocumentIds: [...new Set([...current.includedDocumentIds, documentId])] }), true);
      setCapture(null);
      setNotesOpen(false); setNotice(`Original ${source.title} preserved. Workshop changes stay here until you choose Use this version.`);
    } catch (reason) { if (owns()) report(reason); }
  }
  async function exploreRelationship(relationship: WorkshopRelationship) {
    if (!session || adopting || adoptionOperation.current || navigationBusyRef.current || relationship.status === 'archived') return;
    if (!canChangeExploration()) return;
    const request = ++sourceRead.current;
    const generation = store.generation;
    const owns = () => request === sourceRead.current && store.state.currentSessionId === session.id && !store.locked && !navigationBusyRef.current;
    try {
      const ids = [relationship.fromDocumentId, relationship.toDocumentId];
      const sources = await Promise.all(ids.map(id => readDocument(access, id)));
      if (!owns()) return;
      if (store.generation !== generation) { setNotice('Your exploration changed while the participants opened. Choose the relationship again when you are ready.'); return; }
      if (ids[0] === ids[1] || relationship.sourceHeads.length !== 2 || sources.some((source, index) => {
        const expected = relationship.sourceHeads.find(head => head.documentId === ids[index]);
        return !expected || source.head.documentId !== ids[index] || !['character', 'world'].includes(source.kind)
          || source.head.version !== expected.version || source.head.bodyHash !== expected.bodyHash;
      })) throw new Error('A participant changed. Review the relationship against its current sources before exploring it.');
      const direction = `${sources[0].title} → ${relationship.type} → ${sources[1].title}`;
      const next = { ...newSession('people'), title: direction.slice(0, 160), workingTitle: direction.slice(0, 160),
        relationshipId: relationship.id, selectedScope: `Relationship: ${direction}`, includedDocumentIds: ids,
        brief: relationship.description, workingText: relationship.description,
        focusQuestion: `What could change in ${direction}?`, focusReason: 'Explore this direction of the relationship without assuming the reverse.',
        composer: 'Explore what each participant wants from the other, what each misunderstands, what keeps them connected, and what could change. Keep the two perspectives distinct; do not assume trust or knowledge is mutual. Distinguish established material from uncertain possibilities. Keep unrelated biography and world history unchanged.' };
      if (!canChangeExploration()) return;
      store.edit(current => ({ ...current, sessions: [...current.sessions, next], currentSessionId: next.id }));
      setAction('consequences'); setCapture(null); setNotesOpen(false);
      setNotice('A separate relationship exploration is ready. Review the direction, then choose Explore when you want suggestions.');
    } catch (reason) { if (owns()) report(reason); }
  }
  function selectDetail(candidate: WorkshopCandidate, text: string) {
    if (!text || !session) return;
    if (result?.action === 'moment') { setNotice('Use this noncanon passage as a voice sample. Its events do not belong in the story detail tray.'); return; }
    edit(current => ({ ...current, selectedDetails: [...current.selectedDetails, { id: crypto.randomUUID(), candidateId: candidate.id, text, fixed: false }] }), true);
    setNotice('Detail added to your tray. It has not been chosen for the story.');
  }
  function develop(candidate: WorkshopCandidate) {
    if (!session) return;
    if (result?.action === 'moment') { setNotice('Propose voice guidance from this noncanon sample before choosing style instructions for the story.'); return; }
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
    setCapture(null); setNotice('Working version updated. Review it before choosing it for the story.'); workingEditor.current?.focus();
  }
  function choice(value: CandidateChoice) { edit(current => ({ ...current, choices: [...current.choices.filter(item => item.candidateId !== value.candidateId), value] })); }
  function appendProvisionalInstruction(current: string, instruction: string): string | null {
    const existing = current.trim();
    if (!existing) return instruction.length <= 12000 ? instruction : null;
    if (existing.includes(instruction)) return existing.length <= 12000 ? current : null;
    const appended = `${existing}\n\n${instruction}`;
    return appended.length <= 12000 ? appended : null;
  }
  function candidateComparisonInstruction(candidate: WorkshopCandidate): string {
    return [
      'This is an unaccepted possibility for comparison.',
      `Questioned candidate: ${candidate.title}`,
      `Candidate direction: ${candidate.content}`,
    ].join('\n');
  }
  function prepareCandidateComparison(candidate: WorkshopCandidate, instruction: string): boolean {
    if (!session) return false;
    const nextComposer = appendProvisionalInstruction(session.composer, instruction);
    if (!nextComposer) {
      setNotice('This comparison is too large to add to the saved direction. Shorten the current direction or candidate text first.');
      return false;
    }
    edit(current => ({ ...current, composer: appendProvisionalInstruction(current.composer, instruction) ?? current.composer, selectedScope: candidate.title }));
    setCapture(null);
    setAction('consequences');
    setNotice('A provisional consequence comparison is ready. Review the exact candidate, basis, and assumption, then choose Explore when you want suggestions.');
    return true;
  }
  async function generate(nextAction = action, candidate?: WorkshopCandidate, targetSessionId = session?.id) {
    const target = targetSessionId ? store.state.sessions.find(item => item.id === targetSessionId) : null;
    const targetResults = target ? store.results.filter(item => item.sessionId === target.id) : [];
    const targetResult = targetResults.find(item => item.run.id === selectedRun) ?? targetResults.at(-1) ?? null;
    const targetLive = targetResults.find(item => ['queued', 'running', 'stopping'].includes(item.run.status));
    const targetCapture = target && target.id === session?.id ? capture : null;
    if (!target || requestStarting.current || requestBusy || targetLive || (!pendingRequest.current && (!canGenerate || !providers.state)) || composing.current) return;
    if (nextAction === 'subvert' && !pendingRequest.current && (!subversion || !target.selectedScope.trim() || target.selectedScope === 'Whole working version')) {
      setNotice('Name the convention and choose a transformation before exploring its subversion.'); return;
    }
    requestStarting.current = true; setRequestBusy(true); setNotice('');
    try {
      await store.flush();
      const current = store.state.sessions.find(item => item.id === target.id)!;
      const isCurrentSession = current.id === session?.id;
      pendingRequest.current ??= { operationId: crypto.randomUUID(), exploration: explorationRequest({
        session: current, version: store.version, action: nextAction, candidate,
        dimension: targetResult?.output?.dimension, capture: targetCapture, isCurrentSession, subversion,
      }), selection: structuredClone(providers.state!.settings.active) };
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
    finally { requestStarting.current = false; setRequestBusy(false); }
  }
  async function stop() {
    if (!live) return;
    try { await stopDiscussion(access, live.run.id); await store.refreshResults(); setNotice('Stop requested. Work already sent may still incur provider usage. Partial output remains available.'); } catch (reason) { report(reason); }
  }
  function fork() {
    if (!session || !canChangeExploration()) return;
    const next = { ...structuredClone(session), id: crypto.randomUUID(), parentSessionId: session.id, branchKind: 'whatIf' as const, title: `What if: ${session.title}`, activeRunId: null };
    next.anchorDocumentId = `workshop-${next.id}`;
    const localPreferences = state.preferences.filter(preference => preference.scope === 'exploration' && preference.targetId === session.id).map(preference => ({ ...preference, id: crypto.randomUUID(), targetId: next.id }));
    store.edit(current => ({ ...current, sessions: [...current.sessions, next], currentSessionId: next.id, preferences: [...current.preferences, ...localPreferences] }));
    setNotice('An isolated what-if exploration is ready. Existing story documents are unchanged.');
  }
  function beginAdoption() {
    if (!session || !session.workingText.trim()) return;
    const kind = session.lens === 'world' ? 'world' : session.lens === 'people' ? 'character' : session.lens === 'themes' ? 'theme' : session.lens === 'possibilities' ? 'hook' : 'note';
    setTargets(session.relationshipId ? [] : [{ id: crypto.randomUUID(), documentId: session.focusDocumentId ?? '', title: session.workingTitle || session.title, kind, mode: 'add', text: session.workingText }]);
    setAdoptionLinks([]);
    const affected = new Map<string, WorkshopImpactDraft>();
    for (const { candidate } of selectedBranchCandidates(state, session, store.results)) {
      for (const target of candidate.affectedTargets) {
        // The blank request anchor is internal bookkeeping, not story material.
        if (target.documentId.startsWith('workshop-')) continue;
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
    const instruction = candidateComparisonInstruction(candidate);
    if (!session || !appendProvisionalInstruction(session.composer, instruction)) {
      setNotice('This comparison is too large to add to the saved direction. Shorten the current direction or candidate text first.');
      return;
    }
    edit(current => ({ ...current, composer: appendProvisionalInstruction(current.composer, instruction) ?? current.composer, selectedScope: 'Whole working version' }));
    setAction('subvert'); setSubversion(''); setCapture(null);
    setNotice('This direction is a provisional comparison. Name its convention and choose the transformation below, then Explore.');
  }
  async function prepareAdoption() {
    if (!session || composing.current) return;
    if (!targets.length || targets.some(target => !target.text.trim() || !target.documentId && !target.title.trim())) { setNotice('Choose where this version belongs and review its content before previewing.'); return; }
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
    edit(current => {
      const previous = current.questions.find(question => question.text === current.focusQuestion);
      return { ...current, questions: [...current.questions.filter(question => question.text !== current.focusQuestion), { id: previous?.id ?? crypto.randomUUID(), text: current.focusQuestion, reason: current.focusReason, status, unknownTo: previous?.unknownTo ?? 'both' }] };
    });
    setNotice(status === 'keepMysterious' ? 'Mystery preserved. Choose whether it is unknown to you, the reader, or both below.' : status === 'notNow' ? 'Question saved for later.' : 'Question marked not relevant.');
  }

  function chooseQuestion(text: string, reason: string, lens?: WorkshopLens) {
    if (!session) return;
    const disposition = session.questions.find(question => question.text === text && question.status !== 'open');
    if (disposition) {
      if (lens) edit(current => ({ ...current, lens }));
      const label = disposition.status === 'notNow' ? 'Not now' : disposition.status === 'notRelevant' ? 'Not relevant' : 'Keep mysterious';
      setNotice(`This question is marked ${label}. Reopen it under Open questions and intentional unknowns before exploring it again.`);
      return;
    }
    edit(current => ({ ...current, ...(lens ? { lens } : {}), focusQuestion: text, focusReason: reason }));
    setNotice('');
  }

  function differentQuestion() {
    if (!session) return;
    const chosenIds = new Set(state.decisions.filter(decision => decision.status === 'chosen').map(decision => decision.documentId));
    const chosenText = material.filter(document => chosenIds.has(document.head.documentId)).map(document => plainText(document.body)).join('\n');
    const suggestion = result?.output && !result.stale && result.workingGeneration === session.workingGeneration
      ? { text: result.output.question, reason: result.output.questionReason } : undefined;
    const next = nextWorkshopQuestion(session, chosenText, suggestion);
    if (!next) { setNotice('Your remaining questions are set aside. Reopen one below or ask your own question.'); return; }
    chooseQuestion(next.text, next.reason);
  }

  if (loadingError) return <main className="workshop-loading"><h1>Workshop could not open</h1><p role="alert">{loadingError}</p><button onClick={() => { setLoadingError(''); void store.load().catch(reason => setLoadingError(describeWorkshopError(reason))); }}>Try again</button></main>;
  if (!store.loaded || !session) return <main className="workshop-loading" role="status">Opening your Workshop…</main>;
  const fixed = session.selectedDetails.filter(detail => detail.fixed);
  const protectedChoices = state.decisions.filter(decision => decision.fixed && decision.documentId === session.focusDocumentId);
  const visibleResult = result ? { ...result, stale: result.stale || result.workingGeneration !== session.workingGeneration } : null;
  const organizeNotesDisabled = !canOrganizeNotes();

  return <div className={`story-workshop ${contextOpen ? '' : 'context-hidden'}`} aria-label="Story Workshop" inert={navigationBusy} aria-busy={navigationBusy ? 'true' : undefined}>
    <aside className="workshop-navigation" aria-label="Development lenses">
      <h2>Develop your story</h2><nav>{LENSES.map(lens => <button key={lens.id} aria-current={session.lens === lens.id ? 'page' : undefined} disabled={adopting} onClick={() => chooseLens(lens.id)}>{lens.label}</button>)}</nav>
      <div className="workshop-section-heading"><h3>Explorations</h3><button disabled={adopting} onClick={() => addSession()}>New</button></div>
      <nav aria-label="Saved explorations">{state.sessions.map(item => <button key={item.id} aria-current={session.id === item.id ? 'true' : undefined} disabled={adopting || requestBusy || !!pendingRequest.current || !!adoptionOperation.current} onClick={() => chooseExploration(item.id)}>{item.branchKind === 'whatIf' ? 'What if · ' : ''}{item.title}</button>)}</nav>
      <p className="small-copy">Begin anywhere. Write whenever you want.</p>
    </aside>
    <main className="workshop-workbench" aria-label="Current exploration" onCompositionStart={() => { composing.current = true; setComposingState(true); }} onCompositionEnd={() => { composing.current = false; setComposingState(false); }}>
      <div className="workshop-mobile-navigation"><label>Development lens<select value={session.lens} onChange={event => chooseLens(event.target.value as WorkshopLens)}>{LENSES.map(lens => <option key={lens.id} value={lens.id}>{lens.label}</option>)}</select></label><label>Exploration<select value={session.id} disabled={adopting || requestBusy || !!pendingRequest.current} onChange={event => chooseExploration(event.target.value)}>{state.sessions.map(item => <option value={item.id} key={item.id}>{item.title}</option>)}</select></label><button onClick={() => addSession()}>New exploration</button></div>
      <header className="workshop-heading"><div><h1 ref={heading} tabIndex={-1}>{session.lens === 'overview' && !session.brief ? 'What are you excited about?' : LENSES.find(lens => lens.id === session.lens)!.label}</h1><p>{session.branchKind === 'whatIf' ? 'What-if exploration · separate from your working story' : 'Find a direction. Keep the details that matter.'}</p></div><button aria-expanded={contextOpen} onClick={() => setContextOpen(!contextOpen)}>{contextOpen ? 'Hide working story' : 'Working story & context'}</button></header>
      <div className="workshop-save-status" role="status">{store.status}{store.error && <> · {store.error} <button onClick={() => { void store.flush().catch(report); }}>Retry saving</button></>}</div>
      <details className="workshop-brief" open={!result}><summary>{result ? 'Starting idea · edit or bring notes' : 'Begin with what interests you'}</summary>
         <label>{session.lens === 'overview' ? 'Your idea, image, dialogue, or attraction' : 'What you want to explore'}<textarea value={session.brief} disabled={adopting || store.locked} maxLength={20000} rows={3} onChange={event => edit(current => ({ ...current, brief: event.target.value, title: current.title === 'A new exploration' ? event.target.value.trim().slice(0, 70) || current.title : current.title }))} placeholder="A city where people repair broken magic…" /></label>
        <div className="workshop-actions"><button disabled={adopting} onClick={() => { setAction('directions'); chooseQuestion('What are three different ways to begin with this attraction?', 'Compare a few mechanisms before committing to a setting, character, or ending.'); }}>Start with an idea</button><button disabled={adopting} onClick={() => { setAction('directions'); edit(current => ({ ...current, composer: 'Help me find a direction. Offer three contrasting starting attractions without assuming a genre, cast, or ending.' })); }}>Help me find a direction</button><button aria-expanded={notesOpen} onClick={() => setNotesOpen(!notesOpen)}>Bring existing notes</button></div>
       {notesOpen && <div><label>Saved material<select value="" onChange={event => { void bringDocument(event.target.value); }}><option value="">Choose existing notes…</option>{material.map(document => <option key={document.head.documentId} value={document.head.documentId}>{document.title}</option>)}</select></label><label>Or paste notes to preserve<textarea value={session.originalNotes} disabled={adopting || store.locked} onChange={event => edit(current => ({ ...current, originalNotes: event.target.value, ...(current.lens === 'notebook' && current.selectedScope === NOTES_ORGANIZATION_SCOPE ? {} : { brief: event.target.value }) }), true)} maxLength={20000} /></label><p className="small-copy">Your original stays saved here. Organization is a proposal for you to edit.</p></div>}
      </details>
      {!!session.originalNotes.trim() && <section className="workshop-notes-organization" aria-label="Original notes organization"><h2>Original notes organization</h2><p className="small-copy">Open a separate editable proposal from these exact notes. Your original notes and current working draft stay unchanged until you choose a version.</p><button disabled={organizeNotesDisabled} onClick={organizeNotes}>Organize these notes</button></section>}
      {result?.output && <details key={result.run.id} className="workshop-interpretation" open><summary>How this request interpreted your idea</summary><section aria-label="AI interpretation"><h3>AI suggestion</h3><dl><dt>You said</dt><dd>{result.output.interpretation.youSaid}</dd><dt>Possible direction</dt><dd>{result.output.interpretation.possibleDirection}</dd><dt>Still open</dt><dd>{result.output.interpretation.stillOpen}</dd></dl></section><section aria-label="Current interpretation"><h3>Current interpretation</h3><p className="small-copy">Edit these fields as your working interpretation. The AI suggestion stays above for reference.</p><label>You said<textarea aria-label="Current interpretation · You said" value={session.brief} maxLength={20000} disabled={adopting || store.locked} onChange={event => edit(current => ({ ...current, brief: event.target.value }))} /></label><button disabled={adopting || store.locked} onClick={() => edit(current => ({ ...current, brief: result.output!.interpretation.youSaid }))}>Use suggested You said</button><label>Possible direction<textarea aria-label="Current interpretation · Possible direction" value={session.direction} maxLength={12000} disabled={adopting || store.locked} onChange={event => edit(current => ({ ...current, direction: event.target.value }))} /></label><button disabled={adopting || store.locked} onClick={() => edit(current => ({ ...current, direction: result.output!.interpretation.possibleDirection }))}>Use suggested Possible direction</button><label>Still open<textarea aria-label="Current interpretation · Still open" value={session.stillOpen} maxLength={6000} disabled={adopting || store.locked} onChange={event => edit(current => ({ ...current, stillOpen: event.target.value }))} /></label><button disabled={adopting || store.locked} onClick={() => edit(current => ({ ...current, stillOpen: result.output!.interpretation.stillOpen }))}>Use suggested Still open</button></section><h3>Suggested next question</h3><p>{result.output.question}</p><p>{result.output.questionReason}</p><button disabled={adopting || store.locked} onClick={() => chooseQuestion(result.output!.question, result.output!.questionReason)}>Explore this question</button></details>}
      <section className="workshop-question"><h2>{session.focusQuestion}</h2><p>{session.focusReason}</p><div className="workshop-actions"><button onClick={() => questionStatus('notNow')}>Not now</button><button onClick={() => questionStatus('notRelevant')}>Not relevant</button><button onClick={() => questionStatus('keepMysterious')}>Keep mysterious</button><button onClick={differentQuestion}>Show a different question</button></div>
        {session.lens === 'world' && <details><summary>Explore a different part of this world slice</summary>{WORLD_QUESTIONS.map(question => <button key={question.title} onClick={() => chooseQuestion(question.text, question.reason)}>{question.title}</button>)}<label>Depth<select value={session.depth} onChange={event => edit(current => ({ ...current, depth: event.target.value as WorkshopSession['depth'] }))}><option value="sketch">Sketch</option><option value="develop">Develop</option><option value="document">Document</option></select></label></details>}
        {session.lens === 'people' && <details><summary>Optional character spine</summary><p>Desire · valued commitment · competence · costly habit · relationship pressure · capacity for change</p><button onClick={() => { setAction('situation'); edit(current => ({ ...current, composer: 'Explore behavior under pressure. Use a situation that makes their desire, competence, commitment, and costly habit pull in different directions.' })); }}>Explore through a situation</button></details>}
        {session.lens === 'possibilities' && <button onClick={() => edit(current => ({ ...current, stillOpen: `${current.stillOpen}${current.stillOpen ? '\n' : ''}The ending is intentionally open.` }))}>Leave the ending open</button>}
      </section>
      {!!sessionResults.length && <label className="workshop-result-picker">Saved requests<select value={result?.run.id ?? ''} onChange={event => setSelectedRun(event.target.value)}>{sessionResults.map((item, index) => <option value={item.run.id} key={item.run.id}>{index + 1}. {ACTIONS.find(action => action.id === item.action)?.label ?? item.action} · {item.run.status}</option>)}</select></label>}
      <CandidateBoard reviewKey={String(store.generation)} result={visibleResult} choices={session.choices} selectedDetails={session.selectedDetails} onDevelop={develop} onSelectDetail={selectDetail} onChoice={choice} onSteer={(candidate, instruction) => { prepareCandidateComparison(candidate, instruction); }} onExplore={exploreCandidate} disabled={adopting} />
      {!!session.selectedDetails.length && <section className="workshop-tray" aria-label="Selected details"><div className="workshop-section-heading"><h2>Your selected details</h2><button onClick={() => { setAction('synthesize'); edit(current => ({ ...current, composer: 'Combine the selected details, preserving their wording and showing any new assumptions.' })); }}>Prepare to combine</button></div><p className="small-copy">These are in your tray, not yet chosen story material.</p>{session.selectedDetails.map(detail => <div className="workshop-tray-detail" key={detail.id}><textarea aria-label="Selected detail" value={detail.text} disabled={detail.fixed || adopting} onChange={event => edit(current => ({ ...current, selectedDetails: current.selectedDetails.map(item => item.id === detail.id ? { ...item, text: event.target.value } : item) }), true)} /><div><label><input type="checkbox" checked={detail.fixed} disabled={adopting} onChange={event => edit(current => ({ ...current, selectedDetails: current.selectedDetails.map(item => item.id === detail.id ? { ...item, fixed: event.target.checked } : item) }))} />Keep fixed</label><button disabled={detail.fixed || adopting} onClick={() => edit(current => ({ ...current, selectedDetails: current.selectedDetails.filter(item => item.id !== detail.id) }), true)}>Remove</button></div></div>)}</section>}
      {!!protectedChoices.length && <aside className="workshop-protection" aria-label="Protection for this material"><strong>Keep fixed in this material</strong><p>{protectedChoices.map(decision => decision.title).join(', ')}. You can develop an alternative here. Adopting changes to protected passages requires you to remove their protection explicitly, even when a choice is archived.</p><button onClick={() => setContextOpen(true)}>Review story choices and protection</button></aside>}
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
      {session.branchKind === 'whatIf' && <BranchComparison project={project} state={state} session={session} results={store.results} onOpenDocument={onOpenDocument} />}
      {(session.lens === 'possibilities' || !!session.storyPossibilities?.length) && <StoryPossibilities key={session.id} items={session.storyPossibilities ?? []} selectedText={capture?.text} disabled={adopting || store.locked}
        onChange={items => edit(current => ({ ...current, storyPossibilities: items }), true)}
        onExplore={item => {
          const kind = POSSIBILITY_KINDS.find(group => group.kind === item.kind)!;
          const instruction = appendProvisionalInstruction(session.composer, `Explore this ${kind.label.toLowerCase()} as an author intention, not an established event. Leave other possibilities open.\n${item.text}`);
          if (instruction === null) { setNotice('Your direction is full. Shorten it before adding this possibility.'); return; }
          setCapture(null); setAction(item.kind === 'possibleArc' ? 'arc' : 'directions');
          edit(current => ({ ...current, composer: instruction, selectedScope: kind.label }));
          setNotice('The possibility is ready to explore. Review Your direction and choose Explore when you want suggestions.');
        }} />}
      {session.relationshipId && <section className="workshop-focus" aria-label="Relationship being explored"><h2>Relationship being explored</h2>{activeRelationship && <><p><strong>{material.find(item => item.head.documentId === activeRelationship.fromDocumentId)?.title ?? 'Unavailable participant'} → {activeRelationship.type} → {material.find(item => item.head.documentId === activeRelationship.toDocumentId)?.title ?? 'Unavailable participant'}</strong></p><p>{activeRelationship.description}</p>{activeRelationship.uncertainty && <p>Uncertainty: {activeRelationship.uncertainty}</p>}<p className="small-copy">{activeRelationship.status === 'chosen' ? 'Chosen author intention' : activeRelationship.status === 'archived' ? 'Archived author intention' : 'Tentative author intention'} · Only this direction is being explored.</p></>}{relationshipUnavailable && <p role="status">This relationship is unavailable. Review it in Relationships or clear its scope before exploring further.</p>}<button disabled={adopting} onClick={() => edit(current => ({ ...current, relationshipId: null, selectedScope: 'Whole working version' }), true)}>Clear relationship scope</button></section>}
      <section className="workshop-composer" aria-label="Steer this exploration"><h2>What would you like to explore next?</h2><p><strong>Scope:</strong> {capture ? 'Selected passage in working version' : session.selectedScope || 'Choose a convention below'}</p><label>Next action<select value={action} disabled={adopting} onChange={event => { setAction(event.target.value); if (event.target.value === 'subvert') { setSubversion(''); setCapture(null); edit(current => ({ ...current, selectedScope: 'Whole working version' })); } }}>{ACTIONS.map(item => <option key={item.id} value={item.id}>{item.label}</option>)}</select></label>{action === 'subvert' && <><label>Convention to transform<input value={session.selectedScope === 'Whole working version' ? '' : session.selectedScope} maxLength={160} onChange={event => edit(current => ({ ...current, selectedScope: event.target.value }))} placeholder="For example, inherited special power" /></label><label>Transformation<select value={subversion} onChange={event => setSubversion(event.target.value)}><option value="">Choose a transformation…</option>{SUBVERSIONS.map(item => <option key={item}>{item}</option>)}</select></label></>}<label>Your direction<textarea value={session.composer} maxLength={12000} disabled={adopting} onChange={event => edit(current => ({ ...current, composer: event.target.value }))} placeholder="Keep the apprenticeship, add salvage expeditions, and make the guild’s safety concerns partly justified." /></label><div className="workshop-generation-footer"><span>{selectedModel?.label ?? 'No model available'}{providers.state?.settings.active.reasoning ? ` · ${providers.state.settings.active.reasoning}` : ''}{providers.state?.settings.active.serviceTier ? ` · ${providers.state.settings.active.serviceTier}` : ''}</span>{live ? <button disabled={live.run.status === 'stopping'} onClick={() => { void stop(); }}>{live.run.status === 'stopping' ? 'Stopping…' : 'Stop request'}</button> : <button className="primary-button" disabled={(!canGenerate && !pendingRequest.current) || requestBusy || adopting || relationshipUnavailable && !pendingRequest.current} onClick={() => { void generate(); }}>{requestBusy ? 'Preparing…' : pendingRequest.current ? 'Check request status' : 'Explore'}</button>}</div>
        {!canGenerate && <p className="small-copy">{selectedModel?.statusDetail || 'Connect an available model in Settings to generate.'} You can keep editing, saving, and organizing here.</p>}
        {result && ['failed', 'stopped', 'interrupted'].includes(result.run.status) && <div className="workshop-actions"><button disabled={!canGenerate || requestBusy} onClick={() => { void generate(result.action); }}>Try again with current context</button><button onClick={() => { void retryDiscussionSave(access, result.run.target.documentId, result.run.id).then(() => store.refreshResults()).catch(report); }}>Check retained result save</button></div>}
      </section>
      {(adoptionForm || preview) && <section className="workshop-adoption" aria-label="Adoption preview"><h2>{preview ? 'Choose this version for your story' : 'Where should this version go?'}</h2><p>Saved material stays author only. Chapters and character knowledge are not changed.</p>
        {adoptionForm && <AdoptionImpacts impacts={impactDrafts} documents={project.documents} onChange={setImpactDrafts} />}
        {adoptionForm && <>{session.relationshipId && <p>Choose where this relationship proposal belongs. Review each destination and any relationship changes together before confirming.</p>}{targets.map((target, index) => <fieldset key={target.id}><legend>Material {index + 1}</legend><label>Destination<select value={target.documentId} onChange={event => setTargets(current => current.map((value, at) => at === index ? { ...value, documentId: event.target.value } : value))}><option value="">Create a new document</option>{material.map(document => <option value={document.head.documentId} key={document.head.documentId}>{document.title}</option>)}</select></label>{!target.documentId && <><label>Title<input required value={target.title} maxLength={160} onChange={event => setTargets(current => current.map((value, at) => at === index ? { ...value, title: event.target.value } : value))} /></label><label>Kind<select value={target.kind} onChange={event => setTargets(current => current.map((value, at) => at === index ? { ...value, kind: event.target.value } : value))}>{['world', 'character', 'theme', 'hook', 'scene', 'note'].map(kind => <option key={kind}>{kind}</option>)}</select></label></>}{target.documentId && <label>Change<select value={target.mode} onChange={event => setTargets(current => current.map((value, at) => at === index ? { ...value, mode: event.target.value as 'add' | 'replace' } : value))}><option value="add">Add to existing material</option><option value="replace">Replace this document’s content</option></select></label>}<label>Content to choose<textarea value={target.text} onChange={event => setTargets(current => current.map((value, at) => at === index ? { ...value, text: event.target.value } : value))} /></label>{targets.length > 1 && <button onClick={() => setTargets(current => current.filter((_, at) => at !== index))}>Remove this target</button>}</fieldset>)}<button className={!targets.length ? 'primary-button' : undefined} onClick={() => setTargets(current => [...current, { id: crypto.randomUUID(), documentId: '', title: current.length ? '' : session.workingTitle || session.title, kind: 'note', mode: 'add', text: current.length ? '' : session.workingText }])}>{targets.length ? 'Include related material in this decision' : 'Choose a destination'}</button><AdoptionLinks participants={participants} links={adoptionLinks} onChange={setAdoptionLinks} /><label>Why this version?<textarea value={rationale} maxLength={4000} onChange={event => setRationale(event.target.value)} placeholder="Keep the guild morally mixed: it protects people and its own status." /></label><div className="workshop-actions"><button disabled={adopting} onClick={() => setAdoptionForm(false)}>Cancel</button><button className="primary-button" disabled={adopting || !targets.length || !validAdoptionLinks(adoptionLinks, participants) || targets.some(target => !target.text.trim() || !target.documentId && !target.title.trim())} onClick={() => { void prepareAdoption(); }}>Preview all changes</button></div></>}
        {preview && <><AdoptionPreview preview={preview} documents={project.documents} /><div className="workshop-actions"><button disabled={adopting || !!adoptionOperation.current} onClick={() => setPreview(null)}>Keep exploring</button><button className="primary-button" disabled={adopting} onClick={() => { void commitAdoption(); }}>{adoptionOperation.current ? 'Check adoption result' : 'Confirm Use this version'}</button></div></>}
      </section>}
      {(session.lens === 'people' || session.lens === 'world') && <Relationships state={state} documents={material} session={session} onChange={change => { void saveRelationship(change); }} onOpenDocument={onOpenDocument} onExploreRelationship={relationship => { void exploreRelationship(relationship); }} disabled={adopting || !!adoptionOperation.current} />}
      {(session.lens === 'people' || session.lens === 'world' || namesDocument) && <section className="workshop-names" aria-label="Names for saved people and places">
        <h2>Names & aliases</h2>
        <p className="small-copy">Keep alternate names and optional transliterations with a saved person, place, or group.</p>
        {namedMaterial.length ? <label>Saved person, place, or group<select aria-label="Saved person, place, or group" ref={namesSelect} disabled={adopting || navigationBusy} value={namesDocumentId} onChange={event => { void chooseNamesDocument(event.target.value); }}><option value="">Choose saved material</option>{namedMaterial.map(document => <option key={document.head.documentId} value={document.head.documentId}>{document.title}</option>)}</select></label> : <p className="small-copy">Choose a working version for your story to give it saved names here.</p>}
        {namesDocument && <DocumentAliases key={`${access.projectId}:${namesDocumentId}`} access={access} documentId={namesDocumentId} title={namesDocument.title} visible disabled={adopting || navigationBusy} registerGuard={registerNamesGuard} onClose={() => { setNamesDocumentId(''); namesSelect.current?.focus(); }} onSaved={() => { void store.refreshResults().catch(report); }} />}
      </section>}
      {!!session.questions.length && <details><summary>Open questions and intentional unknowns</summary>{session.questions.map(question => <div className="workshop-question-record" key={question.id}><p>{question.text}</p><label>State<select value={question.status} onChange={event => edit(current => ({ ...current, questions: current.questions.map(item => item.id === question.id ? { ...item, status: event.target.value as typeof question.status } : item) }))}><option value="open">Open</option><option value="notNow">Not now</option><option value="notRelevant">Not relevant</option><option value="keepMysterious">Keep mysterious</option></select></label><label>Unknown to<select value={question.unknownTo} onChange={event => edit(current => ({ ...current, questions: current.questions.map(item => item.id === question.id ? { ...item, unknownTo: event.target.value as typeof question.unknownTo } : item) }))}><option value="author">Me, the author</option><option value="reader">The reader</option><option value="both">Both</option></select></label></div>)}</details>}
      <details className="workshop-recap"><summary>Your stopping point</summary><WorkshopRecap session={session} decisions={state.decisions} saved={store.generation === store.savedGeneration} onOpenDocument={onOpenDocument} /><button onClick={() => { void store.flush().then(() => workshopHistory(access)).then(setHistory).catch(report); }}>Saved exploration versions</button>{history && <ul>{history.slice(0, 30).map(snapshot => <li key={snapshot.version}><details><summary>Workshop version {snapshot.version}</summary>{snapshot.state.sessions.filter(item => item.id === session.id).map(item => <div key={item.id}><p className="workshop-prose">{item.workingText || item.brief}</p><button onClick={() => { if (!canChangeExploration()) return; const restored = { ...structuredClone(item), id: crypto.randomUUID(), title: `Recovered: ${item.title}`, parentSessionId: session.id, branchKind: 'whatIf' as const, activeRunId: null }; restored.anchorDocumentId = `workshop-${restored.id}`; store.edit(current => ({ ...current, currentSessionId: restored.id, sessions: [...current.sessions, restored] })); }}>Recover as a separate exploration</button></div>)}</details></li>)}</ul>}</details>
      {notice && <p className="workshop-notice" role="status">{notice}</p>}
    </main>
      {contextOpen && <WorkshopContextPanel onClose={() => setContextOpen(false)}><h2>Working story</h2><label>Current direction<textarea value={session.direction} maxLength={12000} disabled={adopting || store.locked} onChange={event => edit(current => ({ ...current, direction: event.target.value }))} placeholder="An editable direction, once you choose one." /></label><label>Still open<textarea value={session.stillOpen} maxLength={6000} disabled={adopting || store.locked} onChange={event => edit(current => ({ ...current, stillOpen: event.target.value }))} /></label><Preferences state={state} session={session} onChange={change => { if (!adopting && !store.locked) store.edit(change); }} />
      <section className="workshop-context-preview"><h3>Using for the next exploration</h3><p className="small-copy">Current brief, working version, selected details, and the choices below. The saved request inspector shows what was actually delivered.</p><label><input type="checkbox" checked={session.outsideDirection} disabled={adopting} onChange={event => edit(current => ({ ...current, outsideDirection: event.target.checked }))} />Explore outside my current direction</label><p className="small-copy">Project Must/Never constraints and Keep fixed details remain in force.</p><ul>{applicablePreferences(state, session).map(preference => <li key={preference.id}>{preferenceLabel(preference)} {preference.label} · {preference.scope}</li>)}</ul><details><summary>Include saved material explicitly</summary>{material.map(document => <label key={document.head.documentId}><input type="checkbox" checked={session.includedDocumentIds.includes(document.head.documentId)} onChange={event => edit(current => ({ ...current, includedDocumentIds: event.target.checked ? [...current.includedDocumentIds, document.head.documentId] : current.includedDocumentIds.filter(id => id !== document.head.documentId) }))} />{document.title}</label>)}</details><p className="small-copy">Unrelated chat, rejected candidates, and noncanon moments stay out unless an eligible alternative is explicitly included.</p></section>
      <NextExplorationContext state={state} session={session} results={store.results} documents={material} />
      {result && <section><h3>Saved request context</h3><RequestContext access={access} packetId={result.run.packetId} /><ContextInspector access={access} packetId={result.run.packetId} delivered={result.run.dispatchState === 'delivered'} refreshKey={result.run.sequence} /></section>}
      <section className="workshop-decisions"><h3>Story choices and protection</h3>{state.decisions.filter(decision => decision.status === 'chosen' || decision.fixed).map(decision => <details key={decision.id}><summary>{decision.title}{decision.status !== 'chosen' ? ` · ${decision.status}` : ''}{decision.fixed ? ' · Keep fixed' : ''}</summary><p>Author only · version {decision.head.version}</p><label>Why you chose it<textarea value={decision.rationale} maxLength={4000} onChange={event => store.edit(current => ({ ...current, decisions: current.decisions.map(item => item.id === decision.id ? { ...item, rationale: event.target.value } : item) }))} /></label><label><input type="checkbox" checked={decision.fixed} onChange={event => store.edit(current => ({ ...current, decisions: current.decisions.map(item => item.id === decision.id ? { ...item, fixed: event.target.checked } : item) }))} />Keep fixed</label><button onClick={() => onOpenDocument(decision.documentId)}>Open source</button><button onClick={() => { const source = project.documents.find(document => document.head.documentId === decision.documentId); edit(current => ({ ...current, relationshipId: null, focusDocumentId: decision.documentId, composer: `What might change if we revisit ${decision.title}? Distinguish basis, assumptions, and possible tensions.`, includedDocumentIds: [...new Set([...current.includedDocumentIds, decision.documentId])], selectedScope: source?.title ?? decision.title }), true); setAction('consequences'); }}>What might this change?</button>{decision.status === 'chosen' && <button onClick={() => store.edit(current => ({ ...current, decisions: current.decisions.map(item => item.id === decision.id ? { ...item, status: 'archived' } : item) }))}>Archive choice</button>}</details>)}{!state.decisions.some(decision => decision.status === 'chosen') && <p className="small-copy">No version chosen yet.</p>}</section>
      {!!reviewableImpacts.length && <section><h3>Affected material</h3>{reviewableImpacts.map(impact => <div key={impact.id}><strong>{IMPACT_LABELS[impact.kind]} · {project.documents.find(document => document.head.documentId === impact.documentId)?.title ?? 'Saved material'}</strong><p>{impact.reason}</p>{impact.candidateId && <p className="small-copy">Suggested by: {store.results.flatMap(saved => saved.output?.candidates ?? []).find(candidate => candidate.id === impact.candidateId)?.title ?? 'A retained exploration candidate'}</p>}{impact.relationshipId && <p className="small-copy">Relationship: {state.relationships.find(relationship => relationship.id === impact.relationshipId)?.description ?? 'A retained relationship'}</p>}<button onClick={() => onOpenDocument(impact.documentId)}>Review source</button><label>Review status<select value={impact.status} onChange={event => store.edit(current => ({ ...current, impacts: current.impacts.map(item => item.id === impact.id ? { ...item, status: event.target.value as typeof impact.status } : item) }))}><option value="needsReview">Needs review</option><option value="acknowledged">Reviewed</option><option value="intentional">Intentional ambiguity</option></select></label></div>)}</section>}
    </WorkshopContextPanel>}
  </div>;
});
