# V3 implementation status

**Status date:** 6 September 2026

**Current branch:** `codex/v3-persistence`
**Overall:** in progress; the full V3 goal is not complete.

The native development app supports persistent projects, free-order English writing, document discussion, exact context inspection, adopted guidance, saved discussion sources, optional approved writing briefs, selected-passage and structured block suggestions, saved versions, bounded Windows Codex assistance, independent V2 schema-8 import, and exact Markdown/TXT export. Author-only chapter review stages exact saved prose and its earlier reviewed basis for explicit acceptance. Story memory provides explicit source-linked chapter digests and reuses current views in working discussions when full prose does not fit. Continuation offers explicit Working/Reviewed basis, restricted append-only proposals, editable paragraph previews, and atomic Apply/Reject. The schema-20 package adds an explicit **Author-reviewed snapshot** export basis, exact immutable review provenance, and a final freshness check after the native destination dialog. Schema 21 now adds optional passage-backed reviewed evidence with immutable record sets and audience-filtered delivery. C5 adds partial project-entity reuse, one-pass current-evidence freeze, authenticated object history, and schema-23 promise observations/history. App-local model preferences remain library schema 2.

C0–C2, parts of C3, the F2 review/context core, and C4-A/B/C development slices are implemented. Continuation is CI-qualified as a development slice with 36 strict native checks and one bounded live result. Reviewed export passes the integrated wrapper, local native diagnostic, and strict CI with all 38 native checks. The schema-21 reviewed-evidence package is implemented and passes the final local native diagnostic at 39/40 checks, omitting only the known local OS clipboard case; [CI 34012813796](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34012813796) passes Windows and Ubuntu contracts and all 40 strict native checks with zero errors. Full V3 remains unfinished: higher-level C4 digests, C5/C6, broader Apply, narrative evaluation, and broader provider/native/release qualification remain open.

The explicit Settings connection check validates the exact supported Codex executable and login availability. A live request freezes GPT-5.6-Luna/Max/Fast and the exact packet, runs in an owned Windows process, and saves validated output plus delivery, usage, cleanup, and outcome evidence. Other models/configurations stay unavailable; the local test model works offline. Application byte caps are not provider token limits. See [ADR 0011](ADR_0011_LIVE_CODEX.md) and [qualification](CODEX_QUALIFICATION.md); this is development integration, not full W8 acceptance.

### Current C5 promise history slice

[ADR 0021](ADR_0021_PROMISE_HISTORY.md) adds optional, exact passage-backed
promise observations to schema-23 review stages and ReadyBundles. Authors can
reuse one opaque promise identity across chapters, record setup, payoff,
cancellation, or uncertainty, and inspect an authenticated history in chapter
order. Each observation retains its quotation, source revision, short note,
timing, and author-room or reader-disclosed audience. History always reports
incomplete evidence; a missing payoff is never proof of resolution or absence.

Omitted promise fields inherit the full previous set after source validation.
An explicit empty array clears the set through a new immutable review; storage
uses a nullable empty pair while operation receipts preserve the explicit
request. Possession fields and legacy hashes remain unchanged. Promise-only
review changes advance the context epoch and fence dependent later reviews
without editing their prose. Copied and recovered projects retain historical
records with independent namespaces and no copied review authority.

Working author-room discussion packets and restricted reviewed continuation
share exact promise authentication. Restricted delivery removes author-room
record identities, labels, and notes before history or packet projection.
Source prose has its own information policy: making a record private does not
hide prose already permitted by the reviewed chapter. Available evidence and
records actually delivered remain separate in the context inspector.

The complete local wrapper passed **485 active Rust tests** (460 core and 25
desktop; one existing ignored crash-child fixture), **319 frontend tests in 25
files**, formatting, strict Clippy, TypeScript, and the production frontend
build. Evidence: `.local/promises-check.log`. The local native diagnostic passes
**44/45 checks** with zero errors, omitting only the previously documented local
OS clipboard case. It covers exact lost-acknowledgment retry, cross-chapter
identity reuse, history reads, restricted continuation, record-only fencing,
clearing/reopen, and copied authority. Evidence: `.local/promises-native.log`
and `.local/native-other-results/report.json`, dated
`2026-09-06T08:00:51.377Z`, on WebView2 `152.0.4191.62`. The qualified executable SHA-256 is `b615a6c888cd5e48085967f44db582604c07807522920615c7722870f7bc48e1`, built `2026-09-06T07:57:34.418Z`. Native screenshots were
inspected. Hosted qualification for this schema-23 checkpoint is pending.

The twelfth bounded live generation delivered one reviewed promise in a
Luna/Max/Fast author-room discussion. It quoted the promise and correctly
distinguished missing payoff evidence from proof that it never happened.
Prose stayed unchanged; no proposal or decision was created, and the completed
discussion survived reopening. The response could not name its chapter because
the packet supplied source identifiers and positions without its title.
The corrected packet-v2 format now supplies frozen Author Room source names,
including a promise chapter name when its full body is omitted. Restricted
packets omit these labels. Stored v1 packets keep their exact original
serialization, selection, and receipts; both versions retain the same validation
checks and unknown versions are refused. Regression tests cover old packet
budget selection, exact bytes and hashes, restart/backup, unknown versions,
and missing or stray title fields. Original live evidence is
retained in `.local/live-promise-initial-qualification/qualification.json`.

The thirteenth generation was one fresh explicit request against the corrected
packet. It named **The key and the promise**, quoted the promise, and correctly
kept missing payoff evidence distinct from proof of non-occurrence. It
completed with 4,333 confirmed stdin bytes and reported 1,917 input tokens,
426 output tokens, and 344 reasoning tokens. Effective model settings remained
unknown. It created no proposal/decision, preserved prose, and survived
reopening without another call. Evidence:
`.local/live-promise-named-qualification/qualification.json` and
`.local/promises-live-named.log`. The total is now thirteen live generations.
These are bounded integration/evidence-answer results, not a narrative-quality
or general provider qualification.

### Prior structured suggestions slice

ADR 0020 adds explicit **Selected paragraphs** and **Whole chapter** scopes with
typed rich blocks, editor-owned fresh IDs, a single editable rich preview, and
the existing atomic Apply/Reject protocol. Rust and JavaScript both validate the
complete prepared snapshot; surrounding blocks, marks, scene breaks, hard
breaks, and IDs remain protected. Whole-document endpoints are null in both
wire representations, and changing the prepared scope revokes brief approval.
Schema 22 preserves legacy schema-21 payloads, receipts, and decisions while
using unique `(run_id, ordinal)` identities; that checkpoint required reader 22. The current schema-23 promise slice raises the reader floor to 23. The
preview acknowledgment helper verifies the exact proposal, version, body hash,
and typed payload without changing the wire protocol. Preview, preparation,
Apply/Reject, and inspection make no model call; an explicit suggestion request
may call the selected provider. Applying a suggestion does not establish
narrative truth.

The local wrapper passed **467 active Rust tests** (442 core and 25 desktop,
one existing ignored fixture), **301 frontend tests in 24 files**, formatting,
Clippy with `-D warnings`, TypeScript, and Vite. Evidence is
`.local/structured-check.log`. The focused native helper passed both structured
journeys with no page errors. The full local WebView2 diagnostic passed **42 of
43 checks** with zero errors, omitting only the known local OS clipboard case;
evidence is `.local/structured-native.log` and
`.local/native-other-results/report.json`. The executable SHA-256 is
`ca114dc73db5470332fd7b797c56380eb5244e303fa3595ca78c782eac685b15`,
30,628,864 bytes, built `2026-09-06T06:40:00.255Z`. The bundle is 718.66 KB
JavaScript / 37.16 KB CSS with the existing Vite chunk warning. The bounded
live structured request also passed with one retained proposal, version,
decision, and provider result; its evidence is
`.local/live-structured-qualification/qualification.json` and
`.local/structured-live.log`. It used the same requested Luna/Max/priority
profile, with effective provider identity unreported, and brought the total
live generations to eleven. [CI 34017484597](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34017484597) passes Windows and Ubuntu contract jobs and all 43 strict native checks, including clipboard, with zero errors. Source is `c92df7708a4a48b83c11f25866c1f0a39d4a94b7`; the downloaded report is `.local/ci-34017484597/report.json`, dated `2026-09-06T07:01:18.297Z` on WebView2 `151.0.4129.101`. The two new hosted screenshots were visually inspected.

Two native failures found during qualification are retained in
`.local/structured-native-first-failure.json` and
`.local/structured-native-second-failure.json`. The fixes canonicalize scope
acknowledgments before comparison and read the local mock format from the
exact envelope scope. They do not relax structural or source validation.

The same clean source `c92df7708a4a48b83c11f25866c1f0a39d4a94b7` also passed [package run 34017567399](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34017567399) on Windows Server 2025 `10.0.26100` with WebView2 `151.0.4129.101`. It installed, opened the release Library without the debug trial, created and saved synthetic English prose, reopened it, closed normally, uninstalled in place, and reinstalled the same version with project/document/text retention. The result has no errors and no forced process stop. Completion was `2026-09-06T07:01:49.6493638Z`; installer SHA-256 is `5fa3af0e572c40c9321b9da15ec766d4d823fd46d770316bd285a3d4718f6947`. Downloaded evidence is `.local/ci-34017567399/build-metadata.json` and `.local/ci-34017567399/run-20260906-070045-364/result.json`; the retained reinstall screenshot was visually inspected. This is narrow installed-lifecycle evidence, not offline/no-runtime installation, an upgrade, installed live generation/structured editing, or full release acceptance.

### Prior C5-A evidence history slice

