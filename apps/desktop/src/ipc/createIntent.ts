import type { CreateDocumentIntent, DocumentRecord, OpenedProject, ProjectAccess } from './projects';

export interface CreateIntentTransport {
  createDocument(access: ProjectAccess, intent: CreateDocumentIntent): Promise<DocumentRecord>;
  reconcileProject(projectId: string, session: string): Promise<OpenedProject>;
}

export interface CreateIntentRunOptions {
  projectId: string;
  session: string;
  access: ProjectAccess;
  intent: CreateDocumentIntent;
  transport: CreateIntentTransport;
  /** Adopt the snapshot's fresh lease into a mounted editor, if one exists. */
  onReconciled?: (snapshot: OpenedProject) => Promise<ProjectAccess>;
  /** One initial attempt plus one retry is enough to settle a lost ACK. */
  maxAttempts?: number;
}

export interface CreateIntentResult {
  record: DocumentRecord;
  access: ProjectAccess;
  snapshot: OpenedProject | null;
}

export class CreateIntentUnresolvedError extends Error {
  readonly code = 'CreateIntentUnresolved';
  readonly detail = 'The document creation outcome is still unknown. Reconcile before trying another document.';
  constructor() {
    super('The document creation outcome is still unknown. Reconcile before trying another document.');
    this.name = 'CreateIntentUnresolvedError';
  }
}

/** Marks a recovery failure so the UI retains the immutable intent for retry. */
export class CreateIntentRecoveryError extends Error {
  readonly retainIntent = true;
  readonly code: string;
  readonly detail: string;
  constructor(public readonly cause: unknown) {
    const code = errorCode(cause) ?? 'CreateIntentRecoveryFailed';
    const detail = cause && typeof cause === 'object' && 'detail' in cause ? String((cause as { detail: unknown }).detail) : 'The project could not be reconciled.';
    super(detail);
    this.name = 'CreateIntentRecoveryError';
    this.code = code;
    this.detail = detail;
  }
}

export function errorCode(reason: unknown): string | null {
  if (!reason || typeof reason !== 'object' || !('code' in reason)) return null;
  const code = (reason as { code?: unknown }).code;
  return typeof code === 'string' ? code : null;
}

/**
 * These errors are rejected before a create can commit. Keep this list small:
 * an unknown native/transport error must be reconciled before the intent can
 * be discarded or replayed.
 */
const DEFINITE_CREATE_ERRORS = new Set([
  'InvalidRequest',
  'InvalidDocument',
  'DocumentNotFound',
  'WrongProjectSession',
  'OperationIdReuse',
  'OperationIdReusedWithDifferentPayload',
]);

export function isDefiniteCreateError(reason: unknown): boolean {
  return DEFINITE_CREATE_ERRORS.has(errorCode(reason) ?? '');
}

/** Every outcome outside the explicit pre-commit set is safe only to reconcile. */
export function isUncertainCreateError(reason: unknown): boolean {
  return !isDefiniteCreateError(reason);
}

function withAccess(snapshot: OpenedProject, access: ProjectAccess): OpenedProject {
  return { ...snapshot, access };
}

/**
 * Attempts one stable logical create, then fences and reconciles at most once
 * before retrying the exact same operation and document IDs. A record found in
 * the reconciled snapshot wins, so a committed lost-ACK never creates a copy.
 */
export async function runCreateIntent(options: CreateIntentRunOptions): Promise<CreateIntentResult> {
  let access = structuredClone(options.access);
  const intent = structuredClone(options.intent);
  let snapshot: OpenedProject | null = null;
  const maxAttempts = Math.max(1, Math.min(options.maxAttempts ?? 2, 2));

  for (let attempt = 0; attempt < maxAttempts; attempt += 1) {
    try {
      const record = await options.transport.createDocument(access, structuredClone(intent));
      return { record, access, snapshot };
    } catch (reason) {
      if (!isUncertainCreateError(reason)) throw reason;
      try {
        snapshot = await options.transport.reconcileProject(options.projectId, options.session);
        const recoveredAccess = options.onReconciled ? await options.onReconciled(snapshot) : snapshot.access;
        access = structuredClone(recoveredAccess);
      } catch (recoveryError) {
        throw new CreateIntentRecoveryError(recoveryError);
      }
      snapshot = withAccess(snapshot, access);
      const existing = snapshot.documents.find(document => document.head.documentId === intent.documentId);
      if (existing) return { record: existing, access, snapshot };
      if (attempt + 1 >= maxAttempts) throw new CreateIntentUnresolvedError();
    }
  }
  throw new CreateIntentUnresolvedError();
}
