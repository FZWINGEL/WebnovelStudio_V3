// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { bodyHash } from '../kernel';
import type { DiscussionScope, SafeBriefInput } from '../ipc/discussions';
import { approveChapterBrief, projectBriefScopeJson } from './brief';

const scope: DiscussionScope = { kind: 'passage', start: { blockId: 'p1', utf16Offset: 0 }, end: { blockId: 'p1', utf16Offset: 5 }, quote: 'Quiet', sourceBodyHash: 'a'.repeat(64) };

describe('chapter brief provenance', () => {
  it('does not approve a manually entered brief implicitly', () => {
    const brief: SafeBriefInput = { text: 'Keep the ending.', originMessageId: null, confirmed: false };
    expect(brief.confirmed).toBe(false);
  });

  it('regenerates text and exact serialized scope hashes only on approval', async () => {
    const brief: SafeBriefInput = {
      text: 'Adapted direction.', originMessageId: 'assistant-message', confirmed: false,
      projectOrigin: { version: 'project-conversation-brief.v1', projectId: 'project', operationNamespace: 'namespace', conversationId: 'conversation', messageId: 'assistant-message', target: { documentId: 'chapter', version: '2', bodyHash: 'b'.repeat(64) }, scopeHash: '0'.repeat(64), textHash: '0'.repeat(64) },
    };
    const approved = await approveChapterBrief(brief, scope);
    expect(approved.confirmed).toBe(true);
    expect(approved.projectOrigin?.scopeHash).toBe(await bodyHash(projectBriefScopeJson(scope)));
    expect(approved.projectOrigin?.textHash).toBe(await bodyHash(brief.text));
    expect(approved.projectOrigin?.messageId).toBe('assistant-message');
  });
});
