# WebnovelStudio V3 — Story Workshop
## UX and product specification for developing a novel before drafting it

**Date:** 7 September 2026  
**Status:** Proposed design; not implemented or usability-validated  
**Repository:** `FZWINGEL/WebnovelStudio_V3`  
**Reviewed branch:** `codex/v3-persistence`  
**Reviewed commit:** `085e8e13325639938ff0c0752972156dc7d705d1`

---

## 1. Product decision

Build a **Story Workshop**: a persistent, author-led workspace for discovering, comparing, connecting, and refining the material from which a novel will eventually emerge.

Do not build a larger setup form in front of a chapter generator. Do not make an unbounded chat transcript the primary home of the author's decisions. Do not make the author know the answer to every worldbuilding question before the application becomes useful.

The core interaction is:

> Bring a fragment → explore meaningfully different directions → keep and combine details → examine consequences → adopt a working decision → choose what to explore next.

Tags belong in this system, but as **steering controls**, not as substitutes for developed ideas. A tag such as “earned progression” becomes useful when it affects how a character gains abilities, what those abilities cost, which institutions control access, and what kinds of victories the reader experiences.

The desired feeling is: **“This helps me discover what I want my story to be.”** It is not: “This wrote a large amount of lore for me.”

World-first development must be a fully supported primary workflow. The author may build a rich setting without settling the protagonist, ending, or chapter outline. The application should offer connections to story possibilities without turning every detail into a mandatory plot device.

### The primary user jobs

- Turn a vague attraction, image, trope, or premise into several directions worth considering.
- Understand why one direction feels better, and preserve the exact details that create that feeling.
- Develop connected worlds, people, relationships, themes, and story possibilities without repetitive prompting or manual copying between tools.
- Change an important idea without losing alternatives or silently corrupting dependent material.
- Know what is merely suggested, what is currently chosen, and what the writing assistant may actually use.

### Non-goals

This design does not optimize for generating a complete novel autonomously, producing the largest encyclopedia, enforcing a universal writing method, scoring originality numerically, or constructing an elaborate multi-agent system before the core interaction has been tested.

---

## 2. Repository-grounded diagnosis

The inspected branch already has much of the right foundation. Its project tabs are Chapters, Worldbuilding, Characters, Plot & themes, and Notes. Its AI actions distinguish drafting chapters from developing nonchapter documents. Generated material remains a candidate until explicit review and Apply, and navigation is not meant to trigger paid generation. Structured development is scoped to author-only nonchapter work rather than automatically becoming manuscript authority. These are strengths to preserve. [R1, R2]

The inspected `Writer.tsx` nevertheless gives nonchapter material essentially the same document-editor surface: heading, development action, manuscript-formatting controls, text page, and writing-assistant panel. The empty state asks for an idea, but does not itself offer a richer method of discovering and comparing possibilities. [R3]

Also, `projectTabs.ts` explicitly defaults a blank project to Chapters. For this proposed workflow, a new project should instead offer a lightweight choice between developing a story and starting to write. An author choosing development should not have to create a chapter first. Existing projects should continue opening where their author last worked. [R2]

The gap is therefore not principally missing categories. It is that **“develop this document” remains the interaction unit**, whereas the difficult creative work is often **“help me make and connect this decision.”** That diagnosis is an inference from source and documentation, not a claim based on running the application or observing authors use it.

---

## 3. Research and product precedents

This is a targeted, SOTA-informed design synthesis, not an exhaustive competitive benchmark or a claim of demonstrated superiority.

| Precedent | Relevant documented capability | Lesson for this design |
|---|---|---|
| Sudowrite | Story Bible connects braindump, synopsis, characters, worldbuilding, outline, and drafting; authors can enter at different points. [S1] | Persist useful story material and support guided development, without requiring a completed synopsis. |
| Novelcrafter | Codex, configurable AI context, relations, and extraction of entries or outlines from chat/snippets. [S2] | Conversation should produce reusable project material without copy-and-paste bookkeeping. |
| Plottr | Visual planning, tags, customizable character attributes, and templates. [S3] | Make material browseable and configurable, but do not confuse organization with invention. |
| World Anvil | Guided, customizable templates for many kinds of worldbuilding material. [S4] | Offer good questions and specialist lenses, progressively rather than as a wall of blank fields. |
| TaleStream, UIST 2023 | Uses tropes as story-building elements with steerable suggestions. [S5] | Tags and tropes can be useful creative vocabulary, not merely filing labels. |
| Luminate, CHI 2024 | Structures exploration around dimensions and multiple outputs that users can evaluate and synthesize. [S6] | Generate contrasting possibilities, reveal what differs, and let the author combine parts. |
| IBM's generative-AI design research | Emphasizes multiple outcomes, exploration, user control, and understandable interaction. [S7] | Design for uncertainty and correction instead of presenting the first response as the answer. |

