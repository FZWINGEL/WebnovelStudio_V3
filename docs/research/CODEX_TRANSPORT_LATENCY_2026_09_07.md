# Codex transport latency qualification for V3

**Date:** 7 September 2026  
**Scope:** native Codex app-server/SDK and the pinned `10/chatgpt-codex-proxy` and `Arocial/codex2api` revisions.  
**Decision status:** development evidence only; no V3 production adapter changed.

## Result

The next V3 integration candidate is the persistent official Codex app-server, reached either through the native executable or the Python SDK. It is the only route in this comparison that preserves the native app-server contract without a third-party protocol translation layer. The two live HTTP proxies both produced lower observed first-text latency than the native routes on this host, but the difference is end-to-end and confounded by request envelopes, transport, and upstream behavior. This run does not establish an isolated Go-versus-Rust winner or a production proxy choice.

`gpt-5.6-luna` and `gpt-6-astra` were both available. Each model was run with low reasoning at the two V3 profile settings: Standard (no `service_tier`) and Fast (requesting `priority`). The proxy terminal responses reported `default` for both requested tiers. Native exec and SDK rows have no effective-tier echo. Therefore this run cannot claim that Fast was delivered or that it produced a premium-tier benefit.

## Primary benchmark

The primary artifact contains 100 attempts: four model/tier configurations, five samples per configuration, and four stateless routes (`exec`, persistent SDK, Arocial HTTP, and Go HTTP), for 80/80 successful exact matches, plus 20 Go WebSocket continuation attempts. Every successful stateless response matched the same 215-character passage and reported 47 output tokens. Native input usage was 303 tokens for Luna and 299 for Astra; proxy input usage was 97 tokens for both models because the envelopes differ. The profiles were exported from V3's source profile builder with the same restrictive selections. Timings start at the transport client or native process launch; they exclude Tauri IPC, database work, rendering, and author interaction, so they are not full-application stopwatch measurements. The native executable was 0.153.4; the Python SDK package was `openai-codex==0.147.0`, pointed at that same executable.

`first text` means first non-empty streamed text delta for SDK and proxy routes. Native exec emits the assistant text only on `item.completed`, so its value is first completed message latency, not TTFT. `full completion` includes the terminal event; for native exec it also includes process teardown.

| Model | Requested tier | Route | Pass | First text median (range), s | Full completion median (range), s | Reported tier |
| --- | --- | --- | ---: | ---: | ---: | --- |
| Luna | Standard | native exec | 5/5 | 3.732 (3.520â€“4.562) | 4.785 (4.087â€“5.298) | â€” |
| Luna | Standard | Python SDK | 5/5 | 2.838 (2.509â€“3.058) | 3.714 (3.361â€“4.008) | â€” |
| Luna | Standard | Arocial HTTP | 5/5 | 1.290 (1.190â€“1.352) | 2.293 (2.193â€“4.805) | default |
| Luna | Standard | Go HTTP | 5/5 | 1.206 (1.078â€“1.758) | 2.398 (2.049â€“4.096) | default |
| Luna | Fast | native exec | 5/5 | 3.884 (3.389â€“5.145) | 5.516 (3.855â€“6.283) | â€” |
| Luna | Fast | Python SDK | 5/5 | 3.339 (2.796â€“3.665) | 3.923 (3.578â€“4.528) | â€” |
| Luna | Fast | Arocial HTTP | 5/5 | 1.169 (1.039â€“1.851) | 2.019 (1.654â€“2.639) | default |
| Luna | Fast | Go HTTP | 5/5 | 1.206 (1.033â€“3.038) | 1.766 (1.645â€“4.489) | default |
| Astra | Standard | native exec | 5/5 | 5.878 (4.958â€“6.102) | 6.528 (5.585â€“7.269) | â€” |
| Astra | Standard | Python SDK | 5/5 | 3.771 (3.701â€“4.345) | 5.199 (4.928â€“5.670) | â€” |
| Astra | Standard | Arocial HTTP | 5/5 | 2.217 (1.501â€“4.268) | 3.676 (3.105â€“5.806) | default |
| Astra | Standard | Go HTTP | 5/5 | 2.281 (2.083â€“2.861) | 3.711 (3.546â€“4.137) | default |
| Astra | Fast | native exec | 5/5 | 4.509 (3.854â€“5.370) | 4.917 (4.290â€“6.378) | â€” |
| Astra | Fast | Python SDK | 5/5 | 3.467 (2.828â€“4.416) | 4.305 (3.835â€“5.245) | â€” |
| Astra | Fast | Arocial HTTP | 5/5 | 1.622 (1.328â€“2.059) | 2.558 (2.194â€“5.876) | default |
| Astra | Fast | Go HTTP | 5/5 | 1.390 (1.231â€“2.718) | 2.273 (2.045â€“3.557) | default |

