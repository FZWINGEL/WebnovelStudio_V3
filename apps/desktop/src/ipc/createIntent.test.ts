// @vitest-environment node
import { describe, expect, it } from 'vitest';
import type { CreateDocumentIntent, DocumentRecord, Head, OpenedProject, ProjectAccess, ProjectInfo } from './projects';
import { CreateIntentRecoveryError, CreateIntentUnresolvedError, isUncertainCreateError, runCreateIntent } from './createIntent';
import type { WnsDocument } from '../kernel';

const initialAccess: ProjectAccess = { projectId: 'project-1', session: 'renderer-1', writerLease: 'lease-1', operationNamespace: 'namespace-1' };
const project: ProjectInfo = { projectId: 'project-1', operationNamespace: 'namespace-1', title: 'Story', formatVersion: 1 };
const body: WnsDocument = { schemaVersion: 1, body: { type: 'doc', content: [{ type: 'paragraph', attrs: { id: 'block-1' }, content: [{ type: 'text', text: 'Draft' }] }] } };
const intent: CreateDocumentIntent = { operationId: 'create-1', documentId: 'document-1', title: 'Chapter 1', kind: 'chapter', body };
const head: Head = { documentId: 'document-1', version: '0', bodyHash: 'a'.repeat(64) };
const record: DocumentRecord = { head, title: intent.title, kind: intent.kind, metadataVersion: '0', body, lastCheckpointId: null };
function snapshot(access: ProjectAccess, documents: DocumentRecord[] = []): OpenedProject {
  return { project, access, documents, metadataVersion: '0', viewState: null, libraryWarning: null };
}
function uncertain(): { code: string; detail: string } { return { code: 'UncertainOutcome', detail: 'lost acknowledgment' }; }

describe('runCreateIntent', () => {
  it('captures an immutable intent before a caller changes its form during reconciliation', async () => {
    const mutable = structuredClone(intent);
    const requests: CreateDocumentIntent[] = [];
    await runCreateIntent({ projectId: project.projectId, session: initialAccess.session, access: initialAccess, intent: mutable, transport: {
      createDocument: async (_access, request) => { requests.push(request); if (requests.length === 1) throw uncertain(); return record; },
      reconcileProject: async () => { mutable.title = 'A different chapter'; mutable.documentId = 'different-document'; return snapshot({ ...initialAccess, writerLease: 'lease-2' }); },
    } });
    expect(requests[1]).toEqual(intent);
  });
  it('uses a committed record found after a lost ACK without creating a duplicate', async () => {
    let calls = 0;
    const result = await runCreateIntent({ projectId: project.projectId, session: initialAccess.session, access: initialAccess, intent, transport: {
      createDocument: async () => { calls += 1; throw uncertain(); },
      reconcileProject: async () => snapshot({ ...initialAccess, writerLease: 'lease-2' }, [record]),
    } });
    expect(calls).toBe(1);
    expect(result.record).toEqual(record);
    expect(result.access.writerLease).toBe('lease-2');
  });

  it('treats a plain transport error as a lost ACK and reconciles it', async () => {
    let calls = 0;
    const result = await runCreateIntent({ projectId: project.projectId, session: initialAccess.session, access: initialAccess, intent, transport: {
      createDocument: async () => { calls += 1; throw new Error('native channel closed'); },
      reconcileProject: async () => snapshot({ ...initialAccess, writerLease: 'lease-plain-error' }, [record]),
    } });
    expect(calls).toBe(1);
    expect(result.record.head.documentId).toBe(intent.documentId);
  });

  it('lets explicit pre-commit validation failures clear the intent', async () => {
    let reconciles = 0;
    const validationFailure = { code: 'InvalidDocument', detail: 'unsupported document body' };
    expect(isUncertainCreateError(validationFailure)).toBe(false);
    await expect(runCreateIntent({ projectId: project.projectId, session: initialAccess.session, access: initialAccess, intent, transport: {
      createDocument: async () => { throw validationFailure; },
      reconcileProject: async () => { reconciles += 1; return snapshot(initialAccess); },
    } })).rejects.toMatchObject(validationFailure);
    expect(reconciles).toBe(0);
  });

  it('reconciles an absent first attempt, then retries under the fresh lease', async () => {
    const calls: Array<{ access: ProjectAccess; intent: CreateDocumentIntent }> = [];
    let reconciles = 0;
    const result = await runCreateIntent({ projectId: project.projectId, session: initialAccess.session, access: initialAccess, intent, transport: {
      createDocument: async (access, request) => {
        calls.push({ access, intent: request });
        if (calls.length === 1) throw uncertain();
        return record;
      },
      reconcileProject: async () => { reconciles += 1; return snapshot({ ...initialAccess, writerLease: `lease-${reconciles + 1}` }); },
    }, onReconciled: async recovered => ({ ...recovered.access, writerLease: 'adopted-lease-2' }) });
    expect(calls).toHaveLength(2);
    expect(calls[1].access.writerLease).toBe('adopted-lease-2');
    expect(calls[1].intent.operationId).toBe(intent.operationId);
    expect(calls[1].intent.documentId).toBe(intent.documentId);
    expect(calls[1].intent.body).toEqual(intent.body);
    expect(result.record).toEqual(record);
  });

  it('keeps the caller intent and body unchanged when reconciliation fails', async () => {
    const before = structuredClone(intent);
    await expect(runCreateIntent({ projectId: project.projectId, session: initialAccess.session, access: initialAccess, intent, transport: {
      createDocument: async () => { throw uncertain(); },
      reconcileProject: async () => { throw { code: 'PersistenceUnavailable', detail: 'database unavailable' }; },
    } })).rejects.toSatisfy(error => { expect(error).toBeInstanceOf(CreateIntentRecoveryError); expect(error).toMatchObject({ code: 'PersistenceUnavailable' }); return true; });
    expect(intent).toEqual(before);
    expect(intent.body.body.content[0]).toMatchObject({ attrs: { id: 'block-1' } });
  });

  it('leaves a second lost ACK unresolved after its final reconciliation', async () => {
    let creates = 0;
    await expect(runCreateIntent({ projectId: project.projectId, session: initialAccess.session, access: initialAccess, intent, transport: {
      createDocument: async () => { creates += 1; throw uncertain(); },
      reconcileProject: async () => snapshot({ ...initialAccess, writerLease: `lease-${creates + 1}` }),
    } })).rejects.toBeInstanceOf(CreateIntentUnresolvedError);
    expect(creates).toBe(2);
  });
});
