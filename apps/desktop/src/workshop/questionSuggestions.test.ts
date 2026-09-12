// @vitest-environment node
import { describe, expect, it } from 'vitest';
import type { StoryPossibility, WorkshopSession } from '../ipc/workshop';
import { LENSES } from './catalog';
import { nextWorkshopQuestion } from './questionSuggestions';

function session(lens: WorkshopSession['lens'] = 'overview', overrides: Partial<WorkshopSession> = {}): WorkshopSession {
  const focus = LENSES.find(item => item.id === lens)!;
  return {
    id: 'session-1', title: 'A minimal workshop session', lens, parentSessionId: null, branchKind: 'working',
    brief: '', direction: '', stillOpen: '', focusQuestion: focus.question, focusReason: focus.reason,
    focusDocumentId: null, anchorDocumentId: 'workshop-session-1', depth: 'sketch', outsideDirection: false,
    includedDocumentIds: [], workingText: '', workingTitle: '', workingGeneration: '1', selectedDetails: [], choices: [], questions: [],
    composer: '', selectedScope: 'Whole working version', originalNotes: '', activeRunId: null, ...overrides,
  };
}

function storyPossibility(id: string, kind: StoryPossibility['kind'], text: string, status: StoryPossibility['status'] = 'open'): StoryPossibility {
  return { id, kind, text, status };
}

describe('nextWorkshopQuestion', () => {
  it('prioritizes a kept unresolved question on the possibilities lens initial focus', () => {
    const kept = storyPossibility('kept-question', 'unresolvedQuestion', 'Who left the broken compass?');
    const next = nextWorkshopQuestion(session('possibilities', {
      storyPossibilities: [
        kept,
        storyPossibility('payoff', 'intendedPayoff', 'The compass reveals a safe route.'),
        storyPossibility('arc', 'possibleArc', 'Follow the mapmaker beyond the harbor.'),
      ],
    }));

    expect(next).toEqual({
      text: kept.text,
      reason: 'You kept this unresolved story question. Exploring it can inform the possibilities you are considering.',
    });
  });

  it('uses a fallback relevant to the active lens rather than another lens', () => {
    const next = nextWorkshopQuestion(session('world'))!;

    expect([
      'Which rules, resources, costs, or exceptions matter here?',
      'What does an ordinary day feel like here?',
      'Whose interests or interpretations differ, and why?',
      'What do you want to leave unknown, distant, or mysterious?',
    ]).toContain(next.text);
    expect(next.text).not.toBe('Which detail would you miss most if this idea changed?');
    expect(next.text).not.toBe('Which commitment puts this person under pressure?');
  });

  it('suppresses saved dispositions and questions already represented in current or chosen text', () => {
    const currentQuestion = 'An unresolved question already represented in current notes';
    const chosenQuestion = 'An unresolved question already represented in chosen notes';
    const savedDisposition = 'Which intended payoff needs an earlier promise?';
    const available = 'What remains genuinely unexplored?';
    const next = nextWorkshopQuestion(session('possibilities', {
      workingText: `Current working notes: ${currentQuestion}`,
      questions: [{ id: 'saved', text: savedDisposition, reason: 'For later', status: 'notNow', unknownTo: 'both' }],
      storyPossibilities: [
        storyPossibility('current', 'unresolvedQuestion', currentQuestion),
        storyPossibility('chosen', 'unresolvedQuestion', chosenQuestion),
        storyPossibility('available', 'unresolvedQuestion', available),
      ],
    }), `Chosen material repeats: ${chosenQuestion}`)!;

    expect(next.text).toBe(available);
    expect(next.text).not.toBe(currentQuestion);
    expect(next.text).not.toBe(chosenQuestion);
    expect(next.text).not.toBe(savedDisposition);
  });

  it('ignores archived, blank, and non-question possibilities before using the lens fallback', () => {
    const next = nextWorkshopQuestion(session('possibilities', {
      storyPossibilities: [
        storyPossibility('archived', 'unresolvedQuestion', 'Archived question', 'archived'),
        storyPossibility('blank', 'unresolvedQuestion', '   '),
        storyPossibility('payoff', 'intendedPayoff', 'A payoff that is not a question.'),
        storyPossibility('arc', 'possibleArc', 'An optional arc direction.'),
      ],
    }));

    expect(next?.text).toBe('What could count as progress besides greater power?');
    expect(next?.text).not.toContain('Archived question');
    expect(next?.text).not.toContain('payoff');
    expect(next?.text).not.toContain('arc');
  });
});
