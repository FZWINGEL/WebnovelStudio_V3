# Story Workshop formative author study

**Status: prepared protocol; no author-study results collected.** This is the
human evaluation required by [the specification](V3_STORY_WORKSHOP_UX_SPEC.md#18-evaluation-and-acceptance).
Automated tests establish software behavior, not author ownership or narrative
quality. Conduct this after the exact native build passes its contract checks.

Use the operational [Story Workshop author-study kit](studies/workshop/README.md)
for facilitator cards, the worksheet comparator, independent allocation,
anonymous observation/manifest forms, and the separate blinded human review.
This protocol remains the study contract; the kit supplies blank working forms.

## Participants and conditions

Recruit a small formative group that includes both world-first and discovery
writers. Record their normal planning habits and familiarity with AI writing.
Participation and manuscript sharing are optional; synthetic seeds avoid needing
anyone's unpublished material.

Compare three conditions: the existing generic Develop editor/action, a
tag-heavy preference worksheet followed by the same generation route, and the
Story Workshop workbench. The worksheet is a study control, not a proposed
production setup gate. Use the same active provider, model, reasoning effort,
service tier, request count allowance, and comparable input/output allowances
for each participant's conditions. The default allowance is one initial request
plus up to three author-initiated follow-ups, with the same author-chosen
timebox per condition. Record actual usage when available; unknown usage stays
unknown. Requests remain author initiated and manually sent; do not add
automatic analysis to the generic or worksheet condition.

Rotate condition order across participants (ABC, BCA, CAB, then reversed
sequences) and rotate seed order separately. The six-row allocation in the
[study kit](studies/workshop/allocation.md) balances condition/seed pairings and
task positions; report assigned, completed, and missing counts when the sample
is smaller or incomplete. Do not
give one condition a developed world while another begins with a fragment.
Allow direct edits and rejection in all conditions.

## Comparable tasks

1. A neighborhood repairs broken magic. Explore three ways knowledge could be
   accessed, keep two details, and choose one world decision. Leave protagonist
   and ending undecided.
2. A community tends traveling gardens. Explore three ways people could learn
   the craft, keep two details, and choose one world decision. Leave protagonist
   and ending undecided.
3. A harbor restores forgotten songs. Explore three ways people could preserve
   or share that craft, keep two details, and choose one world decision. Leave
   protagonist and ending undecided.

After the initial task, use the same three grouped follow-up demands in every
condition: propose a competent rival or relationship; propose a revision that
rejects one author-identified assumption and changes one selected detail while
preserving another; propose consequences of changing a foundational decision.
After each response the author identifies alternatives, decides what to adopt,
and explains what drafting may use. Reopening and resuming are observation
tasks that require no extra generation. The exact wording is on the
[condition cards](studies/workshop/condition-cards.md).

## Observe and ask

Capture the exact build, condition/order, model configuration, seed, actual
requests, time spent navigating/re-prompting, corrections, and a short record of
decisions the author knowingly endorses. A large amount of accepted text is not
automatically a better result. Ask what they discarded and why.

Ask the author to explain, in their own words, why each selected decision belongs
in their story and what remains open. Record confusion between saved alternatives,
chosen decisions, writing access, noncanon experiments, and established events.
Ask whether they felt able to change direction and whether the system preserved
the details they cared about. Record any unexpected overwrite or loss separately
as a software defect.

Have humans review the chosen material for specificity and coherence, with the
author's declared preferences visible and the interface condition hidden where
practical. Record disagreements and concrete examples, not just a combined score.
Return to the material in a later writing session and ask which decisions were
useful, which required correction, and which constrained the author unnecessarily.

## Evidence record

For each observation keep: anonymous participant ID, planning habits, exact build,
condition, order, seed, provider/model/traits, request allowance and observed use,
endorsed decisions with rationale, correction/navigation burden, ownership notes,
human coherence/specificity notes, and later usefulness. Preserve raw observations
and separate them from interpretation. Store raw material only under the ignored
`.local/workshop-author-study/<study-id>/` path. Do not label a mock transcript or a model's
self-review as an author observation.

For the default Codex comparison, the read-only provider audit records that
generic Develop and Workshop bind 24,576 UTF-8 serialized packet input bytes and
65,536 UTF-8 retained output bytes. These are application allowances, not token
limits or billed usage. No explicit Codex output-token cap was observed; record
provider usage afterward when available and leave it unknown otherwise. Codex
CLI is unpinned, so record the actual installed version. Do not use the mock
provider as a narrative evaluation substitute because its defaults differ.

The outcome is a list of supported findings, uncertainties, and concrete UX
changes. A small formative study cannot establish universal preference or that
Workshop improves complete novels. The implementation goal's human gate remains
open until this study has actual participants and recorded observations.
