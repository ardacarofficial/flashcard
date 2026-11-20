/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';

const here = (p: string): string => fileURLToPath(new URL(p, import.meta.url));

/**
 * Chromium MV3 build. `public/` (manifest + `_locales`) is copied as-is; the service worker,
 * popup and options page are separate entries with stable file names (the manifest points at them).
 * DTO types come from the desktop app's generated `fc-api` bindings (`@fc/api`, type-only).
 */
export default defineConfig(({ mode }) => ({
  // The e2e capture hook (`__FC_E2E__`) exists only in `vite build --mode e2e`, never in `pnpm build`.
  define: { __FC_E2E__: JSON.stringify(mode === 'e2e') },
  resolve: {
    alias: { '@fc/api': here('../desktop/src/lib/api/generated/index.ts') },
  },
  build: {
    target: 'es2022',
    outDir: 'dist',
    emptyOutDir: true,
    modulePreload: false,
    rollupOptions: {
      input: {
        background: here('src/background/index.ts'),
        popup: here('popup.html'),
        options: here('options.html'),
      },
      output: {
        entryFileNames: '[name].js',
        chunkFileNames: 'chunks/[name]-[hash].js',
        assetFileNames: 'assets/[name]-[hash][extname]',
      },
    },
  },
  test: {
    environment: 'jsdom',
    include: ['tests/unit/**/*.test.ts'],
  },
}));
