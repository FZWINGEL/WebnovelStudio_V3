import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * §4.1's boundary, made mechanical.
 *
 * The plan states the rule — "a feature may import from `kernel/`, `ipc/` and
 * its own directory; cross-feature imports go through a feature's public
 * `index.ts`" — and says a lint rule enforces it. There was no lint rule, so
 * there was no rule: 59 cross-feature edges had accumulated, most reaching
 * straight into another feature's modules, and nothing could tell a deliberate
 * edge from a stray one.
 *
 * This reads the real import graph rather than a list, which is the same shape
 * as `crates/architecture/tests/layering.rs` on the Rust side: the boundary is
 * a property of the tree, so it is checked against the tree.
 *
 * Cycles between features are *not* forbidden here. `chat` and `assistant`
 * import each other, and so do four other pairs; the rule this enforces is that
 * each direction goes through a declared surface, which is what makes the edge
 * reviewable. Forbidding the cycles outright is a separate piece of work.
 */

const SOURCE = join(process.cwd(), 'src');
const FEATURES = ['assistant', 'chat', 'editor', 'providers', 'story', 'workshop'];
const SHARED = ['ipc', 'kernel'];
const IMPORT = /import\s+(?:type\s+)?\{[^}]*\}\s+from\s+'([^']+)'/g;

function sources(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) { sources(path, out); continue; }
    if (!/\.tsx?$/.test(entry) || /\.test\.tsx?$/.test(entry)) continue;
    out.push(path);
  }
  return out;
}

function sliceOf(path: string): string {
  return relative(SOURCE, path).split(/[\\/]/)[0];
}

/** Every relative import in the tree, as (from-slice, to-slice, remainder). */
function edges(): Array<{ file: string; from: string; to: string; rest: string }> {
  const found: Array<{ file: string; from: string; to: string; rest: string }> = [];
  for (const path of sources(SOURCE)) {
    const text = readFileSync(path, 'utf8');
    for (const match of text.matchAll(IMPORT)) {
      const target = match[1];
      if (!target.startsWith('.')) continue;
      const resolved = resolve(dirname(path), target);
      const to = sliceOf(resolved);
      found.push({
        file: relative(SOURCE, path).replace(/\\/g, '/'),
        from: sliceOf(path),
        to,
        rest: relative(join(SOURCE, to), resolved).replace(/\\/g, '/'),
      });
    }
  }
  return found;
}

describe('the feature boundary', () => {
  const all = edges();

  /** Importing a feature's root resolves its `index.ts`; `path.relative`
   *  spells that `''`, and a direct `index` is the same thing. */
  const isSurface = (rest: string) => rest === '' || rest === '.' || rest === 'index';

  // A scanner that found nothing would pass every assertion below it.
  it('reads the real import graph', () => {
    const seen = new Set(all.flatMap(edge => [edge.from, edge.to]));
    expect(all.length).toBeGreaterThan(300);
    for (const slice of [...FEATURES, ...SHARED, 'shell']) expect(seen, slice).toContain(slice);
  });

  it('lets a feature import kernel and ipc freely, and itself', () => {
    const shared = all.filter(edge => SHARED.includes(edge.to));
    expect(shared.length).toBeGreaterThan(100);
  });

  it('never reaches past another feature\'s public surface', () => {
    const deep = all
      .filter(edge => FEATURES.includes(edge.from) && FEATURES.includes(edge.to) && edge.from !== edge.to)
      .filter(edge => !isSurface(edge.rest))
      .map(edge => `${edge.file} -> ${edge.to}/${edge.rest}`);
    expect(deep).toEqual([]);
  });

  it('keeps shell out of the features: only the shell composes', () => {
    // `main.tsx` is the entry point rather than a feature: it is what mounts
    // the shell, so importing it is the one edge that must exist.
    const outward = all
      .filter(edge => edge.to === 'shell' && edge.from !== 'shell' && edge.from !== 'main.tsx')
      .map(edge => `${edge.file} -> shell/${edge.rest}`);
    expect(outward).toEqual([]);
  });

  it('gives every feature another feature uses a public surface', () => {
    const used = new Set(all.filter(edge => FEATURES.includes(edge.from) && FEATURES.includes(edge.to) && edge.from !== edge.to).map(edge => edge.to));
    for (const feature of used) {
      expect(() => readFileSync(join(SOURCE, feature, 'index.ts'), 'utf8'), feature).not.toThrow();
    }
    expect(used.size).toBeGreaterThan(4);
  });
});
