# ADR 0029: Normal close joins writing and background work

Status: implemented development slice; executed qualification is recorded in
[implementation status](IMPLEMENTATION_STATUS.md). This extends the lifecycle
contract in [the architecture](V3_ARCHITECTURE_REFINED.md#82-project-switching-and-normal-close).

## Author experience

Closing the window first waits for the current editor operation and saves the
manuscript, checkpoint, and document view. A save failure keeps the editor open
with its existing recovery actions. If AI replies or story-memory refreshes are
still running in any open project, the author chooses **Stop replies and close**
or **Stay open**. Switching projects continues to leave their jobs running.

Stop waits for accepted requests to finish starting and then requests cleanup.
The dialog remains cancellable while waiting. If a completed result needs a
local save retry, closing is blocked so its retained output remains available.
There is no forced-close button or generation retry in this flow. A bounded
wait that does not reach readiness keeps the application open.

## Ownership and ordering

1. A unique close ID closes native generation admission before the editor
   flush. Acceptance holds a registration inside the actual blocking closure,
   including when its renderer waiter disappears. An accepted start cannot fall
   between the count of starting requests and registration of its worker.
2. The document's existing `detachAfter` guard drains saves and checkpoints the
   document. Its preparation callback contains the entire close check and
   eventual window destruction. Stay open, failed status checks, and failed
   destruction leave the same editor attached.
3. Rust reads active discussion and memory jobs from every open project actor,
   restricted to that project's current operation namespace. This does not
   attach a new renderer, rotate a lease, or use copied historical jobs as work.
4. Explicit Stop captures the exact jobs and cancellation handles. Rust
   persists Stop through the existing lifecycle methods and cancels the owned
   external work even if a local Stop write fails. A late Stop cannot select a
   new job created after Stay open.
5. Worker registrations outlive local result settlement or retention. Normal
   close waits for starting requests, workers, active durable jobs, and retained
   result writes to reach zero. Only after Stop was chosen, with no producer or
   retained result left, may exact orphan job IDs be marked interrupted. Saved
   prefixes and historical receipts remain; no model request is replayed.
6. A final native readiness check keeps admission closed through window
   destruction. If destruction fails, the renderer cancels that same close ID.
   Cancelled IDs remain retired for the process lifetime, so a delayed begin or
   old acknowledgment cannot reopen that close attempt.

The native page-load callback retires an abandoned close gate when a renderer
is replaced. It leaves worker ownership and durable job state intact. Existing
project attachment fences replacement renderer leases before writing. Forced
reload, process termination, OS shutdown, and power loss remain separate from
normal close: an unsent or failed-save editor buffer may be lost, and recovery
never automatically submits a model request.

## Boundaries

This slice uses existing project actors, provider cancellation handles, and
in-memory failed-result retention. It adds no database schema, persistence
queue, indefinitely saved undo stack, or second manuscript store. Local mock,
Codex, Claude, and HTTP generation enter the same admission boundary.

Connection discovery is not generation and is not shown as a paid reply in
the dialog. Native installation/update behavior, actual OS shutdown, and
author acceptance have their own qualification gates. Test-owned synthetic
jobs and native fixtures do not establish live-provider quality.
