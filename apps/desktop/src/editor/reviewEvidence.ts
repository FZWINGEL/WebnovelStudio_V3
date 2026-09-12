import type { Scope } from '../editor/selection';
import type { EvidenceAnchor } from '../ipc/reviews';

export function reviewAnchor(scope: Scope): EvidenceAnchor | null {
  if (scope.start.blockId !== scope.end.blockId || !scope.quote.trim()) return null;
  return { blockId: scope.start.blockId, fromUtf16: scope.start.utf16Offset, toUtf16: scope.end.utf16Offset, quote: scope.quote, quoteHash: '' };
}

export async function evidenceQuoteHash(quote: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(quote));
  return [...new Uint8Array(digest)].map(byte => byte.toString(16).padStart(2, '0')).join('');
}
