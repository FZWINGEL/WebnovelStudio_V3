import { bodyHash, canonicalJson, type WnsDocument } from './document';
import type { CheckpointRequest, DocumentRecord, Head, ProjectAccess, ProjectTransport, ReconciledDocument, SaveAck, SaveSnapshot } from '../ipc/projects';
import type { ApplyAck, ApplyProposal, PreparedProposal, Proposal } from '../ipc/proposals';

export type SessionPhase = 'editing' | 'flushing' | 'applying' | 'saveFailed' | 'reconciling' | 'conflict' | 'disposed';
export interface SessionState {
  phase: SessionPhase; generation: string; savedGeneration: string; head: Head;
  saving: boolean; dirty: boolean; editable: boolean; error: string | null;
}
export class SessionError extends Error {
  constructor(public code: string, message: string) { super(message); this.name = 'SessionError'; }
}
type Capture = { request: SaveSnapshot; json: string; hash: string; payloadHash: string };
/** Preflight is pure; commit dispatches the same prepared editor transaction. */
export interface PreparedEditorChange { body: WnsDocument; commit(): WnsDocument; read?(): WnsDocument }
type ApplyCapture = { request: ApplyProposal; body: WnsDocument; json: string; payloadHash: string; change: PreparedEditorChange };
type Listener = () => void;
function clone<T>(value: T): T { return structuredClone(value); }
function freeze<T>(value: T): T {
  if (value && typeof value === 'object') { Object.values(value).forEach(freeze); Object.freeze(value); }
  return value;
}
function version(value: string): bigint {
  if (!/^(0|[1-9][0-9]*)$/u.test(value) || BigInt(value) > 9223372036854775807n) throw new SessionError('ProtocolError', 'The saved version is invalid.');
  return BigInt(value);
}
function sameHead(a: Head, b: Head): boolean { return a.documentId === b.documentId && a.version === b.version && a.bodyHash === b.bodyHash; }
function errorOf(reason: unknown): SessionError {
  if (reason instanceof SessionError) return reason;
  if (reason && typeof reason === 'object' && 'code' in reason && 'detail' in reason) return new SessionError(String(reason.code), String(reason.detail));
  return new SessionError('UncertainOutcome', 'The save response was lost. Your local text is retained; reconnect to check the saved version.');
}
export function logicalSaveJson(request: SaveSnapshot): string {
  const { session: _session, writerLease: _lease, ...access } = request.access;
  // Rust sorts all object keys recursively, including arbitrary key ordering
  // in the restricted snapshot. Only the rotating renderer/lease are omitted.
  return canonicalJson({ ...request, access });
}

/** Owns save/lifecycle state, never replaces or rebuilds an editor on an acknowledgment. */
export class DocumentSession {
  private access: ProjectAccess;
  private head: Head;
  private current: WnsDocument;
  private currentJson: string;
  private generation = 0n;
  private savedGeneration = 0n;
  private phase: SessionPhase = 'editing';
  private error: string | null = null;
  private composing = false;
  private compositionWaiters: (() => void)[] = [];
  private barrier = false;
  private owner: Promise<unknown> | null = null;
  private flight: Promise<void> | null = null;
  private pending: Capture | null = null;
  private pendingApply: ApplyCapture | null = null;
  private nextCause: SaveSnapshot['cause'] = 'typing';
  private timer: ReturnType<typeof setTimeout> | null = null;
  private dirtySince: number | null = null;
  private listeners = new Set<Listener>();
  private conflict: ReconciledDocument | null = null;
  private viewSaver: (() => Promise<void>) | null = null;

