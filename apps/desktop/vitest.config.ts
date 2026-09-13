import { defineConfig } from 'vitest/config';
import { cpus } from 'node:os';

const defaultWorkers = Math.min(12, Math.max(2, Math.floor((cpus()?.length || 4) / 2)));
const rawWorkers = process.env.VITEST_MAX_WORKERS;
const requestedWorkers = rawWorkers === undefined ? undefined : Number(rawWorkers);
const maxWorkers = Number.isInteger(requestedWorkers) && requestedWorkers! > 0
  ? requestedWorkers!
  : (process.env.CI ? 2 : defaultWorkers);

export default defineConfig({
  test: {
    pool: 'threads',
    fsModuleCache: true,
    environment: 'jsdom',
    include: ['src/**/*.test.{ts,tsx}'],
    // Windows hosted runners occasionally oversubscribe jsdom workers. Keep
    // the full suite deterministic there without changing per-test budgets.
    // Locally on multi-core machines, scale to half the logical cores (capped at 12).
    maxWorkers,
  },
});
