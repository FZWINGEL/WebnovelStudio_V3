# Codex app-server qualification

**Status:** Private development qualification in progress. The optional local
app-server transport has four core live Workshop trials and a separate
three-invocation Astra comparison, but this record does not establish broad
live-generation support, release readiness, performance improvement, or
narrative quality.

**Checked date:** 8 September 2026

**Installed executable:** Codex `0.153.4`

**Executable SHA-256:**
`e5aa76d19c7c94e2e9ef9b707d590206a73ac0e97c8ddc8382181242494bef75`

See [ADR 0033](ADR_0033_CODEX_APP_SERVER.md) for the architecture and the
transport boundaries. Exec remains the default transport while this optional
route is qualified.

## What is qualified

The installed no-generation check passed the app-server startup, account
handoff, restrictive catalog, fresh-thread, pre-turn cancellation, warm reuse,
and shutdown checks. It used the following requested sequence in one server
process:

1. GPT-6 Astra, low reasoning, priority tier.
2. GPT-6 Astra, low reasoning, Standard tier.
3. GPT-6 Astra, low reasoning, priority tier again.
4. GPT-5.6 Luna, with the requested author traits.

Each check used a fresh ephemeral thread and was stopped by the pre-turn
callback, so no model generation turn or paid output occurred. The Standard
check also verified that priority was not inherited from the previous request.
Warm server reuse and final process shutdown completed successfully.

The final-source sample took 8.604 seconds: discovery 3.450 seconds, startup
2.761 seconds, thread checks 38, 93, 93, and 92 milliseconds, and shutdown
2.075 seconds. Its report is `.local/app-server-qualification/final-check.json`.
This is one private no-generation qualification sample, not a comparison
against Exec or evidence of inference latency.

The application keeps the Codex executable unpinned; this identity records the
executable that was actually checked.

## Native desktop evidence

The isolated Windows desktop harness ran the authentic Tauri/WebView2
application with synthetic application data. The Settings smoke used a cold,
empty `CODEX_HOME`; this evidence did not use the author's application data or
launch an author process.

An earlier first mock-harness attempt left the provider revision at zero while
startup discovery was still completing. It consequently launched one
synthetic Codex job, timed out, and was cleaned up through the owned Job. The
job did not use the author's project. The harness was corrected to persist the
mock choice explicitly before discovery could race it. The successful
31-check Workshop run used the deterministic mock provider with zero LLM
calls, and the successful four-check Settings run also used zero LLM calls.
Native evidence therefore does not claim that every earlier harness attempt
was zero-call.

The app-server Settings smoke passed all four checks in:

`.local/isolated-native-launch/20260908-004133948-15548`

It covered the author model, reasoning effort, and service-tier preservation;
the Exec-to-app-server preference save; restart persistence; and returning to
Exec. The flow confirmed that selecting app-server does not fabricate live
readiness or create discussion/memory jobs. The restart was performed through
the real Tauri/WebView2 application. Owned process cleanup completed without a
forced Job termination.

The Story Workshop smoke passed all 31 checks in:

`.local/isolated-native-launch/20260908-004509426-50744`

That run used the deterministic mock provider and includes the corrected
choice-persistence and mock `moment` candidate-card behavior. It is supporting
native application evidence only; it is not live app-server generation
evidence. Cleanup completed without a forced Job termination.

## Successful fourth core live trial

A fourth core live Workshop trial passed after the earlier three attempts
below. It used the requested GPT-5.6 Luna, xhigh reasoning, and priority tier.
The retained evidence is
`.local/workshop-qualification-59ca63cd3ef24b93a8aaa8bdc55e05ac.json`.

The measured stages were preparation 6,165 ms, request start 6,192 ms, first
delta 12,792 ms, terminal completion 26,628 ms, and durable persistence
26,643 ms. Server startup took 2,683 ms. The result included an acknowledged
turn, `terminal: completed`, `requestSettled: true`, and a reusable
connection; managed idle shutdown was also confirmed.

The trial produced three validated directions with stable IDs and an exact
packet receipt. The author-room note anchor and manual state were unchanged,
and no chapter was created. This is successful live behavior for the exercised
Workshop path; it does not by itself qualify concurrent requests, interruption,
all writing paths, performance against exec, or release behavior.

## Bounded transport comparison

A three-case comparison passed for the same requested GPT-6 Astra, low
reasoning, priority configuration. The retained report is
`.local/codex-transport-comparison-78abc62203fb40f0a23cf81be1655fa7.json`.
The same 243-byte application packet and packet hash were used for Exec, a
cold app-server request, and a warm app-server request. The internal transport
envelopes differed as expected. The Exec and cold app-server cases returned the
same 113-byte output; the warm app-server case returned a different 120-byte
output, so this sample does not establish semantic equivalence.

