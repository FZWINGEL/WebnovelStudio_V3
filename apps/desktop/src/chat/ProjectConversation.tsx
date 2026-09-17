import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState, type ComponentProps } from 'react';
import { useSyncExternalStore } from 'react';
import { useProviders } from '../providers';
import type { MockContextBudget } from '../ipc/context';
import type { DocumentRecord, Head, OpenedProject, ProjectAccess } from '../ipc/projects';
import type { AssistantDraft, ChatDispositionOptions, ChatDispositionScope, ChatUnknownTo, ChatAdoptionTarget, ConversationItem, ProjectChapterComposer } from '../ipc/projectChat';
import { readChatAdoptionPreview } from '../ipc/projectChat';
import type { DiscussionRun, SafeBriefInput } from '../ipc/discussions';
import { SafeBriefEditor } from '../assistant';
import { approveChapterBrief } from './brief';
import { draftRefs, ProjectConversationStore } from './conversationStore';
import { DraftReviewPanel, type DraftReviewPanelHandle } from './DraftReviewPanel';
import { ProjectDocumentsPanel } from './ProjectDocumentsPanel';
import { Writer } from './Writer';
import { RequestStatus } from './RequestStatus';
import { useStoryFreshness } from './useStoryFreshness';
import { type ChapterHandoffProposal } from './ChapterHandoff';
import { useDraftReviewContext } from './useDraftReviewContext';
import { readStalePreviewComparison } from './stalePreviewComparison';
import type { StalePreviewComparison } from './DraftReviewPanel';
import { chatViewPreferenceKey, readChatViewPreferences, writeChatViewPreferences } from './viewPreferences';
import './chat.css';
import './CoauthorConversation.css';
import { ChatSplitPane } from './ChatSplitPane';
import { Transcript } from './conversationPanels';
import { dispositionVersion, frozenRequestContext, isStaleAdoptionError, reasonMessage, requestItemForRun } from './conversationHelpers';

export interface ProjectConversationHandle { flush(): Promise<void>; refresh?(): Promise<void>; stageChapter(chapter: ProjectChapterComposer): Promise<void>; attachSource(head: Head): Promise<void> }
export interface ProjectConversationProps {
  project: OpenedProject;
  activeDocument?: DocumentRecord | null;
  onOpenDocument(document: DocumentRecord): Promise<void> | void;
  onPrepareSource?(document: DocumentRecord): Promise<Head>;
  onPrepareChapter?(targetId: string | null, title: string): Promise<DocumentRecord>;
  onDocumentsChanged(documents: DocumentRecord[]): Promise<void> | void;
  onEarlierWorkshop(): void;
  onBeforeAdoption?(targets: ChatAdoptionTarget[]): Promise<void>;
  onAdoptionFailure?(reason: unknown): Promise<void>;
  onCreateChapter?(): Promise<void> | void;
  onCreateNote?(): Promise<void> | void;
  onOpenChapterResult?(run: DiscussionRun): Promise<void> | void;
  onAccessChanged?(access: ProjectAccess): Promise<void> | void;
  /// The document surface's inputs. The chat renders `<Writer>` itself rather
  /// than receiving a built node, so the shell no longer assembles a feature's
  /// JSX and the conversation wiring below is the chat's own, not a round trip
  /// through the shell's ref.
  editor?: Omit<ComponentProps<typeof Writer>, 'conversation'>;
  /// The chapter run whose review the editor should surface, if any. Set by the
  /// shell, which owns the navigation that produces it.
  reviewRunId?: string | null;
  budget?: MockContextBudget;
}

const defaultBudget: MockContextBudget = { modelId: 'mock-story-context', contextWindowTokens: '32768', reservedOutputTokens: '4096', reservedProtocolTokens: '1024' };

