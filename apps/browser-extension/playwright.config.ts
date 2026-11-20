import { defineConfig } from '@playwright/test';

/** e2e: the built extension (`dist/`) in Chromium against a mock local API. Run `pnpm e2e`. */
export default defineConfig({
  testDir: 'tests/e2e',
  timeout: 60_000,
  workers: 1,
  reporter: [['list']],
  use: { trace: 'retain-on-failure' },
});
