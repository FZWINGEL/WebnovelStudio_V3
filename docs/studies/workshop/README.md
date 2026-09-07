# Story Workshop author-study kit

This folder is the operational kit for the formative evaluation in [Story
Workshop §18](../../V3_STORY_WORKSHOP_UX_SPEC.md#18-evaluation-and-acceptance). It turns the
protocol in [`docs/STORY_WORKSHOP_AUTHOR_STUDY.md`](../../STORY_WORKSHOP_AUTHOR_STUDY.md)
into facilitator cards and blank capture forms.

**Kit status:** ready for a human study; no participants, results, or author
contact data are included. W44 remains pending until real observations are
collected and reviewed.

Use these documents in order:

1. [`allocation.md`](allocation.md) assigns condition order and seed order
   separately, with balanced pairings and positions.
2. [`facilitator-guide.md`](facilitator-guide.md) covers consent, setup, the
   common script, timebox, and closeout.
3. [`condition-cards.md`](condition-cards.md) supplies the three conditions,
   exact synthetic seeds, and identical follow-up demands.
4. [`worksheet-comparator.md`](worksheet-comparator.md) is the optional,
   printable tag-heavy control for condition B.
5. [`observation-form.md`](observation-form.md) is the anonymous per-condition
   observation and manifest form.
6. [`review-form.md`](review-form.md) is a separate blinded human review form
   for coherence and specificity.

Raw study material belongs only in the ignored local path
`.local/workshop-author-study/<study-id>/`. This repository contains no blank
participant dataset and the kit must not be used to fabricate one. Keep an
anonymous participant ID; do not record names, email addresses, unpublished
manuscript text, credentials, or an author contact route.

The default study uses synthetic seeds and the same active Codex model,
reasoning effort, and service tier in all conditions. The generic and worksheet
conditions use the generic Develop route; the worksheet is a study comparator,
not a production setup gate. The Workshop condition uses Develop mode
Workshop. Every request is author initiated and manually sent. Do not add
automatic analysis to make the generic conditions resemble Workshop.

The read-only provider audit, aligned with the bounded Codex contract in
[`ADR_0011_LIVE_CODEX.md`](../../ADR_0011_LIVE_CODEX.md), records that generic
Develop and Workshop bind the same Codex application allowances: **24,576 UTF-8 serialized packet bytes of
input** and **65,536 UTF-8 bytes of retained output**. These are comparable app
allowances, not token counts, context-window claims, or billed usage. The live
binding has no explicit Codex output-token cap; record provider usage afterward
when available and leave it unknown when it is not. Codex CLI versions are not
pinned, so record the actual installed version. The mock provider is not a
narrative-evaluation substitute because its defaults differ.

The study budget is one initial request plus up to three explicit follow-ups
per condition, with the same author-chosen timebox for each condition. Record
the requested allowance and actual requests, usage, uncertainties, and any
violation rather than silently correcting the record.
