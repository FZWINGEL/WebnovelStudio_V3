/** Optional vocabulary, never a required form or an inferred author preference. */
export const LENSES = [
  { id: 'overview', label: 'Overview', question: 'What are you excited about?', reason: 'Begin with the part of this story you want to discover.' },
  { id: 'world', label: 'World', question: 'What makes this place worth spending time in?', reason: 'A place, practice, or ordinary routine can give the setting a concrete starting point.' },
  { id: 'people', label: 'People', question: 'What do they want that their habits make difficult?', reason: 'Pressure reveals choices before you need a complete biography.' },
  { id: 'themes', label: 'Themes & tone', question: 'Which question should the story keep returning to?', reason: 'Competing answers can shape people and institutions without dictating a moral.' },
  { id: 'possibilities', label: 'Story possibilities', question: 'What could keep changing without repeating the same conflict?', reason: 'Explore a source of satisfying developments while leaving the ending open.' },
  { id: 'notebook', label: 'Notebook', question: 'Which fragment would you like to return to?', reason: 'Keep an image, a line of dialogue, or a research note without assigning it a role yet.' },
] as const;
export type WorkshopLens = typeof LENSES[number]['id'];

export const ACTIONS = [
  { id: 'directions', label: 'Give alternatives', instruction: 'Offer three genuinely different directions. Name the dimension and concrete mechanism that differs in each.' },
  { id: 'concrete', label: 'Make it concrete', instruction: 'Develop the selected scope into specific practices, behavior, a place, or an event. Preserve the selected details and everything outside the selected scope.' },
  { id: 'consequences', label: 'What might this change?', instruction: 'Suggest three conditional consequences. For each, distinguish its source basis from additional assumptions. These are possibilities, not logical necessities.' },
  { id: 'challenge', label: 'Challenge it', instruction: 'Find a specific tension, loophole, or missing assumption. Offer contrasting ways to explore it without imposing a repair.' },
  { id: 'ordinaryLife', label: 'Add ordinary life', instruction: 'Explore work, food, ritual, travel, humor, beauty, or hospitality. A quiet place need not conceal a rebellion or a plot device.' },
  { id: 'situation', label: 'Put them in a situation', instruction: 'Use one concrete situation to offer three contrasting tentative choices by this person or relationship. Show commitments and pressure through behavior, not a required trauma or biography.' },
  { id: 'moment', label: 'Try a moment', instruction: 'Use the SAME situation in two or three short voice treatments with distinct sentence density, viewpoint distance, humor, exposition, or dialogue rhythm. Label all events noncanon experiments. Do not derive adopted guidance or story facts automatically.' },
  { id: 'voiceGuidance', label: 'Propose voice guidance from this sample', instruction: 'Use the author-selected/current sample as voice evidence only, together with the optional author explanation below. Return exactly three distinct, reviewable STYLE guidance sets. Each candidate content must be STYLE instructions only, with concrete headings for Sentence density, Viewpoint distance, Humor, Exposition, and Dialogue rhythm. Do not quote, continue, or import the sample’s events, facts, characters, or canon. Do not adopt the sample automatically; the author will review any guidance before it is used.' },
  { id: 'arc', label: 'Sketch one possible arc', instruction: 'Offer optional story engines or arc possibilities. Distinguish unresolved promises and intended payoffs from actual events. Progress may mean understanding, belonging, influence, freedom, restored places, or relationships. Leave the ending open unless explicitly directed otherwise.' },
  { id: 'scale', label: 'Explore a different scale', instruction: 'Vary the scope and stakes without assuming larger numbers mean progress. Keep the chosen details and leave chronology open.' },
  { id: 'subvert', label: 'Subvert a convention', instruction: 'Transform the convention using the explicitly selected operation. Distinguish this from merely including or excluding it. Explain what emotional reward remains and what mechanism changes.' },
  { id: 'synthesize', label: 'Combine selected details', instruction: 'Make a coherent working proposal from the author-selected details. Preserve their literal wording unless the author explicitly edited them. Identify substantive changes and assumptions; do not quietly replace the chosen mechanism.' },
] as const;
export type WorkshopAction = typeof ACTIONS[number]['id'];
export const SUBVERSIONS = ['Invert the power relationship', 'Change who pays the cost', 'Literalize the metaphor', 'Keep the emotional reward, change the mechanism'] as const;

