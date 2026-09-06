import { invoke } from '@tauri-apps/api/core';
import type { MockContextBudget, ProviderBinding, SourceRead, SourceRef } from './context';
import type { Head, ProjectAccess } from './projects';
import type { ModelSelection } from './providers';

export type MemoryJobStatus = 'queued' | 'running' | 'stopping' | 'completed' | 'stopped' | 'failed' | 'interrupted';
export type MemoryDispatchState = 'pending' | 'dispatched';
export type MemoryProviderOutcome = 'completed' | 'stopped' | 'timedOut' | 'outputLimit' | 'failed';

export interface DigestEvidence { blockId: string; fromUtf16: number; toUtf16: number; quote: string }
export interface DigestItem { text: string; evidence: DigestEvidence[]; uncertainty: string | null }
export interface DigestCandidate { schemaVersion: string; source: SourceRef; items: DigestItem[] }
export interface MemoryOwner { projectId: string; operationNamespace: string; jobId: string }
export interface MemoryResult {
  jobId: string; eventId: string; rawOutput: string | null; outcome: MemoryProviderOutcome;
  confirmedStdinBytes: string | null;
  usage: { inputTokens: number; cachedInputTokens: number; cacheWriteInputTokens: number; outputTokens: number; reasoningOutputTokens: number } | null;
  cleanup: 'settled' | 'unresolved' | null; error: string | null; validationError: string | null;
  candidate: DigestCandidate | null; effectiveIdentity: string | null; createdAt: string;
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
  /** Desktop recovery envelope; true while a terminal result is being persisted. */
  pendingSave?: boolean;
  pendingJobIds?: string[];
}
export interface StartMemory {
  access: ProjectAccess; operationId: string; expected: Head; budget: MockContextBudget; modelSelection: ModelSelection;
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
