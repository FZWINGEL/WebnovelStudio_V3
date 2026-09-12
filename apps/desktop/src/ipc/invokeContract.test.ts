import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * The one part of the IPC surface §4.4 cannot generate: the *names*.
 *
 * A command is snake_case and its arguments are camelCase, because that is what
 * Tauri's argument deserialization expects. Nothing checked it, so a mismatch
 * was a runtime "missing required key" in the renderer with no failing test
 * anywhere — the same shape as D6, one layer down. The generated bindings fix
 * the types; this fixes the spelling.
 *
 * It reads the real call sites rather than a list, so a new `invoke` anywhere
 * in `src/` is covered the moment it is written.
 */

const SOURCE = join(process.cwd(), 'src');
const COMMAND = /^[a-z][a-z0-9]*(?:_[a-z0-9]+)*$/;
const ARGUMENT = /^[a-z][a-zA-Z0-9]*$/;

function sources(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) { sources(path, out); continue; }
    if (!/\.tsx?$/.test(entry) || /\.test\.tsx?$/.test(entry)) continue;
    out.push(path);
  }
  return out;
}

/** Past a quoted run, so a `)` or `,` inside a string is not read as syntax. */
function pastString(text: string, at: number): number {
  const quote = text[at];
  let i = at + 1;
  while (i < text.length) {
    if (text[i] === '\\') { i += 2; continue; }
    if (text[i] === quote) return i + 1;
    i += 1;
  }
  return i;
}

function matching(text: string, open: number): number {
  let depth = 0;
  for (let i = open; i < text.length; i += 1) {
    const c = text[i];
    if (c === '"' || c === "'" || c === '`') { i = pastString(text, i) - 1; continue; }
    if (c === '/' && text[i + 1] === '/') { const end = text.indexOf('\n', i); i = end === -1 ? text.length : end; continue; }
    if (c === '(') depth += 1;
    if (c === ')') { depth -= 1; if (depth === 0) return i; }
  }
  return -1;
}

/** The top-level keys of an object literal, ignoring spreads. */
function keys(inside: string): string[] {
  const segments: string[] = [];
  let depth = 0;
  let segment = '';
  for (let i = 0; i < inside.length; i += 1) {
    const c = inside[i];
    if (c === '"' || c === "'" || c === '`') {
      const end = pastString(inside, i);
      segment += inside.slice(i, end);
      i = end - 1;
      continue;
    }
    if ('([{'.includes(c)) depth += 1;
    if (')]}'.includes(c)) depth -= 1;
    if (c === ',' && depth === 0) { segments.push(segment); segment = ''; continue; }
    segment += c;
  }
  segments.push(segment);
  return segments.flatMap(raw => {
    const text = raw.trim();
    if (!text || text.startsWith('...')) return [];
    const named = /^([A-Za-z_$][\w$]*)\s*:/.exec(text);
    if (named) return [named[1]];
    const shorthand = /^([A-Za-z_$][\w$]*)$/.exec(text);
    return shorthand ? [shorthand[1]] : [];
  });
}

function callSites(): Array<{ file: string; command: string; arguments: string[] }> {
  const found: Array<{ file: string; command: string; arguments: string[] }> = [];
  for (const path of sources(SOURCE)) {
    const text = readFileSync(path, 'utf8');
    const pattern = /invoke(?:<[^>]*>)?\(\s*'([^']*)'/g;
    for (let match = pattern.exec(text); match; match = pattern.exec(text)) {
      const open = match.index + match[0].indexOf('(');
      const close = matching(text, open);
      if (close === -1) continue;
      const rest = text.slice(open + 1, close);
      const brace = rest.indexOf('{');
      found.push({
        file: relative(SOURCE, path).replace(/\\/g, '/'),
        command: match[1],
        arguments: brace === -1 ? [] : keys(rest.slice(brace + 1, matching(rest, brace))),
      });
    }
  }
  return found;
}

describe('the IPC call convention', () => {
  const sites = callSites();

  // A scanner that found nothing would pass every assertion below it.
  it('reads the real call sites', () => {
    expect(sites.length).toBeGreaterThan(100);
    expect(sites.flatMap(site => site.arguments).length).toBeGreaterThan(100);
  });

  it('names every command in snake_case', () => {
    const wrong = sites.filter(site => !COMMAND.test(site.command)).map(site => `${site.file}: ${site.command}`);
    expect(wrong).toEqual([]);
  });

  it('names every argument in camelCase', () => {
    const wrong = sites
      .flatMap(site => site.arguments.filter(argument => !ARGUMENT.test(argument)).map(argument => `${site.file}: ${site.command}(${argument})`));
    expect(wrong).toEqual([]);
  });
});
