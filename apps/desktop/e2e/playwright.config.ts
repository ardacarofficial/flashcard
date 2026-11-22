/**
 * Playwright config of the journeys (`pnpm e2e`, see ./README.md). `run.mjs` starts the dev
 * bridge and vite and passes their addresses in the environment; run it, not this config
 * directly.
 */
import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './journeys',
  // One shared real profile: journeys run one after another, in file order.
  fullyParallel: false,
  workers: 1,
  retries: 0,
  // The machine may be busy building Rust: be patient rather than flaky.
  timeout: 90_000,
  expect: { timeout: 15_000 },
  reporter: [['list']],
  outputDir: './.results',
  use: {
    baseURL: process.env.FC_E2E_URL,
    headless: true,
    viewport: { width: 1280, height: 800 },
    locale: 'tr-TR',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    launchOptions: {
      executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined,
    },
  },
});