export const ProjectConversation = forwardRef<ProjectConversationHandle, ProjectConversationProps>(function ProjectConversation({ project, activeDocument = null, onOpenDocument, onPrepareSource, onPrepareChapter, onDocumentsChanged, onEarlierWorkshop, onBeforeAdoption, onAdoptionFailure, onCreateChapter, onCreateNote, onOpenChapterResult, onAccessChanged, editor, reviewRunId = null, budget = defaultBudget }, ref) {
  const provider = useProviders();
  const selection = provider.state?.settings.active ?? null;
  const modelReady = !provider.busy && !!provider.state && provider.state.dispatch.kind !== 'blocked' && !!selection;
  const storeRef = useRef<ProjectConversationStore | null>(null);
  const projectRef = useRef(`${project.project.projectId}:${project.access.operationNamespace}:${project.access.session}`);
  const accessKey = `${project.project.projectId}:${project.access.operationNamespace}:${project.access.session}`;
  if (!storeRef.current || projectRef.current !== accessKey) {
    storeRef.current?.dispose();
    storeRef.current = new ProjectConversationStore(project);
    projectRef.current = accessKey;
  }
  const store = storeRef.current;
  const draftReviewRef = useRef<DraftReviewPanelHandle>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const stageChapter = useCallback(async (chapter: ProjectChapterComposer) => {
    await store.stageChapter(chapter);
    if (!mounted.current || storeRef.current !== store) return;
    setMobileSurface('chat');
    setTimeout(() => { if (mounted.current && storeRef.current === store) composerRef.current?.focus(); }, 0);
  }, [store]);
  const attachSource = useCallback(async (head: Head) => {
    await store.attachSource(head);
    composerRef.current?.focus();
  }, [store]);

  useImperativeHandle(ref, () => ({
    refresh: async () => { await store.refresh(); },
    flush: async () => { await draftReviewRef.current?.flush(); await store.flush(); },
    stageChapter,
    attachSource,
  }), [store, stageChapter, attachSource]);
  const state = useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
  const storyFreshness = useStoryFreshness({
    access: project.access,
    run: state.activeRun,
    currentSourceEpoch: state.view?.sourceEpoch ?? null,
    currentPolicyEpoch: state.view?.policyEpoch ?? null,
  });
  const [mobileSurface, setMobileSurface] = useState<'chat' | 'documents' | 'chapter' | 'review'>('chat');
  const [error, setError] = useState('');
  const [adoption, setAdoption] = useState('');
  const [preview, setPreview] = useState<import('../ipc/projectChat').ChatAdoptionPreview | null>(null);
  const [previewVerified, setPreviewVerified] = useState(false);
  const [previewError, setPreviewError] = useState('');
  const applyOperationId = useRef<ReturnType<typeof crypto.randomUUID> | null>(null);
  const [rightMode, setRightMode] = useState<'document' | 'review'>('document');
  const [composerDetailsOpen, setComposerDetailsOpen] = useState(false);
  const [briefOpen, setBriefOpen] = useState(false);
  const [briefApprovalBusy, setBriefApprovalBusy] = useState(false);
  const [briefFocus, setBriefFocus] = useState(0);
  const [transcriptAnchor, setTranscriptAnchor] = useState<string | null>(null);
  const preferencesLoaded = useRef<string | null>(null);
  const skipPreferenceWrite = useRef(false);
  const mounted = useRef(true);
  const briefApprovalSequence = useRef(0);
  const isCurrentConversation = () => mounted.current && storeRef.current === store;
  useEffect(() => { mounted.current = true; void store.load().catch(reason => mounted.current && setError(reasonMessage(reason, 'Could not open the project conversation.'))); return () => { mounted.current = false; store.dispose(); }; }, [store]);
  useEffect(() => {
    try { store.updateAccess(project.access); }
    catch (reason) { if (mounted.current) setError(reasonMessage(reason, 'The project conversation lease changed unexpectedly.')); }
  }, [project.access.operationNamespace, project.access.projectId, project.access.session, project.access.writerLease, store]);
  useEffect(() => {
    if (!['queued', 'running', 'stopping', 'uncertain'].includes(state.status)) return;
    const timer = setInterval(() => { void store.refresh().catch(() => {}); }, 900);
    return () => clearInterval(timer);
  }, [state.status, store]);
  const preferenceKey = state.view ? chatViewPreferenceKey(store.access, state.view.id) : null;
  useEffect(() => {
    if (!preferenceKey || preferencesLoaded.current === preferenceKey) return;
    const preferences = readChatViewPreferences(preferenceKey);
    skipPreferenceWrite.current = true;
    setRightMode(preferences.rightMode ?? 'document');
    setMobileSurface(preferences.rightMode === 'review' ? 'review' : 'chat');
    setTranscriptAnchor(preferences.transcriptAnchor ?? null);
    setPreview(preferences.preview ?? null);
    setPreviewVerified(!preferences.preview);
    applyOperationId.current = preferences.applyOperationId && /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(preferences.applyOperationId)
      ? preferences.applyOperationId as ReturnType<typeof crypto.randomUUID> : null;
    preferencesLoaded.current = preferenceKey;
    if (preferences.preview) {
      void readChatAdoptionPreview(store.access, state.view!.id, preferences.preview.id).then(restored => {
        if (preferencesLoaded.current === preferenceKey) { setPreview(restored); setPreviewVerified(true); }
      }).catch(reason => {
        if (preferencesLoaded.current === preferenceKey) { setPreviewVerified(false); setPreviewError(`The saved adoption preview could not be reloaded: ${reasonMessage(reason, 'The saved preview is unavailable.')}`); }
      });
    }
  }, [preferenceKey]);
  useEffect(() => {
    if (!preferenceKey || preferencesLoaded.current !== preferenceKey) return;
    if (skipPreferenceWrite.current) { skipPreferenceWrite.current = false; return; }
    writeChatViewPreferences(preferenceKey, { rightMode, transcriptAnchor, preview, applyOperationId: applyOperationId.current });
  }, [preferenceKey, preview, rightMode, transcriptAnchor]);
  const drafts = state.view?.drafts ?? [];
  const reviewableDrafts = drafts.filter(draft => draft.disposition === 'pending');
  const activeOrdinaryDocument = activeDocument && (activeDocument.role ?? 'ordinary') === 'ordinary' ? activeDocument : null;
  const autoReviewKey = `${accessKey}:${activeOrdinaryDocument?.head.documentId ?? 'none'}:${reviewableDrafts.map(draft => `${draft.document.head.documentId}:${draft.dispositionVersion}:${draft.stale}`).join('|')}`;
  const lastAutoReviewKey = useRef<string | null>(null);
  useEffect(() => {
    if (lastAutoReviewKey.current === autoReviewKey) return;
    lastAutoReviewKey.current = autoReviewKey;
    if (!activeOrdinaryDocument && reviewableDrafts.length > 0) {
      setRightMode('review');
    }
  }, [autoReviewKey]);
  const send = async () => {
    setError('');
    const currentBrief = store.state.composer.chapter?.safeBrief;
    if (briefApprovalBusy || (currentBrief && !currentBrief.confirmed)) {
      setError('Approve or remove the writing brief before sending this chapter request.');
      return;
    }
    if (!modelReady) {
      setError(provider.busy ? 'Checking your saved model choice…' : provider.state?.dispatch.detail || 'Check Settings before sending. Choose a connected model first.');
      return;
    }
    try { await store.send(selection, budget); }
    catch (reason) { if (mounted.current) setError(reasonMessage(reason, 'The request could not be sent.')); }
  };
  const prepareAdoption = async (selected: AssistantDraft[]) => {
    setAdoption('Preparing an exact adoption preview…'); setPreviewError('');
    try {
      await draftReviewRef.current?.flush();
      const refs = draftReviewRef.current?.currentRefs(selected.map(draft => draft.document.head.documentId)) ?? draftRefs(selected);
      const nextPreview = await store.prepareAdoption(refs);
      applyOperationId.current = crypto.randomUUID();
      setPreview(nextPreview); setPreviewVerified(true); setAdoption('Exact preview ready. Review the before and after versions, then apply it explicitly.'); setRightMode('review');
    } catch (reason) { setAdoption(`The adoption preview could not be prepared: ${reasonMessage(reason, 'The preview request failed.')}`); }
  };
  const applyPreview = async (selectedPreview: import('../ipc/projectChat').ChatAdoptionPreview) => {
    setPreviewError(''); setAdoption('Applying the reviewed preview…');
    try {
      // The preview carries exact draft heads. Flush any edits made after the
      // preview was prepared so the core can reject a stale preview safely.
      await draftReviewRef.current?.flush();
      if (!isCurrentConversation()) return;
      await onBeforeAdoption?.(selectedPreview.targets);
      if (!isCurrentConversation()) return;
      const ack = await store.adopt(selectedPreview, applyOperationId.current ?? (applyOperationId.current = crypto.randomUUID()));
      if (!isCurrentConversation()) return;
      await onDocumentsChanged(ack.documents);
      if (!isCurrentConversation()) return;
      setPreview(null); setPreviewVerified(false); applyOperationId.current = null; setAdoption(`${ack.documents.length} document${ack.documents.length === 1 ? '' : 's'} adopted.`); await switchRightMode('document');
    } catch (reason) {
      if (!isCurrentConversation()) return;
      let recoveryDetail = '';
      try { await onAdoptionFailure?.(reason); }
      catch (recoveryError) { recoveryDetail = ` Editor recovery still needs attention: ${reasonMessage(recoveryError, 'The current saved document could not be restored.')}`; }
      if (!isCurrentConversation()) return;
      const detail = reasonMessage(reason, 'The acknowledgement was lost.') + recoveryDetail;
      setPreviewError(isStaleAdoptionError(reason) ? `The preview is stale: ${detail} Prepare a fresh preview explicitly. The old preview remains available for comparison.` : `The adoption result is not confirmed: ${detail} Check saved documents before retrying; the exact preview is retained.`);
      setAdoption(isStaleAdoptionError(reason) ? 'Preview needs refresh.' : 'Adoption needs confirmation.');
    }
  };
  const compareStalePreview = async (selectedPreview: import('../ipc/projectChat').ChatAdoptionPreview) => {
    const comparison = await readStalePreviewComparison(store.access, selectedPreview);
    if (!isCurrentConversation()) throw new Error('The project changed while reading the comparison.');
    await store.refresh();
    return comparison;
  };
  const prepareAgainstCurrent = async (selected: AssistantDraft[], comparison: StalePreviewComparison) => {
    if (!preview || comparison.previewId !== preview.id) return;
    try {
      await store.refresh();
      if (!isCurrentConversation()) return;
      const current = selected.map(draft => store.draft(draft.document.head.documentId));
      if (current.some(draft => !draft || draft.stale || draft.disposition !== 'pending')) {
        throw new Error('This draft needs a fresh assistant response based on the changed story. Use Refresh with assistant, then review its new draft.');
      }
      await prepareAdoption(current as AssistantDraft[]);
    } catch (reason) {
      if (isCurrentConversation()) setPreviewError(`The preview is stale: ${reasonMessage(reason, 'Current versions could not be prepared.')}`);
    }
  };
  const switchRightMode = async (mode: 'document' | 'review') => {
    if (rightMode === 'review' && mode === 'document') {
      try { await draftReviewRef.current?.flush(); }
      catch (reason) { if (isCurrentConversation()) setPreviewError(reasonMessage(reason, 'Save the active draft before leaving review.')); return false; }
    }
    if (!isCurrentConversation()) return false;
    setRightMode(mode);
    return true;
  };
  const switchMobileSurface = async (surface: 'chat' | 'documents' | 'chapter' | 'review') => {
    if (surface === 'documents' || surface === 'chapter') {
      if (!await switchRightMode('document')) return;
    } else if (surface === 'review') {
      if (!await switchRightMode('review')) return;
    }
    if (isCurrentConversation()) setMobileSurface(surface);
  };
  const openDraft = async (draft: AssistantDraft) => {
    try {
      await draftReviewRef.current?.flush();
      setRightMode('review');
      await draftReviewRef.current?.openDraft(draft.document.head.documentId);
      setMobileSurface('review');
    } catch (reason) { setError(reasonMessage(reason, 'Save the active draft before opening another review.')); }
  };
  const openChapterResult = async (run: DiscussionRun) => {
    try {
      if (!await switchRightMode('document')) return;
      await onOpenChapterResult?.(run);
      if (isCurrentConversation()) setMobileSurface('chapter');
    } catch (reason) { if (isCurrentConversation()) setError(reasonMessage(reason, 'The chapter response could not be opened.')); }
  };
  const attachDocument = async (document: DocumentRecord) => {
    try {
      await draftReviewRef.current?.flush();
      const head = onPrepareSource ? await onPrepareSource(document) : document.head;
      if (!isCurrentConversation()) return;
      await store.attachSource(head);
      if (!isCurrentConversation()) return;
      setMobileSurface('chat');
      composerRef.current?.focus();
    } catch (reason) { setError(reasonMessage(reason, 'Save the source before attaching it to the project conversation.')); }
  };
  const backToChat = async () => {
    try {
      await draftReviewRef.current?.flush();
      setMobileSurface('chat');
      composerRef.current?.focus();
    } catch (reason) { setPreviewError(reasonMessage(reason, 'Save the active draft before returning to chat.')); }
  };
  useEffect(() => {
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      if (mobileSurface === 'review' || mobileSurface === 'documents' || mobileSurface === 'chapter') {
        event.preventDefault();
        void backToChat();
      } else if (rightMode === 'review') {
        event.preventDefault();
        void switchRightMode('document');
      }
    };
    window.addEventListener('keydown', handleEscape);
    return () => window.removeEventListener('keydown', handleEscape);
  }, [mobileSurface, rightMode]);
  const reject = async (draft: AssistantDraft) => {
    try { await draftReviewRef.current?.flush(); await store.setDisposition(draft.document.head.documentId, draft.dispositionVersion, 'rejected'); }
    catch (reason) { setError(reasonMessage(reason, 'The draft could not be rejected.')); }
  };
  const reviseDraft = async (draft: AssistantDraft) => {
    try {
      await draftReviewRef.current?.flush();
      const refs = (state.composer.taskDraftRefs ?? []).filter(reference => reference.head.documentId !== draft.document.head.documentId);
      const exactRef = draftReviewRef.current?.currentRefs([draft.document.head.documentId])[0] ?? { head: draft.document.head, dispositionVersion: draft.dispositionVersion };
      store.setTaskDrafts([...refs, exactRef]);
      if (!state.composer.text.trim()) store.setText('Revise this draft with the project assistant.');
      setMobileSurface('chat');
      composerRef.current?.focus();
    } catch (reason) { setError(reasonMessage(reason, 'The draft could not be attached for revision.')); }
  };
  const reconsiderDraft = async (draft: AssistantDraft) => {
    try {
      await draftReviewRef.current?.flush();
      await store.setDisposition(draft.document.head.documentId, draft.dispositionVersion, 'reconsider');
      const replacement = store.state.view?.drafts.find(candidate => candidate.document.head.documentId === draft.document.head.documentId && candidate.disposition === 'pending');
      if (replacement) store.setTaskDrafts([...(store.state.composer.taskDraftRefs ?? []).filter(reference => reference.head.documentId !== draft.document.head.documentId), { head: replacement.document.head, dispositionVersion: replacement.dispositionVersion }]);
      store.setText(`Reconsider the rejected draft “${draft.document.title}” and propose a fresh direction.`);
      setMobileSurface('chat');
      composerRef.current?.focus();
    } catch (reason) { setError(reasonMessage(reason, 'The draft could not be reconsidered.')); }
  };
  const clearChapter = async () => {
    try { await store.clearChapter(); composerRef.current?.focus(); }
    catch (reason) { setError(reasonMessage(reason, 'The chapter task could not be cleared.')); }
  };
  const openBrief = () => {
    if (!state.composer.chapter) return;
    if (!state.composer.chapter.safeBrief) store.setChapterBrief({ text: '', originMessageId: null, confirmed: false });
    setBriefOpen(true); setBriefFocus(value => value + 1);
  };
  const updateBrief = (brief: SafeBriefInput) => {
    const attempt = ++briefApprovalSequence.current;
    if (!brief.confirmed) { setBriefApprovalBusy(false); store.setChapterBrief(brief); return; }
    const chapter = store.state.composer.chapter;
    if (!chapter) return;
    const conversationId = store.state.view?.id;
    const exactChapter = JSON.stringify(chapter);
    let unchanged = true;
    // Observe every chapter mutation, including a change and later reversal.
    // Typing in the independent request field does not cancel an approval.
    const unsubscribe = store.subscribe(() => {
      if (JSON.stringify(store.state.composer.chapter) !== exactChapter) unchanged = false;
    });
    const stillCurrent = () => isCurrentConversation() && attempt === briefApprovalSequence.current && unchanged && store.state.view?.id === conversationId;
    setBriefApprovalBusy(true);
    void approveChapterBrief(brief, chapter.scope ?? null).then(approved => {
      if (stillCurrent()) store.setChapterBrief(approved);
    }).catch(reason => {
      if (stillCurrent()) setError(reasonMessage(reason, 'The writing brief could not be approved.'));
    }).finally(() => {
      unsubscribe();
      if (isCurrentConversation() && attempt === briefApprovalSequence.current) setBriefApprovalBusy(false);
    });
  };
  const removeBrief = () => { store.setChapterBrief(null); setBriefOpen(false); };
  const adaptBrief = (messageId: string, text: string) => {
    const chapter = store.state.composer.chapter;
    if (!chapter || !state.view) return;
    store.setChapterBrief({
      text,
      originMessageId: messageId,
      confirmed: false,
      projectOrigin: {
        version: 'project-conversation-brief.v1',
        projectId: project.access.projectId,
        operationNamespace: project.access.operationNamespace,
        conversationId: state.view.id,
        messageId,
        target: chapter.target,
        // The provenance is intentionally unapproved until the author edits
        // or accepts the brief. Approval replaces these placeholders with
        // hashes of the exact scope and text.
        scopeHash: '0'.repeat(64),
        textHash: '0'.repeat(64),
      },
    });
    setBriefOpen(true); setBriefFocus(value => value + 1);
  };
  const prepareHandoff = async (proposal: ChapterHandoffProposal, messageId: string, targetId: string | null, title: string) => {
    if (!onPrepareChapter || !state.view) throw new Error('Open the project before preparing chapter writing.');
    await draftReviewRef.current?.flush();
    const target = await onPrepareChapter(targetId, title);
    if (!isCurrentConversation()) return;
    if (target.kind !== 'chapter' || (target.role ?? 'ordinary') !== 'ordinary') throw new Error('Choose an ordinary chapter for this writing task.');
    const text = [proposal.instruction.trim(), proposal.brief.trim()].filter(Boolean).join('\n\n');
    await store.stageChapter({ target: target.head, intent: 'continue', basis: 'working', scope: undefined, safeBrief: {
      text, originMessageId: messageId, confirmed: false,
      projectOrigin: { version: 'project-conversation-brief.v1', projectId: project.access.projectId, operationNamespace: project.access.operationNamespace,
        conversationId: state.view.id, messageId, target: target.head, scopeHash: '0'.repeat(64), textHash: '0'.repeat(64) },
    } });
    if (!isCurrentConversation()) return;
    // A proposed transition never replaces an independently typed request.
    if (!store.state.composer.text.trim()) store.setText(proposal.instruction);
    setBriefOpen(true); setBriefFocus(value => value + 1);
    setMobileSurface('chat');
    composerRef.current?.focus();
  };
  const stageAssumptionCorrection = (originalText: string, revisedText: string): boolean => {
    if (store.state.composer.chapter) {
      setError('Return to the project conversation before staging an author-room assumption correction. Use “Return to project conversation” first.');
      return false;
    }
    const correction = ['Correction for the next draft.', `Original assumption: “${originalText}”`, `Author correction: ${revisedText}`].join('\n');
    const existing = store.state.composer.text;
    const separator = existing && !existing.endsWith('\n') ? '\n\n' : '';
    store.setText(`${existing}${separator}${correction}`);
    setMobileSurface('chat');
    composerRef.current?.focus();
    return true;
  };
  const disposition = (referenceId: string, version: string, value: string, options: ChatDispositionOptions = {}, rationale = '') => { void store.setDisposition(referenceId, version, value, rationale, options).catch(reason => setError(reasonMessage(reason, 'The response decision could not be saved.'))); };
  const composerText = state.composer.text;
  const chapter = state.composer.chapter;
  const exactSourceCount = new Set([...(state.composer.sourceRefs ?? []), ...(state.composer.focusedDocumentRef ? [state.composer.focusedDocumentRef] : [])].map(head => `${head.documentId}:${head.version}:${head.bodyHash}`)).size;
  const taskDraftCount = new Set((state.composer.taskDraftRefs ?? []).map(reference => `${reference.head.documentId}:${reference.head.version}:${reference.head.bodyHash}:${reference.dispositionVersion}`)).size;
  const reviewOrigins = useDraftReviewContext({ access: project.access, view: state.view, enabled: rightMode === 'review' || mobileSurface === 'review' });
  const reviewContext = reviewOrigins.context;
  const previewOriginsReady = !preview || preview.targets.every(target => {
    const draft = drafts.find(draft => draft.document.head.documentId === target.draft.head.documentId);
    return draft && !!reviewContext[draft.originRunId];
  });
  const scopeSummary = [exactSourceCount ? `${exactSourceCount} exact source${exactSourceCount === 1 ? '' : 's'}` : '', taskDraftCount ? `${taskDraftCount} unadopted draft${taskDraftCount === 1 ? '' : 's'}` : '', chapter ? 'chapter task' : ''].filter(Boolean).join(' · ') || 'No sources attached';
  const composerStatus = state.status === 'saving' ? 'Saving request…' : state.status === 'queued' || state.status === 'running' || state.status === 'stopping' ? 'Newer edits stay local until this request is accepted' : state.composerDirty ? 'Unsaved request is kept locally' : 'Composer saved';
  const chapterTarget = chapter ? project.documents.find(document => document.head.documentId === chapter.target.documentId) : null;
  const chapterHasScope = !!chapter?.scope;
  const runContext = frozenRequestContext(requestItemForRun(state.view?.items ?? [], state.activeRun), project.documents);
  const requestContext = {
    surface: chapter ? 'chapterWriting' as const : 'authorRoom' as const,
    targetLabel: chapter ? `Chapter · ${chapterTarget?.title ?? chapter.target.documentId}` : activeDocument ? (state.composer.focusedDocumentRef?.documentId === activeDocument.head.documentId ? `Discussing · ${activeDocument.title}` : `Open document · ${activeDocument.title}`) : 'Project conversation',
    scopeLabel: scopeSummary,
    selection,
    run: runContext,
  };
  const composing = useRef(false);
  return <section className="chat-project-conversation" aria-label={`Conversation for ${project.project.title}`}>
    <RequestStatus status={state.status} run={state.activeRun} error={error || state.error} uncertainOperationId={state.uncertainOperationId} workerIssues={state.view?.workerIssues} requestContext={requestContext} freshness={storyFreshness} onStop={() => void store.stop().catch(reason => { if (isCurrentConversation()) setError(reasonMessage(reason, 'The request could not be stopped.')); })} onRetrySave={runId => void store.retrySave(runId).catch(reason => { if (isCurrentConversation()) setError(reasonMessage(reason, 'The retained response could not be saved.')); })} onReconcile={() => void store.reconcileStart().catch(reason => { if (isCurrentConversation()) setError(reasonMessage(reason, 'The operation could not be reconciled.')); })} />
    <nav className="chat-mobile-tabs" aria-label="Project workspace surfaces"><button type="button" aria-selected={mobileSurface === 'chat'} onClick={() => void switchMobileSurface('chat')}>Chat</button><button type="button" aria-selected={mobileSurface === 'documents'} onClick={() => void switchMobileSurface('documents')}>Documents</button><button type="button" aria-selected={mobileSurface === 'chapter'} onClick={() => void switchMobileSurface('chapter')}>Chapter</button>{rightMode === 'review' && <button type="button" aria-selected={mobileSurface === 'review'} onClick={() => void switchMobileSurface('review')}>Review</button>}</nav>
    <ChatSplitPane initialWidthPercent={47} projectKey={`${project.access.projectId}:${project.access.operationNamespace}`} left={
      <section className={`chat-conversation-surface ${mobileSurface === 'chat' ? 'mobile-visible' : ''}`}>
        <header className="coauthor-conversation-header">
          <div>
            <span className="coauthor-conversation-kicker">Project conversation</span>
            <h1>Let's build your story</h1>
          </div>
          {reviewableDrafts.length > 0 && <span className="coauthor-conversation-draft-count">{reviewableDrafts.length} draft{reviewableDrafts.length === 1 ? '' : 's'} to review</span>}
        </header>
        <Transcript store={store} documents={project.documents} activeDocument={activeDocument} anchor={transcriptAnchor} onAnchorChange={setTranscriptAnchor} onDisposition={disposition} onStageAssumptionCorrection={stageAssumptionCorrection} onOpenDocument={onOpenDocument} onOpenDraft={openDraft} onOpenChapterResult={onOpenChapterResult ? openChapterResult : undefined} onAdaptBrief={chapter && chapter.intent !== 'discuss' ? adaptBrief : undefined} onPrepareHandoff={onPrepareChapter ? prepareHandoff : undefined} onBrowseDocuments={() => void switchMobileSurface('documents')} onCreateChapter={onCreateChapter} onCreateNote={onCreateNote} />
        <div className="chat-composer-wrap">
          <div className="chat-composer-reference">{chapter ? <>Chapter task · <strong>{chapterTarget?.title ?? chapter.target.documentId}</strong></> : activeDocument ? state.composer.focusedDocumentRef?.documentId === activeDocument.head.documentId ? <>Discussing <strong>{activeDocument.title}</strong></> : <>Open document · <strong>{activeDocument.title}</strong> <small>(attach from Writer to include its exact text)</small></> : 'Project conversation'}<span>{composerStatus} · {scopeSummary}</span></div>
          {chapter && <section className="chat-chapter-context" aria-label="Captured chapter task"><div className="chat-chapter-context-heading"><strong>Captured chapter task</strong><button type="button" onClick={() => void clearChapter()}>Return to project conversation</button></div>{chapter.scope && <p className="chat-chapter-scope"><strong>Captured scope:</strong> “{chapter.scope.quote || 'Whole chapter'}”</p>}<div className="chat-chapter-controls"><label>Intent<select value={chapter.intent} onChange={event => store.setChapterIntent(event.target.value as ProjectChapterComposer['intent'])}><option value="discuss">Discuss this chapter</option>{chapterHasScope && <option value="proposeEdits">Suggest edits to the captured scope</option>}<option value="continue">Continue this chapter</option></select></label>{chapter.intent === 'continue' && <label>Continuation basis<select value={chapter.basis ?? 'working'} onChange={event => store.setChapterBasis(event.target.value as NonNullable<ProjectChapterComposer['basis']>)}><option value="working">Current working story</option><option value="reviewed">Reviewed story</option></select></label>}</div>{chapter.intent !== 'discuss' && <div className="chat-brief-launcher"><span>{chapter.safeBrief?.confirmed ? 'Writing brief approved' : chapter.safeBrief ? 'Writing brief needs approval' : 'Writing brief · optional'}</span><button type="button" onClick={openBrief} disabled={briefApprovalBusy}>{chapter.safeBrief ? 'Edit brief' : 'Add writing brief'}</button></div>}{briefOpen && chapter.intent !== 'discuss' && chapter.safeBrief && <SafeBriefEditor value={chapter.safeBrief} disabled={briefApprovalBusy || ['saving', 'queued', 'running', 'stopping', 'uncertain'].includes(state.status)} focusKey={briefFocus} onChange={updateBrief} onRemove={removeBrief} />}</section>}
          <details className="coauthor-composer-details" open={composerDetailsOpen} onToggle={event => setComposerDetailsOpen(event.currentTarget.open)}>
            <summary><span>Sources &amp; details</span><small>{scopeSummary}</small></summary>
            <div className="chat-composer-context" tabIndex={0} aria-label="Request context and scope">
            {((state.composer.sourceRefs ?? []).length > 0 || (state.composer.taskDraftRefs ?? []).length > 0 || state.composer.focusedDocumentRef) && <div className="chat-composer-chips" aria-label="Attached context"><span className="chat-composer-chips-label">Attached context</span>{(state.composer.sourceRefs ?? []).map(head => <span className="chat-composer-chip" key={`source:${head.documentId}`}><span>{project.documents.find(document => document.head.documentId === head.documentId)?.title ?? head.documentId} · v{head.version} · {head.bodyHash.slice(0, 10)}…</span><button type="button" aria-label={`Remove ${head.documentId} source`} onClick={() => store.setSources((state.composer.sourceRefs ?? []).filter(item => item.documentId !== head.documentId))}>×</button></span>)}{state.composer.focusedDocumentRef && <span className="chat-composer-chip"><span>{project.documents.find(document => document.head.documentId === state.composer.focusedDocumentRef?.documentId)?.title ?? state.composer.focusedDocumentRef.documentId} · focused v{state.composer.focusedDocumentRef.version}</span><button type="button" aria-label="Remove focused document" onClick={() => store.setFocus(undefined)}>×</button></span>}{(state.composer.taskDraftRefs ?? []).map(reference => <span className="chat-composer-chip chat-composer-chip-draft" key={`draft:${reference.head.documentId}`}><span>{drafts.find(draft => draft.document.head.documentId === reference.head.documentId)?.document.title ?? reference.head.documentId} · draft v{reference.head.version} · decision {reference.dispositionVersion}</span><button type="button" aria-label={`Remove ${reference.head.documentId} draft`} onClick={() => store.setTaskDrafts((state.composer.taskDraftRefs ?? []).filter(item => item.head.documentId !== reference.head.documentId))}>×</button></span>)}</div>}
            </div>
          </details>
          <textarea ref={composerRef} disabled={!state.view} aria-busy={!state.view} aria-label="Message the project assistant" value={composerText} onChange={event => store.setText(event.target.value)} onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }} onKeyDown={event => { if ((event.ctrlKey || event.metaKey) && event.key === 'Enter' && !composing.current && !event.nativeEvent.isComposing) { event.preventDefault(); void send(); } }} placeholder={chapter ? 'Describe what should happen in this chapter…' : 'Tell me what you want to explore or write next…'} rows={4} />
          <div className="chat-composer-actions"><button type="button" onClick={() => void send()} disabled={!state.view || !modelReady || !composerText.trim() || briefApprovalBusy || !!(chapter?.safeBrief && !chapter.safeBrief.confirmed) || ['queued', 'running', 'stopping', 'saving', 'uncertain'].includes(state.status)} className="primary-button">Send <span>Ctrl+Enter</span></button><button type="button" onClick={onEarlierWorkshop}>Earlier Workshop</button><span className="chat-composer-help">Enter makes a new line · the assistant may ask one useful question</span></div>
        </div>
      </section>
      } right={<section className={`chat-document-surface ${mobileSurface !== 'chat' ? 'mobile-visible' : ''} chat-mobile-${mobileSurface}`}>
        <nav className="chat-right-mode-tabs" aria-label="Document workspace mode"><button type="button" aria-selected={rightMode === 'document'} onClick={() => void switchRightMode('document')}>Document</button><button type="button" aria-selected={rightMode === 'review'} onClick={() => void switchRightMode('review')}>Review drafts{drafts.length ? ` (${drafts.length})` : ''}</button></nav>
        <div className={`chat-document-view ${rightMode === 'review' ? 'is-hidden' : ''}`}>
          <div className="chat-document-editor">{editor ? <Writer key={`${project.project.projectId}:${editor.active.record.head.documentId}`} {...editor} conversation={{ stageChapter, attachSource, reviewRunId: reviewRunId ?? null }} /> : <div className="chat-empty"><h2>Open a document</h2><p>Select a chapter, world note, character, or draft from the document panel.</p></div>}</div>
          <ProjectDocumentsPanel project={project} activeDocument={activeDocument} drafts={drafts} relatedDocumentIds={(state.composer.sourceRefs ?? []).map(head => head.documentId)} onOpenDocument={onOpenDocument} onOpenDraft={openDraft} onAttachSource={attachDocument} chapterTaskActive={!!chapter} onCreateChapter={onCreateChapter} />
        </div>
        <div className={`chat-review-view ${rightMode === 'document' ? 'is-hidden' : ''}`}>
          <button type="button" className="chat-back-to-chat" onClick={() => void backToChat()}>Back to chat</button>
          {reviewOrigins.loading && <p role="status">Loading the drafts’ original requests and assumptions…</p>}
          {reviewOrigins.error && <p role="alert">{reviewOrigins.error} <button type="button" onClick={reviewOrigins.retry}>Retry reading original requests</button></p>}
          <DraftReviewPanel ref={draftReviewRef} reviewContext={reviewContext} project={project} drafts={drafts} preview={preview} previewVerified={previewVerified && previewOriginsReady} previewError={previewError} viewKey={preferenceKey ?? undefined} onPrepareAdoption={prepareAdoption} onApplyPreview={applyPreview} onCompareStalePreview={compareStalePreview} onPrepareAgainstCurrent={prepareAgainstCurrent} onReject={reject} onRevise={reviseDraft} onReconsider={reconsiderDraft} onOpenDraft={openDraft} onOpenDocument={onOpenDocument} onAccessChanged={onAccessChanged} onDraftSaved={async (_draft, _head) => { await store.refresh(); }} />
        </div>
        {adoption && <p className="chat-adoption-status" role="status">{adoption}</p>}
        {previewError && <p className="chat-draft-warning chat-adoption-warning" role="alert">{previewError} The exact preview is retained; review it before retrying.</p>}
      </section>} />
  </section>;
});
