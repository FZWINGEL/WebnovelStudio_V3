# ADR 0001 — W0 editor contract

**Date:** 5 September 2026

**Status:** implemented for the W0 native spike; durable persistence and structural proposal validation remain later work.

This contract records the boundary between the live Tiptap editor and the Rust host. It applies to the built W0 trial and preserves the later lifecycle and restore decisions in [V3_ARCHITECTURE_REFINED.md](V3_ARCHITECTURE_REFINED.md).

## Authority and transport

JavaScript owns live ProseMirror/Tiptap transactions, selection, undo/redo, and local replacement preparation. Rust receives a JSON snapshot through the Tauri `validate_snapshot` command, independently validates and canonicalizes it, computes a receipt, and returns it. The JS IPC adapter compares Rust's canonical JSON and SHA-256 with its own before treating the check as successful. Rust does not interpret serialized ProseMirror steps.

W0 validates snapshots only. It does not write SQLite, create a durable operation receipt, reconcile a saved generation, or claim a durable Apply. The local Apply path validates the prepared in-memory result, then dispatches the already prepared transaction behind a short input barrier. Durable Apply belongs to the later persistence work, with the architecture's shared lifecycle guard.

## Snapshot shape

The document is `WnsDocument { schemaVersion: 1, body: { type: "doc", content: Block[] } }`.

The restricted block set is:

- `paragraph { attrs: { id }, content?: Inline[] }`
- `heading { attrs: { id, level: 1..3 }, content?: Inline[] }`
- `sceneBreak { attrs: { id } }`

Inline nodes are `text` with optional marks and `hardBreak`. Marks are `bold`, `italic`, and `link { attrs: { href } }`. IDs are unique, nonempty ASCII strings of at most 64 letters, digits, `_`, or `-`. Links are absolute `http`, `https`, or constrained `mailto` addresses without credentials, unsafe whitespace, controls, or backslashes. W0 mailto addresses require a dotted domain and reject query/fragment components and percent encoding. Links render inertly in this trial; an external opener is deferred.

Empty block content is omitted. Empty marks are omitted. Adjacent text nodes with equal marks are merged. Marks sort in the fixed contract order `bold`, `italic`, `link`; object keys are sorted by their UTF-8/ASCII byte ordering. Canonicalization changes representation only. It does not apply Unicode normalization, punctuation conversion, or full-width conversion.

The hash is SHA-256 of the canonical JSON encoded as UTF-8. No Unicode normalization occurs before hashing. Rust independently rejects malformed JSON, unknown fields/nodes/marks, invalid links or IDs, duplicate IDs, invalid schema shape, and configured raw-byte, block-count, and UTF-16-unit limits.

The shared fixture at `contracts/fixtures/w0_snapshot_golden.json` is the JS/Rust agreement surface. It includes repeated Unicode text and emoji, combining text, marks and adjacent-run merging, empty paragraphs, scene/hard breaks, mailto links, malformed attributes, unsafe links, duplicate IDs, and invalid surrogate JSON. These are internal robustness cases for English names, accents, and emoji, not a Chinese authoring feature.

## Block identity

Block identity is explicit and is preserved through ordinary in-document operations:

| Operation | Identity rule |
| --- | --- |
| Split | The left block keeps its ID; the right block receives a fresh ID. |
| Merge | The left block keeps its ID; the right block disappears. |
| Move | Moving an existing node retains its ID by contract; W0 has no Move control or qualified move command. |
| Copy or paste | Every copied block receives a fresh ID. |
| Undo/redo | History restores the identities belonging to that history state. |

The W0 `BlockIdentity` extension also repairs missing, malformed, or duplicate IDs after document-changing transactions. Fresh IDs use `crypto.randomUUID()` in the application. Cross-document copy provenance is a later persistence concern.

## Selection and replacement

Selections capture the current generation, source document, quote, start/end block IDs, UTF-16 offsets, inline-only status, replacement permission, and uniform marks. Endpoint snapping uses `Intl.Segmenter('und', { granularity: 'grapheme' })` at runtime so a W0 selection does not split a grapheme cluster. This JavaScript runtime behavior is not yet a locked Unicode contract shared with Rust; W1 owns that contract.

W0 permits an inline replacement on one line and a cross-paragraph replacement only when the selected blocks have matching type/style and no scene break is crossed. The prepared replacement uses a strict fixed-position `ReplaceStep`, not a fit-oriented range helper. It inherits uniform selected marks; a mixed-mark selection uses plain text while surrounding formatting remains. A replacement containing a line break or exceeding the W0 limit is refused. Scene-break and mixed-block-style selections can receive feedback but cannot use the W0 replacement control.

The source document and generation are checked again before dispatch. Any intervening edit, including an edit outside the selection, makes the captured scope stale and requires reselection. Undo and redo are editor-session history actions; they do not imply durable history or a restart-safe undo contract.

While local Apply is checking the prepared result, document-changing input is blocked by a short local barrier. Composition must finish before Apply. The barrier does not wait for a provider, database commit, or external process. W0 applies only to the mounted editor instance and does not persist the resulting body.

## Lifecycle and later persistence boundary

The durable implementation must carry project, document, session, and operation identity through every save, Apply, reconciliation, callback, and job. It must use conservative stale-edit refusal, a shared lifecycle guard for Apply/reconciliation/disposal/switch/close/reload, and a saved-generation watermark. A restore opens an independent recovered project with a new identity and active operation namespace; copied receipts remain historical and cannot authorize new operations. These are preserved architecture decisions, not W0 runtime behavior.

W1 adds an independent structural validator that proves unchanged content, marks, links, block style/identity, and scene boundaries outside the permitted interval. W2/W6 add file-backed SQLite persistence, durable receipts, lost-acknowledgment recovery, and reconciliation. Provider code never writes manuscript bodies.

## Known W0 limits

The trial uses sample content and session-only feedback. It has no author storage, provider, model request, project library, SQLite persistence, durable Apply, receipt, reconciliation, structural proposal validator, or installer qualification. Unicode edge cases and native UI Automation checks support English-editor robustness; they do not establish Chinese authoring or an IME requirement. The remaining English native author trial, minimum-window behavior, external Word paste, and screen-reader user experience remain qualification work.