None of these sources establishes that this exact proposed combination will improve long-form novel quality. That remains a product hypothesis to test with authors.

---

## 4. Information architecture

### Two primary modes

Use **Develop** and **Write** as the main mode switch. Keep the **Story Bible** available from either mode as a reference view, not as a third disconnected copy of the project.

**Develop** opens the Story Workshop. **Write** opens the existing chapter-oriented workspace. The Story Bible is a readable projection of chosen project material, with its source documents and status visible.

Within Develop, provide these lenses:

| Lens | Purpose |
|---|---|
| Overview | Current direction, recent decisions, saved possibilities, and resume point. |
| World | Places, systems, institutions, history, cultures, everyday life, and sensory texture. |
| People | Characters, relationships, groups, loyalties, and differing perspectives. |
| Themes & tone | Dramatic questions, emotional intentions, recurring contrasts, and voice experiments. |
| Story possibilities | Conflicts, mysteries, promises, progression, possible arcs, and optional endings. |
| Notebook | Fragments, inspirations, research notes, and ideas not yet assigned a role. |

These are views and entry points, not compulsory stages. A relationship may be opened from People or from a faction in World. A theme may arise from a character decision rather than being chosen in advance.

Reuse the existing material categories underneath where practical. Do not introduce an independent database merely to recreate the current documents with different names.

### Default layout

```text
┌──────────────────────────────────────────────────────────────────────────┐
│ Project name                  DEVELOP | WRITE           Model · Settings │
├───────────────┬────────────────────────────────────┬─────────────────────┤
│ Overview      │ DEVELOP: ACCESS TO MAGIC           │ Working story       │
│ World         │                                    │                     │
│ People        │ What makes learning difficult?     │ Chosen              │
│ Themes & tone │                                    │ • Earned progress   │
│ Possibilities │ [Knowledge] [Resources] [Risk]      │ • Repair apprentice │
│ Notebook      │                                    │                     │
│               │ A             B             C      │ Keep fixed          │
│ Recent work   │ Distinct      Distinct      Distinct│ • No inherited gift │
│               │ direction     direction     direction│                   │
│               │                                    │ Still open          │
│               │ Selected details / working version │ • Who controls      │
│               │                                    │   apprenticeship?   │
│               │ Tell me what to keep or change…    │                     │
│               │                  [Explore options] │ Context used        │
└───────────────┴────────────────────────────────────┴─────────────────────┘
```

The central surface is a **workbench**, not a scrolling transcript with cards appended beneath it. Discussion remains available, but its useful results are promoted into the working version and decision history.

The right panel is collapsible. On smaller windows, cards become a comparison list, and context opens as a drawer. Do not compress three unreadable columns into a small laptop viewport. Preserve keyboard access, visible focus, non-color-only states, native save feedback, and input-method composition behavior.

### Visual priority

Show the current creative question and selected ideas first. Keep model controls accessible but visually subordinate. Do not place metrics, graph nodes, readiness dashboards, and prompt engineering controls around every decision. Reuse the existing visual system rather than adding a competing design language.

---

## 5. Entry experience: start with attraction, not administration

The opening prompt should be:

> What are you excited about?

Accept a sentence, paragraph, dialogue fragment, collection of tags, existing notes, or an already-developed setting. Project naming is optional at this point; a temporary title is sufficient.

Three entry actions are enough:

**Start with an idea** — free-form input.  
**Help me find a direction** — a small set of examples or guided comparisons.  
**Bring existing notes** — preserve originals and propose an editable organization.

Do not require genre, target length, protagonist, plot structure, viewpoint, ending, and a complete synopsis before the first useful interaction.

### Editable interpretation

After the author's first explicit AI action, show a short interpretation separated into:

- **You said:** content directly present in the input.
- **Possible direction:** tentative interpretation or suggestion.
- **Still open:** a small number of consequential uncertainties.

The author can correct the interpretation in place. Do not convert inferred preferences into permanent settings. Ambiguous existing notes should remain ambiguous until the author resolves them.

### Example

Author input:

> I want a progression fantasy about someone who repairs broken magic. I like exploration and competent rivals, but not an inherited special bloodline or a harem. It should feel hopeful without making everything easy.

The workbench offers an editable direction:

**Reader experience:** discovery, earned competence, hard-won hope.  
**Starting attraction:** repairing magic rather than merely wielding a stronger attack.  
**Avoid:** inherited exceptionalism; harem-centered relationships.  
**Not decided:** setting scale, access to knowledge, cost of magic, protagonist background.

