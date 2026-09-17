import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));

const read = (rel) => readFileSync(resolve(root, rel), 'utf8');

const MANAGED_BLOCK = /<!-- BEGIN pm-conductor rules[\s\S]*?<!-- END pm-conductor rules -->\s*$/;

test('agent instruction files remain synchronized', () => {
  const agents = read('AGENTS.md');
  const gemini = read('GEMINI.md');
  const claudeRoot = read('CLAUDE.md');
  const claudeNested = read('.claude/CLAUDE.md');

  assert.equal(gemini, agents, 'GEMINI.md must be byte-identical to AGENTS.md');
  assert.equal(claudeNested, claudeRoot, '.claude/CLAUDE.md must be byte-identical to CLAUDE.md');

  const claudeShared = claudeRoot.replace(MANAGED_BLOCK, '');
  const claudeBody = claudeShared.replace(/^# CLAUDE\.md\n+/, '');
  assert.equal(
    claudeBody.trimEnd(),
    agents.trimEnd(),
    'CLAUDE.md must embed the AGENTS.md body verbatim; tool-managed blocks belong after the shared body',
  );
});
