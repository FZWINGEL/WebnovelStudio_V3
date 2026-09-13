// @vitest-environment node
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { parseSync, Visitor } from 'rolldown/utils';
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
type Edge = { file: string; from: string; to: string; rest: string };
type SourceImport = { target: string; runtime: boolean; names: string[] };

/** Parse TS/TSX before collecting paths, including type-only dependencies. */
function sourceImports(path: string, text: string): SourceImport[] {
  const parsed = parseSync(path, text);
  if (parsed.errors.length) throw new Error(`Could not parse ${path}: ${parsed.errors.map(error => error.message).join('; ')}`);
  const imports: SourceImport[] = [];
  new Visitor({
    ImportDeclaration: node => {
      const values = node.importKind === 'type' ? [] : node.specifiers.filter(specifier => specifier.type !== 'ImportSpecifier' || specifier.importKind !== 'type');
      imports.push({ target: node.source.value, runtime: node.importKind !== 'type' && (!node.specifiers.length || values.length > 0),
        names: values.map(specifier => specifier.type === 'ImportSpecifier' ? (specifier.imported.type === 'Identifier' ? specifier.imported.name : specifier.imported.value) : specifier.type === 'ImportDefaultSpecifier' ? 'default' : '*') });
    },
    ExportNamedDeclaration: node => {
      if (node.source) imports.push({ target: node.source.value, runtime: node.exportKind !== 'type' && (!node.specifiers.length || node.specifiers.some(specifier => specifier.exportKind !== 'type')), names: ['*'] });
    },
    ExportAllDeclaration: node => { imports.push({ target: node.source.value, runtime: node.exportKind !== 'type', names: ['*'] }); },
    TSImportType: node => { imports.push({ target: node.source.value, runtime: false, names: [] }); },
    TSImportEqualsDeclaration: node => {
      if (node.moduleReference.type === 'TSExternalModuleReference') imports.push({ target: node.moduleReference.expression.value, runtime: node.importKind !== 'type', names: ['*'] });
    },
    ImportExpression: node => {
      const target = node.source.type === 'Literal' && typeof node.source.value === 'string' ? node.source.value
        : node.source.type === 'TemplateLiteral' && node.source.expressions.length === 0 ? node.source.quasis[0].value.cooked : null;
      if (target === null || target === undefined) throw new Error(`Use a literal dynamic import so the feature boundary can resolve its target: ${path}`);
      imports.push({ target, runtime: true, names: ['*'] });
    },
  }).visit(parsed.program);
  return imports;
}

function sourceEdges(path: string, text: string): Edge[] {
  return sourceImports(path, text).filter(({ target }) => target.startsWith('.')).map(({ target }) => {
    const resolved = resolve(dirname(path), target);
    const to = sliceOf(resolved);
    return {
      file: relative(SOURCE, path).replace(/\\/g, '/'), from: sliceOf(path), to,
      rest: relative(join(SOURCE, to), resolved).replace(/\\/g, '/'),
    };
  });
}

export function isTestTarget(sourcePath: string, target: string): boolean {
  if (!target.startsWith('.')) return false;
  const resolved = resolve(dirname(sourcePath), target);
  const base = basename(resolved);
  if (/\.test(\.(tsx?|jsx?|mjs|cjs))?$/i.test(base)) return true;
  for (const ext of ['.ts', '.tsx', '.js', '.jsx']) {
    if (existsSync(resolved + ext) && /\.test$/i.test(base)) return true;
  }
  return false;
}

export function testImportViolations(fileList: string[] = sources(SOURCE)): string[] {
  const violations: string[] = [];
  for (const path of fileList) {
    const text = readFileSync(path, 'utf8');
    for (const { target } of sourceImports(path, text)) {
      if (isTestTarget(path, target)) {
        const fileRel = relative(SOURCE, path).replace(/\\/g, '/');
        violations.push(`${fileRel} -> ${target}`);
      }
    }
  }
  return violations;
}