export const WORLD_QUESTIONS = [
  { title: 'How it works', text: 'Which rules, resources, costs, or exceptions matter here?' },
  { title: 'How people live', text: 'What does an ordinary day feel like here?' },
  { title: 'Who disagrees', text: 'Whose interests or interpretations differ, and why?' },
  { title: 'What remains unexplored', text: 'What do you want to leave unknown, distant, or mysterious?' },
] as const;

export const FAMILIES = ['Genre & tradition', 'Reader experience', 'Story ingredients', 'Relationships', 'World mechanisms', 'Themes', 'Style', 'Content boundaries'] as const;
export type PreferenceFamily = typeof FAMILIES[number];
export interface PreferenceSuggestion { label: string; meaning: string; family: PreferenceFamily; lenses: WorkshopLens[] }
export const PREFERENCE_SUGGESTIONS: PreferenceSuggestion[] = [
  { label: 'Earned progression', meaning: 'Abilities or influence grow through choices, practice, and meaningful costs; numeric levels are optional.', family: 'Reader experience', lenses: ['overview', 'people', 'possibilities'] },
  { label: 'Exploration', meaning: 'Discover unfamiliar places, practices, or ideas, with room for curiosity beyond plot utility.', family: 'Reader experience', lenses: ['overview', 'world'] },
  { label: 'Hard-won hope', meaning: 'Leave room for improvement and connection while allowing serious difficulty.', family: 'Reader experience', lenses: ['themes', 'overview'] },
  { label: 'Competent rivals', meaning: 'Rivals have credible skills and specific disagreements; respect need not imply agreement.', family: 'Relationships', lenses: ['people'] },
  { label: 'Found family', meaning: 'Chosen bonds may become important over time; this does not establish a trusted group at the opening.', family: 'Relationships', lenses: ['people', 'possibilities'] },
  { label: 'Inherited exceptionalism', meaning: 'A special ancestry gives a person an advantage or status others cannot earn.', family: 'Story ingredients', lenses: ['people', 'possibilities'] },
  { label: 'Institutional magic', meaning: 'Explore how teaching, access, safety, or administration shapes a magical practice.', family: 'World mechanisms', lenses: ['world'] },
  { label: 'Ordinary wonders', meaning: 'Unusual details appear in daily work, leisure, food, travel, or hospitality.', family: 'World mechanisms', lenses: ['world'] },
  { label: 'Responsibility and access', meaning: 'Explore competing answers about sharing consequential knowledge without assigning a final moral.', family: 'Themes', lenses: ['themes', 'world'] },
  { label: 'Intimate viewpoint', meaning: 'Stay near a viewpoint character’s perceptions and language; this is a voice preference, not permission to disclose secrets.', family: 'Style', lenses: ['themes', 'people'] },
  { label: 'Translated webnovel register', meaning: 'Optional English terminology and cadence inspired by translated webnovels; preserve the author’s chosen terms without changing language.', family: 'Style', lenses: ['themes'] },
  { label: 'Graphic violence', meaning: 'Explicit sensory detail of bodily injury; distinct from danger, serious consequences, or moral ambiguity.', family: 'Content boundaries', lenses: ['themes'] },
  { label: 'Harem-centered relationships', meaning: 'The protagonist’s relationships center on multiple concurrent romantic partners.', family: 'Content boundaries', lenses: ['people', 'themes'] },
  { label: 'Cultivation fantasy', meaning: 'An optional tradition involving cultivated abilities; social order, morality, cast, and plot remain undecided.', family: 'Genre & tradition', lenses: ['overview', 'world'] },
  { label: 'Mystery', meaning: 'Questions, evidence, and discovery shape reader expectations; a particular detective, crime, or ending is not implied.', family: 'Genre & tradition', lenses: ['overview', 'possibilities'] },
];

/** Presets select vocabulary only after review; they never create world or plot records. */
export const PRESET_SUGGESTIONS = [
  { name: 'Discovery and earned progress', labels: ['Earned progression', 'Exploration', 'Hard-won hope'] },
  { name: 'Cultivation as a starting vocabulary', labels: ['Cultivation fantasy', 'Earned progression', 'Translated webnovel register'] },
  { name: 'Questions and competing answers', labels: ['Mystery', 'Responsibility and access', 'Competent rivals'] },
] as const;
