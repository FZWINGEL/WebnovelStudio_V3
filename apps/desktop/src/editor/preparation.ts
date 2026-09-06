import type { PrepareContinuation, PrepareProposal, PrepareStructured, PreparedProposal } from '../ipc/proposals';
import { bodyHash, canonicalJson } from './document';
import { SessionError } from './session';

/** Confirm the exact immutable preview requested, including its version. */
export async function confirmPreparation(request: PrepareProposal | PrepareContinuation | PrepareStructured, result: PreparedProposal): Promise<PreparedProposal> {
  let expectedVersion: string;
  try { expectedVersion = (BigInt(request.expectedPreparedVersion) + 1n).toString(); }
  catch { throw new SessionError('ProtocolError', 'The preview has an invalid saved version.'); }
  const payloadMatches = 'blocks' in request
    ? !!result.blocks && !result.paragraphs && result.replacementText === '' && canonicalJson(result.blocks) === canonicalJson(request.blocks)
    : 'paragraphs' in request
      ? !!result.paragraphs && !result.blocks && result.replacementText === '' && canonicalJson(result.paragraphs) === canonicalJson(request.paragraphs)
      : !result.paragraphs && !result.blocks && result.replacementText === request.replacementText;
  if (result.proposalId !== request.proposalId || result.version !== expectedVersion || !payloadMatches
    || canonicalJson(result.body) !== canonicalJson(request.body) || result.bodyHash !== await bodyHash(canonicalJson(request.body))) {
    throw new SessionError('ProtocolError', 'The preview response did not match the exact requested prose, formatting, and saved version. Check the same request.');
  }
  return result;
}