Then it asks one productive question:

> What makes magical knowledge difficult to acquire?

It also allows “Explore the city instead,” “Show a different question,” and “Make some suggestions.” The author is not trapped in an interview.

---

## 6. The core interaction: contrasting directions and selective synthesis

### Candidate cards

The default exploration produces **three short, meaningfully different options**. Three is a starting design choice to test, not a proven optimal count. Each card should generally fit a concept, a concrete detail, and its main implications into roughly 80–140 words, with detail expandable.

Cards should differ on an explicit dimension relevant to the task. Different names wrapped around the same mechanism do not count as useful diversity.

For the example question:

| Direction | Core mechanism | Likely story emphasis |
|---|---|---|
| Knowledge monopoly | Guilds restrict teaching and certify legal repairs. | Apprenticeship, secrecy, responsibility, access. |
| Resource frontier | Repair depends on scarce components recovered from damaged places. | Expeditions, salvage, ecological consequences, logistics. |
| Civic contracts | Magic functions through enforceable agreements between communities and practitioners. | Negotiation, institutions, obligations, competing public interests. |

“Likely emphasis” is a creative interpretation, not a guaranteed causal outcome.

### Card actions

Expose only the most relevant actions prominently: **Develop this**, **Select details**, and **Save for later**. Keep more specialized actions in a menu.

Selecting details adds them to a visible working version or selection tray. It does not immediately change accepted project material. The author can then request:

> Keep the guild apprenticeship, add salvage expeditions, and make the guild's safety concerns partly justified.

The response should present the combined working version and identify substantive changes. It should not silently replace every detail with a new concept.

### Refinement actions

The same interaction grammar works across worlds, characters, and themes:

| Action | Expected behavior |
|---|---|
| Make it concrete | Replace abstraction with a specific practice, event, place, behavior, or relationship. |
| Show consequences | Suggest effects elsewhere in the world, explicitly labeled as possibilities or conditional implications. |
| Give alternatives | Vary the selected dimension while preserving chosen invariants. |
| Challenge it | Identify a specific tension, loophole, or missing assumption without inventing a compulsory fix. |
| Add ordinary life | Explore work, food, rituals, travel, humor, and lived experience. |
| Try a moment | Produce a small noncanon vignette or interaction to test feel. |

These actions should not all appear as equal-weight buttons on every card. Recommend the action that fits the current task and expose the rest on demand.

### Direct manipulation and chat

Every important part of a working version is editable directly. The author should not have to say “change the third sentence” in chat to fix it.

Natural-language steering should work at a selected scope: a detail, relationship, character, location, or complete proposal. Display that scope above the composer. An instruction to revise a guild's recruitment should not rewrite its history unless the author expands the scope.

### Adoption

The final action is **Use this version**. A compact preview shows where the selected material will go and whether it adds to or replaces existing content. An author may adopt a coherent packet in one review; the system should not demand separate approval for every sentence.

The alternatives remain recoverable in session history. “Use this version” is distinct from saving a candidate and distinct from making it available to a later writing request.

---

## 7. Tags and preferences: a steering language, not a giant checklist

### Two visible choices, optional strictness

A tag has three ordinary states: neutral, **Want**, and **Avoid**. A small advanced control can make a preference a hard **Must** or **Never** constraint.

Neutral means unspecified. Not choosing romance is not the same as excluding romance. A wanted element is a direction, not an instruction to mention it in every response.

For example:

```text
WANT:   earned progression · exploration · competent rivals
AVOID:  inherited exceptionalism · repetitive humiliation
NEVER:  harem-centered protagonist relationships
```

A hard constraint is an author instruction that the system must preserve and check as carefully as possible. It is not a promise that an LLM can never violate it. Mechanical checks can enforce protected data fields; narrative compliance still requires inspection and potentially model-assisted review.

### Scope

Preferences apply to a **project**, **element**, or **current exploration**. Scope is always visible. A local preference cannot silently override a project-level hard exclusion.

“Low romance” at project level and “this character is romantically impulsive” may coexist. “No resurrection exists in this world” and “this character is resurrected” require an explicit decision. Do not resolve either conflict silently.

An eventual story requirement must also have temporal meaning where necessary. “Found family is central” does not mean the protagonist already has a trusted family in the opening scene.

### Definitions

Tags need editable meanings and examples. “Dark” may mean danger, moral ambiguity, bleakness, visual atmosphere, or graphic violence. Do not quietly bind all those meanings together.

Keep separate families for genre/tradition, reader experience, story ingredients, relationship dynamics, world mechanisms, themes, style, and content boundaries. Organizational labels such as “Book 2” or “Needs review” are not creative instructions unless explicitly designated as such.