/** Each lifecycle owner consumes capabilities rather than another owner's state. */
function workspaceOwnershipViolations(file: string, text: string): string[] {
  const path = join(SOURCE, 'shell', file);
  const violations: string[] = [];
  for (const entry of sourceImports(path, text).filter(entry => entry.runtime)) {
    const target = entry.target.startsWith('.') ? relative(join(SOURCE, 'shell'), resolve(dirname(path), entry.target)).replace(/\\/g, '/') : entry.target;
    const normalized = target.replace(/\.(tsx?|jsx?)$/, '');
    let allowed = true;
    if (file === 'Workspace.tsx') {
      allowed = normalized !== '../ipc' && !normalized.startsWith('../ipc/') && !normalized.startsWith('@tauri-apps/')
        && !['../editor', '../kernel'].some(feature => normalized === feature || normalized.startsWith(`${feature}/`))
        && !['documentWorkspace', 'libraryModel', 'workspaceContracts', 'useAppClose'].includes(normalized);
      if (normalized === 'react') allowed = entry.names.every(name => ['lazy', 'Suspense', 'useMemo'].includes(name));
    } else if (file === 'libraryModel.ts') {
      allowed = ['react', '../ipc/library', 'workspaceContracts'].includes(normalized);
    } else if (file === 'documentWorkspace.ts') {
      allowed = ['react', '../editor', '../kernel', 'workspaceContracts', 'projectTabs', 'workspaceModes'].includes(normalized)
        || normalized.startsWith('../ipc/');
    } else if (file === 'workspaceModel.ts') {
      allowed = ['react', '@tauri-apps/api/core', '../ipc/native', '../ipc/projectActivity',
        'documentWorkspace', 'libraryModel', 'workspaceContracts', 'useAppClose'].includes(normalized);
    } else if (file === 'workspaceContracts.ts') {
      allowed = false;
    }
    if (!allowed) violations.push(`${file} -> ${entry.target}`);
  }
  if (['libraryModel.ts', 'documentWorkspace.ts', 'workspaceContracts.ts'].includes(file)) {
    const parsed = parseSync(path, text);
    let jsx = false;
    new Visitor({ JSXElement: () => { jsx = true; }, JSXFragment: () => { jsx = true; } }).visit(parsed.program);
    if (jsx) violations.push(`${file} renders JSX`);
  }
  return violations;
}

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
function edges(): Edge[] {
  return sources(SOURCE).flatMap(path => sourceEdges(path, readFileSync(path, 'utf8')));
}

/** Importing a feature's root resolves its `index.ts`; `path.relative`
 *  spells that `''`, and a direct `index` is the same thing. */
const isSurface = (rest: string) => rest === '' || rest === '.' || rest === 'index';
const deepImports = (all: Edge[]) => all
  .filter(edge => FEATURES.includes(edge.from) && FEATURES.includes(edge.to) && edge.from !== edge.to)
  .filter(edge => !isSurface(edge.rest))
  .map(edge => `${edge.file} -> ${edge.to}/${edge.rest}`);
const shellImports = (all: Edge[]) => all
  .filter(edge => edge.to === 'shell' && edge.from !== 'shell' && edge.from !== 'main.tsx')
  .map(edge => `${edge.file} -> shell/${edge.rest}`);

function cyclicSlices(all: Edge[]): string[] {
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
  return slices.filter(slice => reaches(slice, slice)).sort();
}

