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
 * It also asserts the graph is acyclic, which the publication rule alone does
 * not give you: four pairs of features were mutually dependent, each edge
 * published through a surface and the whole set still a knot. That is the
 * discharge §3.5 needed on the Rust side, and here it took one move — see the
 * note on that assertion for where the cycles actually lived.
 */

const SOURCE = join(process.cwd(), 'src');
const FEATURES = ['assistant', 'chat', 'editor', 'providers', 'story', 'workshop'];
const SHARED = ['ipc', 'kernel'];
// `export … from` creates the same dependency as `import … from`, and the
// first draft of this test missed it: a cycle injected as a re-export passed.
const IMPORT = /(?:import|export)\s+(?:type\s+)?(?:\{[^}]*\}|\*)\s+from\s+'([^']+)'/g;

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
  const parts = relative(SOURCE, path).split(/[\\/]/);
  // The generated bindings are a build artifact rather than a layer: they carry
  // no dependencies of their own and anything may read them. Folding them into
  // `ipc` invents a cycle — `kernel`'s document model imports the generated
  // `WnsDocument` narrowing, and `ipc`'s wrappers import `kernel`.
  return parts[0] === 'ipc' && parts[1] === 'generated' ? 'ipc/generated' : parts[0];
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
    expect([...used].sort()).toEqual(['assistant', 'editor', 'providers', 'story']);
  });

  /**
   * The rule above can hold and the graph still be a knot: every edge published
   * through a surface, and no order in which the features can be read. All four
   * of these pairs were mutual before this — `assistant` with `chat` and with
   * `editor`, `chat` with `editor`, `editor` with `story` — and the whole of
   * each cycle was six imports in `editor/Writer.tsx`, which composed panels
   * from three features above it. It now lives in `chat`, the feature that
   * renders it, which is what §4.5 said should happen and did not.
   *
   * What is left is a partial order — `providers` and `editor` at the bottom,
   * then `assistant`, then `story`, then `chat`; `workshop` sits above `editor`
   * alone — and this is what keeps it one.
   */
  it('leaves the whole slice graph acyclic, ipc and kernel included', () => {
    // `ipc` and `kernel` are exempt from the publication rule because they are
    // shared infrastructure rather than features — but they are still slices,
    // and `ipc` imported the document model from `editor` while `editor`
    // imported `ipc/projects`, so the two were mutually dependent. Exempting
    // them from *this* check would have hidden exactly that. The model moved to
    // `kernel`, where the rest of the shared vocabulary already was, and the
    // check now covers every slice.
    const slices = [...FEATURES, ...SHARED, 'ipc/generated', 'shell'];
    const depends = new Map<string, Set<string>>(slices.map(slice => [slice, new Set<string>()]));
    for (const edge of all) {
      if (slices.includes(edge.from) && slices.includes(edge.to) && edge.from !== edge.to) depends.get(edge.from)!.add(edge.to);
    }
    const reaches = (from: string, target: string, seen = new Set<string>()): boolean => {
      for (const next of depends.get(from)!) {
        if (next === target) return true;
        if (seen.has(next)) continue;
        seen.add(next);
        if (reaches(next, target, seen)) return true;
      }
      return false;
    };
    expect(slices.filter(slice => reaches(slice, slice)).sort()).toEqual([]);
  });
});
