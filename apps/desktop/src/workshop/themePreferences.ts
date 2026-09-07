import type { WorkshopPreference } from '../ipc/workshop';

export type ThemePreferenceAxis = 'reader-experience' | 'content-intensity';

export interface ThemePreferenceChoice {
  id: string;
  label: string;
  family: Extract<WorkshopPreference['family'], string>;
  meaning: string;
  examples: string;
}

/**
 * These are optional starting points for the author's own preference wording.
 * They are deliberately split by axis: reader feeling does not decide how
 * explicitly difficult material is presented.
 */
export const READER_EXPERIENCE_CHOICES: readonly ThemePreferenceChoice[] = [
  {
    id: 'warmth',
    label: 'Warmth',
    family: 'Reader experience',
    meaning: 'Keep care, humor, hospitality, and ordinary tenderness visible even when the story is under pressure.',
    examples: 'Small acts of care, shared food, wry humor, or moments of welcome can remain present during conflict.',
  },
  {
    id: 'hope',
    label: 'Hope',
    family: 'Reader experience',
    meaning: 'Leave credible room for repair, connection, or improvement after hardship without promising an easy outcome.',
    examples: 'A costly choice can still open a path toward trust, recovery, or a better answer later.',
  },
  {
    id: 'unease',
    label: 'Unease',
    family: 'Reader experience',
    meaning: 'Keep uncertainty, risk, and unresolved tension present without requiring the story to become bleak.',
    examples: 'Let a warning, omission, or unstable alliance make a scene feel unsettled while leaving several outcomes possible.',
  },
  {
    id: 'wonder',
    label: 'Wonder',
    family: 'Reader experience',
    meaning: 'Make discovery, beauty, and strangeness part of the reading experience alongside the story’s conflicts.',
    examples: 'A new place, practice, image, or piece of ordinary life can reward attention even when it is not a plot solution.',
  },
] as const;

export const CONTENT_INTENSITY_CHOICES: readonly ThemePreferenceChoice[] = [
  {
    id: 'restrained-violence',
    label: 'Restrained violence',
    family: 'Content boundaries',
    meaning: 'Show danger and meaningful consequences while keeping bodily harm non-graphic and brief.',
    examples: 'Focus on choices, aftermath, or what a character can no longer do instead of lingering on injury detail.',
  },
  {
    id: 'explicit-injury-detail',
    label: 'Explicit injury detail',
    family: 'Content boundaries',
    meaning: 'Allow sensory detail about bodily injury when it serves the scene; this sets presentation intensity, not the seriousness of consequences or the story’s tone.',
    examples: 'Name physical sensations and visible damage when relevant, while leaving the emotional meaning and outcome open to the author.',
  },
] as const;

export function themePreferenceChoices(axis: ThemePreferenceAxis): readonly ThemePreferenceChoice[] {
  return axis === 'reader-experience' ? READER_EXPERIENCE_CHOICES : CONTENT_INTENSITY_CHOICES;
}
