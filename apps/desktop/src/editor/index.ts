// The public surface of this feature. Everything another feature may use
// is re-exported here; §4.1 makes the edge reviewable by making it a path.

export { SavedProse } from './HistoryPanel';
// The document model lives in `kernel/` now — it is vocabulary, not editing
// behaviour — and is re-exported here so a feature that already reads the
// editor does not need a second import to name a document.
export { bodyHash, canonicalJson, documentBlocks, safeHref, sample, snapshotFromEditor } from '../kernel';
export type { Block, Inline, Mark, SnapshotReceipt, WnsDocument } from '../kernel';
export { blockText, blocksQuote, captureRevisionScope } from './revisionScope';
export { editorExtensions } from './schema';
export { captureSelection, generation, prepareReplacement } from './selection';
export type { Scope } from './selection';
export { DocumentSession } from './session';
export type { SessionState } from './session';
export { structuredEditorBlocks, structuredFromEditor } from './structured';
export { HistoryPanel } from './HistoryPanel';
export { ReviewPanel } from './ReviewPanel';
export { RecoveryCopy } from './RecoveryCopy';
export { prepareContinuation } from './continuation';
export { confirmPreparation } from './preparation';
export { prepareScopedReplacement } from './selection';
export { prepareStructuredReplacement, structuredRange, validateStructuredBlocks } from './structured';
export { SessionError } from './session';
export type { PreparedEditorChange } from './session';
