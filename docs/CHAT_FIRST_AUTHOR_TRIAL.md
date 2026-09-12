# Chat-first author trial

**Status:** protocol only. The human study described here has not been run.

This is a small formative comparison of the opt-in **Project Chat trial** and
the existing Workshop. It is intended to find comprehension and workflow
problems before any default-on decision. A sample of approximately **8–10
authors** is directional evidence, not a statistical benchmark.

## Before each session

Use the same synthetic project and matched task for both surfaces. Keep the
provider, model, reasoning effort, service tier, context budget, and relevant
source material the same. Codex remains the primary author path; record the
requested settings separately from any provider-reported effective identity or
tier. Do not claim a tier that the provider does not report.

Assign the order in advance and counterbalance it across participants:

- Workshop first, then Project Chat;
- Project Chat first, then Workshop.

Each participant should complete one matched task in both surfaces. Across the
cohort, cover every entry path at least once, preferably twice. Use synthetic
stories by default. Use author-owned material only with explicit consent.

Enable Project Chat as an explicit opt-in trial for the session. Do not present
it as the default workflow or explain the intended winner. Ask the author to
think aloud only when that does not interrupt the writing task.

## Matched tasks

| Entry path | Task in both surfaces |
| --- | --- |
| Rough idea | Turn a short attraction into one useful story direction and one knowingly endorsed saved document. |
| World-first | Establish one world rule and its human consequence without completing a world template. |
| Character-first | Develop a motivation and conflict, revise it, then update one linked plan while preserving the original intent. |
| Existing notes | Organize supplied notes while retaining the original and identifying invented connective assumptions. |
| Write-now | Begin prose immediately, revise a selected confrontation, and keep the ending unchanged. |

The author may reject a direction or keep a question unresolved. Rejection is
a valid creative result. Do not require a questionnaire, a complete world, or a
character sheet before useful work begins.

For the existing-notes task, use **Bring a note**, save the supplied text, and
attach that note as a source. After a later edit, ask the author to locate the
version discussed and the current version. For write-now, include one request
for chapter-wide feedback without selecting text; if it proposes a passage,
observe whether the author understands that confirming the passage prepares a
separate edit request and leaves the chapter unchanged. These are trial tasks,
not reports of completed participant sessions.

## Session flow

1. Give the author the assigned starting material and task in neutral language.
2. Observe the first useful action, questions before useful work, navigation or
   search steps, and the time to a decision the author can knowingly endorse.
3. For a generated document, ask the author what they believe is saved, what
   is still a draft, and what would change the story. Ask this before pointing
   to the adoption label. Record **noticed adoption**, **noticed non-adoption**,
   or **unnoticed state**. Any unnoticed adoption is a safety failure.
4. Have the author make one intentional revision or rejection, then return to
   the conversation and continue the task. Record corrections needed to keep
   the author’s intent.
5. On the following day, ask the author to reopen the same project, find the
   prior decision or draft, and continue the task. Record whether the saved
   state is useful without moderator reconstruction.

During ordinary author work, exercise these situations through visible UI
actions where they naturally fit: defer a question, edit a source while a
request is running, switch between two projects, and inspect an unknown or
uncertain outcome if one occurs. Record both the system result and whether the
author understood what happened. Do not ask authors to corrupt a database,
close a process at a particular instant, or manufacture a failed save.

Observer-only synthetic fault checks may separately use the native harness for
lost acknowledgments, failed local materialization, and uncertain operations.
Label those results as observer/test evidence; they are not participant
actions or evidence of author comprehension.

## Record sheet

Keep one row per participant, surface, and entry path. At minimum record:

| Field | Record |
| --- | --- |
| Session | Participant code, date, entry path, surface order, task version, consent/material source |
| Build identity | Source commit, executable SHA-256, executable size/version when available, schema reader floor, OS/WebView2, and whether the run used a development or installed package |
| Request identity | Provider/transport, requested model, reasoning effort, requested service tier, provider-reported model/tier or **unknown**, context budget, and invocation count |
| Workflow | Time to first useful work, time to knowingly endorsed decision, questions, navigation/search steps, revisions, rejection reason, and next-day return usefulness |
| Safety | Draft/adopted distinction, noticed or unnoticed adoption, deferral result, source-change/stale result, project-switch isolation, unknown-outcome recovery, and any lost acknowledged text |
| Accessibility | Keyboard-only completion and focus recovery, 200% zoom/narrow layout, screen-reader announcements, selection retention, and whether status/error was found without transcript hunting |

Never invent a current hash or effective provider setting. Copy identity fields
from the run report or leave them explicitly unknown. Keep raw notes and the
participant’s wording alongside the row; do not reduce the result to a single
preference score.

## Acceptance review

After the sessions, review whether authors could complete all five entry paths
without a compulsory planning checklist, explain what would influence a
chapter, and return to useful work after a normal source change or project
switch. Treat these as stop-ship findings for default-on rollout: unnoticed
adoption, loss of acknowledged author text, cross-project or restricted-context
leakage, partial grouped adoption, or blind provider replay.

The local native 200% zoom check currently passes at a 400×300 CSS viewport.
That is development evidence only. Actual author observations, an installed
package trial, and the screen-reader trial remain pending. This document does
not claim that the human study has been executed or that Project Chat is ready
to become the default.