A tag is a shortcut to a natural-language preference. The author's wording is the authority. A custom tag should work without waiting for a curated taxonomy update.

### Discovery

Offer a small context-sensitive set of suggestions, plus search and “Browse all.” Do not fill every screen with hundreds of chips. A large vocabulary may exist in the catalog without being the default interface.

Optional presets can suggest combinations for cultivation fantasy, progression fantasy, romance, mystery, or other traditions. Presets are editable starting points. They must not automatically import a fixed social order, moral worldview, cast structure, or plot formula.

### Preference learning

After a rejection, offer an optional explanation such as “Too familiar,” “Wrong mood,” or “Breaks a rule,” with free text available. Use that feedback for this exploration. Ask before promoting it to a broader project preference.

Do not infer “the author dislikes romance forever” from rejecting one romantic subplot. Do not build an opaque permanent taste profile.

### Subversion is a separate operation

“Subvert the chosen-one trope” is not equivalent to “include chosen one” or “exclude chosen one.” Let the author choose a convention and explore transformations: invert the power relationship, change who pays the cost, literalize the metaphor, or preserve the emotional reward while replacing the mechanism.

---

## 8. World development: make systems and places tangible

A worldbuilding lens should begin with what the author cares about, not demand a continent map and creation myth.

A useful working unit is a **world slice**: a place and the systems, people, routines, and questions surrounding it. A world-first author can deepen and connect slices until the setting feels sufficiently rich.

For the repair-magic city, the initial slice might include a neighborhood repair shop, a licensing guild, a dangerous salvage route, a public safety ritual, and a disagreement about sharing discoveries. The author can accept only some of this packet.

### Consequence exploration

Selecting a chosen decision should offer **What might this change?**

If guilds control access to magical repair knowledge, the system might propose informal teaching networks, inspection rituals, mistrust of unlicensed repairs, or disputes about affordability. These are optional developments. They do not logically follow without further assumptions.

Each consequence should show its basis:

> **Possible institution:** neighborhood repair cooperatives.  
> **Based on:** restricted guild teaching + the author's preference for hopeful community problem-solving.  
> **Assumption:** residents can perform limited repairs safely with shared instruction.

The author can accept the idea, reject the assumption, or ask for a contrasting implication.

### Development lenses

The World lens can expose four compact questions before deeper templates:

**How it works:** rules, resources, costs, and exceptions.  
**How people live:** routines, labor, access, customs, and sensory experience.  
**Who disagrees:** different interests, interpretations, and institutions.  
**What remains unexplored:** history, distant places, unknown mechanisms, and author-intended mysteries.

These are not a mandatory completeness checklist. A quiet place can be interesting without hiding a rebellion. Humor, beauty, hospitality, and strange ordinary habits are legitimate outcomes.

### Progressive depth

Keep short summaries readable by default. Expand into the full dossier only when requested. Let users decide whether they are sketching, developing, or documenting an element. A village should not demand the same documentation burden as the central city.

Suggested next questions should consider the scope the author is actually exploring. “Which currency do merchants use?” may be useful for an economic story and irrelevant for a brief dream sequence.

---

## 9. Characters and relationships: behavior before biography

Character creation should support appearance and background, but should not make those the main discovery method.

Start with an interesting pressure or contradiction:

> What do they want that their current habits make difficult to obtain?

For the repair apprentice, directions might include a cautious diagnostician who must become publicly accountable, an ambitious salvager who undervalues maintenance, or an excellent craftsperson who cannot stop rescuing other people's failing projects.

An optional character spine contains desire, valued commitment, competence, costly habit, relationship pressure, and capacity for change. It is a lens, not a rule that every protagonist needs trauma, a false belief, or a redemption arc.

### Test behavior

Use **Put them in a situation** to explore the character through choices. For example:

> A rival publicly takes credit for a successful repair, but exposing the truth would reveal that both used a prohibited technique.

Offer different possible responses and let the author identify what feels right. Keep the response tentative until accepted. Characters are not discovered objectively by repeatedly asking an LLM to role-play them.

### Relationships as first-class material

Develop a relationship between two existing people or groups, not just independent biographies. Useful prompts include what each wants from the other, what each misunderstands, what keeps them connected, and what could change the relationship.

For a competent rival, explore specific disputes: one prioritizes public access to knowledge, while the other has seen uncontrolled repairs kill people. They can respect each other's competence without becoming interchangeable or conveniently stupid.

The relationship record should link both participants and preserve directional differences. “A trusts B” does not automatically mean “B trusts A.” A local relationship view should be the default; a complete network graph is optional, not the primary workspace.

### Language

Support names in their original script, aliases, and optional transliterations without assuming that every character follows one naming convention. Preserve the author's chosen writing language. Do not automatically translate lore while organizing it.

