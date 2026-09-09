import { bodyHash, canonicalJson } from '../editor/document';
import type { DocumentSession } from '../editor/session';
import { captureRevisionScope } from '../editor/revisionScope';
import type { ProjectChapterComposer } from '../ipc/projectChat';
import type { SuggestedChapterRange } from './ChapterRangeReview';

export async function confirmChapterRange(
  session: DocumentSession,
  shown: SuggestedChapterRange,
  revalidate: () => Promise<SuggestedChapterRange | null>,
  stage: (task: ProjectChapterComposer) => Promise<void>,
): Promise<void> {
  await session.withLifecycleGuard(async () => {
    await session.flush();
    const current = await revalidate();
    if (!current || canonicalJson(current.target) !== canonicalJson(shown.target)
      || canonicalJson(current.scope) !== canonicalJson(shown.scope)) {
      throw new Error('The suggested passage changed or is no longer available. Read a fresh suggestion before confirming it.');
    }
    const head = session.state.head;
    const hash = await bodyHash(canonicalJson(session.body));
    if (canonicalJson(head) !== canonicalJson(shown.target) || hash !== head.bodyHash
      || shown.scope.kind !== 'blocks' || shown.scope.sourceBodyHash !== hash) {
      throw new Error('The chapter changed after this suggestion. Select the passage again in the current chapter.');
    }
    const scope = captureRevisionScope(session.body, hash, 'blocks', shown.scope);
    if (canonicalJson(scope) !== canonicalJson(shown.scope)) {
      throw new Error('The suggested passage does not match these exact saved paragraphs. Select the passage again.');
    }
    await stage({ target: head, intent: 'proposeEdits', basis: null, scope });
  });
}
