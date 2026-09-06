import type { ComposerBody, DiscussionDraft, FeedbackIntent, SaveDiscussionDraft } from '../ipc/discussions';
import { canonicalJson } from '../editor/document';
import type { ProjectAccess } from '../ipc/projects';

export const emptyComposer = (): ComposerBody => ({ text: '', scope: null, pinnedDocumentIds: [] });
export function composerIntent(body: ComposerBody): FeedbackIntent { return body.intent ?? 'discuss'; }
function key(body: ComposerBody): string { return canonicalJson({ text: body.text, scope: body.scope, pinnedDocumentIds: body.pinnedDocumentIds, intent: composerIntent(body), basis: body.basis ?? null, previousRunId: body.previousRunId ?? null, safeBrief: body.safeBrief ? { text: body.safeBrief.text, originMessageId: body.safeBrief.originMessageId ?? null, confirmed: body.safeBrief.confirmed } : null, lookup: body.lookup ?? null }); }

/** Immutable retry payloads and save watermarks for the unsent composer only. */
export class ComposerSession {
  body: ComposerBody;
  private version: string;
  private saved: string;
  private pending: { request: SaveDiscussionDraft; signature: string } | null = null;
  private flight: Promise<void> | null = null;
  constructor(private documentId: string, draft: DiscussionDraft | null, private access: () => ProjectAccess, private write: (request: SaveDiscussionDraft) => Promise<DiscussionDraft>) {
    this.body = structuredClone(draft ? { text: draft.text, scope: draft.scope, pinnedDocumentIds: draft.pinnedDocumentIds, intent: draft.intent, basis: draft.basis, previousRunId: draft.previousRunId, safeBrief: draft.safeBrief, lookup: draft.lookup } : emptyComposer());
    this.version = draft?.version ?? '0'; this.saved = key(this.body);
  }
  update(body: ComposerBody): void { this.body = structuredClone(body); }
  clearIfUnchanged(sent: ComposerBody): boolean {
    if (key(sent) !== key(this.body)) return false;
    this.update(emptyComposer()); return true;
  }
  get dirty(): boolean { return !!this.pending || key(this.body) !== this.saved; }
  async save(): Promise<void> {
    if (!this.flight) this.flight = this.flush().finally(() => { this.flight = null; });
    await this.flight;
    if (this.dirty) await this.save();
  }
  private async flush(): Promise<void> {
    while (this.dirty) {
      this.pending ??= { request: { ...structuredClone(this.body), access: this.access(), operationId: crypto.randomUUID(), documentId: this.documentId, expectedVersion: this.version }, signature: key(this.body) };
      const captured = this.pending;
      const result = await this.write({ ...structuredClone(captured.request), access: this.access() });
      if (result.documentId !== this.documentId || result.version !== (BigInt(captured.request.expectedVersion) + 1n).toString() || key(result) !== captured.signature) {
        throw new Error('The saved discussion draft did not match this request. Your composer text is retained.');
      }
      this.version = result.version; this.saved = captured.signature; this.pending = null;
    }
  }
}