---

## 10. Themes, tone, and voice: explore an experience

Selecting a theme should open questions and contrasts, not assign a moral answer to the novel.

“Power” might become:

> Who gets to decide whether a dangerous discovery should be shared?

The system can connect competing responses to characters, institutions, and possible situations. A guild may emphasize responsibility, a cooperative may emphasize access, and the protagonist may initially underestimate both risks. None of these viewpoints must be the author's final conclusion.

Tone controls should distinguish the author's desired reader experience from content intensity. A world can contain serious consequences while retaining warmth and hope.

### Taste tests

Offer a short optional **Try a moment** using the same situation in two or three treatments: practical and intimate, wondrous and reflective, or brisk and adventurous. The author selects or edits passages that match the desired experience.

Tests are labeled **Noncanon experiment** and excluded from drafting context by default. The system may propose specific voice guidance based on the author's selections, but the author must confirm it. An attractive sentence does not authorize importing the scene's events into the world.

Allow the author to provide their own short samples and explanations. Extract qualities such as sentence density, viewpoint distance, humor, exposition tolerance, and dialogue rhythm. Avoid reducing voice to a single “more literary” slider.

---

## 11. Story possibilities and long-running potential

This area remains optional while the author builds the world. It explores what kinds of stories the chosen material makes possible without requiring a chapter outline.

A useful question for a serial is:

> What can keep producing new challenges and satisfying changes without simply repeating the same conflict at a larger number?

For the example, a possible engine is:

> Encounter a broken magical system → investigate its failure → acquire or develop a repair method → make a consequential choice about its use → change local lives and obligations → discover a larger connection.

That engine is only a proposal. The author can reject its episodic structure or blend it with political, relational, or exploratory development.

### Progression and promise

Explore what counts as progress: capability, understanding, influence, belonging, freedom, restored places, or changed relationships. Do not equate progression with numeric levels unless the author chooses that format.

Track promising unresolved questions, intended payoffs, and possible arc directions separately from committed events. An intriguing possibility for a future volume should not become a fixed chronology merely because the model mentioned it.

Offer **Sketch one possible arc**, **Explore a different scale**, and **Leave the ending open**. Do not request 200 chapter summaries as evidence that the world is ready.

---

## 12. Adaptive guidance without an endless interview

A session should usually have one main creative question. The author can also work freely, accept a batch of suggestions, or jump to another part of the project.

The recommended next question should have a visible reason:

> Decide how unlicensed repairs are discovered. This affects the guild's authority, the protagonist's opportunities, and the rival's objection.

Prefer questions that materially affect the author's current focus or several important unresolved ideas. Do not surface every missing template field. Do not ask for information already present in chosen material.

“Not now,” “Not relevant,” and “Keep mysterious” are different choices. Not now defers a decision; not relevant removes an unnecessary task; keep mysterious preserves an intentional uncertainty. Author-unknown and reader-unknown are also different states.

### A satisfying stopping point

At the end of a session, show a small recap derived from actual changes:

**We developed:** the apprenticeship system and the rival's position.  
**You chose:** restricted training with partly justified safety rules.  
**Still worth exploring:** who pays for public repairs.  
**Next time:** resume the neighborhood workshop.

This recap should be a projection of saved decisions where possible, not an automatic paid summarization on close.

Do not show “Worldbuilding 82% complete.” A better handoff to writing is a list of specific decisions already available and specific questions still open. The author can write at any point and continue development later.

---

## 13. Trust, reversibility, and the meaning of a decision

### Separate the important axes

Do not compress every state into one “canon” toggle.

**Decision status:** an idea is saved for exploration, chosen for the current working story, or archived/superseded.  
**Protection:** selected content may be marked Keep fixed, independently of its status.  
**Access:** material may remain author-room-only or be explicitly approved for a particular writing use.  
**Narrative evidence:** what the manuscript establishes is tracked separately from what the author intends.  
**Character knowledge:** who knows a fact at a particular point is separate again.

A chosen future betrayal can remain author-only. A character's belief can be false in the setting. A discarded alternative must not become writing context because it contains relevant keywords.

### Preserve intent

A decision should retain a short author-editable explanation of why it was chosen, preferably using the author's own words:

> Keep the guild morally mixed. It should protect people from dangerous repairs and also protect its own status.

This explanation is useful when later suggestions try to simplify the guild into a cartoon villain. It is not hidden model reasoning; it is a visible record of author intent.

### Keep fixed

Protect a selected proposition or passage. Future proposals must identify any request to change it rather than quietly replacing it. Protected literal fields can be checked mechanically. The model's interpretation of a prose proposition remains imperfect and should not be advertised as a semantic guarantee.

