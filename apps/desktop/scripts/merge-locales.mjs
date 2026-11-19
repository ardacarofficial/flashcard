// Merges the per-feature dictionaries `src/lib/i18n/locales/<lang>/<feature>.json` into
// `src/lib/i18n/generated/<lang>.json` (git-ignored), which `core.ts` imports for typed keys.
//
// * Each feature file is a nested object from the root, e.g. decks.json holds `deck.*`,
//   `modules.decks.*` and the deck `errors.*`. Files are deep-merged; the same key in two
//   files is an error, so features never overwrite each other.
// * Every language must have the same feature files (key-level TR/EN completeness is checked
//   by the type system in `core.ts` and by the i18n tests).
//
// Run by `pnpm check|test|build|dev` and by the Vite plugin in `vite.config.ts`.
// A feature adds its own `<lang>/<feature>.json` files; nothing else is edited.
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', 'src', 'lib', 'i18n');
const sourceDir = join(root, 'locales');
const outDir = join(root, 'generated');

function deepMerge(target, source, file, path = []) {
  for (const [key, value] of Object.entries(source)) {
    const at = [...path, key];
    const existing = target[key];
    if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
      if (existing === undefined) target[key] = {};
      else if (typeof existing !== 'object') {
        throw new Error(`${file}: "${at.join('.')}" is a message elsewhere, not a group`);
      }
      deepMerge(target[key], value, file, at);
    } else if (typeof value === 'string') {
      if (existing !== undefined) throw new Error(`${file}: "${at.join('.')}" is defined twice`);
      target[key] = value;
    } else {
      throw new Error(`${file}: "${at.join('.')}" must be a string or an object`);
    }
  }
}

/** Merged dictionaries by language. Throws on conflicts or mismatched feature files. */
export function mergeLocales() {
  const langs = readdirSync(sourceDir, { withFileTypes: true })
    .filter((d) => d.isDirectory())
    .map((d) => d.name)
    .sort();
  const featuresOf = (lang) =>
    readdirSync(join(sourceDir, lang))
      .filter((f) => f.endsWith('.json'))
      .sort();
  const reference = featuresOf('en');
  const merged = {};
  for (const lang of langs) {
    const files = featuresOf(lang);
    if (files.join() !== reference.join()) {
      throw new Error(`locales/${lang} has [${files}] but locales/en has [${reference}]`);
    }
    merged[lang] = {};
    for (const file of files) {
      const rel = `locales/${lang}/${file}`;
      deepMerge(merged[lang], JSON.parse(readFileSync(join(sourceDir, lang, file), 'utf8')), rel);
    }
  }
  return merged;
}

/** Writes `generated/<lang>.json` (only when the content changed). */
export function writeLocales() {
  const merged = mergeLocales();
  mkdirSync(outDir, { recursive: true });
  for (const [lang, dict] of Object.entries(merged)) {
    const file = join(outDir, `${lang}.json`);
    const text = `${JSON.stringify(dict, null, 2)}\n`;
    if (!existsSync(file) || readFileSync(file, 'utf8') !== text) writeFileSync(file, text);
  }
  return Object.keys(merged);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    writeLocales();
  } catch (e) {
    console.error(`merge-locales: ${e.message}`);
    process.exit(1);
  }
}
