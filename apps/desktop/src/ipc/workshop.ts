import type {
  WorkshopExploration,
} from './generated/story';
export type {
  WorkshopExploration,
};

import type {
  CandidateChoice,
  Lens,
  PreviewWorkshopAdoption,
  SaveWorkshop,
  SelectedDetail,
  StoryPossibility,
  WorkshopAdoptionAck,
  WorkshopAdoptionImpact,
  WorkshopAdoptionPreview,
  WorkshopAdoptionTarget,
  WorkshopCandidate,
  WorkshopDecision,
  WorkshopImpact,
  WorkshopImpactDraft,
  WorkshopOutput,
  WorkshopPreference,
  WorkshopPreset,
  WorkshopQuestion,
  WorkshopRelationship,
  WorkshopRelationshipDraft,
  WorkshopResult,
  WorkshopSession,
  WorkshopSnapshot,
  WorkshopState,
  WorkshopView,
} from './generated/workshop';
export type {
  CandidateChoice,
  Lens,
  PreviewWorkshopAdoption,
  SaveWorkshop,
  SelectedDetail,
  StoryPossibility,
  WorkshopAdoptionAck,
  WorkshopAdoptionImpact,
  WorkshopAdoptionPreview,
  WorkshopAdoptionTarget,
  WorkshopCandidate,
  WorkshopDecision,
  WorkshopImpact,
  WorkshopImpactDraft,
  WorkshopOutput,
  WorkshopPreference,
  WorkshopPreset,
  WorkshopQuestion,
  WorkshopRelationship,
  WorkshopRelationshipDraft,
  WorkshopResult,
  WorkshopSession,
  WorkshopSnapshot,
  WorkshopState,
  WorkshopView,
};

import { invoke } from '@tauri-apps/api/core';
import type { DocumentRecord, Head, ProjectAccess } from './projects';
import type { DiscussionRun, DiscussionStart } from './discussions';
import type { ModelSelection } from './providers';
import type { WnsDocument } from '../kernel';

export const readWorkshop = (access: ProjectAccess): Promise<WorkshopView> => invoke('read_workshop', { access });
export const saveWorkshop = (request: SaveWorkshop): Promise<WorkshopSnapshot> => invoke('save_workshop', { request });
export const workshopHistory = (access: ProjectAccess): Promise<WorkshopSnapshot[]> => invoke('workshop_history', { access });
export const previewWorkshopAdoption = (request: PreviewWorkshopAdoption): Promise<WorkshopAdoptionPreview> => invoke('preview_workshop_adoption', { request });
export const adoptWorkshop = (access: ProjectAccess, operationId: string, previewId: string): Promise<WorkshopAdoptionAck> => invoke('adopt_workshop', { access, operationId, previewId });
export const startWorkshop = (access: ProjectAccess, operationId: string, exploration: WorkshopExploration, modelSelection: ModelSelection): Promise<DiscussionStart> => invoke('start_workshop', { request: { access, operationId, exploration, modelSelection } });
export const exportWorkshopPreset = (preset: WorkshopPreset): Promise<string | null> => invoke('export_workshop_preset', { preset });
export const importWorkshopPreset = (): Promise<WorkshopPreset | null> => invoke('import_workshop_preset');
