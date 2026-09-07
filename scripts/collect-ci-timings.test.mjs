import { test } from 'node:test';
import assert from 'node:assert/strict';
import { summarizeRun } from './collect-ci-timings.mjs';
test('admission failure is not a zero-duration successful benchmark', () => {
  const result = summarizeRun({ status: 'completed', conclusion: 'failure' }, [{ conclusion: 'failure', steps: [] }]);
  assert.equal(result.sampleEligible, false);
  assert.equal(result.fullGreenMs, null);
  assert.equal(result.firstFrontendFeedbackMs, null);
});
test('frontend feedback is separate from full green and unclassified cache state stays explicit', () => {
  const run = { status: 'completed', conclusion: 'success', run_started_at: '2026-09-07T00:00:00Z' };
  const job = { conclusion: 'success', started_at: run.run_started_at, completed_at: '2026-09-07T00:10:00Z', steps: [
    { name: 'npm test', conclusion: 'success', started_at: '2026-09-07T00:01:00Z', completed_at: '2026-09-07T00:02:00Z' },
  ] };
  const result = summarizeRun(run, [job]);
  assert.equal(result.firstFrontendFeedbackMs, 120000);
  assert.equal(result.fullGreenMs, 600000);
  assert.equal(result.cacheState, 'unclassified');
  assert.equal(summarizeRun(run, [{ ...job, conclusion: 'cancelled' }]).sampleEligible, false);
});
