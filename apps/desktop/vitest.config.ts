import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.{ts,tsx}'],
    // Windows hosted runners occasionally oversubscribe jsdom workers. Keep
    // the full suite deterministic there without changing per-test budgets.
    maxWorkers: process.env.CI ? 2 : undefined,
  },
});
