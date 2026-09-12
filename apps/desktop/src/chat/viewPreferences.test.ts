// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest';
import type { ChatAdoptionPreview } from '../ipc/projectChat';
import type { ProjectAccess } from '../ipc/projects';
import { chatViewPreferenceKey, readChatViewPreferences, writeChatViewPreferences } from './viewPreferences';

const access: ProjectAccess = { projectId: 'project-1', operationNamespace: 'namespace-1', session: 'session-1', writerLease: 'lease-1' };
const preview = { id: 'preview-1', version: '2', digest: 'digest', projectId: 'project-1', operationNamespace: 'namespace-1', conversationId: 'conversation-1', sourceEpoch: '1', policyEpoch: '1', workshopVersion: '1', targets: [], effects: null } as ChatAdoptionPreview;

beforeEach(() => localStorage.clear());

describe('chat view preferences', () => {
  it('keys preferences by project, operation namespace, and conversation identity', () => {
    const key = chatViewPreferenceKey(access, 'conversation-1');
    writeChatViewPreferences(key, { rightMode: 'review', selectedDraftIds: ['draft-1'], editingDraftId: 'draft-1', transcriptAnchor: 'item-40', preview, applyOperationId: '123e4567-e89b-12d3-a456-426614174000' });
    expect(readChatViewPreferences(key)).toEqual({ rightMode: 'review', selectedDraftIds: ['draft-1'], editingDraftId: 'draft-1', transcriptAnchor: 'item-40', preview, applyOperationId: '123e4567-e89b-12d3-a456-426614174000' });
    expect(readChatViewPreferences(chatViewPreferenceKey(access, 'other-conversation'))).toEqual({});
  });

  it('merges right-pane and draft-review writes instead of erasing the other surface', () => {
    const key = chatViewPreferenceKey(access, 'conversation-1');
    writeChatViewPreferences(key, { rightMode: 'review', preview });
    writeChatViewPreferences(key, { selectedDraftIds: ['draft-1'], editingDraftId: 'draft-1' });
    expect(readChatViewPreferences(key)).toMatchObject({ rightMode: 'review', preview, selectedDraftIds: ['draft-1'], editingDraftId: 'draft-1' });
    writeChatViewPreferences(key, { preview: null });
    expect(readChatViewPreferences(key)).toMatchObject({ rightMode: 'review', preview: null, selectedDraftIds: ['draft-1'] });
  });
});