### What-if branches

Later versions can support lightweight forks of a development session:

> What changes if repair knowledge is public rather than restricted?

The alternate remains isolated from the working story. The interface compares changed decisions and likely affected material. Accepting it proposes changes; it does not silently rewrite existing documents or chapters.

### Change impact

When a chosen foundational rule changes, show **Affected material** with reasons and links. Existing entries are flagged for review, not automatically “fixed.” Distinguish a clear contradiction, a possible tension, a dependent assumption, and a stylistic suggestion. Do not demand that the author repair intentional ambiguity.

---

## 14. Context that supports the UX

A visible **Using for this exploration** panel should show the actual selected context: project direction, relevant preferences, current element, chosen related material, protected decisions, and any deliberately included alternatives.

Default context should exclude unrelated chat, rejected candidates, and noncanon tests. Include an explicit rejection rationale when it helps avoid repeating an unwanted idea, rather than injecting the whole rejected text as though it were story material.

The author should be able to say **Explore outside my current direction** for one session without deleting their project preferences. Hard exclusions remain in force unless explicitly changed.

Do not promise that the model always sees everything. Prefer a compact, relevant, inspectable packet, with excluded or omitted material honestly identified. If key material cannot fit, make the omission visible rather than presenting a misleading “full context” badge.

A development context and a chapter-writing context are different products. Approved author-room intent should cross into restricted writing only through the existing explicit eligibility and briefing mechanisms, extended where necessary. [R1]

---

## 15. Operational UX

Generation is always an explicit author action. Opening a project, choosing a lens, checking a tag, or expanding a saved card should be local unless the UI clearly offers a generation action. Preserve the branch's no-generation-on-navigation contract. [R1]

Show generation scope, active model, and request status. Candidate generation, partial results, failure, cancellation, and adoption have distinct states. Save author edits independently from model requests.

During a request, keep the current workbench usable where safe. Never replace the author's revised working version when an older response arrives. A stale response can be saved as an alternative and explicitly rebased or regenerated; it cannot be automatically adopted.

Do not imply cancellation guarantees zero additional provider cost. Show a clear stopped/request-canceled state and retain any useful partial candidate without treating it as a complete proposal.

Offline, manual development, preferences, history, and local organization remain usable. Generation controls explain the unavailable connection rather than turning the whole development workspace into a disabled screen.

---

## 16. Minimal product contracts

Keep the implementation subordinate to the interaction design.

### Reuse first

Preserve the native desktop shell, local persistence, existing editor save boundaries, scoped proposal review, history, model selection, and context eligibility. The inspected branch already documents these foundations. [R1–R3]

Add a dedicated development surface instead of expanding `Writer.tsx` into a conditional UI for every creative workflow. The current generic document editor should remain available as a full editing view.

### Small set of additional concepts

| Concept | Minimum responsibility |
|---|---|
| Development session | Current focus, author brief, selected context, working version, and recoverable alternatives. |
| Preference | Meaning, polarity, strength, scope, and explicit author confirmation. |
| Candidate | Proposed content, differentiating dimensions, assumptions, affected targets, and source context. |
| Decision record | Chosen material references, author rationale, status, protection, and supersession history. |
| Relationship | Typed, directional link between existing material, with its own uncertainty/status where needed. |

These can be logical concepts mapped to existing records where appropriate. This specification does not require five new independent storage systems.

### Avoid two competing story bibles

Keep document/section content authoritative for the actual saved material. Workshop sessions and decisions reference stable content identities and revisions. A readable Story Bible should be a view or export of that material, not another editable AI-generated summary that can drift independently.

Any extracted structured facet must identify its source revision. Editing the source invalidates or refreshes the facet through an explicit process rather than leaving two contradictory truths.

### Adoption boundary

The first vertical slice can adopt into one author-room nonchapter document using the existing proposal boundary. Cross-document adoption is a later capability and must not be assumed to exist today.

For a later proposal that updates a guild, a character, and a relationship together, show a complete preview and require an atomic, revision-checked commit. Partial adoption must explicitly account for dependent changes; it should not strand a relationship pointing to an uncreated character.

A stale multi-target proposal must be blocked or rebased explicitly. No chapter mutation should be smuggled into an author-room development operation.

### Generation policy

Start with one bounded model request per explicit exploration action where feasible. Use structured response validation and existing model infrastructure. Add a separate, bounded critique pass only where user testing shows a benefit. A swarm of fictional specialists is not necessary to prove the core UX.

---

## 17. Build order

### Slice 1 — prove the loop

Implement one worldbuilding workflow end to end:

