// @vitest-environment node
import { describe, expect, it } from 'vitest';
import fixture from '../../../../contracts/fixtures/w0_snapshot_golden.json';
import {
  bodyHash,
  canonicalJson,
  safeHref,
  snapshotFromEditor,
  type WnsDocument,
} from './document';

type GoldenCase = {
  name: string;
  input: string;
  expected?: {
    canonicalJson: string;
    hash: string;
  };
  error?: string;
};

function readGoldens(): GoldenCase[] {
  return fixture.cases as GoldenCase[];
}

describe('snapshot document helpers', () => {
  it('keeps JS canonical JSON and hash aligned with shared golden fixtures', async () => {
    const valid = readGoldens().filter((fixture): fixture is GoldenCase & { expected: NonNullable<GoldenCase['expected']> } => !!fixture.expected);
    expect(valid.length).toBeGreaterThanOrEqual(6);

    for (const fixture of valid) {
      const input = JSON.parse(fixture.input) as WnsDocument;
      const canonical = canonicalJson(snapshotFromEditor(input.body));
      expect(canonical, fixture.name).toBe(fixture.expected.canonicalJson);
      expect(await bodyHash(canonical), fixture.name).toBe(fixture.expected.hash);
    }
  });

  it('accepts only the strict HTTP(S) and mailto link policy', () => {
    for (const href of [
      'https://example.com/story',
      'http://localhost:3000/path?query=yes',
      'mailto:author@example.com',
    ]) {
      expect(safeHref(href), href).toBe(true);
    }

    for (const href of [
      'javascript:alert(1)',
      'file:///tmp/story.txt',
      'http:example.com',
      'http:/example.com',
      'https://user:pass@example.com/story',
      'https://:80/story',
      'https://[::1/story',
      'https://example.com/story\n',
      'https://example.com/story with-space',
      'https://',
      'mailto:author@example.com?subject=hello',
      'mailto:author@example.com%0d%0abcc:evil',
      'mailto:author@example',
      'mailto:@example.com',
      'mailto:.author@example.com',
      'mailto:author.@example.com',
      'mailto:author@.example.com',
      'mailto:author@example.com.',
    ]) {
      expect(safeHref(href), href).toBe(false);
    }
  });
});