  constructor(access: ProjectAccess, document: DocumentRecord, private transport: ProjectTransport, private options: { autosave?: boolean; newId?: () => string } = {}) {
    this.access = clone(access); this.head = clone(document.head); version(this.head.version);
    this.current = freeze(clone(document.body)); this.currentJson = canonicalJson(this.current);
  }
  get body(): WnsDocument { return this.current; }
  get projectAccess(): ProjectAccess { return clone(this.access); }
  get savedConflict(): DocumentRecord | null { return this.conflict ? clone(this.conflict.document) : null; }
  get state(): SessionState {
    return { phase: this.phase, generation: this.generation.toString(), savedGeneration: this.savedGeneration.toString(), head: clone(this.head), saving: !!this.flight,
      dirty: this.generation !== this.savedGeneration, editable: !this.barrier && ['editing', 'saveFailed'].includes(this.phase), error: this.error };
  }
  subscribe(listener: Listener): () => void { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; }
  setViewSaver(save: (() => Promise<void>) | null): void { this.viewSaver = save; }
  private async saveView(): Promise<void> { try { await this.viewSaver?.(); } catch (reason) { throw this.recordFailure(reason); } }
  async persistView(): Promise<void> { await this.withLifecycleGuard(async () => { await this.flush(); await this.saveView(); }); }
  async projectWrite<T>(write: () => Promise<T>): Promise<T> {
    return this.withLifecycleGuard(async () => { await this.flush(); try { return await write(); } catch (reason) { throw this.recordFailure(reason); } });
  }
  private emit(): void { for (const listener of this.listeners) listener(); }
  private clearTimer(): void { if (this.timer) clearTimeout(this.timer); this.timer = null; }
  private schedule(): void {
    this.clearTimer();
    if (this.options.autosave === false || this.composing || this.phase !== 'editing' || this.generation === this.savedGeneration) return;
    this.dirtySince ??= Date.now();
    const delay = Math.max(0, Math.min(750, 2000 - (Date.now() - this.dirtySince)));
    this.timer = setTimeout(() => { this.timer = null; void this.flush().catch(() => { /* The subscribed state retains the error and live buffer. */ }); }, delay);
  }
  update(body: WnsDocument, cause: SaveSnapshot['cause'] = 'typing'): void {
    if (!this.state.editable) throw new SessionError('LifecycleBusy', 'Wait for the pending document operation.');
    const json = canonicalJson(body);
    if (json === this.currentJson) return;
    this.current = freeze(clone(body)); this.currentJson = json; this.generation += 1n;
    if (cause !== 'typing') this.nextCause = cause;
    this.dirtySince ??= Date.now(); this.schedule(); this.emit();
  }
  setComposing(value: boolean): void {
    this.composing = value;
    if (value) this.clearTimer();
    else { this.compositionWaiters.splice(0).forEach(resolve => resolve()); this.schedule(); }
  }
  private async capture(): Promise<Capture> {
    const request: SaveSnapshot = freeze({ access: clone(this.access), operationId: this.options.newId?.() ?? crypto.randomUUID(), expected: clone(this.head),
      localGeneration: this.generation.toString(), body: clone(this.current), cause: this.nextCause });
    const json = canonicalJson(request.body);
    try {
      const capture = { request, json, hash: await bodyHash(json), payloadHash: await bodyHash(logicalSaveJson(request)) };
      // Later edits may have selected a new cause while hashes were calculated.
      if (this.generation.toString() === request.localGeneration) this.nextCause = 'typing';
      return capture;
    } catch {
      this.phase = 'saveFailed'; this.error = 'Could not prepare this save. Your local text is retained.';
      this.clearTimer(); this.emit(); throw new SessionError('CaptureFailed', this.error);
    }
  }
  private acceptAck(ack: SaveAck, capture: Capture): void {
    const request = capture.request;
    const expectedVersion = version(request.expected.version) + (request.expected.bodyHash === capture.hash ? 0n : 1n);
    if (ack.projectId !== request.access.projectId || ack.session !== request.access.session || ack.session !== this.access.session
      || ack.documentId !== request.expected.documentId || ack.operationNamespace !== request.access.operationNamespace || ack.operationId !== request.operationId
      || ack.savedGeneration !== request.localGeneration || ack.head.documentId !== request.expected.documentId
      || ack.head.bodyHash !== capture.hash || version(ack.head.version) !== expectedVersion) {
      throw new SessionError('ProtocolError', 'The save acknowledgment does not match this editing session. Your text is retained.');
    }
    this.head = clone(ack.head); this.savedGeneration = version(ack.savedGeneration);
    this.pending = null; this.error = null;
    if (this.generation === this.savedGeneration) this.dirtySince = null;
    else this.dirtySince = Date.now();
  }
  private async send(capture: Capture): Promise<void> {
    this.pending = capture;
    try { this.acceptAck(await this.transport.save(capture.request), capture); }
    catch (reason) {
      throw this.recordFailure(reason, true);
    }
  }
  private recordFailure(reason: unknown, clearDefiniteSave = false): SessionError {
    const error = errorOf(reason); this.error = error.message;
    const definite = ['InvalidRequest', 'InvalidDocument', 'PersistenceUnavailable', 'DocumentNotFound', 'OperationIdReuse'].includes(error.code);
    this.phase = definite ? 'saveFailed' : 'reconciling';
    if (definite && clearDefiniteSave) this.pending = null;
    this.clearTimer(); this.emit(); return error;
  }
  private async writeCheckpoint(expected: Head, reason: CheckpointRequest['reason']): Promise<void> {
    const previousPhase = this.phase;
    try {
      const revision = await this.transport.checkpoint({ access: clone(this.access), expected: clone(expected), reason });
      await this.transport.validate(revision.body);
      if (!sameHead(revision.head, expected) || await bodyHash(canonicalJson(revision.body)) !== expected.bodyHash) {
        throw new SessionError('ProtocolError', 'The checkpoint response does not match the saved document.');
      }
    } catch (error) {
      const failure = this.recordFailure(error);
      if (previousPhase === 'conflict' && this.phase === 'saveFailed') { this.phase = 'conflict'; this.emit(); }
      throw failure;
    }
  }
  async flush(): Promise<void> {
    this.clearTimer();
    if (this.phase === 'disposed') throw new SessionError('Disposed', 'This document is closed.');
    if (this.composing) throw new SessionError('CompositionPending', 'Finish entering the current text before saving.');
    if (this.phase === 'reconciling' || this.phase === 'conflict') throw new SessionError('ReconciliationRequired', this.error ?? 'Check the saved version before continuing.');
    if (this.phase === 'saveFailed') { this.phase = this.barrier ? 'flushing' : 'editing'; this.error = null; }
    while (this.generation !== this.savedGeneration || this.flight) {
      if (!this.flight) {
        this.flight = this.capture().then(capture => this.send(capture)).finally(() => { this.flight = null; this.emit(); });
        this.emit();
      }
      await this.flight;
      if (this.composing) { this.schedule(); return; }
    }
    this.emit();
  }
  /** Navigation/normal close share this guard; they wait for an existing local operation. */
  async withLifecycleGuard<T>(work: () => Promise<T>): Promise<T> {
    while (this.owner) { try { await this.owner; } catch { /* Recheck state after the owner releases. */ } }
    const operation = Promise.resolve().then(async () => {
      if (this.phase === 'disposed') throw new SessionError('Disposed', 'This document is closed.');
      if (this.composing) await new Promise<void>(resolve => this.compositionWaiters.push(resolve));
      this.barrier = true;
      if (this.phase === 'editing') this.phase = 'flushing';
      this.emit();
      try { return await work(); }
      finally {
        this.barrier = false;
        if (this.phase === 'flushing' || this.phase === 'applying') this.phase = 'editing';
        this.emit();
      }
    });
    this.owner = operation;
    try { return await operation; } finally { if (this.owner === operation) this.owner = null; }
  }
  async checkpoint(reason: CheckpointRequest['reason']): Promise<void> {
    await this.withLifecycleGuard(async () => {
      await this.flush();
      await this.writeCheckpoint(this.head, reason);
      await this.saveView();
    });
  }
  async detach(reason: 'switch' | 'close' = 'switch'): Promise<void> {
    await this.detachAfter(async () => {}, reason);
  }
  /** Keep the current editor alive if opening the destination fails. */
  async detachAfter<T>(prepareDestination: () => Promise<T>, reason: 'switch' | 'close' = 'switch'): Promise<T> {
    return this.withLifecycleGuard(async () => {
      await this.flush();
      await this.writeCheckpoint(this.head, reason);
      await this.saveView();
      const destination = await prepareDestination();
      this.clearTimer(); this.phase = 'disposed';
      return destination;
    });
  }
  async applyPrepared(proposal: Proposal, prepared: PreparedProposal, preflight: () => PreparedEditorChange): Promise<void> {
    await this.withLifecycleGuard(async () => {
      await this.flush();
      if (!this.transport.apply) throw new SessionError('ApplyUnavailable', 'Applying suggestions is unavailable in this session.');
      if (proposal.historicalCopy || proposal.decision || !proposal.current || !sameHead(proposal.source, this.head)
        || prepared.proposalId !== proposal.id || canonicalJson(proposal.sourceBody) !== this.currentJson) {
        throw new SessionError('SuggestionStale', 'The story or suggestion changed. Refresh it before applying.');
      }
      this.phase = 'applying'; this.emit();
      const change = preflight();
      const body = freeze(clone(change.body)); const json = canonicalJson(body);
      await this.transport.validate(body);
      if (json !== canonicalJson(prepared.body) || await bodyHash(json) !== prepared.bodyHash || prepared.bodyHash === this.head.bodyHash) {
        throw new SessionError('InvalidProposal', 'The editor preview does not match the saved suggestion.');
      }
      const request: ApplyProposal = freeze({ access: clone(this.access), operationId: this.options.newId?.() ?? crypto.randomUUID(),
        proposalId: proposal.id, preparedId: prepared.id, expected: clone(this.head), resultHash: prepared.bodyHash, localGeneration: (this.generation + 1n).toString() });
      const { session: _session, writerLease: _lease, ...access } = request.access;
      const capture: ApplyCapture = { request, body, json, change, payloadHash: await bodyHash(canonicalJson({ ...request, access })) };
      await this.sendApply(capture);
    });
  }
  private validateApplyResult(result: ApplyAck['result'], capture: ApplyCapture): void {
    const request = capture.request; const applied = result.applied;
    if (!applied || applied.proposalId !== request.proposalId || applied.preparedId !== request.preparedId
      || !applied.decisionId || !applied.beforeRevisionId || !applied.afterRevisionId || applied.beforeRevisionId === applied.afterRevisionId
      || result.head.documentId !== request.expected.documentId || result.head.bodyHash !== request.resultHash
      || version(result.head.version) !== version(request.expected.version) + 1n || result.savedGeneration !== request.localGeneration) {
      throw new SessionError('ProtocolError', 'The saved author decision does not match this prepared edit.');
    }
  }
  private commitApply(capture: ApplyCapture, head: Head): void {
    // Input remains gated. The live editor may already contain this exact
    // transaction after an exceptional post-dispatch callback failure.
    let displayed: WnsDocument;
    try { displayed = capture.change.commit(); }
    catch (error) {
      // A callback may fail after the transaction became visible. Keep that
      // actual buffer for a later explicit conflict choice, not the old base.
      if (capture.change.read) this.retainDisplayed(capture.change.read());
      throw error;
    }
    if (canonicalJson(displayed) !== capture.json) {
      this.retainDisplayed(displayed);
      throw new SessionError('ProtocolError', 'The saved edit could not be displayed exactly. Reconnect to recover the committed manuscript.');
    }
    this.current = capture.body; this.currentJson = capture.json; this.head = clone(head);
    this.generation = version(capture.request.localGeneration); this.savedGeneration = this.generation;
    this.pendingApply = null; this.error = null; this.dirtySince = null; this.nextCause = 'typing';
    this.phase = 'flushing'; this.emit();
  }
  private retainDisplayed(displayed: WnsDocument): void {
    const json = canonicalJson(displayed);
    if (json !== this.currentJson) { this.current = freeze(clone(displayed)); this.currentJson = json; this.generation += 1n; }
  }
  private async sendApply(capture: ApplyCapture): Promise<void> {
    this.pendingApply = capture;
    let receivedAck = false;
    try {
      const ack = await this.transport.apply!(capture.request);
      receivedAck = true;
      if (ack.operationId !== capture.request.operationId || canonicalJson(ack.access) !== canonicalJson(this.access)) {
        throw new SessionError('ProtocolError', 'The Apply acknowledgment belongs to another editing session.');
      }
      this.validateApplyResult(ack.result, capture);
      await this.transport.validate(ack.document.body);
      if (!sameHead(ack.document.head, ack.result.head) || canonicalJson(ack.document.body) !== capture.json) {
        throw new SessionError('ProtocolError', 'The saved manuscript advanced beyond this Apply result. Reconnect to compare the current version.');
      }
      this.commitApply(capture, ack.document.head);
    } catch (reason) {
      const error = errorOf(reason);
      if (!receivedAck && ['SuggestionStale', 'SuggestionAlreadyDecided', 'PreparedVersionConflict', 'ProposalNotPrepared', 'ScopeViolation', 'InvalidProposal', 'InvalidRequest', 'InvalidDocument', 'PersistenceUnavailable', 'ContextProjectMismatch', 'OperationIdReusedWithDifferentPayload'].includes(error.code)) {
        this.pendingApply = null; this.phase = 'flushing'; this.error = null; throw error;
      }
      this.phase = 'reconciling'; this.error = error.message; this.clearTimer(); this.emit(); throw error;
    }
  }
  private async reconcileApply(capture: ApplyCapture): Promise<void> {
    this.phase = 'reconciling'; this.clearTimer(); this.emit();
    try {
      const restored = await this.transport.reconcile({ projectId: this.access.projectId, operationNamespace: this.access.operationNamespace, session: this.access.session,
        documentId: this.head.documentId, pendingOperationIds: [capture.request.operationId] });
      await this.transport.validate(restored.document.body);
      if (restored.access.projectId !== this.access.projectId || restored.access.operationNamespace !== this.access.operationNamespace
        || restored.access.session !== this.access.session || !restored.access.writerLease || restored.access.writerLease === this.access.writerLease
        || restored.document.head.documentId !== this.head.documentId || await bodyHash(canonicalJson(restored.document.body)) !== restored.document.head.bodyHash) {
        throw new SessionError('ProtocolError', 'The recovered Apply result does not belong to this session.');
      }
      version(restored.document.head.version); this.access = clone(restored.access);
      const receipts = restored.receipts.filter(item => item.operationId === capture.request.operationId);
      if (receipts.length > 1) throw new SessionError('ProtocolError', 'The Apply operation has duplicate receipts.');
      const receipt = receipts[0];
      if (receipt) {
        if (receipt.operationKind !== 'apply' || receipt.payloadHash !== capture.payloadHash || !receipt.result.applied) throw new SessionError('ProtocolError', 'The recovered receipt does not match the pending Apply.');
        this.validateApplyResult({ ...receipt.result, applied: receipt.result.applied }, capture);
        if (!sameHead(restored.document.head, receipt.result.head) || canonicalJson(restored.document.body) !== capture.json) { this.setConflict(restored); return; }
        this.commitApply(capture, restored.document.head);
      } else if (sameHead(restored.document.head, capture.request.expected) && canonicalJson(restored.document.body) === this.currentJson) {
        // Only the fenced query establishes absence. Reuse the exact logical
        // operation under its new lease, never invent another Apply identity.
        await this.sendApply({ ...capture, request: freeze({ ...clone(capture.request), access: clone(this.access) }) });
      } else { this.setConflict(restored); }
    } catch (reason) { const error = errorOf(reason); this.phase = 'reconciling'; this.error = error.message; this.emit(); throw error; }
  }
  async reconcile(): Promise<void> {
    await this.withLifecycleGuard(async () => {
      if (this.pendingApply) { await this.reconcileApply(this.pendingApply); return; }
      if (this.flight) { try { await this.flight; } catch { /* Its immutable pending capture is retained. */ } }
      this.phase = 'reconciling'; this.clearTimer(); this.emit();
      const capture = this.pending;
      try {
        const restored = await this.transport.reconcile({ projectId: this.access.projectId, operationNamespace: this.access.operationNamespace, session: this.access.session,
          documentId: this.head.documentId, pendingOperationIds: capture ? [capture.request.operationId] : [] });
        await this.transport.validate(restored.document.body);
        if (restored.access.projectId !== this.access.projectId || restored.access.operationNamespace !== this.access.operationNamespace
          || restored.access.session !== this.access.session || !restored.access.writerLease || restored.access.writerLease === this.access.writerLease
          || restored.document.head.documentId !== this.head.documentId || await bodyHash(canonicalJson(restored.document.body)) !== restored.document.head.bodyHash) {
          throw new SessionError('ProtocolError', 'The recovered document does not match this session.');
        }
        version(restored.document.head.version);
        this.access = clone(restored.access);
        const matching = capture ? restored.receipts.filter(item => item.operationId === capture.request.operationId) : [];
        if (matching.length > 1) throw new SessionError('ProtocolError', 'The recovered operation has duplicate receipts.');
        const receipt = matching[0];
        if (capture && receipt) {
          if (receipt.operationKind !== 'save' || receipt.payloadHash !== capture.payloadHash || receipt.result.savedGeneration !== capture.request.localGeneration
            || receipt.result.head.documentId !== capture.request.expected.documentId || receipt.result.head.bodyHash !== capture.hash
            || version(receipt.result.head.version) !== version(capture.request.expected.version) + (capture.hash === capture.request.expected.bodyHash ? 0n : 1n)) throw new SessionError('ProtocolError', 'The recovered receipt does not match the pending save.');
          if (!sameHead(restored.document.head, receipt.result.head)) { this.setConflict(restored); return; }
          this.head = clone(restored.document.head); this.savedGeneration = version(receipt.result.savedGeneration); this.pending = null;
        } else if (capture && sameHead(restored.document.head, capture.request.expected)) {
          // The fence proved absence. Retry exactly the same logical operation under the new lease.
          const retry: Capture = { ...capture, request: freeze({ ...clone(capture.request), access: clone(this.access) }) };
          await this.send(retry);
        } else if (!capture && sameHead(restored.document.head, this.head)) {
          this.pending = null;
        } else { this.setConflict(restored); return; }
        this.phase = 'editing'; this.error = null;
        if (this.generation === this.savedGeneration) this.dirtySince = null;
      } catch (reason) { const error = errorOf(reason); this.phase = 'reconciling'; this.error = error.message; this.emit(); throw error; }
    });
    this.schedule();
  }
  private setConflict(restored: ReconciledDocument): void {
    this.conflict = clone(restored); this.phase = 'conflict';
    this.error = 'The saved manuscript also changed. Your local text is retained. Compare both versions before choosing what to keep.';
    this.emit();
  }
  /** Only a visible author choice may replace a diverged saved body or the local buffer. */
  async resolveConflict(choice: 'keepLocal' | 'useSaved'): Promise<WnsDocument> {
    return this.withLifecycleGuard(async () => {
      if (this.phase !== 'conflict' || !this.conflict) throw new SessionError('NoConflict', 'There is no saved conflict to resolve.');
      const saved = this.conflict.document;
      // Preserve the other side before an explicitly chosen overwrite.
      await this.writeCheckpoint(saved.head, 'manual');
      this.head = clone(saved.head); this.pending = null; this.pendingApply = null; this.conflict = null; this.error = null;
      this.phase = 'flushing';
      if (choice === 'useSaved') {
        this.current = freeze(clone(saved.body)); this.currentJson = canonicalJson(this.current);
        this.generation += 1n; this.savedGeneration = this.generation; this.dirtySince = null;
      } else { this.generation += 1n; await this.flush(); }
      return this.current;
    });
  }
}
