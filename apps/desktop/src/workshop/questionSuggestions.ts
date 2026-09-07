import type { WorkshopSession } from '../ipc/workshop';
import { LENSES, WORLD_QUESTIONS } from './catalog';

type Question = { text: string; reason: string };
const QUESTIONS: Record<WorkshopSession['lens'], Question[]> = {
  overview: [
    { text: 'Which detail would you miss most if this idea changed?', reason: 'Identify what to preserve before exploring a different direction.' },
    { text: 'What experience would you like a reader to return for?', reason: 'A desired experience can connect otherwise unrelated fragments.' },
    { text: 'Which part would you like to leave undecided?', reason: 'Keeping a question open lets you develop the parts that interest you now.' },
  ],
  world: [...WORLD_QUESTIONS],
  people: [
    { text: 'Which commitment puts this person under pressure?', reason: 'A valued commitment can reveal behavior without requiring a full biography.' },
    { text: 'Who interprets this person differently, and why?', reason: 'A different perspective can expose relationship tension without assuming hostility.' },
    { text: 'When does their competence make a problem harder?', reason: 'A useful skill can also create a meaningful cost or blind spot.' },
  ],
  themes: [
    { text: 'Which two sincere answers could this story hold in tension?', reason: 'Competing answers keep a theme open rather than assigning the novel a moral.' },
    { text: 'Where could the desired reader experience appear in an ordinary moment?', reason: 'A small situation can test tone without changing the story’s content limits.' },
    { text: 'Which voice quality would you like to test in the same situation?', reason: 'Changing one quality makes the treatments easier to compare.' },
  ],
  possibilities: [
    { text: 'What could count as progress besides greater power?', reason: 'Understanding, freedom, belonging, or restored places can sustain change.' },
    { text: 'Which intended payoff needs an earlier promise?', reason: 'A future reward can suggest setup without becoming an established event.' },
    { text: 'What could vary when this story engine repeats?', reason: 'Different costs and choices can keep a serial from repeating the same conflict.' },
  ],
  notebook: [
    { text: 'Which fragment would you like to connect to another idea?', reason: 'A connection can make a note useful without assigning it a permanent role.' },
    { text: 'Which parts are observations, wishes, or unresolved questions?', reason: 'Separating these helps preserve uncertainty when organizing existing notes.' },
    { text: 'Which wording do you want to preserve exactly?', reason: 'A liked phrase can stay intact while the surrounding idea develops.' },
  ],
};
const normalize = (text: string) => text.trim().toLocaleLowerCase('en').replace(/\s+/g, ' ');

/** Local suggestions follow the active lens; semantic answer detection remains a model/author task. */
export function nextWorkshopQuestion(session: WorkshopSession, chosenText = '', suggestion?: Question): Question | undefined {
  const disposition = new Set(session.questions.map(question => normalize(question.text)));
  const alreadyWritten = normalize([session.workingText, chosenText].join('\n'));
  const possibilities = session.lens === 'possibilities' ? (session.storyPossibilities ?? [])
    .filter(item => item.kind === 'unresolvedQuestion' && item.status === 'open' && item.text.trim())
    .map(item => ({ text: item.text, reason: 'You kept this unresolved story question. Exploring it can inform the possibilities you are considering.' })) : [];
  const relationship = session.relationshipId ? [{ text: 'What does each participant want that the other misunderstands?', reason: 'This exploration concerns a directional relationship; separate perspectives can change the same interaction.' }] : [];
  const focus = LENSES.find(lens => lens.id === session.lens)!;
  const choices = [...possibilities, ...(suggestion ? [suggestion] : []), ...relationship, ...QUESTIONS[session.lens], { text: focus.question, reason: focus.reason }]
    .filter((question, index, all) => question.text.trim() && all.findIndex(item => normalize(item.text) === normalize(question.text)) === index);
  const start = choices.findIndex(question => normalize(question.text) === normalize(session.focusQuestion));
  return Array.from({ length: choices.length }, (_, offset) => choices[(start + offset + 1) % choices.length])
    .find(question => normalize(question.text) !== normalize(session.focusQuestion)
      && !disposition.has(normalize(question.text)) && !alreadyWritten.includes(normalize(question.text)));
}
