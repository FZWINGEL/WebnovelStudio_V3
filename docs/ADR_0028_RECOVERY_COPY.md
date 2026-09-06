# ADR 0028 — Recovery copy from the live editor buffer

**Status:** Implemented and qualified in the native development app; installed-package qualification remains pending

**Date:** 2026-09-06

## Context

When project saving fails, the author may still have newer text in the live
editor. The recovery action must preserve that buffer without depending on the
project database or changing the saved project state.

## Decision

The Writer exposes **Save recovery copy** alongside save-error recovery
actions. It captures a structured editor snapshot synchronously at click time,
before any asynchronous work. The capture is immutable while the author
chooses a destination; it does not flush, read, save, checkpoint, reconcile,
or advance the saved watermark. New typing after the click is therefore not
part of that copy.

The native command validates the restricted snapshot, opens an explicit
`Save recovery copy as a new file` dialog, and accepts a Markdown `.md`
destination. Core validates the same snapshot, projects it through the
existing Markdown projection, and installs the file only when the destination
does not already exist. Basename validation rejects Windows device names,
alternate data stream syntax, trailing-dot/space names, and other invalid
filenames. The transfer has no project, database, writer-lease, or source-root
dependency.

The result acknowledges the captured `snapshotHash`, output hash, and UTF-8
byte count. Cancellation or failure leaves the editor buffer available and
does not retry automatically. A missing or mismatched acknowledgment asks the
author to inspect the destination before trying again. The action is a single
document copy; it does not copy discussions, history, backups, or other
documents.

## Verification boundary

Focused core transfer and frontend recovery-copy checks cover click-time
capture, Markdown projection, existing-file refusal, invalid snapshots,
destination failure, and receipt handling. The native WebView2 check injects a
SQLite save failure, uses the actual Save/Cancel dialogs, verifies exact
Markdown and unchanged durable text, blocks navigation, and then retries the
project save. Current executable and evidence are recorded in
[implementation status](IMPLEMENTATION_STATUS.md). Actual disk-full/ACL and installed-package
qualification remain pending. This slice does not claim zero-keystroke-loss,
W6 completion, or full release qualification.

Related: [discussion recovery](ADR_0009_DISCUSSION_RECOVERY.md), [draft export](ADR_0006_DRAFT_EXPORT.md), and [Windows package qualification](WINDOWS_PACKAGE_QUALIFICATION.md).
