/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import type { Plugin } from 'vite';

/** Re-merges `src/lib/i18n/locales/<lang>/<feature>.json` on start and on every change. */
function mergeLocales(): Plugin {
  const script = fileURLToPath(new URL('./scripts/merge-locales.mjs', import.meta.url));
  const run = (): void => {
    execFileSync(process.execPath, [script], { stdio: 'inherit' });
  };
  return {
    name: 'fc-merge-locales',
    buildStart: run,
    configureServer(server) {
      const dir = fileURLToPath(new URL('./src/lib/i18n/locales', import.meta.url));
      server.watcher.add(dir);
      server.watcher.on('all', (_event, file) => {
        if (file.startsWith(dir) && file.endsWith('.json')) run();
      });
    },
  };
}

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [mergeLocales(), svelte(), svelteTesting()],
  resolve: {
    alias: { $lib: fileURLToPath(new URL('./src/lib', import.meta.url)) },
  },
  // Tauri expects a fixed port and must not have the terminal cleared.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    target: 'es2022',
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    // `dist/.vite/manifest.json`: the chunk graph `scripts/check-bundle.mjs` budgets.
    manifest: true,
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts', 'tests/**/*.test.ts'],
  },
});
