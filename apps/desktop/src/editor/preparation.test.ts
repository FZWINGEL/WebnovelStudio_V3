import { describe, expect, it } from 'vitest';
import type { PrepareContinuation, PrepareProposal, PrepareStructured, PreparedProposal } from '../ipc/proposals';
import type { WnsDocument } from '../kernel';
import { bodyHash, canonicalJson } from '../kernel';
import { confirmPreparation } from './preparation';

const access = { projectId: 'project', operationNamespace: 'namespace', session: 'session', writerLease: 'lease' };
const sourceBody: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'source' }, content: [{ type: 'text', text: 'Original.' }] }] } };

async function prepared(request: PrepareProposal | PrepareContinuation | PrepareStructured, payload: Partial<PreparedProposal> = {}): Promise<PreparedProposal> {
  return {
    id: 'prepared-1', proposalId: request.proposalId, version: '1', replacementText: 'replacement',
    body: structuredClone(request.body), bodyHash: await bodyHash(canonicalJson(request.body)), ...payload,
  };
}

describe('preparation acknowledgment contract', () => {
  it('accepts a valid passage receipt without changing its shape', async () => {
    const body: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'source' }, content: [{ type: 'text', text: 'Replacement.' }] }] } };
    const request: PrepareProposal = { access, operationId: 'passage-op', proposalId: 'proposal', expectedPreparedVersion: '0', replacementText: 'Replacement.', body };
    const result = await prepared(request, { replacementText: request.replacementText });
    await expect(confirmPreparation(request, result)).resolves.toEqual(result);
  });

  it('accepts a valid structured receipt with exact typed blocks and formatting', async () => {
    const blocks = [{ type: 'paragraph' as const, content: [{ type: 'text' as const, text: 'A ', marks: [] }, { type: 'text' as const, text: 'bold', marks: [{ type: 'bold' as const }] }] }];
    const body: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'fresh' }, content: [{ type: 'text', text: 'A ' }, { type: 'text', text: 'bold', marks: [{ type: 'bold' }] }] }] } };
    const request: PrepareStructured = { access, operationId: 'structured-op', proposalId: 'proposal', expectedPreparedVersion: '0', blocks, body };
    const result = await prepared(request, { replacementText: '', blocks });
    await expect(confirmPreparation(request, result)).resolves.toEqual(result);
  });

  it('accepts a valid continuation receipt with its paragraph payload', async () => {
    const body: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [
      ...sourceBody.body.content,
      { type: 'paragraph', attrs: { id: 'fresh' }, content: [{ type: 'text', text: 'Added.' }] },
    ] } };
    const paragraphs = ['Added.'];
    const request: PrepareContinuation = { access, operationId: 'continuation-op', proposalId: 'proposal', expectedPreparedVersion: '0', paragraphs, body };
    const result = await prepared(request, { replacementText: '', paragraphs });
    await expect(confirmPreparation(request, result)).resolves.toEqual(result);
  });

  it.each([
    ['foreign proposal', async (request: PrepareProposal, result: PreparedProposal) => ({ ...result, proposalId: 'other-proposal' })],
    ['wrong version', async (_request: PrepareProposal, result: PreparedProposal) => ({ ...result, version: '2' })],
    ['wrong body hash', async (_request: PrepareProposal, result: PreparedProposal) => ({ ...result, bodyHash: '0'.repeat(64) })],
    ['format payload', async (request: PrepareProposal, result: PreparedProposal) => ({ ...result, replacementText: `${request.replacementText} changed` })],
  ] as const)('refuses a %s mismatch', async (_label, mutate) => {
    const request: PrepareProposal = { access, operationId: 'mismatch-op', proposalId: 'proposal', expectedPreparedVersion: '0', replacementText: 'Replacement.', body: sourceBody };
    const result = await prepared(request, { replacementText: request.replacementText });
    const mismatched = await mutate(request, result);
    await expect(confirmPreparation(request, mismatched)).rejects.toMatchObject({ code: 'ProtocolError' });
  });

  it('refuses a structured formatting mismatch even when the body hash is correct', async () => {
    const blocks = [{ type: 'paragraph' as const, content: [{ type: 'text' as const, text: 'A line.' }] }];
    const body: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'fresh' }, content: [{ type: 'text', text: 'A line.' }] }] } };
    const request: PrepareStructured = { access, operationId: 'format-mismatch-op', proposalId: 'proposal', expectedPreparedVersion: '0', blocks, body };
    const result = await prepared(request, { replacementText: '', blocks: [{ type: 'paragraph', content: [{ type: 'text', text: 'A line.', marks: [{ type: 'italic' }] }] }] });
    await expect(confirmPreparation(request, result)).rejects.toMatchObject({ code: 'ProtocolError' });
  });
});
