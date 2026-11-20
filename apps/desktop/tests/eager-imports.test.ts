/**
 * Bundle hygiene (v1.1): code loaded before the first screen (`src/main.ts` and everything it
 * imports statically — App, the shell, the always-mounted hosts) imports `$lib/ui` components
 * file by file (`$lib/ui/Button.svelte`, `$lib/ui/icons`), never the `$lib/ui` barrel, which
 * would pull every component into the initial chunk. Lazy screens may use the barrel.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const app = resolve(__dirname, '..');
const src = join(app, 'src');
const BARREL = join(src, 'lib/ui/index.ts');

/** Static, value-level import specifiers of a source file (`import type` is erased). */
export function staticImports(code: string): string[] {
  const out: string[] = [];
  const re = /(?:^|[\s;])(import|export)\s+(type\s+)?(?:[^'"`;()]*?\sfrom\s+)?['"]([^'"]+)['"]/g;
  for (const m of code.matchAll(re)) {
    if (m[2]) continue;
    out.push(m[3]!);
  }
  return out;
}

function resolveSpec(from: string, spec: string): string | null {
  let base: string;
  if (spec.startsWith('$lib/')) base = join(src, 'lib', spec.slice(5));
  else if (spec === '$lib') base = join(src, 'lib');
  else if (spec.startsWith('.')) base = resolve(dirname(from), spec);
  else return null;
  for (const c of [base, `${base}.ts`, `${base}.svelte.ts`, `${base}.js`, join(base, 'index.ts')]) {
    if (existsSync(c) && !c.endsWith('/') && /\.(ts|js|svelte)$/.test(c)) return c;
  }
  return null;
}

function scriptOf(file: string): string {
  const code = readFileSync(file, 'utf8');
  if (!file.endsWith('.svelte')) return code;
  return [...code.matchAll(/<script[^>]*>([\s\S]*?)<\/script>/g)].map((m) => m[1]).join('\n');
}

/** Files of the initial load, each with the chain that reaches it. */
function eagerGraph(entry: string): Map<string, string[]> {
  const seen = new Map<string, string[]>([[entry, [entry]]]);
  const queue = [entry];
  while (queue.length) {
    const file = queue.shift()!;
    if (file === BARREL) continue;
    for (const spec of staticImports(scriptOf(file))) {
      const dep = resolveSpec(file, spec);
      if (!dep || seen.has(dep)) continue;
      seen.set(dep, [...seen.get(file)!, dep]);
      queue.push(dep);
    }
  }
  return seen;
}

describe('eager imports', () => {
  it('finds static value imports and skips type-only and dynamic ones', () => {
    const code = `import { a } from '$lib/x';\nimport type { B } from '$lib/y';\nimport './z.css';\nconst c = import('$lib/lazy');\nexport { d } from './d';`;
    expect(staticImports(code)).toEqual(['$lib/x', './z.css', './d']);
  });

  it('nothing loaded before the first screen imports the $lib/ui barrel', () => {
    const graph = eagerGraph(join(src, 'main.ts'));
    expect(graph.size).toBeGreaterThan(10);
    const chain = graph.get(BARREL);
    expect(chain?.map((f) => relative(app, f)).join(' → ') ?? null).toBeNull();
  });
});