1. Start from an idea or existing note.
2. Set a few Want/Avoid preferences, with clear scope.
3. Generate three contrasting directions.
4. Select details and refine a working version.
5. Preserve selected details across another generation.
6. Review and adopt into one existing nonchapter document.
7. Close and reopen with session state, candidates, and adopted material intact.

The existing chapter-writing path must remain unchanged. This slice should be useful even without relationship graphs, sophisticated automated critique, or a huge tag catalog.

### Slice 2 — connect the material

Add consequence exploration, character/relationship lenses, author rationale, context previews, and carefully specified cross-document adoption. A single decision should become connected material without manual copy-and-paste.

### Slice 3 — deepen author control

Add lightweight what-if branches, change-impact review, noncanon taste tests, serial-promise exploration, and exportable project presets. Introduce richer views only after authors can reliably understand their states.

### Defer deliberately

Defer a large public template marketplace, a global node graph as the home screen, automatic all-world generation, multi-agent debates, proprietary originality scores, and mandatory lore-completeness tracking. These are not prerequisites for a strong development experience.

---

## 18. Evaluation and acceptance

The central product hypothesis is that this interaction improves **author-controlled development**, not merely output volume.

Run a small formative study with authors who have different working habits, including world-first and discovery-oriented writers. Compare the existing generic Develop flow, a tag-heavy form, and the proposed workshop using the same model and comparable generation budgets. Counterbalance task order and use different but comparable seeds to reduce simple carryover. A small study is formative evidence, not proof of universal superiority.

### Questions to observe

Can an author start with an incomplete idea and identify a direction they genuinely want to keep? Can they explain how candidates differ? Can they preserve a liked detail while changing another? Can they resume later without reading a long transcript? Can they tell which material will influence a chapter request?

Measure time to a knowingly endorsed decision, author-reported ownership, retained usefulness on a later revisit, corrections needed to preserve intent, and navigation/re-prompting burden. Do not optimize exclusively for low decision time or high acceptance rates; fast agreement with AI can be a bad outcome.

Assess coherence and specificity through human review of comparable material. Model-only quality scores should not establish success.

### Behavioral acceptance cases

| Scenario | Required behavior |
|---|---|
| Author enters no genre | Exploration still works without choosing a forced genre template. |
| Author wants world-first work | No protagonist, plot, or chapter gate blocks development. |
| A tag is not selected | It remains unspecified, not excluded. |
| Local request conflicts with a hard project rule | The conflict is surfaced; there is no silent override. |
| Author changes one selected detail | Unselected protected content is not overwritten. |
| Author rejects an option | It remains recoverable but is not active writing context. |
| A request completes after the working version changes | The result is not automatically applied. |
| Author switches to Chapters | Unsaved work is handled through the existing save boundary; no generation occurs. |
| An author-only secret is chosen | It does not become character knowledge or unrestricted drafting input. |
| A vignette introduces an attractive new fact | That fact remains noncanon unless explicitly adopted. |
| A core decision changes | Affected material is flagged for review, not silently rewritten. |
| Author resumes offline | Saved material, preferences, and session state remain usable. |

The release criterion is not “can generate a complete world.” It is “can help an author develop and safely retain a world they recognize as their own.”

---

## Appendix A — Example starter vocabulary

This is a proposed starter catalog, not a literary taxonomy or an exhaustive set. Show only a small context-relevant selection at a time. Authors may redefine or add entries.

| Family | Example entries |
|---|---|
| Reader experience | wonder, earned competence, intimacy, suspense, catharsis, discovery, warmth, unease, exhilaration, melancholy |
| Story emphasis | exploration, repair, investigation, survival, romance, political change, community building, artistic ambition, recovery, rivalry |
| Relationship dynamics | found family, uneasy allies, respectful rivals, mentor tension, estranged siblings, competing loyalties, intergenerational care, slow trust |
| World mechanisms | scarce resources, restricted knowledge, public infrastructure, contractual power, ecological limits, inherited institutions, disputed history, trade dependence |
| Social texture | hospitality rituals, professional pride, informal markets, seasonal work, public festivals, shared maintenance, local humor, status etiquette |
| Character tendencies | observant, ambitious, cautious, generous, status-conscious, duty-bound, improvisational, skeptical, playful, stubborn |
| Thematic tensions | freedom versus responsibility, belonging versus autonomy, memory versus reinvention, mercy versus justice, access versus safety, repair versus replacement |
| Voice qualities | concrete, reflective, brisk, restrained, lyrical, dryly humorous, intimate, dialogue-forward, sensory, deliberately sparse |
| Optional conventions | academy, tournament, cultivation, system interface, regression, hidden identity, heist crew, closed-circle mystery, apprenticeship, political marriage |