The native SDK showed earlier first text in this sample. That difference combines incremental text delivery, process/connection reuse, and upstream variation; it is not an isolated measurement of process startup. The four SDK server startups measured 72.160â€“164.037 ms and are excluded from per-turn values. Median creation of a fresh thread on an already-running SDK server was 7.306 ms. The HTTP proxy rows use smaller request envelopes than native rows, so their lower observed input usage and latency must not be interpreted as proxy-only overhead.

## Persistent WebSocket behavior

The Go proxy WebSocket route was intentionally kept separate from the stateless comparison. Its 20 primary attempts reused one downstream connection per model/tier and relied on the proxy's automatic `previous_response_id` continuation. Five completed and 15 failed after connection idle/reuse events. The five successful rows had first-text medians of 1.470 s (Luna Standard, n=2), 1.868 s (Luna Fast, n=1), 2.786 s (Astra Standard, n=1), and 3.158 s (Astra Fast, n=1); these are too sparse for a route ranking. Failures included upstream keepalive ping timeouts, aborted writes, and invalid `previous_response_id` responses.

Source inspection at the Go revision found that the upstream WebSocket has no background reader or ping service between turns. Gorilla WebSocket control frames are processed by reads, but the handler waits on the downstream connection while idle. The controlled idle probe below reproduced the corresponding keepalive failure. The same source normalizes `priority` in stateless HTTP, but its WebSocket payload builder omits `service_tier`, so persistent WebSocket results cannot qualify Fast-tier delivery.

The separate control probe completed three immediate requests and one request after a 45.373-second idle interval with the downstream read pump active: 4/4 passed, with first text 1.138â€“2.251 s and full completion 2.084â€“3.600 s. A second probe completed three immediate requests, then after 90.241 seconds of idle the next request failed in 0.724 ms with upstream `1011 keepalive ping timeout`, despite active downstream receive/pong handling. These controls are outside the 100-sample main dataset. Together with the source inspection, they establish a material idle-reuse risk for this revision; they do not show that every persistent WebSocket request fails. The primary 15 failures also included aborted writes and `Invalid previous_response_id` after reconnect attempts, so they must not all be classified as keepalive failures.

The separate representative-context follow-up completed 32/32 exact responses (12,448-byte input, 47 output tokens per success, two samples per route/configuration). It is exploratory: two observations per cell do not establish a latency distribution, and a separate WebSocket control briefly overlapped the follow-up. Rankings moved with model/tier: for example, Astra Fast Go was 3.590 s first text / 6.078 s full completion, while Rust was 1.788 s / 2.702 s. It does not change the primary short-input comparison.

## Local proxy forwarding overhead

The two local fixtures measured 800 requests total: 400 for each proxy, split evenly between concurrency 1 and 4, with 100 direct and 100 proxied requests at each concurrency. These are local forwarding controls, not model calls. The Go fixture required HTTPS because its authentic upstream transport rejects plaintext; Arocial used HTTP. That transport difference prevents a direct language ranking.

| Proxy | Concurrency | Metric | Paired extra median | Paired extra p95 |
| --- | ---: | --- | ---: | ---: |
| Go | 1 | First delta, timer-adjusted | +0.449 ms | +0.735 ms |
| Go | 1 | Terminal, timer-adjusted | +0.505 ms | +0.960 ms |
| Go | 4 | First delta, timer-adjusted | +0.589 ms | +3.314 ms |
| Go | 4 | Terminal, timer-adjusted | +0.688 ms | +3.169 ms |
| Arocial | 1 | First delta, timer-adjusted | +0.542 ms | +0.816 ms |
| Arocial | 1 | Terminal, timer-adjusted | +0.573 ms | +0.925 ms |
| Arocial | 4 | First delta, timer-adjusted | +0.773 ms | +1.296 ms |
| Arocial | 4 | Terminal, timer-adjusted | +0.798 ms | +1.410 ms |

The fixture requested delta/terminal delays of 10/20 ms; Windows timers actually fired around 15/30 ms. Each adjusted sample subtracts its own measured fixture delay before pairing proxied and direct requests. Raw near-zero differences can therefore mask forwarding overhead. Go used HTTP/2 over TLS upstream and an HTTPS/1 direct control; Rust used plaintext HTTP.

