import { errorCode, errorTextFor, sameDocumentHead } from '../kernel';
import type { ModelSelection } from '../ipc/providers';
import {
  adoptChatPreview,
  emptyProjectComposer,
  prepareChatAdoption,
  readProjectConversation,
  retryProjectChatSave,
  saveProjectComposer,
  setChatDisposition,
  startProjectChat,
  startProjectChapter,
  type AssistantDraft,
  type ChatDispositionOptions,
  type ChatAdoptionAck,
  type ChatAdoptionPreview,
  type ConversationItem,
  type ProjectChatDraftRef,
  type ProjectComposer,
  type ProjectChapterComposer,
  type ProjectConversationView,
  type SaveProjectComposer,
} from '../ipc/projectChat';
import type { DiscussionRun, DiscussionStart } from '../ipc/discussions';
import type { SafeBriefInput } from '../ipc/discussions';
import { stopDiscussion } from '../ipc/discussions';
import type { MockContextBudget } from '../ipc/context';
import type { Head, OpenedProject, ProjectAccess } from '../ipc/projects';

type OperationUuid = ReturnType<typeof crypto.randomUUID>;

export type ConversationStatus = 'idle' | 'loading' | 'saving' | 'queued' | 'running' | 'stopping' | 'uncertain' | 'failed';

export interface ConversationSnapshot {
  status: ConversationStatus;
  view: ProjectConversationView | null;
  composer: ProjectComposer;
  composerVersion: string;
  composerDirty: boolean;
  activeRun: DiscussionRun | null;
  error: string | null;
  uncertainOperationId: string | null;
  olderLoading: boolean;
  olderError: string | null;
}

export interface ProjectConversationTransport {
  read(access: ProjectAccess, before?: string | null, limit?: number): Promise<ProjectConversationView>;
  save(request: SaveProjectComposer): Promise<{ conversationId: string; version: string; body: ProjectComposer }>;
  start(request: Parameters<typeof startProjectChat>[0], selection: ModelSelection | null): Promise<DiscussionStart>;
  retrySave(access: ProjectAccess, conversationId: string, runId: string): Promise<void>;
  stop(access: ProjectAccess, runId: string): Promise<{ run: DiscussionRun }>;
  disposition(access: ProjectAccess, conversationId: string, referenceId: string, expectedVersion: string, disposition: string, rationale?: string, options?: ChatDispositionOptions): Promise<ConversationItem>;
  prepareAdoption(access: ProjectAccess, conversationId: string, drafts: ProjectChatDraftRef[]): Promise<ChatAdoptionPreview>;
  adopt(access: ProjectAccess, preview: ChatAdoptionPreview, operationId?: OperationUuid): Promise<ChatAdoptionAck>;
}

const transport: ProjectConversationTransport = {
  read: readProjectConversation,
  save: saveProjectComposer,
  start: (request, selection) => request.composer.chapter ? startProjectChapter(request, selection) : startProjectChat(request, selection),
  retrySave: retryProjectChatSave,
  stop: stopDiscussion,
  disposition: (access, conversationId, referenceId, expectedVersion, disposition, rationale = '', options = {}) => setChatDisposition(access, conversationId, referenceId, expectedVersion, disposition, rationale, crypto.randomUUID(), options),
  prepareAdoption: (access, conversationId, drafts) => prepareChatAdoption(access, conversationId, drafts),
  adopt: (access, preview, operationId) => adoptChatPreview(access, preview, operationId),
};