Content boundaries should use clearly defined author language rather than ambiguous shorthand. Separate presentation limits, prohibited mechanisms, and structural preferences.

---

## Appendix B — Complete example session

**Seed:** A hopeful progression story about repairing broken magic.

**Direction:** Earned competence, exploration, competent rivals. Avoid inherited special power and harem structure.

**First exploration:** Why is magical repair difficult to learn? Compare restricted knowledge, scarce resources, and civic contracts.

**Selection:** Keep restricted apprenticeship and dangerous salvage. Do not adopt civic-contract magic.

**Refinement:** The guild has real safety expertise and also benefits from exclusion. Preserve that ambiguity.

**World slice:** A workshop district depends on licensed repairs for flood barriers. Salvaged components can lower costs but have unpredictable failure modes. These are proposed specifics, not assumed facts until chosen.

**Ordinary life:** Offer a public inspection day, neighborhood repair exchanges, or apprentices listening to faults through tuned instruments. The author keeps the instruments and repair exchanges.

**Character pressure:** The protagonist can diagnose subtle failures through trained observation, but lacks credentials. Their ability is learned, not an inherited exception.

**Relationship:** A licensed rival respects the protagonist's diagnostic skill but distrusts the uncontrolled distribution of repair techniques. The author chooses mutual respect rather than immediate hostility.

**Theme:** Who should decide whether dangerous knowledge is shared? Keep the question open.

**Stress test:** A neighborhood barrier is failing; a cheap repair would require an unlicensed technique. Explore responses by the protagonist, rival, and residents. This is a noncanon test until specific material is adopted.

**Adoption preview:** Update the repair system and guild document; create or update the protagonist and rival; add a directional relationship; save the thematic question. In Slice 1, keep this as one coherent nonchapter packet. In a later multi-document implementation, adopt transactionally with revision checks.

**Remaining questions:** How are repair failures investigated? Who bears liability? Does the protagonist want recognition, independence, or affordable repairs for their community?

**Stop:** The author can now deepen the city, explore another place, refine the people, or begin writing. No chapter outline is required, and the world is not labeled complete.

---

## Sources

### Repository sources at the pinned commit

**R1.** `docs/ADR_0027_AI_WRITING_WORKSPACE.md` — workspace areas, Draft/Continue/Develop, explicit requests and Apply, author-room scope, and verification boundary.  
`https://github.com/FZWINGEL/WebnovelStudio_V3/blob/085e8e13325639938ff0c0752972156dc7d705d1/docs/ADR_0027_AI_WRITING_WORKSPACE.md`

**R2.** `apps/desktop/src/shell/projectTabs.ts` — category mapping, saved tab preferences, and blank-project chapter default.  
`https://github.com/FZWINGEL/WebnovelStudio_V3/blob/085e8e13325639938ff0c0752972156dc7d705d1/apps/desktop/src/shell/projectTabs.ts`

**R3.** `apps/desktop/src/shell/Writer.tsx`, especially the rendering section inspected from line 225 onward — common editor/development surface and assistant integration.  
`https://github.com/FZWINGEL/WebnovelStudio_V3/blob/085e8e13325639938ff0c0752972156dc7d705d1/apps/desktop/src/shell/Writer.tsx`

### External primary sources

**S1.** Sudowrite, “How Sudowrite Works: Story Bible, Muse, and the Tools,” September 2026. Product documentation/description, not independent quality evidence.  
`https://sudowrite.com/blog/how-sudowrite-works/`

**S2.** Novelcrafter, official feature overview and product site, accessed 7 September 2026.  
`https://www.novelcrafter.com/features`  
`https://www.novelcrafter.com/`

**S3.** Plottr, official features, accessed 7 September 2026.  
`https://plottr.com/features/`

**S4.** World Anvil, official worldbuilding templates, accessed 7 September 2026.  
`https://www.worldanvil.com/features/worldbuilding-templates`

**S5.** Chou et al., “TaleStream: Supporting Story Ideation with Trope Knowledge,” UIST 2023; Adobe Research publication page.  
`https://research.adobe.com/publication/talestream-supporting-story-ideation-with-trope-knowledge/`

**S6.** Suh et al., “Luminate: Structured Generation and Exploration of Design Space with Large Language Models for Human-AI Co-Creation,” CHI 2024; authors' project page.  
`https://luminate-research.github.io/`

**S7.** Weisz et al., “Toward General Design Principles for Generative AI Applications,” 2023, and “Design Principles for Generative AI Applications,” CHI 2024; IBM Research publication pages.  
`https://research.ibm.com/publications/toward-general-design-principles-for-generative-ai-applications`  
`https://research.ibm.com/publications/design-principles-for-generative-ai-applications`