The prior C5-A slice adds one-pass batching of current selected review sets during Working author-room freezes and Restricted reviewed continuation, explicit project-wide entity reuse with first-chapter context, and authenticated object history over a frozen context. It preserves chapter and within-chapter record order, filters restricted records before labels or results, retains unknown holders and incomplete observations, and exposes no inferred current owner. It does not add a schema or paid model call. The local native diagnostic passes 40/41 checks with zero errors, omitting only the known local OS clipboard case. Its retained local log is `.local/evidence-history-native.log`; the executable SHA-256 is `7d59f072b578b2574734ddcc51eae4f28d037215c5439cf7a3c713dd9d8e662c`, 30,142,976 bytes, built `2026-09-06T05:38:17.9965544Z`. [CI 34014694823](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34014694823) passes both contract jobs and all 41 strict native checks with zero errors.

The complete local wrapper passed: **459 active Rust tests** (435 core and 24 desktop, one existing ignored fixture), **275 frontend tests in 22 files**, formatting, Clippy, TypeScript, and the production frontend build. Evidence: `.local/evidence-history-check.log`. The native build is recorded in `.local/evidence-history-native-build.log`; the bundle is 706.26 KB JavaScript / 35.37 KB CSS, with the existing Vite chunk warning. No additional live generation was run; the total remains ten.

Two final release samples of Working/AuthorRoom freeze returned all expected sources and records at every size. At 50, 100, and 200 synthetic chapters, freeze took 7.124-7.493, 15.213-15.304, and 43.027-44.063 ms, respectively; the prior implementation took about 61, 305, and 1,742 ms. This measures freeze only. Separate authenticated history measurements and the remaining scaling issue are described below. Full evidence and environment details are retained in `.local/reviewed-evidence-freeze-benchmark/result-v3.json`.

The source checkpoint `284625b6576540939f3dabb065f1e9320d0bb01e` passed [CI 34014694823](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34014694823): Windows and Ubuntu contracts, and all 41 Windows native checks with zero errors, including the clipboard and cross-chapter history journeys. Its retained report is `.local/ci-34014694823/report.json`, dated `2026-09-06T05:56:35.15Z`, on WebView2 `151.0.4129.101`.

### Prior reviewed-evidence checkpoint

[ADR 0018](ADR_0018_REVIEWED_STORY_EVIDENCE.md) extends the immutable F2 review boundary with optional passage-backed possession records. The author can select one text-bearing block, capture an exact quotation, choose an opaque project-local object and holder identity (or unknown), record passage timing, and explicitly choose author-room or reader audience. An omitted record array inherits the complete prior set after revalidation; an explicit empty array clears it through a new stage and bundle. Records are never edited beneath an existing stage, silently dropped when stale, or promoted to canon by context delivery.

Rust schema-21 persistence, canonical hashing, exact UTF-16 anchor validation, replacement/inheritance/clear semantics, historical authentication, frozen context provenance, reader-only restricted projection, bounded packet accounting, omission counts, and context inspection are implemented. The prior local wrapper (`.local/reviewed-evidence-check.log`) was green with **446 active Rust tests** (422 core and 24 desktop, one existing ignored fixture) and **271 frontend tests in 22 files**. The prior native evidence is retained in `.local/reviewed-evidence-native.log`; prior hosted evidence is `.local/ci-34012813796/report.json`. Review actions call no model and do not change prose. This checkpoint used the local test model; existing Codex requests can consume evidence through the shared packet path, but no new live generation was run. Total authorized live generations remains ten.

### Next implementation focus

The next context step is a narrow C6 Author Room discussion slice: explicit
opt-in, the initial invocation plus at most two further invocations, and Rust's
existing frozen `search`/`read` operations. A shared usage allowance and exact
child-packet/read receipts must be committed before submission. Every read and
invocation rechecks ownership, policy, Stop, and stale-basis rules; crashes and
lost acknowledgments must never automatically replay a model call.

The current Codex profile disables tools and exposes assistant text only.
A strict application-controlled lookup-response format therefore needs separate
mock and live qualification before the UI advertises lookup support. This is
not provider function calling. Existing one-invocation requests and historical
provider results must remain unchanged; restricted writing and state/knowledge
tools follow only after their disclosure-projected results are qualified.
Reuse discussion and packet ownership where possible, adding only the missing
read and invocation receipts. No paid autosave analysis, unlimited research,
silent source widening, or manuscript writes belong in this step. The design
is preparation for the next slice, not implemented C6 behavior.

Broader work remains: C5 relationship, knowledge/belief, rule, and multi-resolution
digest views; C6 bounded model lookups; arbitrary partial multi-block editing,
batch Apply, and manual rebinding; broader providers; native author trials; and
narrative-quality and release qualification. Generated chapter memory remains
an unreviewed navigation aid, and the full V3 goal remains open.

The exact bundle/revision cache follow-up is measured in
`.local/reviewed-evidence-freeze-benchmark/review-validation-cache-result.json`.
Across two samples, authenticated history was 5.635/5.672 ms at 50 chapters,
16.129/15.976 ms at 100, and 65.550/62.221 ms at 200, compared with the prior
42.469/42.058, 225.729/229.732, and 1,486.716/1,437.192 ms measurements.
The cache preserves bundle and revision authentication and does not establish
whole-request latency or large-book readiness. Measure full requests as richer
C5 views are introduced before choosing further validation optimizations.

### Prior author-reviewed export checkpoint

[ADR 0017](ADR_0017_REVIEWED_EXPORT.md) is implemented across core, schema 20, native IPC, and the existing export dialog. The chapter-only basis choice requires an exact current author-reviewed bundle; a reviewed first chapter is valid with an empty earlier prefix. Reviewed preparation creates no checkpoint or model call. After destination selection, the project actor rechecks the selected bundle, exact source, earlier review basis, policy, and session before installing a new file. Historical records remain verifiable after later edits or review replacement. Copies and recovered projects retain history without inheriting review authority. Existing working-preview serialization, projections, and output records retain their legacy representation.

The local WebView2 diagnostic passes **37 of 38 checks with zero errors**, omitting only the known local OS clipboard case. New visible journeys cover author review, exact Markdown/TXT preview and native Save, bound records, no reviewed checkpoint, copied-authority refusal, retained output after a forced record failure, and refusal after a background save made while the real destination dialog was open. Reopening verifies historical records and changed prose. The root inspected the actual Markdown preview and stale-refusal screenshots. Evidence: `.local/reviewed-export-native.log` and the retained hosted report `.local/ci-34010306332/report.json` (`2026-09-06T04:10:43.182Z`, WebView2 `151.0.4129.101`), executable SHA `456a0cf1c327c330c0f56e4e472630e5dfeaf2976cb969cc17f052696208492d`, 29,184,000 bytes, built `2026-09-06T03:51:09Z`. The bundle is 690.55 KB JavaScript / 33.29 KB CSS with the existing Vite chunk warning.

The first native attempt stopped because the Save helper incorrectly expected a file for the intentionally refused stale export. The helper now explicitly allows that negative test to wait for the UI refusal and verify file absence; successful Save cases still require file creation. The failed attempt is preserved at `.local/reviewed-export-first-native-failure.json`. No production change or additional live request was needed. Total authorized live generations remains ten.

The full `scripts/desktop.ps1 -Command check` wrapper passed: rustfmt, workspace Clippy with `-D warnings`, **435 active Rust tests** (411 core and 24 desktop; one existing ignored fixture), TypeScript/Vite, and **256 frontend tests in 21 files**. Evidence is `.local/reviewed-export-check.log`. The seven new export tests cover exact projection, reviewed eligibility, final staleness, replay, historical recovery, record failure, and truthful schema-19 archive migration. An older synthetic schema-18 continuation archive fixture needed the schema-20 column removed before downgrade; that test-only correction is included. [CI 34010306332](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34010306332) passes Windows/Ubuntu contracts and all 38 strict native checks, including clipboard, with zero errors. Downloaded evidence is `.local/ci-34010306332/report.json`, dated `2026-09-06T04:10:43.182Z`, WebView2 `151.0.4129.101`. Prior continuation evidence below remains historical.

### Prior story-continuation checkpoint

The current local slice lets an author choose Working draft or Reviewed story, request one typed append-only candidate for the exact target, edit its paragraphs, and Apply or Reject it through the existing atomic operation. Schema 19 retains the selected basis, continuation kind, prepared paragraphs, operation identity, generated IDs, and exact prepared body so an uncertain acknowledgment can be reconciled without regenerating prose or IDs.

The full `scripts/desktop.ps1` check passed and is recorded in `.local/continuation-check.log`: 428 active Rust tests (404 core and 24 desktop; one existing fixture remains ignored), 252 frontend tests in 21 files, rustfmt, workspace Clippy with `-D warnings`, TypeScript, and Vite. The final native build contains 688.70 KB JavaScript and 32.98 KB CSS with the existing Vite chunk warning. The rebuilt local WebView2 diagnostic passes 35 of 36 checks with zero errors; only the known local OS clipboard check is omitted. The continuation journey covers refusal of Reviewed without an eligible prefix followed by explicit Working selection, exact lost-preview-ack retry with one stored version/receipt, Apply in the same editor with preserved prefix IDs, visible Undo/Redo, and reopened history. The retained local evidence is `.local/continuation-native.log`; the hosted evidence is `.local/ci-34008911179/report.json`. The bounded live result used the earlier `cbb036fc…` executable and is recorded separately in [Codex qualification](CODEX_QUALIFICATION.md). [CI 34008911179](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34008911179) passed both contract jobs and all 36 strict native checks, including the full clipboard check, with zero errors. These development checks do not establish full provider or release qualification.

## Repository and CI evidence

- Evidence-history checkpoint `284625b6576540939f3dabb065f1e9320d0bb01e` is pushed and passed [CI 34014694823](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34014694823): both contract jobs and all 41 strict native checks, with zero errors. Downloaded evidence is `.local/ci-34014694823/report.json`.

