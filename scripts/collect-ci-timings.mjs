import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const execute = promisify(execFile);
export function summarizeRun(run, jobs) {
  const start = Date.parse(run.run_started_at);
  const elapsed = (from, to) => {
    const value = Date.parse(to) - Date.parse(from);
    return Number.isFinite(value) && value >= 0 ? value : null;
  };
  const measuredJobs = jobs.map(job => ({ id: job.id, name: job.name, conclusion: job.conclusion,
    durationMs: elapsed(job.started_at, job.completed_at),
    steps: (job.steps ?? []).map(step => ({ name: step.name, conclusion: step.conclusion,
      durationMs: elapsed(step.started_at, step.completed_at), completedAt: step.completed_at })) }));
  const feedback = measuredJobs.flatMap(job => job.steps).filter(step =>
    /npm (?:test|run build)/.test(step.name) && ['success', 'failure'].includes(step.conclusion))
    .map(step => Date.parse(step.completedAt) - start).filter(value => Number.isFinite(value) && value >= 0);
  const completed = jobs.map(job => Date.parse(job.completed_at)).filter(Number.isFinite);
  const eligible = run.status === 'completed' && run.conclusion === 'success'
    && jobs.length > 0 && jobs.every(job => job.conclusion === 'skipped' || job.conclusion === 'success')
    && measuredJobs.some(job => job.steps.length > 0);
  return { runId: run.id, commit: run.head_sha, event: run.event, conclusion: run.conclusion,
    cacheState: 'unclassified', changeKind: 'unclassified', variant: 'unclassified',
    sampleEligible: eligible, firstFrontendFeedbackMs: feedback.length ? Math.min(...feedback) : null,
    fullGreenMs: eligible && Number.isFinite(start) && completed.length ? Math.max(...completed) - start : null,
    jobs: measuredJobs };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const id = process.argv[2]; assert.match(id ?? '', /^\d+$/);
  const base = `repos/FZWINGEL/WebnovelStudio_V3/actions/runs/${id}`;
  const responses = await Promise.all([
    execute('gh', ['api', base], { windowsHide: true, maxBuffer: 10_000_000 }),
    execute('gh', ['api', '--paginate', '--slurp', `${base}/jobs?per_page=100`], { windowsHide: true, maxBuffer: 10_000_000 }),
  ]);
  const run = JSON.parse(responses[0].stdout);
  const jobs = JSON.parse(responses[1].stdout).flatMap(page => page.jobs);
  const directory = fileURLToPath(new URL('../.local/performance/', import.meta.url));
  await mkdir(directory, { recursive: true });
  const summary = summarizeRun(run, jobs);
  await writeFile(resolve(directory, `ci-${id}.json`), JSON.stringify({ summary, run, jobs }, null, 2));
  console.log(JSON.stringify(summary, null, 2));
}
