import { invoke } from '@tauri-apps/api/core';
import type { DocumentRecord, Head, ProjectAccess } from './projects';
import type { DiscussionRun, DiscussionStart } from './discussions';
import type { ModelSelection } from './providers';
import type { WnsDocument } from '../editor/document';

export type Lens = 'overview' | 'world' | 'people' | 'themes' | 'possibilities' | 'notebook';
export interface WorkshopPreference {
  id: string; label: string; family: string; meaning: string; examples: string; timing: string;
  polarity: 'neutral' | 'want' | 'avoid'; strength: 'soft' | 'hard';
  scope: 'project' | 'element' | 'exploration'; targetId: string | null; confirmed: boolean;
}
export interface SelectedDetail { id: string; candidateId: string | null; text: string; fixed: boolean }
export interface CandidateChoice { candidateId: string; status: 'saved' | 'rejected' | 'archived'; rationale: string; includeInContext: boolean }
export interface WorkshopQuestion { id: string; text: string; reason: string; status: 'open' | 'notNow' | 'notRelevant' | 'keepMysterious'; unknownTo: 'author' | 'reader' | 'both' }
export interface WorkshopSession {
  id: string; title: string; lens: Lens; parentSessionId: string | null; branchKind: 'working' | 'whatIf';
  brief: string; direction: string; stillOpen: string; focusQuestion: string; focusReason: string;
  focusDocumentId: string | null; anchorDocumentId: string | null; depth: 'sketch' | 'develop' | 'document';
  relationshipId?: string | null;
  outsideDirection: boolean; includedDocumentIds: string[]; workingText: string; workingTitle: string;
  workingGeneration: string; selectedDetails: SelectedDetail[]; choices: CandidateChoice[]; questions: WorkshopQuestion[];
  composer: string; selectedScope: string; originalNotes: string; activeRunId: string | null;
}
export interface WorkshopDecision {
  id: string; sessionId: string; title: string; documentId: string; revisionId: string; head: Head;
  candidateIds: string[]; rationale: string; status: 'chosen' | 'archived' | 'superseded';
  fixed: boolean; protectedText: string[]; access: 'authorRoom'; supersedesId: string | null;
}
export interface WorkshopRelationship {
  id: string; fromDocumentId: string; toDocumentId: string; type: string; description: string;
  uncertainty: string; status: 'tentative' | 'chosen' | 'archived'; sourceHeads: Head[];
}
export interface WorkshopImpact {
  id: string; decisionId: string; documentId: string;
  candidateId?: string | null; relationshipId?: string | null;
  kind: 'contradiction' | 'possibleTension' | 'dependentAssumption' | 'styleSuggestion';
  reason: string; status: 'needsReview' | 'acknowledged' | 'intentional';
}
export interface WorkshopRelationshipDraft {
  id: string; fromDocumentId: string; toDocumentId: string; type: string; description: string; uncertainty: string;
  fromExpected: Head | null; toExpected: Head | null;
}
export interface WorkshopImpactDraft { documentId: string; kind: WorkshopImpact['kind']; reason: string }
export interface WorkshopAdoptionImpact extends WorkshopImpactDraft { candidateId: string; status: WorkshopImpact['status'] }
export interface WorkshopPreset { id: string; name: string; preferences: WorkshopPreference[] }
export interface WorkshopState {
  schemaVersion: 1; currentSessionId: string | null; sessions: WorkshopSession[];
  preferences: WorkshopPreference[]; decisions: WorkshopDecision[]; relationships: WorkshopRelationship[];
  impacts: WorkshopImpact[]; presets: WorkshopPreset[];
}
export interface WorkshopSnapshot { version: string; state: WorkshopState }
export interface WorkshopCandidate {
  id: string; title: string; content: string; dimensionValue: string;
  implications: Array<{ text: string; basis: string; assumption: string }>;
  assumptions: string[]; affectedTargets: Array<{ documentId: string; reason: string }>;
  preservedDetails: string[]; changedDetails: string[];
}
export interface WorkshopOutput {
  schemaVersion: 'story-workshop-output.v1'; requestKind: string; question: string; questionReason: string;
  dimension: string; interpretation: { youSaid: string; possibleDirection: string; stillOpen: string };
  candidates: WorkshopCandidate[];
}
export interface WorkshopExploration {
  sessionId: string; expectedVersion: string; workingGeneration: string; action: string; instruction: string;
  selectedScope: string; selectedText: string;
  workingSelection?: { from: number; to: number; text: string } | null;
}
export interface WorkshopResult {
  run: DiscussionRun; sessionId: string; workingGeneration: string; action: string;
  output: WorkshopOutput | null; validationError: string | null; stale: boolean;
  workingSelection?: { from: number; to: number; text: string } | null;
}
export interface WorkshopView extends WorkshopSnapshot { results: WorkshopResult[] }
export interface SaveWorkshop { access: ProjectAccess; operationId: string; expectedVersion: string; state: WorkshopState }
export interface WorkshopAdoptionTarget {
  documentId: string; expected: Head | null; title: string; kind: string; body: WnsDocument;
  mode: 'add' | 'replace';
}
export interface PreviewWorkshopAdoption {
  access: ProjectAccess; sessionId: string; expectedVersion: string; candidateIds: string[];
  targets: WorkshopAdoptionTarget[]; rationale: string; protectedText: string[];
  relationships?: WorkshopRelationshipDraft[]; impactDrafts?: WorkshopImpactDraft[];
}
export interface WorkshopAdoptionPreview {
  id: string; sessionId: string; expectedVersion: string; targets: WorkshopAdoptionTarget[];
  before: DocumentRecord[]; rationale: string; protectedText: string[]; candidateIds: string[];
  relationships?: WorkshopRelationship[]; endpointSources?: DocumentRecord[]; impacts?: WorkshopAdoptionImpact[];
}
export interface WorkshopAdoptionAck { snapshot: WorkshopSnapshot; documents: DocumentRecord[]; decisionIds: string[] }

export const readWorkshop = (access: ProjectAccess): Promise<WorkshopView> => invoke('read_workshop', { access });
export const saveWorkshop = (request: SaveWorkshop): Promise<WorkshopSnapshot> => invoke('save_workshop', { request });
export const workshopHistory = (access: ProjectAccess): Promise<WorkshopSnapshot[]> => invoke('workshop_history', { access });
export const previewWorkshopAdoption = (request: PreviewWorkshopAdoption): Promise<WorkshopAdoptionPreview> => invoke('preview_workshop_adoption', { request });
export const adoptWorkshop = (access: ProjectAccess, operationId: string, previewId: string): Promise<WorkshopAdoptionAck> => invoke('adopt_workshop', { access, operationId, previewId });
export const startWorkshop = (access: ProjectAccess, operationId: string, exploration: WorkshopExploration, modelSelection: ModelSelection): Promise<DiscussionStart> => invoke('start_workshop', { request: { access, operationId, exploration, modelSelection } });
export const exportWorkshopPreset = (preset: WorkshopPreset): Promise<string | null> => invoke('export_workshop_preset', { preset });
export const importWorkshopPreset = (): Promise<WorkshopPreset | null> => invoke('import_workshop_preset');
