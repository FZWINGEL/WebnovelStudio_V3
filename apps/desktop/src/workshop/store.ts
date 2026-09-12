import { readWorkshop, saveWorkshop, type SaveWorkshop, type WorkshopResult, type WorkshopSession, type WorkshopSnapshot, type WorkshopState, type WorkshopView } from '../ipc/workshop';
import type { ProjectAccess } from '../ipc/projects';
import { createSaveLoop, errorCode } from '../kernel';
import { LENSES, type WorkshopLens } from './catalog';

export const emptyWorkshop = (): WorkshopState => ({ schemaVersion: 1, currentSessionId: null, sessions: [], preferences: [], decisions: [], relationships: [], impacts: [], presets: [] });
export function newSession(lens: WorkshopLens = 'overview', brief = ''): WorkshopSession {
  const id = crypto.randomUUID();
  const focus = LENSES.find(item => item.id === lens)!;
  return { id, title: brief.trim().slice(0, 70) || 'A new exploration', lens, parentSessionId: null, branchKind: 'working', brief, direction: '', stillOpen: '', focusQuestion: focus.question, focusReason: focus.reason, focusDocumentId: null, anchorDocumentId: `workshop-${id}`, depth: 'sketch', outsideDirection: false, includedDocumentIds: [], workingText: '', workingTitle: '', workingGeneration: '0', selectedDetails: [], choices: [], questions: [], composer: '', selectedScope: 'Whole working version', originalNotes: '', activeRunId: null };
}

export interface WorkshopTransport {
  read(access: ProjectAccess): Promise<WorkshopView>;
  save(request: SaveWorkshop): Promise<WorkshopSnapshot>;
}
const transport: WorkshopTransport = { read: readWorkshop, save: saveWorkshop };
export function describeWorkshopError(error: unknown): string {
  return error && typeof error === 'object' && 'detail' in error ? String(error.detail) : error instanceof Error ? error.message : String(error);
}

/** Manual drafts have an independent save watermark; provider polling never replaces them. */
export class WorkshopStore {
  private currentState = emptyWorkshop();
  private currentResults: WorkshopResult[] = [];
  private currentVersion = '0';
  private currentGeneration = 0;
  private persistedGeneration = 0;
  private isLoaded = false;
  private interactionLocked = false;
  private lastError = '';
  private listeners = new Set<() => void>();
  private timer: ReturnType<typeof setTimeout> | null = null;
  private loadSequence = 0;

  constructor(readonly access: ProjectAccess, private readonly api: WorkshopTransport = transport) {}
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  private notify() { for (const listener of this.listeners) listener(); }

  /**
   * A refusal that provably happened before the transaction may be discarded —
   * retaining those bytes would prevent the author from saving their correction.
   * Anything else may have committed, so its bytes and operation id are kept.
   */
  private readonly loop = createSaveLoop({
    isDirty: () => this.currentGeneration !== this.persistedGeneration,
    capture: () => {
      if (!this.isLoaded || this.currentGeneration === this.persistedGeneration) return null;
      const request: SaveWorkshop = { access: this.access, operationId: crypto.randomUUID(), expectedVersion: this.currentVersion, state: structuredClone(this.currentState) };
      const generation = this.currentGeneration;
      return {
        send: async () => {
          const ack = await this.api.save(request);
          this.currentVersion = ack.version;
          this.persistedGeneration = generation;
          if (this.currentGeneration === generation) this.currentState = ack.state;
        },
        commit: () => {},
        discardOn: error => ['InvalidRequest', 'InvalidDocument', 'InvalidWorkshopCandidate', 'PreferenceConflict', 'ProtectedContentChanged', 'StaleRelationship', 'UnsupportedSchema'].includes(errorCode(error) ?? ''),
        fail: error => { this.lastError = describeWorkshopError(error); },
      };
    },
  });

  get state(): Readonly<WorkshopState> { return this.currentState; }
  get results(): readonly WorkshopResult[] { return this.currentResults; }
  get version() { return this.currentVersion; }
  get generation() { return this.currentGeneration; }
  get savedGeneration() { return this.persistedGeneration; }
  get loaded() { return this.isLoaded; }
  get locked() { return this.interactionLocked; }
  get error() { return this.lastError; }
  get saving() { return this.loop.saving; }
  get dirty() { return this.currentGeneration !== this.persistedGeneration || this.loop.hasPending; }
  get status() { return this.lastError ? 'Not saved' : this.saving ? 'Saving…' : this.dirty ? 'Unsaved changes' : 'Saved on this computer'; }

  /** Synchronize render's admission guard without notifying React during render. */
  setInteractionLocked(locked: boolean) { this.interactionLocked = locked; }

  /** Run output is separate from the author's draft and its save watermark. */
  recordProvisionalResult(result: WorkshopResult) {
    this.currentResults = [...this.currentResults.filter(item => item.run.id !== result.run.id), structuredClone(result)];
    this.notify();
  }

  async load() {
    const sequence = ++this.loadSequence;
    const value = await this.api.read(this.access);
    if (sequence !== this.loadSequence || this.dirty) return;
    this.currentState = value.state; this.currentVersion = value.version; this.currentResults = value.results;
    this.isLoaded = true; this.notify();
  }
  edit(change: (state: WorkshopState) => WorkshopState) {
    if (this.interactionLocked) return;
    if (!this.isLoaded) throw new Error('Wait for the saved Workshop to open.');
    this.currentState = change(structuredClone(this.currentState)); this.currentGeneration += 1; this.notify();
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => { this.timer = null; void this.flush().catch(() => {}); }, 250);
  }
  editSession(sessionId: string, change: (session: WorkshopSession) => WorkshopSession, changesWorkingVersion = false) {
    this.edit(state => ({ ...state, sessions: state.sessions.map(session => session.id !== sessionId ? session : {
      ...change(session), workingGeneration: changesWorkingVersion ? String(BigInt(session.workingGeneration) + 1n) : session.workingGeneration,
    }) }));
  }
  async refreshResults() {
    const startedVersion = this.currentVersion;
    const startedGeneration = this.currentGeneration;
    const value = await this.api.read(this.access);
    this.currentResults = value.results;
    // Reads during typing only update immutable run output. Even a matching
    // version is not permission to replace the author's newer local buffer.
    if (!this.dirty && !this.saving && this.currentVersion === startedVersion && this.currentGeneration === startedGeneration && BigInt(value.version) > BigInt(this.currentVersion)) {
      this.currentState = value.state; this.currentVersion = value.version;
    }
    this.notify();
  }
  async flush(): Promise<void> {
    if (this.timer) { clearTimeout(this.timer); this.timer = null; }
    if (!this.isLoaded || !this.dirty) return;
    this.lastError = ''; this.notify();
    try {
      // A joined drain may have settled before this caller's edit. Its
      // debounce was cancelled above, so do not return until that edit saves.
      do { await this.loop.flush(); } while (this.dirty);
    }
    finally { this.notify(); }
  }
  acceptAdoption(snapshot: WorkshopSnapshot, expectedGeneration: number) {
    if (this.dirty || this.currentGeneration !== expectedGeneration) throw new Error('Your working version changed. Reopen the saved Workshop before continuing.');
    this.currentState = snapshot.state; this.currentVersion = snapshot.version; this.lastError = ''; this.notify();
  }
  dispose() { if (this.timer) clearTimeout(this.timer); this.timer = null; this.listeners.clear(); }
}
