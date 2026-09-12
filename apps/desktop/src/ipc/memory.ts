import type {
  MemoryJobStatus,
  MemoryDispatchState,
  MemoryOwner,
  MemoryResult,
  MemoryJob,
} from './generated/story';
export type {
  MemoryJobStatus,
  MemoryDispatchState,
  MemoryOwner,
  MemoryResult,
  MemoryJob,
};

import type { MemoryView } from './generated/story';
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

export type MemoryProviderOutcome = 'completed' | 'stopped' | 'timedOut' | 'outputLimit' | 'failed';

/** The frontend's name for Rust's `MemoryView`; generated in `./generated/story`. */
export type MemoryViewRecord = MemoryView;

export interface MemoryRead {
  documentId: string;
  jobs: MemoryJob[];
  views: MemoryViewRecord[];
  /**
   * The desktop command's recovery envelope, not core's `MemoryRead`.
   *
   * `read_memory` in `apps/desktop/src-tauri` answers with `DesktopMemoryRead`
   * — `#[serde(flatten)] read: MemoryRead` plus these two — so the wire is flat
   * and this is the type the frontend actually sees. Core's `MemoryRead` is the
   * inner half and does not carry them.
   *
   * That settles how §4.4 treats this module: it cannot be generated from
   * `wns-story`, because the type it mirrors belongs to the app shell. Sweeping
   * `crates/` for a producer and finding none was not evidence that none
   * exists — the producer is one directory outside the sweep.
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
