/**
 * v1.0 code splitting: the logic of `scripts/check-bundle.mjs` (the budget itself runs on a
 * real build via `pnpm bundle:check`) and the screen registrations it relies on.
 */
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { initialFiles, lazyScreens, problems, type Manifest } from '../scripts/check-bundle.mjs';
import { SCREENS } from '../src/lib/features';

const manifest: Manifest = {
  'index.html': { file: 'assets/index.js', isEntry: true, imports: ['_shared.js'], css: ['assets/index.css'], dynamicImports: ['src/lib/features/stats/StatsScreen.svelte'] },
  '_shared.js': { file: 'assets/shared.js', css: ['assets/shared.css'] },
  'src/lib/features/stats/StatsScreen.svelte': { file: 'assets/StatsScreen.js', isDynamicEntry: true, imports: ['_shared.js', 'node_modules/katex/dist/katex.mjs'] },
  'node_modules/katex/dist/katex.mjs': { file: 'assets/katex.js', isDynamicEntry: true },
};
const sizes: Record<string, number> = {
  'assets/index.js': 100,
  'assets/shared.js': 50,
  'assets/index.css': 10,
  'assets/shared.css': 5,
};
const sizeOf = (f: string) => sizes[f] ?? 1_000_000;
const stats = [{ key: 'src/lib/features/stats/StatsScreen.svelte' }];

describe('bundle budget', () => {
  it('the initial load is the entry, its static imports and their CSS — not lazy chunks', () => {
    expect(initialFiles(manifest)).toEqual({
      js: ['assets/index.js', 'assets/shared.js'],
      css: ['assets/index.css', 'assets/shared.css'],
    });
    expect(problems(manifest, sizeOf, stats, { js: 150, css: 15 })).toEqual([]);
  });

  it('reports a budget overrun, a statically imported screen and an eager heavy library', () => {
    const eager: Manifest = {
      ...manifest,
      'index.html': { ...manifest['index.html']!, imports: ['_shared.js', 'src/lib/features/stats/StatsScreen.svelte'] },
    };
    expect(problems(eager, sizeOf, [...stats, { key: 'src/lib/features/x/Gone.svelte' }], { js: 149, css: 15 })).toEqual([
      expect.stringContaining('initial JS'),
      expect.stringContaining('StatsScreen.svelte is in the initial load'),
      'lazy screen src/lib/features/x/Gone.svelte has no chunk of its own',
      expect.stringContaining('katex'),
    ]);
  });

  it('every screen except the first one the shell opens is lazy, and the script sees them all', () => {
    const eager = SCREENS.filter((s) => s.component).map((s) => s.id);
    expect(eager).toEqual(['study']);
    const lazy = lazyScreens(join(__dirname, '..', 'src', 'lib', 'features'));
    expect(lazy.length).toBe(SCREENS.length - eager.length);
  });
});
