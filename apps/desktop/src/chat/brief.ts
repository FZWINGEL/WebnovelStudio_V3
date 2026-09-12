import { bodyHash } from '../editor';
import type { DiscussionScope, SafeBriefInput } from '../ipc/discussions';

/**
 * Rust hashes the serialized DiscussionScopeInput directly. Keep this field
 * order explicit; canonicalJson sorts keys for editor snapshots and would not
 * reproduce the scope provenance contract.
 */
export function projectBriefScopeJson(scope: DiscussionScope | null | undefined): string {
  if (!scope) return 'null';
  return JSON.stringify({
    kind: scope.kind,
    start: scope.start,
    end: scope.end,
    quote: scope.quote,
    sourceBodyHash: scope.sourceBodyHash,
  });
}

/** Re-hash provenance only for the explicit author approval action. */
export async function approveChapterBrief(brief: SafeBriefInput, scope: DiscussionScope | null | undefined): Promise<SafeBriefInput> {
  if (!brief.text.trim()) throw new Error('Write a brief before approving it.');
  if (!brief.projectOrigin) return { ...brief, confirmed: true };
  const [scopeHash, textHash] = await Promise.all([
    bodyHash(projectBriefScopeJson(scope)),
    bodyHash(brief.text),
  ]);
  return { ...brief, confirmed: true, projectOrigin: { ...brief.projectOrigin, scopeHash, textHash } };
}
