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
  state = emptyWorkshop();
  results: WorkshopResult[] = [];
  version = '0';
  generation = 0;
  savedGeneration = 0;
  loaded = false;
  locked = false;
  error = '';
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
    isDirty: () => this.generation !== this.savedGeneration,
    capture: () => {
      if (!this.loaded || this.generation === this.savedGeneration) return null;
      const request: SaveWorkshop = { access: this.access, operationId: crypto.randomUUID(), expectedVersion: this.version, state: structuredClone(this.state) };
      const generation = this.generation;
      return {
        send: async () => {
          const ack = await this.api.save(request);
          this.version = ack.version;
          this.savedGeneration = generation;
          if (this.generation === generation) this.state = ack.state;
        },
        commit: () => {},
        discardOn: error => ['InvalidRequest', 'InvalidDocument', 'InvalidWorkshopCandidate', 'PreferenceConflict', 'ProtectedContentChanged', 'StaleRelationship', 'UnsupportedSchema'].includes(errorCode(error) ?? ''),
        fail: error => { this.error = describeWorkshopError(error); },
      };
    },
  });

  get saving() { return this.loop.saving; }
  get dirty() { return this.generation !== this.savedGeneration || this.loop.hasPending; }
  get status() { return this.error ? 'Not saved' : this.saving ? 'Saving…' : this.dirty ? 'Unsaved changes' : 'Saved on this computer'; }

  async load() {
    const sequence = ++this.loadSequence;
    const value = await this.api.read(this.access);
    if (sequence !== this.loadSequence || this.dirty) return;
    this.state = value.state; this.version = value.version; this.results = value.results;
    this.loaded = true; this.notify();
  }
  edit(change: (state: WorkshopState) => WorkshopState) {
    if (this.locked) return;
    if (!this.loaded) throw new Error('Wait for the saved Workshop to open.');
    this.state = change(structuredClone(this.state)); this.generation += 1; this.notify();
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => { this.timer = null; void this.flush().catch(() => {}); }, 250);
  }
  editSession(sessionId: string, change: (session: WorkshopSession) => WorkshopSession, changesWorkingVersion = false) {
    this.edit(state => ({ ...state, sessions: state.sessions.map(session => session.id !== sessionId ? session : {
      ...change(session), workingGeneration: changesWorkingVersion ? String(BigInt(session.workingGeneration) + 1n) : session.workingGeneration,
    }) }));
  }
  async refreshResults() {
    const startedVersion = this.version;
    const startedGeneration = this.generation;
    const value = await this.api.read(this.access);
    this.results = value.results;
    // Reads during typing only update immutable run output. Even a matching
    // version is not permission to replace the author's newer local buffer.
    if (!this.dirty && !this.saving && this.version === startedVersion && this.generation === startedGeneration && BigInt(value.version) > BigInt(this.version)) {
      this.state = value.state; this.version = value.version;
    }
    this.notify();
  }
  async flush(): Promise<void> {
    if (this.timer) { clearTimeout(this.timer); this.timer = null; }
    if (!this.loaded || !this.dirty) return;
    this.error = ''; this.notify();
    try { await this.loop.flush(); }
    finally { this.notify(); }
  }
  acceptAdoption(snapshot: WorkshopSnapshot, expectedGeneration: number) {
    if (this.dirty || this.generation !== expectedGeneration) throw new Error('Your working version changed. Reopen the saved Workshop before continuing.');
    this.state = snapshot.state; this.version = snapshot.version; this.error = ''; this.notify();
  }
  dispose() { if (this.timer) clearTimeout(this.timer); this.timer = null; this.listeners.clear(); }
}