function copy<T>(value: T): T { return structuredClone(value); }
const errorMessage = errorTextFor('The conversation could not be saved.');
function isDefiniteRequestError(reason: unknown): boolean {
  return ['InvalidRequest', 'InvalidDocument', 'InvalidComposer', 'ConversationNotFound', 'ProjectNotFound', 'VersionConflict', 'OperationIdReusedWithDifferentPayload'].includes(errorCode(reason) ?? '');
}
function isDefiniteStartError(reason: unknown): boolean {
  return ['InvalidRequest', 'InvalidProjectChat', 'InvalidChapterRequest', 'InvalidSafeBrief', 'InvalidScope', 'InvalidContext', 'InvalidDocument', 'ProjectChatBusy', 'ProviderUnavailable', 'ModelChoiceChanged', 'ContextBudgetExceeded', 'ContextChanged', 'ContextPreparationFailed', 'VersionConflict', 'OperationIdReusedWithDifferentPayload', 'InvalidProviderBinding', 'UnsupportedProvider', 'InvalidModelSelection'].includes(errorCode(reason) ?? '');
}
function sameComposer(a: ProjectComposer, b: ProjectComposer): boolean { return JSON.stringify(a) === JSON.stringify(b); }
function nextVersion(value: string): string {
  try { return (BigInt(value) + 1n).toString(); } catch { return value; }
}
function versionAfter(candidate: string | undefined, current: string): string | null {
  if (!candidate) return null;
  try { return BigInt(candidate) > BigInt(current) ? candidate : null; } catch { return candidate === current ? null : candidate; }
}
function runFromItem(item: ConversationItem): DiscussionRun | null {
  const payloadRun = item.payload?.run;
  if (!payloadRun || typeof payloadRun !== 'object' || typeof (payloadRun as { operationId?: unknown }).operationId !== 'string') return null;
  return payloadRun as DiscussionRun;
}
function latestRun(view: ProjectConversationView): DiscussionRun | null {
  return view.items.slice().sort((a, b) => Number(b.sequence) - Number(a.sequence)).map(runFromItem).find(Boolean) ?? view.activeRun ?? null;
}
function runError(run: DiscussionRun | null): string | null {
  if (!run || (run.status !== 'failed' && run.status !== 'interrupted')) return null;
  return run.stopReason ?? run.providerResult?.error ?? 'The last request failed before a usable response was saved.';
}

/**
 * Renderer-side projection for the project conversation.
 *
 * The store owns only composer and request state. Rust owns messages, runs,
 * drafts, revisions, and authority. A request captures an immutable composer
 * value before dispatch, so typing or a late read cannot overwrite newer text.
 */
export class ProjectConversationStore {
  access: ProjectAccess;
  private readonly api: ProjectConversationTransport;
  private listeners = new Set<() => void>();
  private timer: ReturnType<typeof setTimeout> | null = null;
  private readSequence = 0;
  private disposed = false;
  private saveFlight: Promise<void> | null = null;
  private pendingSave: { request: SaveProjectComposer; body: ProjectComposer } | null = null;
  private request: { operationId: string; composer: ProjectComposer; run: DiscussionRun | null; accepted: boolean } | null = null;
  private sendFlight: Promise<DiscussionStart> | null = null;
  private olderFlight: Promise<boolean> | null = null;
  private olderPagesLoaded = false;
  private snapshot: ConversationSnapshot = {
    status: 'idle', view: null, composer: emptyProjectComposer(), composerVersion: '0',
    composerDirty: false, activeRun: null, error: null, uncertainOperationId: null, olderLoading: false, olderError: null,
  };

  constructor(project: OpenedProject, api: ProjectConversationTransport = transport) {
    this.access = copy(project.access);
    this.api = api;
  }

