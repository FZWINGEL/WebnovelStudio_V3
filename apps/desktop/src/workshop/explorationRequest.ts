import type { WorkshopCandidate, WorkshopExploration, WorkshopSession } from '../ipc/workshop';
import { ACTIONS, NOTES_ORGANIZATION_SCOPE, ORGANIZE_NOTES_INSTRUCTION } from './catalog';

export interface WorkingCapture { from: number; to: number; text: string; generation: string }
export function explorationRequest({ session, version, action, candidate, dimension, capture, isCurrentSession, subversion }: {
  session: WorkshopSession; version: string; action: string; candidate?: WorkshopCandidate;
  dimension?: string; capture: WorkingCapture | null; isCurrentSession: boolean; subversion: string;
}): WorkshopExploration {
  if (isCurrentSession && !candidate && capture && (capture.generation !== session.workingGeneration
    || session.workingText.slice(capture.from, capture.to) !== capture.text)) {
    throw new Error('The selected passage changed. Select it again before exploring.');
  }
  const selectedAction = ACTIONS.find(item => item.id === action) ?? ACTIONS[0];
  const candidateDimension = action === 'directions' && candidate && dimension !== undefined
    ? `Compare alternatives along the existing dimension: ${dimension}.\nUse the selected candidate as an unaccepted starting point. Preserve author-chosen invariants and Keep fixed details; vary this dimension rather than replacing the whole idea.` : '';
  const organizationInstruction = session.lens === 'notebook' && session.selectedScope === NOTES_ORGANIZATION_SCOPE && action === 'directions' && !candidate && !capture
    ? ORGANIZE_NOTES_INSTRUCTION : '';
  const instruction = [selectedAction.instruction, organizationInstruction, candidateDimension,
    action === 'subvert' ? `Convention to transform: ${session.selectedScope}.\nTransformation: ${subversion}.` : '', session.composer].filter(Boolean).join('\n\n');
  return {
    sessionId: session.id, expectedVersion: version, workingGeneration: session.workingGeneration,
    action, instruction,
    selectedScope: action === 'voiceGuidance' ? 'Voice qualities from the sample' : candidate?.title ?? (isCurrentSession && capture ? 'Selected passage in working version' : session.selectedScope),
    selectedText: candidate?.content ?? (isCurrentSession ? capture?.text : undefined) ?? (action === 'voiceGuidance' && isCurrentSession ? session.workingText : ''),
    workingSelection: action !== 'voiceGuidance' && !candidate && isCurrentSession && capture ? { from: capture.from, to: capture.to, text: capture.text } : undefined,
  };
}
