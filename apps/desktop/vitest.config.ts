import { defineConfig } from 'vitest/config';
import { cpus } from 'node:os';

const defaultWorkers = Math.min(12, Math.max(2, Math.floor((cpus()?.length || 4) / 2)));

export default defineConfig({
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.{ts,tsx}'],
    // Windows hosted runners occasionally oversubscribe jsdom workers. Keep
    // the full suite deterministic there without changing per-test budgets.
    // Locally on multi-core machines, scale to half the logical cores (capped at 12).
    maxWorkers: process.env.CI ? 2 : (process.env.VITEST_MAX_WORKERS ? Number(process.env.VITEST_MAX_WORKERS) : defaultWorkers),
  },
});
