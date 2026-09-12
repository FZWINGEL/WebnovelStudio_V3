// The public surface of this feature. Everything another feature may use
// is re-exported here; §4.1 makes the edge reviewable by making it a path.

export { SavedProse } from './HistoryPanel';
export { Writer } from './Writer';
export { bodyHash, canonicalJson, safeHref, sample, snapshotFromEditor } from './document';
export type { Block, Inline, SnapshotReceipt, WnsDocument } from './document';
export { blockText, blocksQuote, captureRevisionScope } from './revisionScope';
export { editorExtensions } from './schema';
export { captureSelection, generation, prepareReplacement } from './selection';
export type { Scope } from './selection';
export { DocumentSession } from './session';
export type { SessionState } from './session';
export { structuredEditorBlocks, structuredFromEditor } from './structured';
