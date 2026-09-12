import assert from 'node:assert/strict';
export function expectedIdentities(inventory) {
  return Object.entries(inventory['rust-suites']).flatMap(([binary, suite]) => Object.entries(suite.testcases)
    .filter(([, test]) => !test.ignored).map(([name, test]) => {
      assert.equal(test['filter-match'].status, 'matches', 'A runner filter omitted an ordinary test');
      return { binary, name };
    }));
}
export function verifyLibtestOutput(stdout, expected) {
  const actual = [...stdout.matchAll(/^test (.+) \.\.\. ok\s*$/gm)].map(match => match[1]);
  assert.deepEqual(actual.sort(), expected.map(test => test.name).sort(), 'Libtest execution identities differ');
}
export function verifyNextestOutput(stderr, expected) {
  // Final failure summaries repeat failed IDs; compare identities, not log-line counts.
  const actual = new Set(stderr.split(/\r?\n/).map(line =>
    line.match(/^\s*(?:PASS|FAIL|TIMEOUT|LEAK)\s+\[[^\]]+\]\s+\(\s*\d+\/\d+\)\s+(\S+) (.+)\s*$/))
    .filter(Boolean).map(match => `${match[1]} ${match[2].trim()}`));
  assert.deepEqual([...actual].sort(), expected.map(test => `${test.binary} ${test.name}`).sort(), 'Nextest execution identities differ');
}