All 400 measured requests per proxy returned HTTP 200 with the expected nine SSE events and matching lifecycle/output checks. These fixtures isolate local transport and serialization work; they omit model service time, upstream network, OAuth refresh, HTTP-to-WebSocket adaptation, and V3 production behavior.

## Repository qualification

The tested sources were cloned at the exact revisions below. The real Go binary has one local patch restricting its listener to loopback; its mock-only helper is separate. Arocial used unchanged source. These are source identities, not claims that every retained clone worktree remains clean after fixture preparation:

| Component | Source | HEAD | Qualification |
| --- | --- | --- | --- |
| Go proxy | [`10/chatgpt-codex-proxy`](https://github.com/10/chatgpt-codex-proxy) | `89973f46dc565874527520f822f1971a9c6a5652` | Build succeeded; Go test packages `internal/turn`, `internal/openai`, `internal/codex`, and `internal/server` passed. |
| Rust proxy | [`Arocial/codex2api`](https://github.com/Arocial/codex2api) | `b1fd4ca3985f3a72c3d8cdd8acd0df85db2c0b70` | `cargo build --release --locked` succeeded in 27.12 s; the unchanged release binary passed the bounded live HTTP requests. |
| Node proxy | [`rokyplay/codex2api`](https://github.com/rokyplay/codex2api) | `a0d5acd6442048811df58fab15dac42560a254e2` | `npm ci --ignore-scripts --no-audit --no-fund` succeeded, but `node server.mjs` exited after 70 ms with missing tracked source `lib/converter/utils.mjs`; no latency result. |

The native integration surface is the [Codex app-server](https://developers.openai.com/codex/app-server/) and [Python SDK](https://learn.chatgpt.com/docs/codex-sdk). These links describe the primary upstream surface; the latency values above are local qualification data.

## Qualification limits and next action

This is a small single-host cohort using a synthetic short response. It does not qualify swarm concurrency, narrative quality, tool calls, structured outputs, asynchronous tool calls, mid-turn steering, long generations, account routing, or release readiness. The main SSE run was started before the final parser fix; first-text and terminal measurements remain valid, but its `Requests.Session` reuse flags do not establish live TCP reuse. The corrected parser drains the final blank byte through HTTP EOF, and all eight harness contract tests pass; the loopback reuse test establishes client capability only. No credentials or raw account logs are included.

Use the persistent official app-server as the next V3 integration candidate, then run a controlled follow-up covering production-shaped context, tools, continuation, and concurrency. Keep both HTTP proxies as benchmarked compatibility candidates until those gates are measured. This report reflects the verified 7 September state and supersedes the quoted 5 September ranking for this local decision; it makes no public-release claim.

## Evidence

The ignored, reproducible evidence folder is [`.local/codex-latency-2026-09-07`](../../.local/codex-latency-2026-09-07/). The primary dataset is [`main.jsonl`](../../.local/codex-latency-2026-09-07/main.jsonl), summarized by [`summary.json`](../../.local/codex-latency-2026-09-07/summary.json). Supporting files include [`context.jsonl`](../../.local/codex-latency-2026-09-07/context.jsonl), [`ws-control.jsonl`](../../.local/codex-latency-2026-09-07/ws-control.jsonl), [`ws-control-90.jsonl`](../../.local/codex-latency-2026-09-07/ws-control-90.jsonl), [`bench.py`](../../.local/codex-latency-2026-09-07/bench.py), [`bench-review.md`](../../.local/codex-latency-2026-09-07/bench-review.md), [`bench-verification.md`](../../.local/codex-latency-2026-09-07/bench-verification.md), [`go-build-info.json`](../../.local/codex-latency-2026-09-07/go-build-info.json), [`go-ws-source-notes.md`](../../.local/codex-latency-2026-09-07/go-ws-source-notes.md), [`overhead-go-summary.md`](../../.local/codex-latency-2026-09-07/overhead-go-summary.md), and [`overhead-rust-summary.md`](../../.local/codex-latency-2026-09-07/overhead-rust-summary.md). The linked result artifacts contain synthetic benchmark data and sanitized diagnostics. Private server logs are excluded. See [`manifest.json`](../../.local/codex-latency-2026-09-07/manifest.json) for final file hashes and [`README.md`](../../.local/codex-latency-2026-09-07/README.md) for rerun steps. Both real proxies and all fixture listeners were stopped; five copied auth/key files were removed while the original Codex login remained intact, as recorded in [`cleanup.json`](../../.local/codex-latency-2026-09-07/cleanup.json).
