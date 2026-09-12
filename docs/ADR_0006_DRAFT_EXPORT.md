# ADR 0006 — Preview and retain explicit draft exports

**Status:** development implementation, 5 September 2026. Executed checks and native/package qualification are tracked in [implementation status](IMPLEMENTATION_STATUS.md).

**Export draft** exports one complete saved document as UTF-8 plain text or Markdown. The author sees the exact output, its format limitations, and its saved version before opening the native Save dialog. This is a working-draft action; it does not mark a chapter ready, publish it, or establish reviewed story material. Unavailable ready-manuscript actions remain absent.

## Frozen source and output

The writing session flushes pending typing under its existing lifecycle guard. Rust checkpoints the requested exact head, validates the complete restricted document, and projects the retained revision. The preview includes project and operation namespace, document head, immutable revision ID, format and format version, exact UTF-8 text, byte count, SHA-256, and format-loss explanation. The renderer checks ownership, bytes, and hash before displaying inert text.

Changing format or explicitly refreshing prepares a new preview. Saving uses the selected frozen revision even if newer writing exists. No missing source is silently substituted or omitted. The single-document preview is the full output; multi-document selection and ready-story omission manifests belong to later work.

The author chooses the destination through a native dialog. Rust revalidates the active project/session/lease, retained source and projection before installation. It writes a staged file and installs it without overwriting an existing destination. A destination inside the active project is refused. The renderer cannot supply an arbitrary write path to this command. The old direct TXT IPC/core export path has been removed. Basenames reject Windows device names, alternate streams, reserved characters, and trailing dots/spaces. Parent paths are canonicalized, but hostile concurrent same-user directory replacement is outside this local single-author boundary.

## Durable export record and failure boundary

Schema 9 adds immutable `export_records`. Each record retains the preview ID, project and namespace, document/revision/head, working-draft label, format/version, output bytes/hash, destination basename, and creation time. Absolute destination paths are not copied into project records. Recovered projects retain old records as historical evidence under their original namespaces.

The project actor serializes final validation, bounded local file installation, and metadata recording. It rejects a previously recorded preview before creating another output; changed payloads cannot reuse its ID. Reusing an already exported preview requires an explicit newly prepared export.

Filesystem installation and the subsequent SQLite record are separate durability boundaries. If installation succeeds but recording fails, the file remains and the caller receives `ExportRecordUnavailable`. An uncertain database commit fences the project connection. There is no automatic external-file retry, rollback deletion, or claim that filesystem and SQLite committed atomically.

Canceling destination selection retains the preview. An existing destination permits choosing another name. A lost acknowledgment or possible post-install failure tells the author to inspect the chosen destination and requires an explicit fresh preview in the UI before another attempt. This fresh-preview rule belongs to the caller: if a deterministic record insert failed, the core has no durable row proving that attempt and cannot prohibit a separate direct caller from reusing its ID at a new destination. No automatic retry is exposed. Exact file content can be checked against the retained hash; an export record is not evidence that the destination still exists unchanged.

## Interface and lifecycle

The modal preview keeps the existing editor mounted and renders output in a plain `pre` element. Focus starts on format selection and returns to **Export draft** on close. Close, Escape, and format changes are disabled while a native save is pending. Owner and sequence guards discard late responses after a document/session change. An old pending save cannot block or clear a new owner's request.

Comments, conversations, history, and context records are not part of the prose export. Format limitations are shown before destination selection. The canonical rich document remains the only editable source; Markdown and TXT are projections.
