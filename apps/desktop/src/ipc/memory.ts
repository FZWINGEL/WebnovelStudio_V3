import type {
  DigestCandidate,
  DigestEvidence,
  DigestItem,
} from './generated/context';
export type {
  DigestCandidate,
  DigestEvidence,
  DigestItem,
};

import { invoke } from '@tauri-apps/api/core';
import type { AppServerDelivery, MockContextBudget, ProviderBinding, SourceRead, SourceRef } from './context';
import type { ProviderResult } from './discussions';
import type { Head, ProjectAccess } from './projects';
import type { ModelSelection } from './providers';

export type MemoryJobStatus = 'queued' | 'running' | 'stopping' | 'completed' | 'stopped' | 'failed' | 'interrupted';
export type MemoryDispatchState = 'pending' | 'dispatched';
export type MemoryProviderOutcome = 'completed' | 'stopped' | 'timedOut' | 'outputLimit' | 'failed';

export interface MemoryOwner { projectId: string; operationNamespace: string; jobId: string }
export interface MemoryResult {
  jobId: string; eventId: string; rawOutput: string | null; outcome: MemoryProviderOutcome;
  confirmedStdinBytes: string | null;
  usage: { inputTokens: number; cachedInputTokens: number; cacheWriteInputTokens: number; outputTokens: number; reasoningOutputTokens: number } | null;
  cleanup: 'settled' | 'unresolved' | null; error: string | null; validationError: string | null;
  candidate: DigestCandidate | null; effectiveIdentity: string | null; createdAt: string;
  /** HTTP memory receipts are optional so historical Codex/mock results keep their old shape. */
  delivery?: ProviderResult['delivery'];
  /** Persistent Codex app-server delivery is separate from exec/HTTP evidence. */
  appServer?: AppServerDelivery;
}
export interface MemoryViewRecord {
  id: string; jobId: string; projectId: string; operationNamespace: string; documentId: string; target: Head;
  source: SourceRef; snapshotId: string; packetId: string; contextSourceEpoch: string; disclosurePolicyVersion: string;
  candidate: DigestCandidate | null; current: boolean; sourceChanged: boolean; policyAvailable: boolean; createdAt: string;
  historical?: boolean;
}
export interface MemoryJob {
  id: string; owner: MemoryOwner; operationId: string; payloadHash: string; target: Head; source: SourceRef;
  snapshotId: string; packetId: string; contextSourceEpoch: string; disclosurePolicyVersion: string;
  providerBinding: ProviderBinding | null; status: MemoryJobStatus; dispatchState: MemoryDispatchState;
  stopReason: string | null; result: MemoryResult | null; view: MemoryViewRecord | null; createdAt: string; updatedAt: string;
  historical?: boolean;
}
export interface MemoryRead {
  documentId: string;
  jobs: MemoryJob[];
  views: MemoryViewRecord[];
  /**
   * A designed recovery envelope that Rust does not send.
   *
   * `read_memory` returns a document's jobs and views and nothing else — there
   * is no `pending_save` or `pending_job_ids` anywhere in the workspace. These
   * two fields were declared here and read by `ChapterMemory`, so the
   * pre-existing branch that used them was unreachable in the app while the
   * tests, which pass them in by hand, exercised it. Anything built on them —
   * the retry-a-pending-save path in `reconcile` — has never run.
   *
   * Kept declared rather than deleted because the consuming code is real and
   * reviewed; what is missing is the producer. Either `MemoryRead` gains the
   * two fields and the path goes live, or the path is removed. That is a
   * decision about whether the app should offer recovery here, so it is not
   * taken by a type migration.
   */
  pendingSave?: boolean;
  pendingJobIds?: string[];
}
export interface StartMemory {
  access: ProjectAccess; operationId: string; expected: Head; budget: MockContextBudget; modelSelection: ModelSelection;
  /** Revision of the separately saved story-memory provider preference. */
  maintenanceRevision: string;
}

/** The native command freezes the selected model and supplies its trusted binding. */
export const startMemory = ({ modelSelection, ...request }: StartMemory): Promise<MemoryJob> =>
  invoke('start_memory', { request: { ...request, modelSelection } });
export const readMemory = (access: ProjectAccess, documentId: string): Promise<MemoryRead> =>
  invoke('read_memory', { access, documentId });
export const readMemorySource = (access: ProjectAccess, viewId: string): Promise<SourceRead> =>
  invoke('read_memory_source', { access, viewId });
export const stopMemory = (access: ProjectAccess, jobId: string): Promise<MemoryJob> =>
  invoke('stop_memory', { access, jobId });
/** Reconciles/install-persisting a completed local result; it never starts a model request. */
export const retryMemorySave = (access: ProjectAccess, jobId: string): Promise<MemoryJob> =>
  invoke('retry_memory_save', { access, jobId });