- Reviewed-evidence checkpoint `a8d73c36a3989e390682cf855f6eea071e58c91b` is pushed and passed [CI 34012813796](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34012813796): both contract jobs and all 40 strict native checks, with zero native errors. Downloaded evidence is `.local/ci-34012813796/report.json`.
- Reviewed-export checkpoint `a19e7bf7d30746cc02cd2769e6295779a4bcbb4d` is pushed. [CI 34010306332](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34010306332) passed both contract jobs and all 38 strict native checks with zero errors.
- Story-continuation checkpoint `430831fc133dbe37be47fa477d7c3ea3b312e505` is pushed and passed [CI 34008911179](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34008911179): Windows contracts, Ubuntu contracts, and all **36 strict native checks**, with zero errors. Native report `2026-09-06T03:33:50.416Z`, WebView2 `151.0.4129.101`; downloaded evidence is `.local/ci-34008911179/report.json`. This includes the complete clipboard check and continuation preview retry/Apply/Undo/Redo/reopen journey.

- C4-C checkpoint `7c4599c92bdeb970ca847115553905fba999d39a` is pushed and passed [CI 34006858440](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34006858440): Windows contracts, Ubuntu contracts, and all **35 strict native checks**, with no errors. Native report `2026-09-06T02:44:27.539Z`, WebView2 `151.0.4129.101`. This qualifies chapter-only freshness, original generation provenance, schema-18 migration, and native reuse after unrelated writing while old requests remain stale. Broader V3 and release gates remain open.
- C4-B checkpoint `bd36e036eb3e8575cb856bf88972120e2bab88da` is pushed and passed [CI 34005563244](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34005563244): Windows contracts, Ubuntu contracts, and all **35 strict native checks**, with no errors. Native report `2026-09-06T02:16:39.763Z`, WebView2 `151.0.4129.101`. This includes the corrected source-confirmation wait, clipboard handling, and automatic chapter-memory reuse with immutable evidence and historical retention. It qualifies this development checkpoint; chapter-only freshness, richer memory, and broader provider/native/release gates remain separate.
- C4-B checkpoint `4c5ebbd24b31e443829591cede745556a7a6b451` is pushed. [CI 34004973069](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34004973069) passed both Windows and Ubuntu contracts. Its native job passed desktop Rust tests and compilation, then failed an existing source-confirmation assertion before reaching C4-B. The harness read a controlled chooser immediately after an inspector click, before its React adoption effect committed; the failure screenshot already showed the correct choice. The corrected harness waits for that exact choice before asserting and preserves the check that opening confirmation does not save a pin. Independent review confirmed the race. A local rerun using the same executable passed all 34 diagnostic checks at `2026-09-06T02:04:26.371Z`, with only clipboard omitted and no errors. The green `bd36e03` rerun above supersedes this failed native attempt.
- Chapter-memory checkpoint `7a271352cc95754866516735bf0438799f31d02d` is pushed and passed [CI 34003001409](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/34003001409): Ubuntu contracts, Windows contracts, and all **34 strict native checks**, with no errors. Native report `2026-09-06T01:14:14.762Z`, WebView2 `151.0.4129.101`. This includes the three C4-A native flows and the full clipboard check. It qualifies this development checkpoint; installed-release, broader provider, and narrative-quality gates remain separate.
- Reviewed-context checkpoint `6618f6e9f6954e2fac5afc799c79b37ce56dcdf6` is pushed and passed [CI 33999551379](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33999551379): Ubuntu contracts, Windows contracts, and all **31 strict native checks**, with no errors. Native report `2026-09-05T23:55:38.385Z`, WebView2 `151.0.4129.101`. This qualifies the exact reviewed-source preparation checkpoint; it predates C4 chapter-memory implementation.
- Author-review checkpoint `c3f0f83d85bbb8e00caf58d46eb3787f0739197f` is pushed and passed [CI 33998398286](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33998398286): Ubuntu contracts, Windows contracts, and all **30 strict native checks**, with no errors. Native report `2026-09-05T23:27:34.812Z`, WebView2 `151.0.4191.62`. This includes author-only review, resumption, reconciliation, and the changed-earlier journey. This run predates the F2-B checkpoint above.
- Historical live-provider/import checkpoint `568959740045bfbb9202c4370f4e3d26db360c11` is pushed. [CI 33997192681](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33997192681) passed the Windows-native job and all 28 strict native checks. Ubuntu Clippy failed on an unnecessary `return` in the non-Windows import-list branch; the sibling Windows contract job was cancelled by the matrix. The return was corrected locally afterward, but this partial run is not a green cross-platform result and is superseded in status by the later c3f0f83 author-review checkpoint.
- The same source passed [installed package run 33997290012](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33997290012), completed `2026-09-05T23:06:52.6074002Z`, with installer SHA `78be293c04bfcd0b174f9ca2a54f9e0dbc2fba1c6cccb17c91860d3a98ee3639`. Windows Server 2025/WebView2 `151.0.4129.101` passed install, write/reopen, normal close, in-place uninstall, and same-version reinstall retaining prose. No errors or forced stop were reported; broader package gates remain open.
- Historical committed baseline `be1d93c579f50dcabf78b3151e4dd299b3c95d45` passed [CI 33994609086](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33994609086): Windows/Ubuntu contracts and all 28 strict native checks, WebView2 `151.0.4129.101`, report `2026-09-05T22:07:38.437Z`, no errors. The same source passed [package run 33994616334](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33994616334): install, create/write/reopen, normal close, in-place uninstall, same-version reinstall, and retained prose. Installer SHA `dec8ec5393c4453fd43a6f4ff36957f58eb32d45d08fe1f7f0068d4af02ce918`. These runs predate live-provider/import integration. Broader package gates remain open.
- The private repository is [FZWINGEL/WebnovelStudio_V3](https://github.com/FZWINGEL/WebnovelStudio_V3). Its default branch is `main`, whose current tip is `d0eebfd780e435c068ef1017cac580786360d36b`.
- C3 saved-source checkpoint `16bf8bca2448cbf767886c38c6cc54d9e35c3131` passed [CI run 33990073110](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33990073110), including both contract platforms and all 25 strict native checks. A later run of the same app source, [33990403189](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33990403189) for the package-harness-only `1e6e556`, passed both contract jobs but failed the duplicate/restart prose assertion. Its harness killed the app after clicking All projects without waiting for asynchronous navigation/flush to finish. The corrected harness waits for the Library heading before killing the process. This is a supported race diagnosis, not a claim that the failed run passed. The corrected wait and writing-brief flow subsequently passed all 26 strict native checks in run 33991463598 for `508aee194fafa44a76afcd2885d42559d9d77853`.
- C2 implementation checkpoint `cd1d8a68ef3e55733a6252f8625da6812819a639` contains the C0–C2 implementation and the context inspector/source-pin preparation. Its [CI run 33975322590](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33975322590) completed successfully across Windows native, Windows, and Ubuntu contract jobs.
- W4 checkpoint `e92eef8a212312972609057c61d6667db8d27c5f` introduced persistent discussion. W5 checkpoint `a2a01632890a44efbe84bc526674fc9bf06d3d94` is pushed; [CI run 33981203728](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33981203728) passed Ubuntu and Windows contracts plus the strict 21-check Windows-native flow. Earlier `9161fc8` and [run 33979310784](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33979310784) remain historical native19 evidence. W6 `1a6beb76456c49f1da559d6dd7325dbb6d508e44` is pushed; [run 33982596342](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33982596342) passed both contract jobs and the native build, then failed waiting for keyboard Redo.
- W7 `78a9fde847fb5d25375603d055a997d3e43ef3ab` is pushed. [Run 33986475862](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33986475862) passed Windows/Ubuntu contracts, workspace Clippy/tests, and the native build. On WebView2 `151.0.4129.101`, the native flow passed all 22 checks through history restore, including delayed caret-save acknowledgment and keyboard Redo. It failed at export because the native dialog saved to its default `Chapter draft.md`, despite the helper reading back its requested filename. The failure capture showed the successful-export presentation, but did not qualify the chosen path. The subsequent correction passed as recorded below.
- [CI run 33988660050](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33988660050) for `78367cd` passed Windows/Ubuntu contracts and the strict 24-check native flow on WebView2 `151.0.4129.101`, including clipboard, history recovery, exact chosen-path export, and focus restoration. This is the completed W7 development qualification checkpoint, separate from the newer C3 changes and installed-package lifecycle.
- Standard run [33993367813](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33993367813) for `cd7fb77` passed the Windows and Ubuntu contract jobs. Its Windows-native job failed in the descendant process-cleanup regression before reaching the UI flow; the focused Windows process suite now passes 20/20 locally after the bounded cleanup fix. This is not a new native UI pass.
- Package run [33993370498](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33993370498) for `cd7fb77` passed the narrow installed lifecycle: create, write, reopen, normal close, in-place uninstall, reinstall at the same version, and text retention. The source SHA was `cd7fb77f0bac8bc2ce1756f28044d406c356ff20`, installer SHA `9dcd7fc46dc8f259733d8487e11992b5c5b07d99dddd4a14f71c21baba8fd8ae`, and WebView2 was `151.0.4129.101`. Offline installation, upgrades, and broader W7 qualification remain open.
- Historical [CI run 33973213684](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33973213684) for `73db1bc` passed all three jobs. [CI run 33973390433](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33973390433) for `6456d33` exposed the native duplicate-edit failure; its ProseMirror transaction wait fix is included in pushed `c2a5262` and subsequently passed; the newer W6 failure is recorded above.
- The previous [CI run 33971040177](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33971040177), for `b8d4cb8`, passed both contract jobs, Windows Clippy/tests, and native build; native smoke reached the UI but timed out on a CSS-hidden diagnostic label. The committed fix waits for attachment and checks the actual runtime through IPC.
- The historical [W0 qualification record](W0_QUALIFICATION.md) remains the record for that spike. Current implementation status and remaining gates are maintained here.

## Current work

| Area | Status | Evidence or boundary | Remaining |
| --- | --- | --- | --- |
| W0 native editor | Partial baseline retained | Explicit sample editor trial in the real Tauri/WebView2 window, Rust snapshot validation over IPC, shared fixtures, session-only feedback/replacement | English native author trial, minimum-window behavior, external Word paste, screen-reader use, native backup/export dialog journey, and broader qualification |
| W1 structural scope | Implemented; P foundation covered | Shared JS/Rust snapshot/scope fixtures, exact structural validator, mutation/Unicode/size refusal tests, and local/CI contract checks | Requalify extensions when new editor nodes or grants are added; W0/release trials remain separate |
| W2 project/session/save | In progress | Core/frontend sessions, receipts/reconciliation, default persistent UI wiring, stable create-retry operation/document/block IDs, empty-project `reconcileProject`, mounted-editor lease adoption, and generic lost-acknowledgment tests are implemented | Complete file-backed persistence/reconciliation evidence and the broader author-trial integration; no full W2/V3 completion claim |
| W3 library/transfer | Implemented development surface | Registry, rename/archive/duplicate, isolated recovered projects, migration backups, persistent library/workspace, and schema-16 backup validation | Complete the A author trial and broader recovery/native-dialog evidence |
| W4 persistent discussions | Active local slice | Rust-owned threads/messages/runs/drafts, selected scope grants, frozen packet receipts, deterministic mock output, durable sequence/terminal events, queued Stop sealing, running Stop intent with Rust cleanup settlement, retained partial messages, retry/reload recovery, and native FeedbackPanel/ContextInspector integration | Finish malformed-output/cancellation/error/UI coverage, then durable Apply and broader W4 evidence |
| C2 context packet compiler | Implemented | Exact target/instruction/scope/mandatory sources; full eligible text or explicit whole-block packing; durable exact messages/options/hashes; live application byte allowance separate from Mock accounting | Qualified model token accounting, richer relevance, and broader context evaluation |
| C3 author guidance | Partial local implementation | Chat or direct entry opens an editable author-confirmed instruction; immutable exact versions support Next request, This document, and This project scopes, with CAS/idempotent mutation receipts, source-epoch invalidation, recovery retention/fencing, exact mandatory AuthorRoom packet binding, separate guidance handles, and inspector display. Restricted writing excludes all current guidance; request use is consumed only with a successful persisted discussion start. Unchanged unsuccessful retries retain original active one-use guidance and durable composer mode. Schema-10 persistent AuthorRoom sources have exact required-source receipts, recovery retention, and uncertain-save controls. Schema 11 retains optional writing briefs with explicit approval and exact restricted-packet binding | Complete richer conversation selection and broader Apply integration; keep qualification evidence current |
| W5 proposal review and Apply | Implemented selected-passage slice; pushed and CI-covered | Schema-8 proposals, explicit intent, restricted context, immutable prepared versions/decisions, exact structural validation, atomic Apply/Reject, stale/replay fences, and mounted-editor handoff | Whole-chapter/block/manual-rebind work, broader B trial, and separately owned F5 batch Apply |
| W6 saved versions and restore | Implemented development slice | Bounded metadata paging, exact inert comparison, atomic whole-document restore, shared Apply/restore reconciliation, before/after retention, process-interruption and rollback evidence | Remaining lifecycle/renderer-loss combinations and B trial |
| W7 exports/package | Development slice and narrow installed lifecycle passed | Working and author-reviewed Markdown/TXT preview, exact native Save, immutable export records, reviewed freshness/record-failure native checks, stable release data, installed lifecycle CI33994616334 | Offline/no-runtime installation, true upgrade, physical/assistive native trials, full N gates |
| W8 bounded Codex | Integrated development path | Explicit exact-binary connection check, immutable packet/model, Job-owned streaming, bounded output, durable provider results, Stop and explicit local save retry | Broader refusal/truncation/auth/cleanup matrix, isolation review, qualified token limits, complete W8/E3 |
| F1 V2 import | Implemented schema-8 development slice | Explicit working-body choices, independent staged installation, inert history, Library Check import, source-free receipt recovery, full pre-move validation, and seven-check synthetic native import journey | Broader pending-import native recovery and representative author-approved acceptance |
| F2 author review | Prose review, reviewed-context core, continuation, and schema-21 possession evidence CI-qualified as a development slice | Exact stages/revisions/earlier prefix, explicit selected bundles, immutable reader-position pins, current validity, saved-stage resumption, independent recovered history, core/IPC freeze of reviewed prefix plus current target, schema-19 Working/Reviewed append preview with atomic Apply/Reject, and the first typed passage-backed record set with audience-filtered delivery | Broader records/summaries/exceptions, provider qualification, and full F2 qualification |
| C4-A chapter memory | Implemented development slice; CI-covered | Explicit Refresh for one full current chapter; schema 16 job/result/view records; strict evidence validation; recovery/cleanup regressions; native CI and one live Codex memory request | C4-B extends packet integration; broader provider/native qualification and narrative quality remain open |
| C4-B navigation context | Implemented development slice; CI-covered | Automatic current chapter-view selection, schema 17 immutable pins, exact generated coverage, full-text preference, mandatory-source protection, stale/copy/policy fences, and native evidence inspection | C4-C extends freshness; higher-level digests, restricted/reviewed integration, temporal state, lookup loop, and narrative quality remain open |
| C4-C chapter freshness | Implemented development slice; CI-covered | Exact closed chapter dependencies preserve reuse after unrelated edits; original generation epochs and historical bytes remain intact; schema 18 reader floor | Richer memory and narrative qualification remain open |
| GitHub/CI | Reviewed-export checkpoint passed | `a19e7bf` passed CI 34010306332 with Windows/Ubuntu contracts and all 38 strict native checks | Requalify subsequent source changes; no full V3/release completion claim |

## Current local evidence

The C4-C integrated wrapper passed rustfmt, workspace Clippy with `-D warnings`, **407 active Rust tests** (384 core and 23 desktop; one existing ignored subprocess fixture), TypeScript/Vite, and **228 frontend tests** in 20 files. The final focused targets passed 16 memory storage, eight navigation storage, ten navigation packet, and ten migration tests. Regressions cover earlier generation provenance, malformed/future epochs, late completion after unrelated writing, immutable coarse-stale rows, reopening and reusing the exact view, ordinary discussion staleness, source edits, and cross-epoch backup/recovered-copy isolation. Schema-17 upgrade preserves nonempty generated snapshots, pins, candidate JSON, request bytes, and manuscript heads, with an unchanged pre-upgrade backup. Independent storage review found no actionable defects. The bundle remains 681.42 KB JavaScript and 32.48 KB CSS, with the existing Vite size warning. No additional live model requests were made.

The C4-C rebuilt Tauri app passed **34 of 35 native diagnostic checks** at `2026-09-06T02:34:02.347Z`, WebView2 `152.0.4191.62`, with no errors. Only the previously documented local OS clipboard check is omitted; the strict CI harness retains all 35. The extended navigation flow starts a fresh local mock discussion after an unrelated chapter edit, confirms the previous packet is stale, reuses the same generated view and exact source with its original generation epoch, and confirms one total memory job. It then edits that view's source chapter and verifies stale-view exclusion, historical retention, unchanged target prose, and reload behavior. The native mock response is awaited to completion before the source edit. Executable SHA `3bfbcb23a95d46051bb0f1903bc4d9c85b4dc3165d78eab1bb616e6c4449738e`, 28,851,712 bytes, built `2026-09-06T02:33:10Z`. The refreshed context screenshot was inspected. This is development evidence; the strict CI result is recorded above, and broader provider/native/release qualification remains separate.

### Earlier C4-B checkpoint

The C4-B final wrapper passed rustfmt, workspace Clippy with `-D warnings`, **403 active Rust tests** (380 core and 23 desktop; one existing ignored subprocess fixture), TypeScript/Vite, and **228 frontend tests** in 20 files. Eight navigation packet tests cover full-text byte/hash preservation, exact mandatory sources, whole-view budget growth, typed omissions, and dependency/quotation tampering. Eight storage tests cover immutable pins, historical retention, changed unrelated evidence, policy revocation, recovered-copy isolation, recursion, and missing schema-17 tables even with no snapshots. The nine migration tests include schema-16 byte-preserving upgrade and its durable original backup. Seventeen inspector tests include policy changes during an evidence read/search and late results after a project switch. The bundle is 681.42 KB JavaScript and 32.48 KB CSS, with the existing Vite size warning. No additional live model requests were made.

The final rebuilt Tauri executable passed **34 of 35 native diagnostic checks** at `2026-09-06T01:49:52.355Z`, WebView2 `152.0.4191.62`, with no errors. Only the documented local OS clipboard check was omitted. The new ordinary-discussion flow uses a synthetic story larger than the input allowance, supplies one pre-existing generated view, expands its evidence quotation, opens the complete original chapter, preserves the mounted manuscript, and retains the exact old packet after a source edit and renderer reload. The next freeze excludes the stale view; the project-wide job count remains one. Executable SHA `0c5d6be8bd6cff7a33d25be6cfc8b3e15e8f4cdd407c92d67be3c3173c4044db`, 28,847,616 bytes, built `2026-09-06T01:48:47.3850312Z`. The generated-summary native capture passed independent visual review. This proves development integration, not narrative understanding, full native accessibility, or release qualification.

### Earlier chapter-memory checkpoint

The C4-A wrapper passed rustfmt, workspace Clippy with `-D warnings`, and **386 active Rust tests** (363 core and 23 desktop; one existing ignored subprocess fixture). After the final connection-recovery UI correction, TypeScript/Vite and all **220 frontend tests** passed. Focused memory coverage includes 9 pure response-contract tests, 5 packet tests, 15 storage tests, and 14 controller/panel tests. The desktop suite covers lost durable claims, retained terminal and install faults, Stop, permission revocation, and explicit registry-close/reopen retention as interrupted history. A fenced background commit exposes a local check that renews the document lease before retrying the retained result. Recovered-source UI coverage uses the original project identity and validates the exact retained source without granting operation authority. The frontend bundle is 679.07 KB JavaScript and 32.48 KB CSS, with the existing Vite size warning.

The final rebuilt Tauri executable passed **33 of 34 native diagnostic checks** at `2026-09-06T01:01:50.070Z`, WebView2 `152.0.4191.62`, with no errors. Only the documented local OS clipboard check was omitted; the strict tracked harness retains all 34. The three memory flows prove an explicit single refresh with a lost acknowledgment, unchanged editor/prose and no autosave generation, retained stale evidence, local terminal-save retry after navigation/reload, exact source lookup, and policy revocation. Executable SHA `abde8c8a1a9be9d90a711398fc146fdbdbae7c24c06d038db8ad140653f4d9b5`, 28,552,704 bytes, built `2026-09-06T01:01:05.5466457Z`. The chapter-memory screenshot was inspected. This is native development evidence; live-provider, strict CI, and installed-release qualification remain separate.

The ninth live Codex generation produced three evidence-linked chapter-memory items and one installed view with settled cleanup. Its frozen input was 3019 bytes; reported usage was 1257 input, 1931 output, and 1552 reasoning-output tokens. A Windows path-check error stopped the initial harness after generation; a same-data continuation verified the retained result, exact source, and unchanged prose with zero further requests. This is one narrow live-memory result, not narrative-quality qualification. See [Codex qualification](CODEX_QUALIFICATION.md#ninth-dispatch-native-chapter-memory).

### Earlier reviewed-context checkpoint

The reviewed-context wrapper passed rustfmt, workspace Clippy with `-D warnings`, **349 active Rust tests** (334 core and 15 desktop; one ignored subprocess fixture), TypeScript/Vite, and **206 frontend tests**. The seven reviewed-context tests cover exact prefix/replay, restricted boundaries, tied chapter positions, optional packing, historical reader-position integrity, missing-pin corruption, and schema-14 archive recovery. Five additional adversarial tests cover manifest substitution, ancestry, namespaces, stale history, and recovered-copy refusal. A schema-14 opening regression preserves old snapshot JSON and selected author reviews. Strengthened archive assertions then passed the seven-test target plus Clippy and formatting: recovery retains the exact old snapshot JSON, adds nullable pins, upgrades the independent copy, and leaves the source archive unchanged. The frontend bundle is 658.20 KB JavaScript and 28.84 KB CSS, with the existing Vite size warning. No model was called for this work.

The final rebuilt Tauri executable passed **30 of 31 native diagnostic checks** at `2026-09-05T23:45:44.310Z`, WebView2 `152.0.4191.62`, with no errors. The only omitted check is the documented local OS clipboard case; all 31 remain in the strict CI harness. The new check uses actual IPC and SQLite to freeze an exact reviewed prefix plus working target, exclude later/private material, prepare a local packet, retain stale historical evidence, refuse a newly invalid basis, and enforce policy revocation. It makes no provider request and does not qualify a continuation UI. Executable SHA `1791cf2a1ac8d8ba774b4dcf041bea2a3f3ab16db953f59d0f2c48c6e13ed37f`, 27,228,160 bytes, built `2026-09-05T23:44:13.1373407Z`. The refreshed changed-earlier screenshot was inspected; later prose remains present and editable.

### Earlier author-review checkpoint

The author-review wrapper passed rustfmt, workspace Clippy with `-D warnings`, TypeScript/Vite, and **205 frontend tests**. After four additional review regressions, the final workspace Rust run passed **336 active tests** (321 core and 15 desktop; one ignored subprocess fixture). The eleven review core tests cover exact source and prefix validity, stale activation, policy/order changes, replay after lease rotation, independent recovery, corrupt historical references, and refusal of unimplemented reviewed context. Eleven review UI tests cover explicit acceptance, stale/uncertain outcomes, retained editing, restart resumption, exact earlier prose, and callback ownership. The bundle is 657.79 KB JavaScript and 28.84 KB CSS, with the existing Vite size warning.

The rebuilt native app passed **29 of 30 diagnostic checks** at `2026-09-05T23:19:20.734Z` on actual Tauri/WebView2 `152.0.4191.62`, with no errors. Only the known local OS clipboard check was omitted; the tracked strict harness retains all 30 checks for CI. Executable SHA `921dbed4621f88e1d1ab40ac476755d710f1c0b86c3fd125c084eb60124f649b`, 27,053,568 bytes, built `2026-09-05T23:14:17Z`. The new flow previews and resumes an unaccepted exact review after restart, reconciles a real committed Mark whose acknowledgment was dropped, reads the exact earlier revision, and preserves later prose plus its changed-earlier status across reopen. Both review screenshots were inspected. No live model was called.

The initial review diagnostic stopped after 28 checks because it filled the old editor before the new chapter mounted. Read-only inspection of the synthetic database confirmed that the old chapter had received the new text, its accepted bundle was intact, and the new chapter was empty. The corrected harness waits for the requested chapter heading and for Library navigation before reload; the success above follows that correction. No product code changed for this harness failure.

The [V2 importer](V2_IMPORT_PREVIEW.md) has Windows read-only preview/list, schema-8 ownership validation, stable owned source copies, explicit missing-prose choices, staged independent installation, inert legacy retention, and immutable operation replay. Fifteen import tests cover source deletion, Library restart, an open target with a running discussion, hash tampering, backup/recovery, exact JSON choice binding, and rejecting a corrupt staged identity before moving it. Four dialog tests cover review/choice and uncertain-result retry/close. Library **Check import** reconstructs the recorded choices and reconciles the original operation; it never opens a new source picker or invents a new import identity. Reconciliation validates storage within one read transaction and preserves current legitimate edits.

Before C4-A, eight real Codex requests had been recorded. The seventh native request completed with ordinary prose, correctly yielding no proposal and no Apply. That exposed the missing response-format instruction. The eighth request used the fixed frozen `proposal-output.v1` contract and retained one suggestion (`short promise` → `solemn vow`). Explicit native Apply preserved the protected ending. A harness selector ambiguity stopped the first run after Apply; a read-only continuation reopened the same data and verified persisted prose, requested traits, and Used context without another generation. Total dispatches for that flow remained one. Its exact executable SHA was `529e3ff58bb6a23cbfe5123ac95eed447160958455022a3a29b056b274210155`, built `2026-09-05T22:37:06.694Z`. The receipt reports 3360 stdin bytes, 1375 input tokens, 188 output tokens, 144 reasoning tokens, settled cleanup, and no echoed effective identity. No page errors or owned process remained. This qualifies that narrow native live-edit development flow, not all W8 behavior or narrative quality. See [Codex qualification](CODEX_QUALIFICATION.md) for all experiments and limitations.

### Earlier checkpoint evidence

The final native V2 import journey passed seven checks at `2026-09-05T22:52:29.674Z`: fresh Tauri library, the owned native source chooser, explicit same-chapter draft choice, readable/editable imported narrative note, independent V3 identity/namespace, byte-for-byte unchanged source, and reopened chapter prose. The executable SHA was `9c13a273210ab0fc890645f176cf8b8c6c8bd4b3cd8808f48bc47479862c6d1f` (26,400,256 bytes, built `2026-09-05T22:47:52Z`). Earlier attempts exposed only chooser automation and accessible-name selector mismatches; the corrected ignored harness passed. Review/open/reopen screenshots were inspected. This uses a synthetic schema-8 fixture and does not qualify author databases, other schemas/platforms, the installer, or every pending-import native recovery case.

At `2026-09-05T22:45:52.293Z`, the native diagnostic passed 27 of 28 checks on WebView2 `152.0.4191.62`, with no errors. It used the same `529e3ff5…` development executable as live request eight and covered the common writing, project, model-preference, context, guidance, Apply/history, and export flows. It omitted the known local OS clipboard check; the strict tracked harness retains that check for CI. This build predates the final Library import-recovery addition. The diagnostic uses only the local test model; the separate live flow is recorded above.

The earlier saved-source C3 wrapper check passed rustfmt, workspace Clippy with `-D warnings`, **230 active Rust tests** (229 core and one desktop; one ignored subprocess entry), TypeScript/Vite, and **162 frontend tests** on Windows 11 Pro `10.0.26200`. The bundle is 623.41 KB and retains Vite's size warning. Source-choice tests cover scope/CAS/replay, recovery, unavailable sources, mandatory overflow, stale packets, retry identity, restricted exclusion, uncertain acknowledgments, current-list refresh, keyboard focus, and late owners. A legacy receipt regression preserves exact packet input after adding the optional required-source annotation. The earlier release compile passed; final C3 installed-release qualification remains separate. Final receipt-validation guards passed the 12-test source-pin target, the 24-test transfer target, the 10-test context-packet target, and workspace Clippy. These add one regression after the full wrapper run, for 231 active Rust tests across the executed targets. The final embedded development build again passed the 24-check native diagnostic with clipboard omitted.

The new [Windows process contract](WINDOWS_PROCESS_CONTRACT.md) distinguishes explicit `finish_or_stop` cleanup from best-effort Drop. The incremental observer delivers only accepted bounded stream prefixes, including the normal final drain, and tests retain live child/grandchild handles, verify Job accounting reaches zero, reject incomplete zero-exit input delivery, and prove a previously requested Stop sends no packet bytes. Pending overlapped I/O retains stable owned storage; unresolved cancellation can retain that bounded allocation and detach readers rather than claim successful cleanup. The focused Windows process suite now passes 20/20 locally after the bounded descendant-cleanup fix; the standard CI failure above remains the historical pre-fix result. The pure Claude and Codex parsers reject malformed/unknown/tool events, retain validated partial assistant text, and strip upstream diagnostic details. Their usage tests cover signed-negative conversion and bounded counters. These parser/process checks alone do not qualify the later live integration or upstream isolation/billing guarantees.

The Rust Stop lifecycle now seals a queued discussion immediately, records a durable stopping intent for a running discussion, and lets the worker settle cleanup as stopped or interrupted. Retained partial output is included in the inspectable terminal message; failed claim/completion/Stop local writes remain owner-keyed app-memory state with a visible “Retry saving response” action, current-lease/reconciliation fencing, and no generation replay. This local contract is explained in [ADR 0009](ADR_0009_DISCUSSION_RECOVERY.md).

Earlier runs exposed an Ubuntu test synchronization issue, unsupported UIA focus, a native-dialog default-filename error, and return-focus loss. Their corrections are covered by the successful strict run 33988660050. Run 33988179660 passed contracts but was canceled as superseded before native completion; its cancellation is not a functional failure.

The earlier full W7 wrapper check, before the ordinary-period export correction and W8 process work, passed rustfmt, workspace Clippy with `-D warnings`, **186 active Rust tests** (185 core and one desktop; one ignored subprocess target), TypeScript/Vite, and **154 frontend tests**. That historical bundle was 615.31 KB.

W7 adds thirteen core transfer regressions, including exact projection/bytes, source tampering, frozen older revisions, no overwrite, duplicate/concurrent finalization, post-install begin/insert failure, immutable-record recovery, schema-8 migration, and Markdown punctuation/URL/indentation. The final transfer target passed all 24 tests after ordinary-period and Windows-basename fixes; the old direct TXT export IPC/core path was removed. Ten ExportDialog tests cover ownership, corrupt preview, explicit save/cancel, possible writes, existing destinations, and late results. Six background-caret tests keep typing available during delayed acknowledgments, defer stale views, fence cleanup, and make later lifecycle operations wait before flushing current writing. The release-profile Windows x64 binary also compiled successfully; the installer and native journey remain separately tracked below.

The unsigned Windows x64 NSIS package builds with the bundled offline WebView2 installer and stable release data path. Hosted package run 33990404236 built and installed `1e6e556`, opened the release Library, created synthetic prose, and read it back, then failed before returning to Library. Its owned-window capture showed an uncertain save. Source inspection identified `validate_snapshot` incorrectly guarded by `debug_assertions`, although production autosave calls it. The shared command registration is corrected. The later package run [33993370498](https://github.com/FZWINGEL/WebnovelStudio_V3/actions/runs/33993370498) for `cd7fb77` passed the narrow installed lifecycle: create, write, reopen, normal close, in-place uninstall, reinstall at the same version, and text retention. It used source SHA `cd7fb77f0bac8bc2ce1756f28044d406c356ff20`, installer SHA `9dcd7fc46dc8f259733d8487e11992b5c5b07d99dddd4a14f71c21baba8fd8ae`, and WebView2 `151.0.4129.101`. Offline installation, upgrade coverage, and broader W7 qualification remain open. See [Windows package qualification](WINDOWS_PACKAGE_QUALIFICATION.md).

W6 adds six core history tests, including cursor pagination through the final page, a six-stage rollback matrix, backup decision-link tampering, and recovered-project namespace behavior. A real process is killed after restore COMMIT and before acknowledgment; replay recovers the historical result once. Eight frontend restore tests and eight HistoryPanel tests cover the input/lifecycle barrier, exact preflight, lost acknowledgment, fenced absence, later-head conflict, failed display recovery, and late panel responses. Existing W5 proposal, Apply, scope, and history-boundary tests remain green.

The tracked C3 flow now has **26 checks**, including approved writing briefs; the final rebuilt local diagnostic passed **25** with only clipboard omitted. Checkpoint `508aee1` has a strict **26-check** hosted pass, including clipboard and writing briefs. The earlier saved-source checkpoint retains its separate strict **25-check** hosted pass. Native checks include real committed Apply/restore with a discarded acknowledgment, same-editor reconciliation, one-step undo/redo, exact export bytes, and retained history. Transport-loss injection lives only in the external harness.

The strict local native flow still stops at W0 Ctrl+V: copy serializes correctly, but paste receives empty clipboard data. This also occurred with an older binary. A separate Win32 diagnostic received access denied from `OpenClipboard(NULL)` in 20 of 20 attempts; no owner window was reported. The cause remains unresolved; no clipboard service was restarted and no clipboard contents were inspected. The successful strict W5 GitHub run is separate evidence on the hosted Windows machine. Ignored `.local/native-other-results/` explicitly records omitted clipboard coverage; `.local/native-results/` contains strict-run output. A stale failure file does not supersede a later dated success report.

The owned-dialog export helper uses `WM_NEXTDLGCTL`, `EM_SETSEL`, and `EM_REPLACESEL`, verifies exact readback, and sends no global filename keystrokes. ExportDialog closes before parent focus is restored. Both behaviors passed the strict hosted native flow; installed-package qualification is separate.

Default-size history comparison and restored-history screenshots were inspected. An additional 800×600 CSS-viewport check inside actual WebView2 shows no horizontal overflow and keeps comparison, Restore, and manuscript controls reachable. This is emulated viewport layout evidence, not physical window-resize, DPI, accessibility, or screen-reader qualification. Native dialogs, normal close, packaging, and broader author trials remain separately open.

At the earlier mock-only checkpoint, three bounded Codex CLI `0.153.3` generation dispatches are recorded in [Codex qualification](CODEX_QUALIFICATION.md): an initial success, a dedicated-home authentication failure, and a success using the existing managed login with stricter requested capability controls. No tool events were observed, and effective model/traits were not independently echoed. The empty-cwd, requested-control, host-authentication, and upstream-retry boundaries remain explicit. Descendant containment, Stop, failure, and context-budget qualification remained open at that checkpoint; the current bounded adapter and later experiments are recorded above.

## Full completion checklist

C2 review added explicit scope requirements for prose-producing purposes, mandatory evidence-dependency closure, non-sensitive policy exclusion counts in the delivered envelope, and a 10,000-block regression for bounded packing memory. The context inspector is mounted in the persistent discussion surface, and source pins are captured. C3 now persists author-confirmed guidance and binds frozen typed guidance exactly into AuthorRoom packets; its mutation receipt is distinct from packet guidance handles. Recent author-room discussion context is implemented as a bounded recency selection: up to four complete delivered exchanges and 16 KiB of exact turn records from the same current document thread and policy. Frozen snapshots and receipts retain the exact messages and scopes; the inspector shows supplied exchanges and omissions separately from guidance and story evidence. Stopped or partial output, other documents, revoked-policy turns, and copied historical threads are excluded from automatic reuse. This is recency selection, not semantic retrieval or adopted story truth. See [ADR 0003](ADR_0003_DISCUSSION_CONTEXT.md). Preparing or reading a packet does not authorize dispatch: W4 atomically claims a run against current source/policy epochs before invoking the deterministic mock. An explicit linked retry of a stopped, failed, or interrupted discussion retains its exact original request-scoped guidance when those versions are still active. Feedback, selected scope, and ordered source pins must match the original request; editing any of them starts a new request. Current document/project guidance and current permitted story sources are compiled afresh. Newly waiting request guidance is reserved for the next new request. Edited or retired inherited instructions, revoked policies, completed runs, and recovered-copy links are refused. The schema-7 composer stores the retry link with its draft and immutable save receipt, so navigation/reload preserves the choice. The original guidance-use receipt remains the only consumption record; retries do not consume it again. Persistent document/project discussion sources are implemented with schema-10 CAS receipts and mandatory packet binding; optional approved writing briefs are implemented; the bounded Codex development path is integrated with full qualification still open; the selected-passage Apply slice is implemented.

The following checklist preserves the approved work-package order and gates. A package is complete only when its implementation, failure coverage, and named evidence gate are recorded. W2 completion does not complete A, B, C, or the full V3 goal. W5 development CI is green. A checked implementation item does not by itself close a broader author-trial or release gate.

Persistent document/project source choices are implemented for AuthorRoom discussions. **Keep source…** opens an explicit confirmation form; **Include next time** remains a one-request choice. Rust merges current saved choices with transient pins into the frozen packet, retains exact mandatory-source receipts, and refuses unavailable or oversized required sources. The current target is already mandatory and is included once. Restricted edit requests exclude these saved discussion choices. Changed choices advance the source epoch, while retries preserve their original transient request identity. See [ADR 0007](ADR_0007_DISCUSSION_SOURCE_PINS.md).

### W0 — native editor spike and contract lock

**Gate:** initial N-spike evidence; E1 begins here. **Status:** partial/in progress.

- [x] Keep a real Tauri/WebView2 development window with the restricted Tiptap schema and persistent mounted editor instance.
- [x] Record the snapshot, identity, scope, canonicalization, hash, short local barrier, and session-history boundary in [ADR 0001](ADR_0001_EDITOR_CONTRACT.md).
- [x] Keep shared JS/Rust golden fixtures and real IPC snapshot validation.
- [x] Exercise session-only feedback, selection quotation/focus, preview/reject, local strict replacement, and undo/redo in the development surface.
- [ ] Complete the English native author trial, minimum-window/DPI behavior, external Word paste, native backup/export dialog journey, and assistive-technology trial.
- [x] Record a successful remote CI rerun after the native ProseMirror transaction wait fix and close the nominated Windows configuration evidence.
- [ ] Do not claim W0, N, an installed release, or real-manuscript readiness from the current spike alone.

### W1 — canonical documents and structural scope validation

**Gate:** P foundation. **Status:** implemented; P contract evidence recorded below.

The P foundation is covered by shared snapshot and scope fixtures, independent Rust validation, and file-backed persistence tests. `crates/core/src/lib.rs::shared_snapshot_fixtures_match`, `crates/core/tests/scope.rs::shared_scope_fixtures_match`, `scope.rs`/`text_replacement.rs` mutation tests, and frontend `document.test.ts`/`scope.test.ts` establish the current restricted schema. The full check recorded above passed, and W5 CI exercised the unchanged shared contract on Windows and Ubuntu. This qualifies the implemented document contract; new editor nodes or scope kinds require new evidence, and it does not close W0 or release trials.

- [x] Implement the restricted document schema and canonicalization/hash contract as shared JS/Rust behavior.
- [x] Implement block identity rules and Unicode endpoint conversion for the supported English editor surface.
- [x] Implement the independent Rust structural token iterator and scope validator.
- [x] Prove unchanged outside-scope text, marks, links, block style/identity, and scene boundaries.
- [x] Cover surrogate-pair, combining-mark, ZWJ, repeated-occurrence, empty-block, inline-only, cross-paragraph, malformed, and oversized fixtures as internal Unicode robustness cases.
- [x] Keep Rust free of a general ProseMirror-step interpreter.
- [x] Record P evidence before treating W1 as complete.

### W2 — real project persistence and document session

**Gate:** P. **Status:** in progress.

Core and frontend session work, receipts/reconciliation, and default persistent Library/Workspace wiring are implemented. Broader file-backed qualification and author-trial integration remain open; W2 does not complete the full V3 goal.

- [x] Implement core file-backed project storage with an owned connection/session boundary.
- [x] Implement migrations, working documents, immutable checkpoints, command receipts, writer leases, and typed Save/Reconcile operations.
- [x] Implement frontend `DocumentSession` generation watermarks, immutable in-flight payloads, serialized saves, lifecycle identity, and error buffers.
- [x] Prove delayed acknowledgments cannot replace newer editor text.
- [x] Prove operation/payload idempotency, changed-payload rejection, stale-version/lease rejection, definite-save error retention, and uncertain-outcome fencing/reconciliation.
- [x] Carry project/document/session identity through callbacks and reject late callbacks.
- [x] Prove process interruption after commit and before acknowledgment recovers the correct body.
- [x] Read back WAL/FULL/foreign-key configuration in file-backed tests.
- [ ] Complete and qualify the persistence integration across the UI and author trial; W2 does not complete the full V3 goal.

### W3 — library, free-order work, recovery, and A trial

**Gate:** M/P plus the development-native author trial. **Status:** active work.

- [x] Implement New/Open/Rename/Duplicate/Archive/Locate and blank note/character/chapter creation.
- [x] Persist last item/caret state, switch only after flush, and enforce project locks and captured ownership.
- [x] Restore into a new recovered project with a new identity and isolated operation namespace; keep the original untouched on failure.
- [x] Keep copied receipts historical and unable to authorize new operations.
- [x] Add the explicit UTF-8 `Export draft` action from a flushed, frozen source.
- [ ] Complete the A trial across two offline projects, restart/resume, recovery copy, and draft export.
- [x] Expose Import V2 only after F1 staged import and reconciliation exist; native qualification remains separately recorded.

### W4 — persistent conversation and deterministic jobs

**Gate:** M/P; no B trial yet. **Status:** active local slice.

Integrate the C0–C3 context foundation before or alongside this package: frozen snapshots and exact eligible sources, source epoch, deterministic exact retrieval with dirty-index fallback, mandatory-budget refusal, scoped author guidance, and actual-packet receipts/inspector. C0 and C1 are implemented in the core; C2's pure compiler, durable exact receipts, and native IPC are pushed and covered by local smoke; C3 guidance and bounded recent discussion compilation are pushed; linked retry guidance and durable composer mode are implemented and pushed. Persistent discussion sources and optional approved writing briefs are implemented; richer relevance selection remains open. These context packages do not replace the base save, Apply, lifecycle, or authority contracts.

- [x] Implement threads/messages, source checkpoints, frozen context receipts, model descriptors, durable jobs/output sequences, and a deterministic mock provider.
- [x] Implement queued Stop sealing, running Stop intent, cleanup settlement, retained partial output, and inspectable terminal recovery.
- [ ] Cover delayed output, malformed structured output, partial failure, cancellation, and exact repeatable suggestions.
- [ ] Keep provider code from mutating manuscript bodies.
- [x] Preserve author-room/prose-context separation and optional author-approved brief rules for deliberately transferred directions; see [ADR 0008](ADR_0008_WRITING_BRIEF.md).
- [x] Recover discussion and job state on reload and project switching; retry creates a linked new run.

### W5 — review cards, prepared snapshots, and single author Apply

**Gate:** M/P; B trial depends on W6. **Status:** selected-passage slice implemented and CI-covered; broader package remains open.

This W5 checkpoint adds explicit `Discuss`/`ProposeEdits` intent while retaining `Discuss` as the wire default. `ProposeEdits` currently requires a selected passage in a chapter and builds a `Working`/`RestrictedWriting`/`Revise` request at the chapter reader frontier; author-room private/future material, current guidance, and recent chat are excluded. Provider terminal handling accepts only a strict 1–3 candidate `ProposalOutput`; malformed or unsupported output is retained as raw unplaced discussion text without a repair call. The deterministic mock returns three alternatives.

Candidates, prepared versions, and decisions are immutable. Prepare accepts historical records against their immutable original source and validates exact JS result text, marks, and block scope with CAS versioning; only Apply requires the current head. Apply and Reject are explicit author actions. Apply atomically records body, source epoch, before/after revisions, decision, and receipt; any source edit or current-policy change makes a pending proposal stale. Reject leaves the epoch unchanged. Duplicate and cross-operation receipt collisions are fenced; Apply acknowledgment carries the separate latest result, and replay returns the latest head without reapplying. Recovered-copy proposals remain read-only.

The parent editor session flushes and preflights behind a pending-Apply barrier, commits the durable operation before dispatching the exact existing-editor transaction with `closeHistory`, retains newer edits on conflict, and reconciles uncertain acknowledgments without autosave. Native IPC mock commands are wired. Focused W5 tests and the diagnostic native subset pass locally; strict native21 passed in W5 GitHub CI. The local clipboard issue remains recorded above. Whole-chapter/block/manual-rebind proposals and F5 batch Apply remain open; W6 now owns explicit revision restore.

- [x] Implement source-bound proposals, editable prepared versions, exact before/after preview, one-at-a-time Apply, and Reject.
- [x] Bind each proposal to its context snapshot, exact target/scope, source epoch, policy, and context receipt; F2 alone owns reviewed authority.
- [x] Use the short local mutation barrier, preflighted editor transaction, durable decision/before/after/receipt transaction, and saved-generation handoff.
- [x] Add selection toolbar, context-menu action, and keyboard/menu alternative.
- [x] Prove undecided suggestions remain unchanged, repeated Apply cannot mutate twice, stale work is refused, and selected edits cannot change neighboring text, style, or boundaries.
- [x] Keep Apply all/batch Apply out of this package; F5 owns that later contract.

The checked items describe the selected-passage slice. Evidence includes `crates/core/tests/proposals.rs` preparation, scope, replay, rollback, recovery, and tamper cases; frontend `apply.test.ts` and `ProposalPanel.test.tsx`; `Writer.tsx` selection entry points; and the W5 strict native CI result. Whole-chapter/block/manual-rebind behavior and the broader B/E4 trial remain open.

### W6 — lost acknowledgment, shared lifecycle, history, and interruption hardening

**Gate:** P with native reruns; closes the B trial gate. **Status:** history/restore and shared pending-change reconciliation implemented; remaining trial and interruption combinations open.

See [ADR 0005](ADR_0005_DOCUMENT_HISTORY.md). Restore preserves the current writing in a checkpoint and advances the story source epoch. The operation receipt is its immutable author decision; schema 8 needs no new decision table.

- [x] Reconcile pending operation IDs and latest heads after lost acknowledgment.
- [ ] Share one lifecycle guard across Apply, reconciliation, editor disposal, switching, close, and application-controlled reload.
- [x] Add in-session history boundaries, significant undo/redo checkpoints, restart comparison, and explicit restore.
- [ ] Cover forced renderer loss, process interruption, restore A while B runs, and old-or-new transaction outcomes.
- [ ] Include context snapshots, source epoch, policy, delivered packet, and receipt in restart/fence/Stop coverage; no late context operation may trigger an implicit paid retry.
- [ ] Retain the live buffer on disk-full and permission errors; never hide external retries or paid restarts.
- [ ] Run the E4 stale-proposal friction check and keep conservative staleness unless measured evidence supports a bounded alternative.
- [ ] Run the B feedback trial only after the durable Apply and lifecycle evidence is complete.

### W7 — explicit exports and packaged native qualification

**Gate:** M/P/N complete. **Status:** export implementation and package build present; full native/release qualification remains open.

- [x] Add Markdown beside draft TXT with an exact frozen single-document preview, formatting/omission disclosure, immutable source record, and explicit working-draft export. Schema-20 author-reviewed chapter export is separately qualified; collection/publication export remain open.
- [ ] Finish keyboard navigation, accessible labels, focus restoration, resizing, native dialogs, and offline installation.
- [ ] Remove test-only command access and embedded automation from shipping builds.
- [ ] Qualify the packaged Windows WebView for keyboard/dead-key input, clipboard, focus, accessibility, high DPI, long chapters, recovery, Unicode/formatted projections, and export omissions.
- [ ] Do not infer publication, reviewed-story readiness, or continuity validity from export.

### W8 — one qualified live provider

**Gate:** L. **Status:** planned.

- [ ] Qualify one exact provider/model/configuration with explicit model and supported traits.
- [ ] Qualify one deterministic, fully recorded context packet first; treat the C6 bounded read loop as additional qualification after the one-packet route.
- [ ] Cover streamed completion, refusal/truncation, authentication failure, broken/partial streams, Stop, process cleanup, and recovered terminal history.
- [ ] Qualify credential entry/storage and inspect logs/backups for leakage.
- [ ] Document opaque upstream retries and the limits of local idempotency; do not claim exactly-once external billing.
- [ ] Keep unqualified adapters unavailable and never silently substitute a provider/model.

### F1 — V2 migration

**Gate:** named migration evidence. **Status:** planned.

- [x] Implement staged read-only import and reconciliation for schema 8, with explicit missing-prose choices and new V3 identities.
- [x] Retain original legacy evidence and rebuild V3 document projections without promoting V2 approvals into authority.
- [x] Distinguish editable imported writing/notes from inert legacy evidence and leave V2 source/application unchanged.
- [x] Cover synthetic supported-schema snapshots and reconciliation before exposing Import V2.
- [x] Complete the synthetic schema-8 native chooser/import/reopen journey with unchanged source evidence.
- [ ] Complete representative author-approved migration acceptance and broader pending-import native recovery cases.

### F2 — reviewed story boundary

**Gate:** architecture scenario 5. **Status:** author-only prose review implemented in development; full F2 remains open.

The first part stages an exact saved chapter and its complete earlier selected reviewed prefix, then records an explicit author-only bundle without changing prose. The native Story review panel previews the immutable revision before confirmation. Review remains optional for writing. Source edits and changed earlier selections invalidate current review eligibility while preserving historical reviews and later text. Schema 14 introduced stages, bundles, heads, and suffix fences; schema 15 adds the reviewed basis manifest and immutable reader-position pins, while recovered copies clear active review pointers. The core/IPC freeze resolves the exact earlier reviewed prefix plus current working target. The current continuation slice consumes that boundary for explicit Working/Reviewed generation and append-only Apply/Reject. Schema 20 adds reviewed export with exact immutable bundle provenance and final freshness validation. Schema 21 adds the locally checked passage-backed reviewed-evidence set and reader-only restricted projection described in [ADR 0018](ADR_0018_REVIEWED_STORY_EVIDENCE.md). Broader native/live qualification, accepted summaries, exceptions, and the wider C5 state model remain open. See [ADR 0012](ADR_0012_AUTHOR_REVIEW.md) and [ADR 0013](ADR_0013_REVIEWED_CONTEXT.md).

- [x] Stage exact author-only prose review, inspect earlier reviewed revisions, explicitly select immutable bundles, preserve history, and reject stale source/basis activation. Resume saved unaccepted reviews explicitly after restart.
- [x] Freeze the exact reviewed prefix and current working target through the core/IPC boundary, retaining immutable reader positions and historical namespace fencing.
- [x] Extend these bundles with the first typed passage-backed reviewed record set, including exact evidence, identity choice, audience filtering, inheritance, explicit clear, historical validation, and restricted projection. This bounded possession slice does not establish complete continuity.
- [ ] Extend the reviewed-story model with additional rules and records, accepted summaries, issue decisions and exceptions, known dependency evidence, and broader continuity views.
- [x] Enable reviewed-source continuation and append-only preview/Apply only with explicit validity; do not imply exhaustive continuity.
- [x] Exercise continuation in local native WebView2, one bounded selected live provider, and strict CI 34008911179.
- [x] Implement author-reviewed chapter export, exact projection and record validation, final freshness refusal, local native acceptance, and strict CI 34010306332; this establishes no canon/publication authority.
- [ ] Complete broader provider/native/release gates and remaining author trials.
- [ ] Retain sole ownership of reviewed authority; context packets and generated digests cannot accept canon.

### F3 — context quality

**Gate:** measured task-specific context evidence. **Status:** C4-A chapter navigation memory implemented with local tests, native diagnostic, and one live-provider result. Broader C4 derived-view packet integration and C5/C6 remain open; strict CI and full provider/release qualification remain separate.

- [ ] Add source packing, author-room/prose-context separation, safe briefs, exact previous prose, aliases/search, and freshness checks.
- [x] Implement the C4-A source-linked chapter navigation digest slice with explicit Refresh, exact source revision, strict evidence validation, separate job/result/view records, stale/revocation fencing, and local recovery boundaries. Generated views remain inspection-only and are not supplied to future model packets.
- [x] Verify C4-A cleanup, uncertain claims, archive/reopen, native development flows, and one bounded live Codex result. Strict CI and broader provider/release gates remain separate.
- [ ] Integrate derived views into frozen packets through a separate dependency and coverage contract.
- [ ] Own broader C4 generated views and C5 thin temporal/relationship/thread views, with C5 depending on F2; add richer quality only after the evidence-first C0–C3 foundation.
- [ ] Measure omissions and permissions before claiming a memory or prompt improvement.

### F4 — narrative evaluation

**Gate:** independent author-labelled evaluation. **Status:** planned.

- [ ] Run retrieval, continuity, prose, and author-acceptance cases using frozen model/settings.
- [ ] Limit conclusions to the evaluated English tasks, genres, lengths, and models.

### F5 — batch Apply

**Gate:** B complete and separate atomicity evidence. **Status:** planned.

- [ ] Implement same-base disjoint preparation and one atomic Apply/decision transaction.
- [ ] Validate the common context snapshot, policy, and source epoch as part of the atomic batch.
- [ ] Cover overlap, repeated operation IDs, stale/already-decided members, and lost acknowledgment.
- [ ] Preserve individual Apply as a single explicit author decision.

## Story Context extension completion ledger

The maintained [Story Context system](V3_STORY_CONTEXT_SYSTEM.md) and [first-slice plan](V3_STORY_CONTEXT_FIRST_SLICE.md) are an adopted design extension. Their C0–C6 packages are part of the full V3 goal and preserve the base save, Apply, lifecycle, and reviewed-authority ownership. C0 is implemented with pure contracts and 16 adversarial tests; C1 is implemented as a working-basis snapshot/retrieval slice; C2 is implemented and pushed as a Rust pure deterministic compiler with durable exact packet receipts and native IPC, and is covered by the current native development flow and earlier hosted checkpoints; C3 guidance persistence and packet binding are pushed, as are bounded recent discussion context and inspector display. Linked retry guidance and saved composer mode are implemented and pushed. Persistent discussion sources and optional approved writing briefs are implemented; richer relevance selection remains open. No full C0–C6 completion is claimed.

| Package | Planned owner and scope | Status | Required evidence before completion |
| --- | --- | --- | --- |
| C0 | Before/alongside W4; freeze contracts and adversarial eligibility fixtures | Implemented | Pure contracts and 16 adversarial tests cover source eligibility, disclosure boundaries, digest restrictions, and authority separation |
| C1 | Before/alongside W4; immutable source snapshots, exact retrieval, source epoch, and dirty-index fallback | Implemented working basis | Rust actor snapshots pin canonical revisions; exact literal/lexical scan, source-only alias matches, Unicode UTF-16 spans, revocation epoch, conservative source staleness, snapshot retry/restart, and yielding disposable per-document index rebuild. Ten C1 tests include a 1,000-chapter, 8,280,000-byte cold snapshot measured at 1.1 seconds in local debug; six context migration tests cover schema-2/3 upgrade and recovery, with transfer coverage for schema-1 recovery. Reviewed/history/character policies remain unavailable until authority work; aliases remain private to AuthorRoom until safe grants, and AuthorRoom Revise/Continue is blocked |
| C2 | Before/alongside W4; deterministic multi-resolution packet compilation, mandatory-budget errors, and actual-packet receipts | Implemented and pushed; development smoke covered | Pure compiler and durable receipt tests pass; exact packet/messages/options/hash survive restart; source body/descriptor/projection/eligibility/scope validation, target/instruction/scope/mandatory-pin preservation, full-eligible-when-fitting and whole-block-prefix packing with explicit omissions are implemented. Mock accounting is UTF-8-byte based only; provider tokenization, live AI integration, and release qualification remain open |
| C3 | Before/alongside W4; scoped author guidance and the context inspector | Partial: guidance persistence, bounded recent exchanges, inspector, transient pins, persistent discussion sources, and approved writing briefs integrated | Chat or direct entry can be saved, edited, and retired as immutable exact versions at Next request, This document, or This project scope. CAS/idempotent guidance receipts, source-epoch invalidation, recovery retention/fencing, exact mandatory AuthorRoom packet binding, one-use consumption after successful persisted start, separate guidance handles in the inspector, and GuidancePanel lost-ack/late-response coverage are covered locally. Recent complete exchanges are frozen and packed with exact message receipts and explicit omissions; stopped/partial, other-document, revoked-policy, and copied historical turns are excluded. Unchanged unsuccessful retries preserve original one-use instructions without consuming newly waiting guidance; request identity, current policy, active versions, restart, and recovered-copy boundaries are tested. Optional approved briefs preserve exact restricted request text without transferring private origin material. Richer conversation selection and broader Apply integration remain open |
| C4-A | F3; source-bound single-chapter navigation digest without automatic canon | Implemented development slice; local/native/live evidence recorded | Exact full-chapter revision, strict `navigation-digest.v1` UTF-16/evidence checks, separate job/result/view records, stale/revocation/recovery boundaries, native/provider evidence, and no paid autosave/open calls |
| C4 | F3; derived-view packet integration and richer quality without automatic canon | Open after C4-A | Freeze view identity/dependencies/policy/coverage for future packets; rebuild, late-result, source-change, deletion, restore, and quality evidence |
| C5 | F3 after F2; thin temporal, relationship, knowledge, and thread views | Partial: C5-A entity reuse, batched current-evidence freeze, and authenticated object history CI-qualified as a development slice; broader quality evaluation pending | Build relationship/knowledge/thread/rule views and multi-resolution digests with source-bound retrieval, disclosure, uncertainty, and historical dependencies; add quality evidence for supported English tasks |
| C6 | W8 additional qualification; bounded provider-side read loop | Planned | Stop/budget/duplicate-event/crash boundaries, visible unknown outcomes, and fresh invocation labeling |

The public promise is layered: stored evidence, permitted available sources, the packet actually delivered, and what a model understood are separate states; the last requires evaluation. C1 retains original source and does not make copied historical snapshots authoritative for a new project. Context work does not authorize automatic canon or replacement of source text with a large rolling summary.

## Completion rule

The full V3 goal is complete only after the applicable W0–W8, F1–F5, and C0–C6 gates have their implementation, failure coverage, and evidence recorded. Current W1/W2 progress and active W3 work are necessary groundwork; they do not close the A writing trial, B feedback trial, C release qualification, live-provider qualification, migration, reviewed-story, context-quality, narrative-evaluation, context-extension, or batch-Apply gates.