describe('the feature boundary', () => {
  const all = edges();

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
    expect(deepImports(all)).toEqual([]);
  });

  it('keeps shell out of the features: only the shell composes', () => {
    // `main.tsx` is the entry point rather than a feature: it is what mounts
    // the shell, so importing it is the one edge that must exist.
    expect(shellImports(all)).toEqual([]);
  });

  it('gives every feature another feature uses a public surface', () => {
    const used = new Set(all.filter(edge => FEATURES.includes(edge.from) && FEATURES.includes(edge.to) && edge.from !== edge.to).map(edge => edge.to));
    for (const feature of used) {
      expect(() => readFileSync(join(SOURCE, feature, 'index.ts'), 'utf8'), feature).not.toThrow();
    }
    expect([...used].sort()).toEqual(['assistant', 'editor', 'providers', 'story']);
  });

  it('prohibits production source files from importing test files', () => {
    expect(testImportViolations()).toEqual([]);
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
    expect(cyclicSlices(all)).toEqual([]);
  });

  const importForms = [
    ['named, single quotes', "import { value } from '%target%';"],
    ['named, double quotes', 'import { value } from "%target%";'],
    ['default', "import value from '%target%';"],
    ['namespace', "import * as value from '%target%';"],
    ['default and named', "import value, { other } from '%target%';"],
    ['side effect', "import '%target%';"],
    ['type-only import', "import type { Value } from '%target%';"],
    ['named export', "export { value } from '%target%';"],
    ['star export', "export * from '%target%';"],
    ['namespace export', "export * as value from '%target%';"],
    ['type-only export', "export type { Value } from '%target%';"],
    ['dynamic import', "const load = () => import('%target%');"],
    ['template literal import', 'const load = () => import(`%target%`);'],
    ['import type expression', "type Value = import('%target%').Value;"],
    ['import equals', "import value = require('%target%');"],
  ];
  it.each(importForms)('enforces shell, surface and cycle rules for %s', (_name, code) => {
    const file = join(SOURCE, 'assistant', 'boundary-probe.tsx');
    const shell = sourceEdges(file, code.replace('%target%', '../shell/workspaceModel'));
    expect(shellImports(shell)).toEqual(['assistant/boundary-probe.tsx -> shell/workspaceModel']);
    expect(cyclicSlices([...all, ...shell])).toContain('assistant');
    const deep = sourceEdges(file, code.replace('%target%', '../editor/session'));
    expect(deepImports(deep)).toEqual(['assistant/boundary-probe.tsx -> editor/session']);
    const surface = sourceEdges(file, code.replace('%target%', '../editor'));
    expect(deepImports(surface)).toEqual([]);
    expect(shellImports(surface)).toEqual([]);
    expect(cyclicSlices([...all, ...surface])).toEqual([]);
  });
  it('ignores import-shaped comments, strings and JSX text', () => {
    expect(sourceEdges(join(SOURCE, 'assistant', 'boundary-probe.tsx'), `
      // import { value } from '../shell/workspaceModel';
      /* export * from '../shell/workspaceModel'; */
      const message = "import('../shell/workspaceModel')";
      const example = <pre>import value from '../shell/workspaceModel';</pre>;
    `)).toEqual([]);
  });
  it('refuses unparseable source and unresolved dynamic imports', () => {
    const file = join(SOURCE, 'assistant', 'boundary-probe.ts');
    expect(() => sourceEdges(file, 'import {')).toThrow('Could not parse');
    expect(() => sourceEdges(file, 'const load = () => import(target);')).toThrow('Use a literal dynamic import');
  });
});

