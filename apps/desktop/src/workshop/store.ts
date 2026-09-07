import { readWorkshop, saveWorkshop, type SaveWorkshop, type WorkshopResult, type WorkshopSession, type WorkshopSnapshot, type WorkshopState, type WorkshopView } from '../ipc/workshop';
import type { ProjectAccess } from '../ipc/projects';
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
  saving = false;
  locked = false;
  error = '';
  private listeners = new Set<() => void>();
  private timer: ReturnType<typeof setTimeout> | null = null;
  private flight: Promise<void> | null = null;
  private pending: { request: SaveWorkshop; generation: number } | null = null;
  private loadSequence = 0;

  constructor(readonly access: ProjectAccess, private readonly api: WorkshopTransport = transport) {}
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  private notify() { for (const listener of this.listeners) listener(); }
  get dirty() { return this.generation !== this.savedGeneration || this.pending !== null; }
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
    if (this.flight) { await this.flight; if (this.dirty) await this.flush(); return; }
    if (!this.loaded || !this.dirty) return;
    this.saving = true; this.error = ''; this.notify();
    const run = async () => {
      while (this.dirty) {
        this.pending ??= { request: { access: this.access, operationId: crypto.randomUUID(), expectedVersion: this.version, state: structuredClone(this.state) }, generation: this.generation };
        const captured = this.pending;
        try {
          const ack = await this.api.save(captured.request);
          this.version = ack.version;
          this.savedGeneration = captured.generation;
          if (this.generation === captured.generation) this.state = ack.state;
          this.pending = null;
        } catch (error) {
          // Keep exact bytes and operation ID for a lost-ack retry. A newer
          // author edit must not mutate the uncertain request.
          const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : '';
          // Explicit validation failures precede a transaction. Retaining those
          // bytes would prevent the author from saving their correction.
          if (['InvalidRequest', 'InvalidDocument', 'InvalidWorkshopCandidate', 'PreferenceConflict', 'ProtectedContentChanged', 'StaleRelationship', 'UnsupportedSchema'].includes(code)) this.pending = null;
          this.error = describeWorkshopError(error); throw error;
        }
      }
    };
    this.flight = run();
    try { await this.flight; }
    finally { this.flight = null; this.saving = false; this.notify(); }
  }
  acceptAdoption(snapshot: WorkshopSnapshot, expectedGeneration: number) {
    if (this.dirty || this.generation !== expectedGeneration) throw new Error('Your working version changed. Reopen the saved Workshop before continuing.');
    this.state = snapshot.state; this.version = snapshot.version; this.error = ''; this.notify();
  }
  dispose() { if (this.timer) clearTimeout(this.timer); this.timer = null; this.listeners.clear(); }
}