The report's times use a separate clock for each case:

| Case | First visible result | Terminal | Usage |
| --- | ---: | ---: | --- |
| Exec | Completed message at 7,626 ms | 8,012 ms | 505 input, 28 output tokens |
| Cold app-server | Assistant delta at 2,986 ms | 3,362 ms | 251 input, 28 output tokens |
| Warm app-server | Assistant delta at 3,515 ms | 3,942 ms | 251 input, 30 output tokens |

App-server startup took 2,721 ms separately from the cold request clock. All
three cases settled cleanup; the shared server was reusable and its idle
shutdown was confirmed. Effective model, reasoning, service tier, and
inference counts were not reported by the provider. There is one sample per
case, so this is a transport diagnostic, not a benchmark, a general speed
claim, or a Stage E workload matrix. It establishes Astra/low fixed-profile
generation on both transports for this synthetic request, not the semantic
quality of an actual navigation digest.

## Live-turn attempts

The three initial live-turn attempts below remain preserved as failed or
non-adoptable evidence. Together with the successful fourth Workshop trial
above, they make four core Workshop trials. The three successful Astra
comparison invocations above are separate and are not included in that count.
Failed receipts remain preserved without automatic replay:

| Attempt | Observed result | Qualification consequence |
| --- | --- | --- |
| `28ac...` | Failed without a failure code. | No successful live response; do not infer provider behavior from the missing code. |
| `b8a0...` | Failed with code `httpConnectionFailed`; no HTTP status, text, or usage was recorded. | No successful live response and no retry authorization. |
| `7b18...` | After adding only `SystemRoot`, the provider returned a complete 8,011-byte JSON response. Cleanup then remained unresolved, so the result was conservatively non-adoptable. | The driver exposed a protocol bug: empty text was incorrectly treated as authoritative. The correction was exercised by the successful fourth trial; this earlier receipt remains non-adoptable. |

The third attempt demonstrates why response bytes alone do not qualify a
turn. The application must settle transport cleanup and validate the result
before presenting it as adoptable. A later process or request must not replay
an uncertain turn merely because the first attempt did not produce a usable
receipt.

## Evidence boundaries

| Area | Current conclusion |
| --- | --- |
| Installed app-server startup and no-generation isolation | Passed for the checked Codex 0.153.4 executable and the recorded synthetic inputs. |
| Fresh threads and warm process reuse | Passed in the no-generation check. |
| Native Settings persistence and no-generation behavior | Passed 4/4 on the isolated Tauri/WebView2 desktop. |
| Workshop native behavior | Passed 31/31 with the deterministic mock provider. |
| Core live Workshop generation and terminal cleanup | Four core Workshop trials are recorded: one successful Luna/xhigh/priority trial and three failed or non-adoptable attempts; broader route and failure qualification remains open. |
| Astra/low transport comparison | Passed 3/3 for one synthetic request across Exec, cold app-server, and warm app-server; semantic equivalence and general speed remain unknown. |
| Effective model, reasoning, and service-tier behavior during generation | Unconfirmed; the provider does not echo the effective model, reasoning effort, or service tier. |
| Performance versus exec | One sample per case exists in the Astra comparison; it is not a benchmark or a general speed claim. |
| Release readiness, installed upgrade behavior, and broad native qualification | Pending. |
| Prose quality, continuity quality, and author usefulness | Not measured by these checks. |

The hosted CI attempt `34165285951` was blocked before any steps ran. GitHub's
billing annotation prevented execution, so it provides no hosted qualification
evidence.

## Local verification

The final full-check log `.local/app-server-final-check.log` records 839 Rust
tests: 103 core unit, 654 integration, and 82 desktop, plus one intentionally
ignored subprocess-helper core test. It also records 600 frontend tests across 56 files.
Formatting, strict Clippy, TypeScript, the production frontend build, and
25 tooling checks passed in the same run. These checks do not close the open gates for
native live/recovery, concurrent live/Stop, a representative comparison
workload matrix, release readiness, or author quality.

The final debug build passed after the source checks; it was not launched on
the author's desktop. Build log: `.local/app-server-final-build.log`.

| Field | Final debug build |
| --- | --- |
| Path | `target/debug/webnovel-desktop.exe` |
| Product version | 3.0.0 |
| Built UTC | 2026-09-08 01:08:49 |
| Bytes | 49,910,784 |
| SHA-256 | `50323b9603bca3a713f317722c3089ea769de9a60ea5d36a7c590a79f9a57f60` |

The native Settings and mock Workshop runs preceded the final protocol and
receipt hardening. Those changes passed the final Rust/desktop regression
suite; their new executable has not repeated a native live-generation trial.
This debug build does not replace or requalify the earlier private installer.