describe('workspace ownership', () => {
  it('keeps layout, composition, document lifecycle and library dependencies separate', () => {
    for (const file of ['Workspace.tsx', 'workspaceModel.ts', 'documentWorkspace.ts', 'libraryModel.ts', 'workspaceContracts.ts']) {
      expect(workspaceOwnershipViolations(file, readFileSync(join(SOURCE, 'shell', file), 'utf8')), file).toEqual([]);
    }
  });

  it.each([
    "import { readDocument as read } from '../ipc/projects';",
    'import * as ipc from "../ipc/projects";',
    "export { readDocument } from '../ipc/projects';",
    "export * from '../ipc/projects';",
    "export {} from '../ipc/projects';",
    "const load = () => import('../ipc/projects');",
    "import ipc = require('../ipc/projects');",
    "import { DocumentSession } from '../editor';",
    "import { useState as state } from 'react';",
    "import React from 'react';",
    "import { useDocumentWorkspace } from './documentWorkspace';",
  ])('refuses layout mutation dependencies: %s', source => {
    expect(workspaceOwnershipViolations('Workspace.tsx', source)).not.toEqual([]);
  });

  it.each([
    "import type { ProjectAccess } from '../ipc/projects';",
    "import { type ProjectAccess } from '../ipc/projects';",
    "export type { ProjectAccess } from '../ipc/projects';",
    "export { type ProjectAccess } from '../ipc/projects';",
    "type Access = import('../ipc/projects').ProjectAccess;",
    "import type Access = require('../ipc/projects');",
    "import { lazy, Suspense } from 'react';",
    "import { Writer } from '../chat';",
    "import './workspaceModes.css';",
  ])('allows layout types and rendering dependencies: %s', source => {
    expect(workspaceOwnershipViolations('Workspace.tsx', source)).toEqual([]);
  });

  it('does not mistake a mixed import for a type-only dependency', () => {
    expect(workspaceOwnershipViolations('Workspace.tsx', "import { type ProjectAccess, readDocument } from '../ipc/projects';")).not.toEqual([]);
  });

  it.each([
    ['libraryModel.ts', "import { DocumentSession } from '../editor';"],
    ['libraryModel.ts', "import { readDocument } from '../ipc/projects';"],
    ['libraryModel.ts', "import { useDocumentWorkspace } from './documentWorkspace';"],
    ['documentWorkspace.ts', "import { useLibraryModel } from './libraryModel';"],
    ['documentWorkspace.ts', "import { Writer } from '../chat';"],
    ['documentWorkspace.ts', "import { ExportDialog } from './ExportDialog';"],
    ['workspaceModel.ts', "import { DocumentSession } from '../editor';"],
    ['workspaceModel.ts', "import { libraryCreate } from '../ipc/library';"],
    ['workspaceContracts.ts', "import { readDocument } from '../ipc/projects';"],
  ])('refuses crossed ownership in %s: %s', (file, source) => {
    expect(workspaceOwnershipViolations(file, source)).not.toEqual([]);
  });

  it('allows owners to consume their explicit contracts', () => {
    expect(workspaceOwnershipViolations('libraryModel.ts', "import { libraryCreate } from '../ipc/library'; import type { ProjectNavigation } from './workspaceContracts';")).toEqual([]);
    expect(workspaceOwnershipViolations('documentWorkspace.ts', "import { DocumentSession } from '../editor'; import { readDocument } from '../ipc/projects';")).toEqual([]);
    expect(workspaceOwnershipViolations('workspaceModel.ts', "import { useLibraryModel } from './libraryModel'; import { useDocumentWorkspace } from './documentWorkspace';")).toEqual([]);
  });

  it('refuses JSX in lifecycle modules and ignores documentation examples', () => {
    expect(() => workspaceOwnershipViolations('documentWorkspace.ts', 'const view = <div />;')).toThrow('Could not parse');
    expect(workspaceOwnershipViolations('libraryModel.ts', `
      // import { DocumentSession } from '../editor';
      const example = "import { readDocument } from '../ipc/projects';";
    `)).toEqual([]);
  });
});

describe('production-to-test import prohibition', () => {
  const probeFile = join(SOURCE, 'assistant', 'boundary-probe.tsx');

  const testImportForms = [
    ['named import', "import { probe } from './probe.test';"],
    ['default import', "import probe from './probe.test';"],
    ['namespace import', "import * as probe from './probe.test';"],
    ['side-effect import', "import './probe.test';"],
    ['explicit extension', "import './probe.test.tsx';"],
    ['commented-specifier import', "import { probe } from /* explanation */ './probe.test';"],
    ['dynamic import', "const load = () => import('./probe.test');"],
    ['re-export named', "export { probe } from './probe.test';"],
    ['re-export all', "export * from './probe.test';"],
    ['type-only import', "import type { Probe } from './probe.test';"],
    ['import equals', "import probe = require('./probe.test');"],
  ];

  it.each(testImportForms)('detects prohibited test import via %s', (_name, code) => {
    const targets = sourceImports(probeFile, code).map(entry => entry.target);
    expect(targets.some(target => isTestTarget(probeFile, target))).toBe(true);
  });

  it('ignores commented-out test imports, strings and JSX text', () => {
    const code = `
      // import './probe.test';
      /* import { probe } from './probe.test'; */
      const text = "import './probe.test';";
      const jsx = <div>import './probe.test';</div>;
    `;
    const targets = sourceImports(probeFile, code).map(entry => entry.target);
    expect(targets.some(target => isTestTarget(probeFile, target))).toBe(false);
  });
});

