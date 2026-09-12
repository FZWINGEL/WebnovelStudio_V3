# Facilitator guide

## Before the session

Create a study ID and anonymous participant ID. Put all raw notes, recordings,
transcripts, and manifests under `.local/workshop-author-study/<study-id>/`.
For each participant-condition, create a separate fresh synthetic project with
no chapters. Do not reuse a project or carry preferences, pins, guidance,
decisions, or shared chat between conditions. Preserve each project for the
later revisit. Never reset or replace a database beneath an open editor. Do not
import an author's manuscript. Record the exact app build/revision, operating
system, provider, model, reasoning effort, service tier, and observed Codex CLI
version before the first condition.

Use [`allocation.md`](allocation.md) to assign the three condition order and
seed order. Prepare one copy of each condition card and one blank
[`observation-form.md`](observation-form.md) per condition. The author chooses
one timebox and that same timebox applies to all three conditions.

The default budget is one initial generation and up to three explicit
follow-ups per condition. A follow-up is sent only after the author asks for
it. The facilitator may remind the author of the next scripted demand, but may
not author a request, add automatic critique, or continue after the allowance.

## Consent script

Read this before recording any observation:

> We are comparing three ways to develop a fictional idea. Participation is
> voluntary. You may skip a question, decline to share any text, pause, or stop
> at any time without giving a reason. We will use synthetic story seeds by
> default and an anonymous participant ID. We are studying the interaction and
> your decisions, not judging you. We will ask what you knowingly endorse and
> what you reject or leave open. Your permission to participate is separate from
> optional permission to record audio or screen activity. You may withdraw that
> recording permission at any time. If you stop, we stop the session and follow
> your choice about deleting the raw notes or recording. No contact details are
> collected in this kit.

Ask separately and record only `yes`, `no`, or `withdrawn`:

| Consent item | Response |
| --- | --- |
| Voluntary participation understood |  |
| Synthetic seed / no private manuscript understood |  |
| Optional audio recording |  |
| Optional screen recording |  |
| Permission to retain anonymized study notes |  |

If recording is declined, do not record. If recording permission is withdrawn,
stop it immediately and mark the time. On a delete request, remove the raw
recording and raw notes from the study-ID folder, then record only that a
deletion was requested and completed; do not preserve a copy elsewhere.

## Common briefing

Tell the author:

> In each condition, start from the seed on your card. Explore possibilities,
> keep details you want, and leave other things open. You remain the author.
> Please say when you knowingly endorse a decision, when you reject one, and
> when you are unsure. You may edit or write plain notes. In every condition,
> use only requests you personally want to send.

Do not describe one condition as expected to be better. Do not explain hidden
implementation differences during the task. If the author asks what a saved
item means, ask what they think it means first, then read the neutral wording
available in that condition.

## Setup and provider boundary

The study's comparable provider setup is the same active Codex model, effort,
and service tier in every condition. The observed audit binding for generic
Develop and Workshop is 24,576 UTF-8 serialized packet input bytes and 65,536
UTF-8 retained output bytes (see the [bounded Codex contract](../../ADR_0011_LIVE_CODEX.md)).
These are application allowances; never call them tokens or billed usage. There
is no explicit Codex output-token cap in the binding. Record provider-reported
usage afterward when present, otherwise write `unknown`.

The Codex CLI is unpinned. Record its actual version and the exact build/revision
under test. Do not substitute the mock provider for narrative evaluation. If a
live Codex check is unavailable, stop the comparative generation portion and
record the reason as a study gate; do not quietly mix mock and live results.

Use generic route descriptions until the current build's labels have been
checked. Condition A opens a nonchapter document in the Write desk and uses its
generic Develop action. Condition B completes the worksheet, the author checks
the assembled instruction, and then uses the same A route. Condition C opens
Develop mode Workshop. Generic and worksheet routes do not get a structured
alternatives manager added for the study; plain document notes are enough to
preserve choices and let burden be observed.

## Running each condition

1. Read the assigned seed verbatim; do not add genre, protagonist, or ending
   details.
2. Start the author-chosen timebox and record the start time. When the author
   first knowingly endorses a decision, record the request ID, timestamp, and
   elapsed seconds; a generated candidate does not count.
3. Allow one initial author-initiated request.
4. Offer the three common follow-up demands in order. The author may use zero
   to three; never exceed three. Use the exact wording on the condition card.
5. Observe navigation, re-prompting, corrections, endorsements, rejected
   alternatives, open questions, and any accidental or unauthorized request.
   Count navigation and re-prompt episodes and record their seconds, not only a
   subjective burden rating.
6. Ask the author to close and reopen the condition after the last allowed
   request, then complete the resume demand as a task. Record whether a new
   generation was requested; do not create one for the study.
7. Stop at the timebox or allowance, whichever comes first. Do not coach the
   author toward a preferred choice.

At the end of the condition ask:

- Which decision, if any, do you knowingly endorse?
- What did you reject, change, or leave open, and why?
- What would you let drafting use right now?
- What did you have to correct, re-prompt, or hunt for?
- Did the system preserve the details you cared about?
- Could you change direction without losing ownership?

Record the author's words as quotes where practical and distinguish them from
facilitator interpretation.

## Closeout

After all three conditions, ask for a preference only after the per-condition
forms are complete. Ask which route they would choose for a new idea and what
would make that choice change. Do not turn preference into a quality score.

Copy only the anonymized material needed by the blinded reviewer. Remove
condition names, UI screenshots with labels, model metadata, and participant
identity from the review packet. Use [`review-form.md`](review-form.md) for the
separate human review. Report study imbalance, missing conditions, and any
protocol violation with the final notes; do not fill gaps from memory.