  // Keep the identity stable for React's useSyncExternalStore. All writes
  // replace the immutable projection; consumers never mutate this object.
  get state(): ConversationSnapshot { return this.snapshot; }
  getSnapshot = (): ConversationSnapshot => this.snapshot;
  subscribe = (listener: () => void): (() => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  private notify(): void { if (!this.disposed) for (const listener of this.listeners) listener(); }
  private assertActive(): void {
    if (this.disposed) throw new Error('The project conversation is closed.');
  }
  private set(change: Partial<ConversationSnapshot>): void {
    if (this.disposed) return;
    this.snapshot = { ...this.snapshot, ...change };
    this.notify();
  }
  private assertProject(view: ProjectConversationView): void {
    // The command is project-scoped. The view has no duplicated project ID,
    // so its conversation anchor is accepted only through this store's access.
    if (!view.id) throw new Error('The project conversation has no durable identity.');
  }

  private mergeView(incoming: ProjectConversationView, current = this.snapshot.view): ProjectConversationView {
    if (!current || current.id !== incoming.id) return copy(incoming);
    const byId = new Map<string, ConversationItem>();
    for (const item of current.items) byId.set(item.id, item);
    for (const item of incoming.items) byId.set(item.id, item);
    return {
      ...copy(incoming),
      // A refresh is always the newest page, so it must not rewind a cursor
      // that the author has already advanced by loading earlier pages.
      olderBefore: this.olderPagesLoaded ? current.olderBefore : incoming.olderBefore,
      items: [...byId.values()].sort((a, b) => Number(a.sequence) - Number(b.sequence)),
    };
  }

  /** Rotate only the lease used by future IPC calls; keep composer/request identity intact. */
  updateAccess(access: ProjectAccess): void {
    if (this.disposed) return;
    if (access.projectId !== this.access.projectId || access.operationNamespace !== this.access.operationNamespace || access.session !== this.access.session) {
      throw new Error('The project conversation cannot change project identity while it is open.');
    }
    this.access = copy(access);
    if (this.pendingSave) this.pendingSave = { ...this.pendingSave, request: { ...this.pendingSave.request, access: copy(access) } };
  }

  async load(): Promise<ConversationSnapshot> {
    const sequence = ++this.readSequence;
    this.set({ status: 'loading', error: null });
    try {
      const view = await this.api.read(this.access, null, 40);
      if (this.disposed || sequence !== this.readSequence) return this.state;
      this.assertProject(view);
      const dirty = this.snapshot.composerDirty || this.pendingSave !== null;
      const mergedView = this.mergeView(view);
      const visibleRun = latestRun(mergedView);
      this.snapshot = {
        ...this.snapshot,
        status: this.statusFor(visibleRun), view: mergedView, activeRun: visibleRun ? copy(visibleRun) : null,
        ...(dirty ? {} : { composer: copy(view.composer.body), composerVersion: view.composer.version }),
        composerDirty: dirty,
        error: runError(visibleRun), olderLoading: false, olderError: null,
      };
      this.notify();
      return this.state;
    } catch (reason) {
      if (!this.disposed && sequence === this.readSequence) this.set({ status: 'failed', error: errorMessage(reason) });
      throw reason;
    }
  }

  async refresh(): Promise<ConversationSnapshot> {
    const sequence = ++this.readSequence;
    try {
      const view = await this.api.read(this.access, null, 40);
      if (this.disposed || sequence !== this.readSequence) return this.state;
      this.assertProject(view);
      const mergedView = this.mergeView(view);
      const pending = this.request;
      const pendingRun = pending ? this.findRun(mergedView, pending.operationId) : null;
      if (pending && pendingRun) pending.run = copy(pendingRun);
      const activeRun = pendingRun ?? latestRun(mergedView);
      const status = this.snapshot.status === 'uncertain' && pending && !pendingRun
        ? 'uncertain' : this.statusFor(activeRun);
      this.snapshot = { ...this.snapshot, view: mergedView, activeRun: activeRun ? copy(activeRun) : null, status, error: status === 'uncertain' ? this.snapshot.error : runError(activeRun) };
      if (pending && pendingRun && this.snapshot.status !== 'uncertain') this.acceptedRequest(pending, pendingRun);
      this.notify();
      return this.state;
    } catch (reason) {
      if (!this.disposed && sequence === this.readSequence) this.set({ status: 'failed', error: errorMessage(reason) });
      throw reason;
    }
  }

  /** Load the next older timeline page without disturbing the live request or composer. */
  async loadOlder(): Promise<boolean> {
    if (this.olderFlight) return this.olderFlight;
    const before = this.snapshot.view?.olderBefore;
    if (!before) return false;
    const flight = (async () => {
      this.set({ olderLoading: true, olderError: null });
      try {
        const page = await this.api.read(this.access, before, 40);
        if (this.disposed) return false;
        this.assertProject(page);
        this.snapshot = { ...this.snapshot, view: this.mergeView(page), olderLoading: false, olderError: null };
        this.olderPagesLoaded = true;
        this.notify();
        return true;
      } catch (reason) {
        if (!this.disposed) this.set({ olderLoading: false, olderError: errorMessage(reason) });
        return false;
      }
    })();
    this.olderFlight = flight;
    try { return await flight; } finally { if (this.olderFlight === flight) this.olderFlight = null; }
  }

  private hasConfirmedRun(view: ProjectConversationView, operationId: string): boolean {
    return !!this.findRun(view, operationId);
  }
  private findRun(view: ProjectConversationView, operationId: string): DiscussionRun | null {
    if (view.activeRun?.operationId === operationId) return view.activeRun;
    return view.items.map(runFromItem).find(run => run?.operationId === operationId) ?? null;
  }

  private statusFor(run: DiscussionRun | null): ConversationStatus {
    if (!run) return this.snapshot.composerDirty ? 'idle' : 'idle';
    if (run.status === 'queued') return 'queued';
    if (run.status === 'running') return 'running';
    if (run.status === 'stopping') return 'stopping';
    if (run.status === 'failed' || run.status === 'interrupted') return 'failed';
    return 'idle';
  }

  private acceptedRequest(pending: { composer: ProjectComposer; operationId: string; run: DiscussionRun | null; accepted: boolean }, run: DiscussionRun): void {
    if (this.disposed) return;
    // The Rust command clears the entire accepted composer in the same
    // transaction as the request. A newer renderer buffer remains intact and
    // will be saved only after this accepted watermark is known.
    if (!pending.accepted) {
      // A reconciliation read is authoritative when it includes the server's
      // post-start composer watermark. Fall back to the deterministic +1 used
      // by the start acknowledgement when the run was accepted locally.
      const serverVersion = versionAfter(this.snapshot.view?.composer.version, this.snapshot.composerVersion);
      this.snapshot = { ...this.snapshot, composerVersion: serverVersion ?? nextVersion(this.snapshot.composerVersion) };
    }
    pending.accepted = true;
    if (sameComposer(this.snapshot.composer, pending.composer)) {
      this.snapshot = { ...this.snapshot, composer: emptyProjectComposer(), composerDirty: false };
    }
    pending.run = copy(run);
    if (this.snapshot.composerDirty && !['queued', 'running', 'stopping'].includes(this.snapshot.status)) this.scheduleSave();
  }

  setText(text: string): void { this.updateComposer({ text }); }
  setSources(sourceRefs: Head[]): void { this.updateComposer({ sourceRefs: copy(sourceRefs) }); }
  setTaskDrafts(taskDraftRefs: ProjectChatDraftRef[]): void { this.updateComposer({ taskDraftRefs: copy(taskDraftRefs) }); }
  setFocus(focusedDocumentRef: Head | undefined): void { this.updateComposer({ focusedDocumentRef: focusedDocumentRef ? copy(focusedDocumentRef) : undefined }); }
  async stageChapter(chapter: ProjectChapterComposer): Promise<void> {
    this.assertActive();
    if (!this.snapshot.view) await this.load();
    this.assertActive();
    this.updateComposer({ chapter: copy(chapter), taskDraftRefs: [], focusedDocumentRef: undefined });
    await this.flush();
  }
  async attachSource(head: Head): Promise<void> {
    this.assertActive();
    if (!this.snapshot.view) await this.load();
    this.assertActive();
    const existing = (this.snapshot.composer.sourceRefs ?? []).some(item => item.documentId === head.documentId);
    const sourceRefs = existing
      ? (this.snapshot.composer.sourceRefs ?? []).map(item => item.documentId === head.documentId ? copy(head) : item)
      : [...(this.snapshot.composer.sourceRefs ?? []), copy(head)];
    // Attaching material is an explicit return to the broad project context.
    // It must not leave a restricted chapter task attached to the next send.
    this.updateComposer({ sourceRefs: copy(sourceRefs), chapter: null });
    await this.flush();
  }
  setChapterIntent(intent: NonNullable<ProjectChapterComposer['intent']>): void {
    const chapter = this.snapshot.composer.chapter;
    if (!chapter) return;
    if (intent === 'proposeEdits' && !chapter.scope) { this.set({ error: 'Select text in the chapter before requesting edits.' }); return; }
    const safeBrief = chapter.safeBrief ? { ...copy(chapter.safeBrief), confirmed: false } : chapter.safeBrief;
    this.updateComposer({ chapter: { ...copy(chapter), intent, basis: intent === 'continue' ? chapter.basis ?? 'working' : null, safeBrief } });
  }
  setChapterBasis(basis: NonNullable<ProjectChapterComposer['basis']>): void {
    const chapter = this.snapshot.composer.chapter;
    if (!chapter || chapter.intent !== 'continue') return;
    const safeBrief = chapter.safeBrief ? { ...copy(chapter.safeBrief), confirmed: false } : chapter.safeBrief;
    this.updateComposer({ chapter: { ...copy(chapter), basis, safeBrief } });
  }
  setChapterBrief(brief: SafeBriefInput | null): void {
    const chapter = this.snapshot.composer.chapter;
    if (!chapter) return;
    this.updateComposer({ chapter: { ...copy(chapter), safeBrief: brief ? copy(brief) : null } });
  }
  async clearChapter(): Promise<void> {
    this.assertActive();
    if (!this.snapshot.view) await this.load();
    this.assertActive();
    this.updateComposer({ chapter: null });
    await this.flush();
  }
  private updateComposer(changes: Partial<ProjectComposer>): void {
    if (this.disposed) return;
    const composer = { ...copy(this.snapshot.composer), ...changes };
    this.snapshot = { ...this.snapshot, composer, composerDirty: true, error: null };
    this.notify();
    this.scheduleSave();
  }

  private scheduleSave(): void {
    if (this.disposed) return;
    if (this.timer) clearTimeout(this.timer);
    if (this.request && !this.request.accepted && ['queued', 'running', 'stopping', 'uncertain'].includes(this.snapshot.status)) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      if (this.disposed) return;
      void this.flush().catch(() => { /* visible state retains exact input */ });
    }, 350);
  }

  async flush(): Promise<void> {
    if (this.disposed) return;
    if (this.timer) { clearTimeout(this.timer); this.timer = null; }
    if (this.request && !this.request.accepted) throw new Error('The active request must be accepted or reconciled before saving the composer.');
    if (this.snapshot.status === 'uncertain' && this.pendingSave) throw new Error('Reconcile the saved composer before trying to save it again.');
    if (this.saveFlight) { await this.saveFlight; if (!this.disposed && this.snapshot.composerDirty) await this.flush(); return; }
    if (!this.snapshot.view || !this.snapshot.composerDirty) return;
    const request: SaveProjectComposer = this.pendingSave?.request ?? {
      access: copy(this.access), operationId: crypto.randomUUID(), conversationId: this.snapshot.view.id,
      expectedVersion: this.snapshot.composerVersion, body: copy(this.snapshot.composer),
    };
    if (!this.pendingSave) this.pendingSave = { request, body: copy(request.body) };
    const captured = this.pendingSave;
    this.set({ status: 'saving', error: null });
    const flight = (async () => {
      try {
        const ack = await this.api.save(captured.request);
        if (this.disposed) return;
        if (ack.conversationId !== captured.request.conversationId || ack.version === undefined) throw new Error('The composer acknowledgment belongs to another conversation.');
        this.pendingSave = null;
        this.snapshot = {
          ...this.snapshot,
          composerVersion: ack.version,
          status: this.snapshot.activeRun ? this.statusFor(this.snapshot.activeRun) : 'idle',
          composerDirty: this.snapshot.composerDirty && JSON.stringify(this.snapshot.composer) !== JSON.stringify(captured.body),
        };
        this.notify();
      } catch (reason) {
        if (!this.disposed) this.set({ status: isDefiniteRequestError(reason) ? 'failed' : 'uncertain', error: errorMessage(reason), uncertainOperationId: captured.request.operationId });
        throw reason;
      }
    })();
    this.saveFlight = flight;
    try { await flight; } finally { if (this.saveFlight === flight) this.saveFlight = null; }
    // The acknowledgment advances only the captured composer version. If the
    // author kept typing while it was in flight, persist that newer immutable
    // buffer in a separate CAS operation after the first flight settles.
    if (this.snapshot.composerDirty) await this.flush();
  }

  async send(selection: ModelSelection | null, budget: MockContextBudget): Promise<DiscussionStart> {
    this.assertActive();
    if (this.sendFlight) throw new Error('A project request is already being submitted.');
    const flight = this.sendOnce(selection, budget);
    this.sendFlight = flight;
    try { return await flight; } finally { if (this.sendFlight === flight) this.sendFlight = null; }
  }

  private async sendOnce(selection: ModelSelection | null, budget: MockContextBudget): Promise<DiscussionStart> {
    this.assertActive();
    if (this.request && ['queued', 'running', 'stopping', 'uncertain'].includes(this.snapshot.status)) throw new Error('A project request is already in progress.');
    if (this.snapshot.status === 'uncertain') throw new Error('Reconcile the saved composer before sending it again.');
    if (!this.snapshot.view) await this.load();
    await this.flush();
    const view = this.snapshot.view;
    if (!view) throw new Error('The project conversation is still opening.');
    const composer = copy(this.snapshot.composer);
    if (!composer.text.trim()) throw new Error('Write a request before sending it.');
    const operationId = crypto.randomUUID();
    this.request = { operationId, composer, run: null, accepted: false };
    this.set({ status: 'queued', error: null, uncertainOperationId: null });
    try {
      const acceptedVersion = nextVersion(this.snapshot.composerVersion);
      const started = await this.api.start({ access: copy(this.access), operationId, conversationId: view.id, expectedComposerVersion: this.snapshot.composerVersion, composer, budget }, selection);
      if (this.disposed) return started;
      this.request.run = copy(started.run);
      this.request.accepted = true;
      const currentComposer = this.snapshot.composer;
      const clearedComposer = sameComposer(currentComposer, composer) ? emptyProjectComposer() : currentComposer;
      this.snapshot = { ...this.snapshot, composer: clearedComposer, composerVersion: acceptedVersion, composerDirty: !sameComposer(currentComposer, composer), activeRun: copy(started.run), status: this.statusFor(started.run), view: this.snapshot.view ? { ...this.snapshot.view, activeRun: copy(started.run), composer: { ...this.snapshot.view.composer, version: acceptedVersion, body: copy(clearedComposer) } } : this.snapshot.view };
      if (this.snapshot.composerDirty) this.scheduleSave();
      this.notify();
      return started;
    } catch (reason) {
      if (!this.disposed) {
        const definite = isDefiniteStartError(reason);
        if (definite) this.request = null;
        this.set({ status: definite ? 'failed' : 'uncertain', error: errorMessage(reason), uncertainOperationId: definite ? null : operationId });
      }
      throw reason;
    }
  }

  async reconcileStart(): Promise<boolean> {
    if (this.disposed) return false;
    const pending = this.request;
    if (!pending) {
      if (this.pendingSave) { await this.retrySave(); return true; }
      return false;
    }
    await this.refresh();
    if (this.disposed) return false;
    const confirmed = !!this.snapshot.view && this.hasConfirmedRun(this.snapshot.view, pending.operationId);
    if (confirmed) {
      const run = this.snapshot.view ? this.findRun(this.snapshot.view, pending.operationId) : null;
      pending.run = run ? copy(run) : pending.run;
      if (run) this.acceptedRequest(pending, run);
      this.set({ status: this.statusFor(run), uncertainOperationId: null, error: null });
    }
    return confirmed;
  }

  async retrySave(runId?: string): Promise<void> {
    if (this.disposed) return;
    const pending = this.pendingSave;
    if (runId) {
      // A local terminal/materialization failure must name the exact retained
      // run. Never infer it from the latest timeline item: a newer request may
      // have completed since the renderer lost the original save result.
      const view = this.snapshot.view;
      if (!view || !view.workerIssues.some(issue => issue.runId === runId)) {
        throw new Error('That retained local result is no longer available.');
      }
      try {
        await this.api.retrySave(this.access, view.id, runId);
        if (this.disposed) return;
        await this.refresh();
      } catch (reason) {
        if (!this.disposed) this.set({ status: 'failed', error: errorMessage(reason) });
        throw reason;
      }
      return;
    }
    if (!pending) {
      throw new Error('Choose the retained result to retry locally.');
    }
    this.set({ status: 'saving', error: null });
    try {
      const ack = await this.api.save(pending.request);
      if (this.disposed) return;
      this.pendingSave = null;
      this.snapshot = { ...this.snapshot, composerVersion: ack.version, composerDirty: JSON.stringify(this.snapshot.composer) !== JSON.stringify(pending.body), status: this.snapshot.activeRun ? this.statusFor(this.snapshot.activeRun) : 'idle', error: null, uncertainOperationId: null };
      this.notify();
    } catch (reason) { this.set({ status: 'uncertain', error: errorMessage(reason), uncertainOperationId: pending.request.operationId }); throw reason; }
  }

  async stop(): Promise<void> {
    if (this.disposed) return;
    const run = this.request?.run ?? this.snapshot.activeRun;
    if (!run) return;
    this.set({ status: 'stopping', error: null });
    try { await this.api.stop(this.access, run.id); await this.refresh(); }
    catch (reason) { this.set({ status: 'failed', error: errorMessage(reason) }); throw reason; }
  }

  async setDisposition(referenceId: string, expectedVersion: string, disposition: string, rationale = '', options: ChatDispositionOptions = {}): Promise<ConversationItem> {
    this.assertActive();
    const view = this.snapshot.view;
    if (!view) throw new Error('The project conversation is not loaded.');
    const item = await this.api.disposition(this.access, view.id, referenceId, expectedVersion, disposition, rationale, options);
    await this.refresh();
    return item;
  }

  async prepareAdoption(drafts: ProjectChatDraftRef[]): Promise<ChatAdoptionPreview> {
    this.assertActive();
    const view = this.snapshot.view;
    if (!view) throw new Error('The project conversation is not loaded.');
    return this.api.prepareAdoption(this.access, view.id, drafts);
  }

  async adopt(preview: ChatAdoptionPreview, operationId: OperationUuid = crypto.randomUUID()): Promise<ChatAdoptionAck> {
    this.assertActive();
    const ack = await this.api.adopt(this.access, preview, operationId);
    await this.refresh();
    return ack;
  }

  draft(documentId: string): AssistantDraft | null { return this.snapshot.view?.drafts.find(draft => draft.document.head.documentId === documentId) ?? null; }
  dispose(): void { this.disposed = true; this.readSequence += 1; if (this.timer) clearTimeout(this.timer); this.timer = null; this.listeners.clear(); }
}

export function conversationItems(view: ProjectConversationView | null): ConversationItem[] { return view ? view.items.slice().sort((a, b) => Number(a.sequence) - Number(b.sequence)) : []; }
export function draftRefs(drafts: AssistantDraft[]): ProjectChatDraftRef[] { return drafts.filter(draft => draft.disposition === 'pending' && !draft.stale).map(draft => ({ head: copy(draft.document.head), dispositionVersion: draft.dispositionVersion })); }
