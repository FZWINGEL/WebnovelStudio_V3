# Anonymous observation and manifest form

Use one copy per condition. Leave a field blank or write `unknown`; never infer
usage, ownership, or a rationale. Keep the completed form under
`.local/workshop-author-study/<study-id>/` and remove identifying material before
sharing with a reviewer.

## Study manifest

| Field | Value |
| --- | --- |
| Study ID |  |
| Anonymous participant ID |  |
| Condition (A/B/C) |  |
| Condition position (1/2/3) |  |
| Seed ID and exact seed |  |
| Allocation row |  |
| Session date (YYYY-MM-DD) |  |
| Facilitator code |  |
| Planning habit (world-first / discovery-oriented / mixed / unknown) |  |
| Familiarity with AI writing (author's description) |  |
| Consent status |  |
| Audio recording status |  |
| Screen recording status |  |
| Author-chosen timebox |  |
| Session start / end |  |
| Stop/delete request and action |  |

## Fresh-project setup check

Create three independent fresh synthetic projects for this participant, one per
condition. Use no chapters. Do not reuse a project or carry preferences, pins,
guidance, decisions, or shared chat across conditions. Preserve each project
for the later revisit. Do not reset or replace a database beneath an open
editor.

| Condition | Fresh project ID/path | Exact assigned seed matches card? | No prior state confirmed? | Preserved for revisit? |
| --- | --- | --- | --- | --- |
| A |  | ☐ | ☐ | ☐ |
| B |  | ☐ | ☐ | ☐ |
| C |  | ☐ | ☐ | ☐ |

## Build and provider identity

| Field | Value |
| --- | --- |
| Exact app build/revision |  |
| OS and WebView/runtime version |  |
| Provider |  |
| Observed Codex CLI version |  |
| CLI executable identity/hash, if exposed |  |
| Expected app input allowance (UTF-8 bytes) | 24576 |
| Expected app retained output allowance (UTF-8 bytes) | 65536 |
| Explicit output-token cap | none observed / unknown |
| Usage accounting note | byte allowances are not billed usage |

The expected allowances above are separate from the observed frozen binding.
Record the requested and resolved model, effort, tier, binding/profile, and
limits for every request below. If a setting differs, stop treating the cell as
comparable and document the reason.

## Request identity and resolved binding

| Request | Request/run ID | Requested model | Requested effort | Requested tier | Resolved model | Resolved effort | Resolved tier | Frozen binding/profile and limits | Evidence reference |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Initial |  |  |  |  |  |  |  |  |  |
| F1 |  |  |  |  |  |  |  |  |  |
| F2 |  |  |  |  |  |  |  |  |  |
| F3 |  |  |  |  |  |  |  |  |  |

## Request ledger

| Request | Author initiated? | Purpose/demand | Sent? | Start/end | Actual input bytes | Input usage value/unit | Output usage value/unit | Reasoning usage value/unit | Unknowns/notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Initial |  | Seed exploration |  |  |  |  |  |  |  |
| F1 |  | Rival/relationship + alternatives; author explains current choice afterward |  |  |  |  |  |  |  |
| F2 |  | Reject/revise/preserve proposal; author accepts or rejects |  |  |  |  |  |  |  |
| F3 |  | Foundational-change consequences; author identifies drafting use and resumes afterward |  |  |  |  |  |  |  |

For provider usage columns, write the numeric value with unit `tokens` when the
provider reports tokens; otherwise write `unknown`. Keep confirmed serialized
input bytes in the separate byte column and never relabel them as tokens.

Actual request count: ____ / 4 maximum. Automatic or facilitator-initiated
requests: ____ (describe below). Never convert bytes to tokens.

## Decision timing and navigation totals

Time starts when the condition becomes available to the author. A first
knowingly endorsed decision is an author-stated choice, not the first generated
candidate.

| Measure | Value |
| --- | --- |
| First knowingly endorsed decision: request ID |  |
| First knowingly endorsed decision: timestamp |  |
| Time to first knowingly endorsed decision (seconds) |  |
| Navigation episodes (count) |  |
| Navigation time (seconds) |  |
| Re-prompts (count) |  |
| Re-prompting time (seconds) |  |
| Corrections (count) |  |
| Correction time (seconds) |  |

## Decisions and ownership

| Decision/material ID | Exact short description or excerpt | Status (endorsed/rejected/open/uncertain) | Knowingly endorsed? | Author's rationale or quote | What drafting may use | Protection/correction needed |
| --- | --- | --- | --- | --- | --- | --- |
|  |  |  |  |  |  |  |
|  |  |  |  |  |  |  |
|  |  |  |  |  |  |  |
|  |  |  |  |  |  |  |

## Navigation, re-prompting, and corrections

| Time/request | Trigger or goal | What the author did | Re-prompt or navigation needed | Correction made | Burden (low/medium/high) | Facilitator intervention |
| --- | --- | --- | --- | --- | --- | --- |
|  |  |  |  |  |  |  |
|  |  |  |  |  |  |  |
|  |  |  |  |  |  |  |

Record whether the author could find alternatives, the current choice, the
drafting-use boundary, and saved material after close/reopen. Record any
unexpected overwrite, lost text, stale result, or other product defect as an
observation; do not repair it silently.

## Ownership and burden notes

| Prompt | Anonymous notes / author quote |
| --- | --- |
| Could the author change direction without losing ownership? |  |
| What did the author reject or correct? |  |
| What did the author leave open? |  |
| Which detail mattered most to preserve? |  |
| What felt like the system's decision rather than the author's? |  |
| What would the author choose in a later session? |  |

## Violations, uncertainty, and later revisit

| Field | Entry |
| --- | --- |
| Protocol violation or unexpected automatic analysis |  |
| Privacy or consent issue |  |
| Provider/build mismatch |  |
| Missing or unknown usage |  |
| Data loss or recovery issue |  |
| Later revisit date |  |
| Later useful decisions |  |
| Later corrections or unnecessary constraints |  |
| Evidence path/hash, if author approved |  |

## Facilitator interpretation (keep separate)

Write interpretations only after the factual fields are complete. Cite the
observation row or recording timestamp supporting each claim.

| Finding | Evidence row/timestamp | Confidence (supported/uncertain) |
| --- | --- | --- |
|  |  |  |
|  |  |  |
