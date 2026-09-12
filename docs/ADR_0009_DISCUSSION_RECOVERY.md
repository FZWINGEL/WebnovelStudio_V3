# Discussion Stop and local save recovery

Status: implemented development contract, 5 September 2026. Production discussion still uses only the deterministic mock. This extends existing run records; it does not change manuscript saving, Apply, or story authority.

## Stop ownership

Stopping a queued run seals it immediately without dispatch. Stopping a running run durably records `Stopping`. The worker finishes its owned work and calls the Rust-only settlement method; the renderer cannot acknowledge cleanup. The mock has no external process or outstanding I/O when it settles. A future provider must establish its own process and stream cleanup first.

While stopping, delivery, output appends, completion, and failure refuse further changes. Repeated Stop is idempotent. Settlement checks the exact project, operation namespace, run, output sequence, and retained prefix, then atomically writes final output, its terminal event, and an immutable assistant message. Confirmed cleanup becomes `Stopped`; unresolved cleanup becomes `Interrupted`. Both retain the partial response with its incomplete explanation. A repeated settlement must match the first payload. No stopped response creates proposals.

Completion also preserves the exact durable output prefix. Completion and Stop may retain up to the existing 2 MiB response limit; neither may replace previously saved text. Reopening a project interrupts unfinished runs and never resumes generation automatically.

## A failed local write is visible

The desktop retains a pending local outcome when a claim or final response write cannot be confirmed. Its in-memory map is keyed by project, operation namespace, and run. Reads validate the current document lease before showing its recovery action. The entry survives renderer reload and project navigation within the same app process.

Claims and local recovery share a short synchronization boundary. Once a failed claim needs checking, a duplicate start cannot dispatch that run. A successful claim remains exclusively owned by its worker. Only finished workers or failed undispatched claims can add pending outcomes; the renderer cannot insert one or take over an active worker.

The discussion panel offers **Retry saving response**. It first uses existing document reconciliation to recover an uncertain connection and obtain a fresh lease, preserving newer editor content. The desktop then reads the current durable run and performs only local storage work:

- An already-terminal run needs no second mutation.
- A stopping run settles Stop using the current durable prefix, even if a completion had been pending.
- A failed completion write retries its exact retained completion payload.
- A failed start or output operation seals failure against the current sequence and preserves the saved prefix.

Another storage failure leaves the recovery action available. A fenced discussion read offers **Check saved discussion**, so losing the original renderer callback does not require blindly resending a request. A completed retry refreshes proposals through the ordinary read path.

These actions do not run a model, consume a new request allowance, or apply a manuscript edit. Preparing another model attempt remains a separate, explicit author action.

## Limits and evidence

Pending outcomes are not a second durable queue. If the app exits, its memory disappears; the next project open retains durable partial output and marks the unfinished run interrupted. This does not promise recovery of bytes that never reached storage. The current mock UI displays only persisted chunks. A future live adapter must separately qualify buffering, cancellation, provider usage, and unresolved process cleanup.

File-backed tests cover terminal-write rollback, persistent storage failure, navigation and stale leases, queued claim failure, duplicate starts, and refusal to take over a running worker. Core tests cover Stop ordering, exact replay, preserved prefixes, atomic rollback, and independent recovered-project identity. Frontend tests cover retry without generation, retained errors, and late replies after navigation. Native smoke injects a terminal-write fault into its own synthetic database, reloads the renderer, and checks that recovery finishes without adding a run or changing prose. Executed counts and native evidence belong in [implementation status](IMPLEMENTATION_STATUS.md).
