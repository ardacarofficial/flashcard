// Fails on paths that collide case-insensitively (Windows/macOS) and on extension-less import
// specifiers that could resolve to two files differing only by case/extension.
// Usage: node check-case-collisions.mjs [root=repo] [dir...]; exports `scan` for tests.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, resolve, basename } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const SKIP = new Set(['node_modules', 'target', '.git', 'dist', 'build', '.svelte-kit', 'screens']);
const SRC = /\.(svelte|ts|js|mjs|cjs|tsx|jsx)$/;
const IMPORT = /(?:from\s*|import\s*\(?\s*|import\s+|export\s+[^;'"]*?from\s*)['"](\.{1,2}\/[^'"]+)['"]/g;

function walk(dir, files = [], dirs = []) {
  dirs.push(dir);
  for (const name of readdirSync(dir)) {
    if (SKIP.has(name)) continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, files, dirs);
    else files.push(p);
  }
  return { files, dirs };
}

export function scan(roots) {
  const problems = [];
  const all = [];
  for (const r of roots) {
    try { all.push(walk(r)); } catch { /* missing root */ }
  }
  const byDir = new Map();
  for (const { files, dirs } of all) {
    for (const p of [...files, ...dirs]) {
      const d = dirname(p);
      if (!byDir.has(d)) byDir.set(d, new Map());
      const m = byDir.get(d);
      const k = basename(p).toLowerCase();
      if (!m.has(k)) m.set(k, new Set());
      m.get(k).add(basename(p));
    }
  }
  for (const [d, m] of byDir)
    for (const names of m.values())
      if (names.size > 1) problems.push(`case collision in ${d}: ${[...names].join(' vs ')}`);
  for (const { files } of all) {
    for (const f of files.filter((x) => SRC.test(x))) {
      const text = readFileSync(f, 'utf8');
      for (const mt of text.matchAll(IMPORT)) {
        const target = resolve(dirname(f), mt[1]);
        const dir = dirname(target);
        const base = basename(target).toLowerCase();
        let entries = [];
        try { entries = readdirSync(dir); } catch { continue; }
        // Candidates a bundler may try: exact, + extension, /index.*
        const hits = entries.filter((e) => {
          const l = e.toLowerCase();
          return l === base || (l.startsWith(base + '.') && !l.slice(base.length + 1).includes('.'));
        });
        if (hits.length > 1)
          problems.push(`ambiguous import '${mt[1]}' in ${f}: ${hits.join(', ')}`);
      }
    }
  }
  return problems;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const args = process.argv.slice(2);
  const roots = args.length ? args : ['apps/desktop/src', 'apps/browser-extension/src', 'crates', 'docs'].map((x) => join(root, x));
  const problems = scan(roots);
  for (const p of problems) console.error(p);
  if (problems.length) process.exit(1);
  console.log('no case collisions');
}
