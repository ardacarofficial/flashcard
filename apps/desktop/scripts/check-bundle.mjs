// Bundle budget (v1.0 performance, docs/02 §34 "açılış < 2 sn"): run after `vite build`
// (`pnpm bundle:check`). Reads Vite's chunk graph (`dist/.vite/manifest.json`) and checks
//
// * the initial load — the entry chunk, every chunk it imports statically and their CSS —
//   stays under the budget below (what the window must parse before the first screen);
// * every lazy screen (`load: () => import('./X.svelte')` in `features/*/screen.ts`) is a
//   dynamic entry of its own, not pulled into the initial load by a static import;
// * heavy libraries (KaTeX, highlight.js) stay out of the initial load.
//
// Budgets are raw (minified, not gzipped) bytes, set from the v1.0 measurement with ~25%
// headroom; raise them deliberately, in the same commit as the reason.
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const appDir = join(dirname(fileURLToPath(import.meta.url)), '..');

/** Raw byte budgets of the initial load (measured at v1.0: JS 248.8 kB, CSS 40.6 kB). */
export const BUDGETS = { js: 320_000, css: 52_000 };

/** Libraries that must only ever load lazily (matched against manifest keys). */
export const LAZY_ONLY = [/node_modules\/katex\//, /node_modules\/highlight\.js\//];

/** The entry chunk's key (`index.html`) in a Vite manifest. */
export function entryKey(manifest) {
  const keys = Object.keys(manifest).filter((k) => manifest[k].isEntry);
  if (keys.length !== 1) throw new Error(`expected one entry chunk, found [${keys}]`);
  return keys[0];
}

/** Manifest keys loaded before the first screen: the entry and its static imports, transitively. */
export function initialChunks(manifest) {
  const seen = new Set();
  const visit = (key) => {
    if (seen.has(key)) return;
    const chunk = manifest[key];
    if (!chunk) throw new Error(`manifest has no chunk "${key}"`);
    seen.add(key);
    for (const dep of chunk.imports ?? []) visit(dep);
  };
  visit(entryKey(manifest));
  return seen;
}

/** Output files (JS and CSS) of the initial load. */
export function initialFiles(manifest) {
  const js = new Set();
  const css = new Set();
  for (const key of initialChunks(manifest)) {
    const chunk = manifest[key];
    js.add(chunk.file);
    for (const f of chunk.css ?? []) css.add(f);
  }
  return { js: [...js], css: [...css] };
}

/** `[feature/screen.ts, './X.svelte']` for every lazy screen registration. */
export function lazyScreens(featuresDir) {
  const out = [];
  for (const feature of readdirSync(featuresDir)) {
    const file = join(featuresDir, feature, 'screen.ts');
    let text;
    try {
      if (!statSync(file).isFile()) continue;
      text = readFileSync(file, 'utf8');
    } catch {
      continue;
    }
    for (const m of text.matchAll(/load:\s*\(\)\s*=>\s*import\('(\.\/[\w-]+\.svelte)'\)/g)) {
      out.push({ feature, key: `src/lib/features/${feature}/${m[1].slice(2)}` });
    }
  }
  return out;
}

/** Every violation of the rules above (empty = within budget). */
export function problems(manifest, sizeOf, lazy, budgets = BUDGETS) {
  const out = [];
  const initial = initialChunks(manifest);
  const files = initialFiles(manifest);
  const total = (list) => list.reduce((n, f) => n + sizeOf(f), 0);
  const js = total(files.js);
  const css = total(files.css);
  if (js > budgets.js) out.push(`initial JS is ${js} bytes, budget ${budgets.js}`);
  if (css > budgets.css) out.push(`initial CSS is ${css} bytes, budget ${budgets.css}`);
  for (const { key } of lazy) {
    const chunk = manifest[key];
    if (!chunk) out.push(`lazy screen ${key} has no chunk of its own`);
    else if (!chunk.isDynamicEntry) out.push(`lazy screen ${key} is not a dynamic entry`);
    else if (initial.has(key)) out.push(`lazy screen ${key} is in the initial load (static import?)`);
  }
  for (const key of initial) {
    if (LAZY_ONLY.some((re) => re.test(key))) out.push(`${key} is in the initial load; import it lazily`);
  }
  return out;
}

function main() {
  const dist = join(appDir, 'dist');
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(join(dist, '.vite', 'manifest.json'), 'utf8'));
  } catch {
    console.error('check-bundle: dist/.vite/manifest.json missing — run `pnpm build` first');
    process.exit(1);
  }
  const sizeOf = (f) => statSync(join(dist, f)).size;
  const lazy = lazyScreens(join(appDir, 'src', 'lib', 'features'));
  const files = initialFiles(manifest);
  const kb = (n) => `${(n / 1000).toFixed(1)} kB`;
  for (const kind of ['js', 'css']) {
    const raw = files[kind].reduce((n, f) => n + sizeOf(f), 0);
    const gz = files[kind].reduce((n, f) => n + gzipSync(readFileSync(join(dist, f))).length, 0);
    console.log(`initial ${kind}: ${kb(raw)} (gzip ${kb(gz)}), budget ${kb(BUDGETS[kind])}`);
  }
  console.log(`lazy screens: ${lazy.length}`);
  const found = problems(manifest, sizeOf, lazy);
  for (const p of found) console.error(`check-bundle: ${p}`);
  process.exit(found.length ? 1 : 0);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main();
