# Windows child-process contract

This document records the current Windows process-boundary foundation for a
future qualified CLI adapter. It is a local execution contract, not provider
support. The implementation remains disconnected from production discussions,
which are still mock-only.

## Boundary

[`windows_process.rs`](../crates/core/src/providers/cli/windows_process.rs)
accepts one absolute executable, a fixed argument vector, an explicit working
directory and environment policy, one bounded packet, and per-invocation
limits. Packet bytes are written to stdin; they are never appended to command
arguments. The primitive does not parse provider protocols, expose shell
commands, read credentials, or retry upstream requests. Callers may use the
bounded `finish_or_stop_with_output` observer when they need incremental
bytes; it remains a local capture hook rather than a provider protocol
streaming contract.

`spawn` validates paths and limits, creates the child suspended, assigns it to
a kill-on-close Job Object before `ResumeThread`, and passes only the three
intended standard handles through `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`. Parent
handles are non-inheritable. The fixture and handle-list regression are in
[`windows-process-fixture.rs`](../crates/core/src/bin/windows-process-fixture.rs)
and `handle_list_excludes_an_unrelated_inheritable_sentinel`.

## I/O and lifecycle

Stdout and stderr have bounded reader queues. The combined retained output cap
keeps the accepted prefix and marks `truncated`; it does not claim that the
child stopped writing at the exact cap. Stdin uses one boxed pending write with
an `UnsafeCell<OVERLAPPED>`, its event, buffer, and parent pipe handle owned by
the same allocation. The allocation remains alive until completion or an
explicit unresolved-cancellation leak, so Windows is never left with a pointer
to freed packet storage.

`RunningChild::finish_or_stop` is the proof-bearing path. It polls the stop
signal and overall deadline, allows the configured stop grace, then terminates
the Job Object when required. Before returning a terminal outcome it waits for
the root process, waits for Job Object `ActiveProcesses == 0`, drains both
reader streams, and joins both reader workers. Completion also terminates the
Job Object so a descendant cannot keep inherited pipes open.

Before termination, the proof-bearing path takes a bounded Job PID snapshot and
retains `PROCESS_SYNCHRONIZE` handles for the members reported by that
snapshot. It takes a second snapshot immediately after `TerminateJobObject` to
cover members observed during that termination race. A snapshot whose reported
PID count is lower than its assigned-process count fails cleanup rather than
being treated as complete. The retained handles and Job accounting are checked
within the same cleanup deadline. These handles are bounded evidence for the
members returned by complete snapshots; Job containment remains authoritative
for the process tree, and this does not prove a universal guarantee for a
member that appears after the final snapshot or for processes outside the Job.

`finish_or_stop_with_output` uses the same proof path and invokes its
synchronous callback only for bytes accepted into the combined bounded
prefix. It preserves per-stream read order and also observes the final drain.
Once the finish path observes `Stop`, the callback receives no later bytes;
those bytes may still be retained in `ChildOutput` while cleanup settles. The
callback must be short and nonblocking. The process deadline cannot bound an
arbitrarily blocking callback, so provider adapters must keep callback work
bounded.

`ChildOutput` reports the exit code when available, confirmed
`stdin_bytes_written`, typed stdout/stderr bytes, truncation, and typed I/O
errors. A zero-exit process with incomplete stdin delivery is a
`ContainmentError::Cleanup` with `partial` output rather than a successful
provider result. Cleanup, worker, read, write, and delivery failures remain
distinct from a child nonzero exit. A future adapter must keep `partial`
private, avoid automatic retries, and disable the provider after a containment
failure until an explicit recovery decision is made.

`Drop` is a bounded best-effort guard. It requests local stop, terminates the
Job Object, cancels pending stdin, closes shared pipe handles, and waits for
reader completion for bounded intervals. Because `Drop` cannot return an
error, it may detach a reader worker after unresolved OS cleanup or retain a
bounded pending operation owner when cancellation does not settle. Only
`finish_or_stop` establishes the zero-active-process and settled-worker
contract; neither path claims a universal kernel or host-process guarantee.

## Evidence and limits

The focused Windows integration suite is currently 20/20, and the module unit
subset is 3/3. The tests retain live process handles for root/child/grandchild
fixtures and assert those handles become signaled for Stop, timeout, and
completion cleanup. They also cover combined output caps, packet-only stdin,
early zero-exit with incomplete delivery, cancellation, Drop queue behavior,
argument quoting, and the restricted handle list. The standard CI descendant
cleanup failure predates the bounded post-termination snapshot fix; no new
native UI pass is implied by the focused process result. See
[`windows_process.rs` tests](../crates/core/tests/windows_process.rs).

This evidence does not qualify a live provider, model budgets, upstream retry
or billing semantics, protocol parsing, provider streaming semantics,
credentials, provider interruption semantics, or author-data isolation. No
live provider call was made for this contract. The separate
[Codex qualification](CODEX_QUALIFICATION.md) remains synthetic evidence and
does not enable the adapter.
