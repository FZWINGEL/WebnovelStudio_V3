import type { ComposerBody, DiscussionDraft, FeedbackIntent, SaveDiscussionDraft } from '../ipc/discussions';
import { canonicalJson } from '../editor/document';
import { createSaveLoop } from '../kernel';
import type { ProjectAccess } from '../ipc/projects';

export const emptyComposer = (): ComposerBody => ({ text: '', scope: null, pinnedDocumentIds: [] });
export function composerIntent(body: ComposerBody): FeedbackIntent { return body.intent ?? 'discuss'; }
function key(body: ComposerBody): string { return canonicalJson({ text: body.text, scope: body.scope, pinnedDocumentIds: body.pinnedDocumentIds, intent: composerIntent(body), basis: body.basis ?? null, previousRunId: body.previousRunId ?? null, safeBrief: body.safeBrief ? { text: body.safeBrief.text, originMessageId: body.safeBrief.originMessageId ?? null, confirmed: body.safeBrief.confirmed } : null, lookup: body.lookup ?? null }); }

/** Immutable retry payloads and save watermarks for the unsent composer only. */
export class ComposerSession {
  body: ComposerBody;
  private version: string;
  private saved: string;
  constructor(private documentId: string, draft: DiscussionDraft | null, private access: () => ProjectAccess, private write: (request: SaveDiscussionDraft) => Promise<DiscussionDraft>) {
    this.body = structuredClone(draft ? { text: draft.text, scope: draft.scope, pinnedDocumentIds: draft.pinnedDocumentIds, intent: draft.intent, basis: draft.basis, previousRunId: draft.previousRunId, safeBrief: draft.safeBrief, lookup: draft.lookup } : emptyComposer());
    this.version = draft?.version ?? '0'; this.saved = key(this.body);
  }

  /**
   * A mismatch is a protocol error, not a refusal: the write may have landed, so
   * the exact payload and operation id are retained for reconciliation. The
   * loop's default is to retain, so this attempt declares no `discardOn`.
   */
  private readonly loop = createSaveLoop({
    isDirty: () => key(this.body) !== this.saved,
    capture: () => {
      if (key(this.body) === this.saved) return null;
      const request: SaveDiscussionDraft = { ...structuredClone(this.body), access: this.access(), operationId: crypto.randomUUID(), documentId: this.documentId, expectedVersion: this.version };
      const signature = key(this.body);
      return {
        send: async () => {
          const result = await this.write({ ...structuredClone(request), access: this.access() });
          if (result.documentId !== this.documentId || result.version !== (BigInt(request.expectedVersion) + 1n).toString() || key(result) !== signature) {
            throw new Error('The saved discussion draft did not match this request. Your composer text is retained.');
          }
          this.version = result.version; this.saved = signature;
        },
        commit: () => {},
      };
    },
  });

  update(body: ComposerBody): void { this.body = structuredClone(body); }
  clearIfUnchanged(sent: ComposerBody): boolean {
    if (key(sent) !== key(this.body)) return false;
    this.update(emptyComposer()); return true;
  }
  get dirty(): boolean { return this.loop.hasPending || key(this.body) !== this.saved; }

  /**
   * The composer has no debounce timer, so it re-checks itself after a drain to
   * catch an edit made while the last write was in flight.
   */
  async save(): Promise<void> {
    await this.loop.flush();
    if (this.dirty) await this.save();
  }
}
