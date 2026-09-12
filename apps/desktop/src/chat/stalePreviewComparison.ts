import { readProjectConversation, type ChatAdoptionPreview } from '../ipc/projectChat';
import { readDocument, type ProjectAccess } from '../ipc/projects';
import type { StalePreviewComparison } from './DraftReviewPanel';

/** Read saved target versions without changing the retained preview or its authority. */
export async function readStalePreviewComparison(access: ProjectAccess, preview: ChatAdoptionPreview): Promise<StalePreviewComparison> {
  if (preview.projectId !== access.projectId || preview.operationNamespace !== access.operationNamespace) {
    throw new Error('This preview belongs to another project conversation.');
  }
  const before = await readProjectConversation(access);
  if (before.id !== preview.conversationId) throw new Error('The active project conversation changed.');
  const targets = await Promise.all(preview.targets.map(async target => {
    try { return { documentId: target.documentId, current: await readDocument(access, target.documentId) }; }
    catch (reason) {
      if (reason && typeof reason === 'object' && 'code' in reason && reason.code === 'DocumentNotFound') {
        return { documentId: target.documentId, current: null };
      }
      throw reason;
    }
  }));
  const after = await readProjectConversation(access);
  if (after.id !== before.id || after.sourceEpoch !== before.sourceEpoch || after.policyEpoch !== before.policyEpoch) {
    throw new Error('The story changed while the comparison was being read. Compare again to inspect one saved state.');
  }
  return { previewId: preview.id, targets, sourceEpoch: preview.sourceEpoch, currentSourceEpoch: after.sourceEpoch,
    policyEpoch: preview.policyEpoch, currentPolicyEpoch: after.policyEpoch };
}
